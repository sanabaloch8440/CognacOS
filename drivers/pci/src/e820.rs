//! E820 RAM-reservation clipping for PCI host-bridge _CRS apertures.
//!
//! The input ranges represent physical memory which must remain unavailable
//! for PCI BAR placement (RAM and firmware-owned RAM), not every non-RAM E820
//! entry. Apply clipping once to decoded host-bridge windows before publishing
//! them to the PCI resource arbiter. I/O apertures are never clipped.
//! A platform quirk may disable clipping when firmware reserves an entire
//! _CRS window despite that window being required for hot-plug capacity.
#![no_std]
use crate::resource_manager::Aperture;
use crate::resources::BarKind;

pub const MAX_E820_RANGES: usize = 256;
pub const MAX_CLIPPED_APERTURES: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct E820Range { pub start: u64, pub length: u64 }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipError { InvalidRange, Overflow, Capacity }

fn end(start: u64, length: u64) -> Result<u64, ClipError> {
    if length == 0 { return Err(ClipError::InvalidRange); }
    start.checked_add(length).ok_or(ClipError::Overflow)
}

fn validate_apertures(input: &[Aperture]) -> Result<(), ClipError> {
    if input.len() > MAX_CLIPPED_APERTURES { return Err(ClipError::Capacity); }
    for aperture in input {
        end(aperture.start, aperture.length)?;
    }
    Ok(())
}

/// Subtract protected RAM ranges from host-bridge memory apertures.
/// I/O apertures pass through unchanged. All ranges are validated and output
/// is staged, so every error leaves the caller's output untouched.
pub fn clip_host_apertures(
    input: &[Aperture],
    protected_ram: &[E820Range],
    output: &mut [Aperture],
) -> Result<usize, ClipError> {
    validate_apertures(input)?;
    if protected_ram.len() > MAX_E820_RANGES { return Err(ClipError::Capacity); }
    for range in protected_ram { end(range.start, range.length)?; }

    let empty = Aperture { start: 0, length: 0, kind: BarKind::Io, prefetchable: false };
    let mut staged = [empty; MAX_CLIPPED_APERTURES];
    let mut count = 0usize;

    for aperture in input.iter().copied() {
        if aperture.kind == BarKind::Io {
            if count == staged.len() { return Err(ClipError::Capacity); }
            staged[count] = aperture;
            count += 1;
            continue;
        }

        let mut fragments = [E820Range { start: 0, length: 0 }; MAX_CLIPPED_APERTURES];
        let mut fragment_count = 1usize;
        fragments[0] = E820Range { start: aperture.start, length: aperture.length };

        // Sequential subtraction also handles unsorted and overlapping E820
        // records. Duplicate reservations cannot reintroduce removed space.
        for reserved in protected_ram.iter().copied() {
            let reserved_end = end(reserved.start, reserved.length)?;
            let mut next = [E820Range { start: 0, length: 0 }; MAX_CLIPPED_APERTURES];
            let mut next_count = 0usize;
            for fragment in fragments[..fragment_count].iter().copied() {
                let fragment_end = end(fragment.start, fragment.length)?;
                if reserved.start >= fragment_end || reserved_end <= fragment.start {
                    if next_count == next.len() { return Err(ClipError::Capacity); }
                    next[next_count] = fragment;
                    next_count += 1;
                    continue;
                }
                if reserved.start > fragment.start {
                    if next_count == next.len() { return Err(ClipError::Capacity); }
                    next[next_count] = E820Range {
                        start: fragment.start,
                        length: reserved.start - fragment.start,
                    };
                    next_count += 1;
                }
                if reserved_end < fragment_end {
                    if next_count == next.len() { return Err(ClipError::Capacity); }
                    next[next_count] = E820Range {
                        start: reserved_end,
                        length: fragment_end - reserved_end,
                    };
                    next_count += 1;
                }
            }
            fragments = next;
            fragment_count = next_count;
            if fragment_count == 0 { break; }
        }

        for fragment in fragments[..fragment_count].iter().copied() {
            if count == staged.len() { return Err(ClipError::Capacity); }
            staged[count] = Aperture {
                start: fragment.start,
                length: fragment.length,
                kind: aperture.kind,
                prefetchable: aperture.prefetchable,
            };
            count += 1;
        }
    }

    if output.len() < count { return Err(ClipError::Capacity); }
    output[..count].copy_from_slice(&staged[..count]);
    Ok(count)
}

/// Policy-aware root-window preparation. The opt-out must come only from the
/// immutable, DMI-validated platform policy. Even on opt-out, malformed or
/// overflowing firmware apertures are rejected; the quirk disables clipping,
/// not input validation.
pub fn prepare_host_apertures(
    input: &[Aperture],
    protected_ram: &[E820Range],
    ignore_e820: bool,
    output: &mut [Aperture],
) -> Result<usize, ClipError> {
    validate_apertures(input)?;
    if ignore_e820 {
        if output.len() < input.len() { return Err(ClipError::Capacity); }
        output[..input.len()].copy_from_slice(input);
        return Ok(input.len());
    }
    clip_host_apertures(input, protected_ram, output)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ap(start: u64, length: u64) -> Aperture {
        Aperture { start, length, kind: BarKind::Memory32, prefetchable: false }
    }

    #[test]
    fn clips_partial_overlap_and_preserves_both_sides() {
        let input = [ap(0x8000, 0x8000)];
        let ram = [E820Range { start: 0x9000, length: 0x2000 }];
        let mut out = [ap(0, 0); 4];
        assert_eq!(clip_host_apertures(&input, &ram, &mut out), Ok(2));
        assert_eq!(out[0], ap(0x8000, 0x1000));
        assert_eq!(out[1], ap(0xb000, 0x5000));
    }

    #[test]
    fn fully_reserved_window_is_removed_and_exact_quirk_bypasses_only_clipping() {
        let input = [ap(0x8000, 0x4000)];
        let ram = [E820Range { start: 0x7000, length: 0x6000 }];
        let mut out = [ap(0, 0); 2];
        assert_eq!(clip_host_apertures(&input, &ram, &mut out), Ok(0));
        assert_eq!(prepare_host_apertures(&input, &ram, true, &mut out), Ok(1));
        assert_eq!(out[0], input[0]);
    }

    #[test]
    fn io_windows_are_not_clipped() {
        let io = Aperture { start: 0x1000, length: 0x100, kind: BarKind::Io, prefetchable: false };
        let ram = [E820Range { start: 0x1000, length: 0x100 }];
        let mut out = [io; 1];
        assert_eq!(clip_host_apertures(&[io], &ram, &mut out), Ok(1));
        assert_eq!(out[0], io);
    }

    #[test]
    fn overlapping_and_unsorted_ram_records_are_safe() {
        let input = [ap(0x1000, 0x9000)];
        let ram = [
            E820Range { start: 0x6000, length: 0x1000 },
            E820Range { start: 0x3000, length: 0x4000 },
            E820Range { start: 0x9000, length: 0x1000 },
        ];
        let mut out = [ap(0, 0); 4];
        assert_eq!(clip_host_apertures(&input, &ram, &mut out), Ok(2));
        assert_eq!(out[0], ap(0x1000, 0x2000));
        assert_eq!(out[1], ap(0xa000, 0));
    }

    #[test]
    fn invalid_ram_range_does_not_mutate_output() {
        let input = [ap(0x8000, 0x1000)];
        let ram = [E820Range { start: 0x9000, length: 0 }];
        let mut out = [ap(0xdead, 0xbeef); 2];
        let before = out;
        assert_eq!(clip_host_apertures(&input, &ram, &mut out), Err(ClipError::InvalidRange));
        assert_eq!(out, before);
    }

    #[test]
    fn opt_out_still_rejects_overflowing_firmware_windows() {
        let input = [ap(u64::MAX - 3, 8)];
        let mut out = [ap(0xdead, 0xbeef); 1];
        let before = out;
        assert_eq!(prepare_host_apertures(&input, &[], true, &mut out), Err(ClipError::Overflow));
        assert_eq!(out, before);
    }

    #[test]
    fn insufficient_output_capacity_is_transactional() {
        let input = [ap(0x8000, 0x8000)];
        let ram = [E820Range { start: 0x9000, length: 0x2000 }];
        let mut out = [ap(0xdead, 0xbeef); 1];
        let before = out;
        assert_eq!(clip_host_apertures(&input, &ram, &mut out), Err(ClipError::Capacity));
        assert_eq!(out, before);
    }
}
