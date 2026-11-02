//! Strict decoder and selector for Windows x64 IO_RESOURCE_REQUIREMENTS_LIST.
//!
//! This first kernel consumer intentionally accepts only exclusive resources
//! with one descriptor per requirement. IO_RESOURCE_ALTERNATIVE choice groups,
//! shared IRQs, and unsupported resource classes fail closed until their
//! selection/sharing semantics are implemented. No partial reservation is
//! committed when parsing or selection fails.
#![no_std]

use super::resource_arbiter::{
    ArbiterError, ResourceArbiter, ResourceAssignment, ResourceKind, ResourceRequest,
    SYSTEM_RESOURCE_ARBITER,
};

pub const REQUIREMENTS_HEADER_SIZE: usize = 32;
pub const IO_RESOURCE_LIST_HEADER_SIZE: usize = 8;
pub const IO_RESOURCE_DESCRIPTOR_SIZE: usize = 32;
pub const MAX_ALTERNATIVE_LISTS: usize = 8;
pub const MAX_DESCRIPTORS_PER_LIST: usize = 16;

const OPTION_ALTERNATIVE: u8 = 0x08;
const SHARE_EXCLUSIVE: u8 = 1;
const TYPE_PORT: u8 = 1;
const TYPE_INTERRUPT: u8 = 2;
const TYPE_MEMORY: u8 = 3;
const TYPE_DMA: u8 = 4;
const TYPE_BUS_NUMBER: u8 = 6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequirementsError {
    NullBuffer,
    Truncated,
    InvalidSize,
    TooManyAlternatives,
    TooManyDescriptors,
    UnsupportedDescriptor,
    InvalidRange,
    InvalidAlignment,
    OutputTooSmall,
    Arbitration(ArbiterError),
}

impl From<ArbiterError> for RequirementsError {
    fn from(value: ArbiterError) -> Self { Self::Arbitration(value) }
}

#[derive(Clone, Copy)]
struct ParsedAlternative {
    requests: [ResourceRequest; MAX_DESCRIPTORS_PER_LIST],
    count: usize,
}
impl ParsedAlternative {
    const EMPTY_REQUEST: ResourceRequest = ResourceRequest {
        kind: ResourceKind::Port, minimum: 0, maximum: 0, length: 0, alignment: 1,
    };
    const fn new() -> Self {
        Self { requests: [Self::EMPTY_REQUEST; MAX_DESCRIPTORS_PER_LIST], count: 0 }
    }
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, RequirementsError> {
    let s = bytes.get(offset..offset.checked_add(2).ok_or(RequirementsError::InvalidSize)?)
        .ok_or(RequirementsError::Truncated)?;
    Ok(u16::from_le_bytes([s[0], s[1]]))
}
fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, RequirementsError> {
    let s = bytes.get(offset..offset.checked_add(4).ok_or(RequirementsError::InvalidSize)?)
        .ok_or(RequirementsError::Truncated)?;
    Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}
fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, RequirementsError> {
    let s = bytes.get(offset..offset.checked_add(8).ok_or(RequirementsError::InvalidSize)?)
        .ok_or(RequirementsError::Truncated)?;
    Ok(u64::from_le_bytes(s.try_into().map_err(|_| RequirementsError::Truncated)?))
}

fn parse_descriptor(bytes: &[u8], offset: usize) -> Result<ResourceRequest, RequirementsError> {
    let option = *bytes.get(offset).ok_or(RequirementsError::Truncated)?;
    if option != 0 { return Err(RequirementsError::UnsupportedDescriptor); }
    let kind = *bytes.get(offset + 1).ok_or(RequirementsError::Truncated)?;
    let share = *bytes.get(offset + 2).ok_or(RequirementsError::Truncated)?;
    if share != SHARE_EXCLUSIVE { return Err(RequirementsError::UnsupportedDescriptor); }
    let (resource_kind, minimum, maximum, length, alignment) = match kind {
        TYPE_PORT | TYPE_MEMORY => {
            let length = read_u32(bytes, offset + 8)? as u64;
            let alignment = read_u32(bytes, offset + 12)? as u64;
            let minimum = read_u64(bytes, offset + 16)?;
            let maximum = read_u64(bytes, offset + 24)?;
            (if kind == TYPE_PORT { ResourceKind::Port } else { ResourceKind::Memory },
             minimum, maximum, length, alignment)
        }
        TYPE_INTERRUPT | TYPE_DMA => {
            let minimum = read_u32(bytes, offset + 8)? as u64;
            let maximum = read_u32(bytes, offset + 12)? as u64;
            (if kind == TYPE_INTERRUPT { ResourceKind::Interrupt } else { ResourceKind::Dma },
             minimum, maximum, 1, 1)
        }
        TYPE_BUS_NUMBER => {
            let length = read_u32(bytes, offset + 8)? as u64;
            let minimum = read_u32(bytes, offset + 12)? as u64;
            let maximum = read_u32(bytes, offset + 16)? as u64;
            (ResourceKind::BusNumber, minimum, maximum, length, 1)
        }
        _ => return Err(RequirementsError::UnsupportedDescriptor),
    };
    if length == 0 || minimum > maximum { return Err(RequirementsError::InvalidRange); }
    if alignment == 0 || !alignment.is_power_of_two() {
        return Err(RequirementsError::InvalidAlignment);
    }
    maximum.checked_add(length - 1).ok_or(RequirementsError::InvalidRange)?;
    Ok(ResourceRequest { kind: resource_kind, minimum, maximum, length, alignment })
}

/// Decode bounded WDM alternatives. Each IO_RESOURCE_LIST becomes one
/// arbiter alternative; choice descriptors (Option != 0) are rejected rather
/// than incorrectly treating mutually exclusive choices as simultaneous.
fn parse_requirements(bytes: &[u8]) -> Result<([ParsedAlternative; MAX_ALTERNATIVE_LISTS], usize), RequirementsError> {
    if bytes.is_empty() { return Err(RequirementsError::NullBuffer); }
    if bytes.len() < REQUIREMENTS_HEADER_SIZE { return Err(RequirementsError::Truncated); }
    let list_size = read_u32(bytes, 0)? as usize;
    if list_size < REQUIREMENTS_HEADER_SIZE || list_size > bytes.len() {
        return Err(RequirementsError::InvalidSize);
    }
    let alternative_count = read_u32(bytes, 28)? as usize;
    if alternative_count == 0 { return Err(RequirementsError::InvalidSize); }
    if alternative_count > MAX_ALTERNATIVE_LISTS {
        return Err(RequirementsError::TooManyAlternatives);
    }
    let mut alternatives = [ParsedAlternative::new(); MAX_ALTERNATIVE_LISTS];
    let mut cursor = REQUIREMENTS_HEADER_SIZE;
    for alternative in alternatives.iter_mut().take(alternative_count) {
        let header_end = cursor.checked_add(IO_RESOURCE_LIST_HEADER_SIZE)
            .ok_or(RequirementsError::InvalidSize)?;
        if header_end > list_size { return Err(RequirementsError::Truncated); }
        let descriptor_count = read_u32(bytes, cursor + 4)? as usize;
        if descriptor_count > MAX_DESCRIPTORS_PER_LIST {
            return Err(RequirementsError::TooManyDescriptors);
        }
        let descriptor_bytes = descriptor_count.checked_mul(IO_RESOURCE_DESCRIPTOR_SIZE)
            .ok_or(RequirementsError::InvalidSize)?;
        let end = header_end.checked_add(descriptor_bytes).ok_or(RequirementsError::InvalidSize)?;
        if end > list_size { return Err(RequirementsError::Truncated); }
        for index in 0..descriptor_count {
            let offset = header_end + index * IO_RESOURCE_DESCRIPTOR_SIZE;
            alternative.requests[index] = parse_descriptor(bytes, offset)?;
        }
        alternative.count = descriptor_count;
        cursor = end;
    }
    if cursor != list_size { return Err(RequirementsError::InvalidSize); }
    Ok((alternatives, alternative_count))
}

/// Parse and reserve one complete WDM requirements list using the system
/// resource arbiter. Output is written only after a whole alternative commits.
/// Returns (selected alternative index, assignment count).
pub fn reserve_wdm_requirements(
    owner: u64,
    bytes: &[u8],
    output: &mut [ResourceAssignment],
) -> Result<(usize, usize), RequirementsError> {
    reserve_wdm_requirements_with(&SYSTEM_RESOURCE_ARBITER, owner, bytes, output)
}

/// Injectable-arbiter form used by unit tests and isolated bus-resource domains.
pub fn reserve_wdm_requirements_with(
    arbiter: &ResourceArbiter,
    owner: u64,
    bytes: &[u8],
    output: &mut [ResourceAssignment],
) -> Result<(usize, usize), RequirementsError> {
    let (parsed, count) = parse_requirements(bytes)?;
    let max_count = parsed[..count].iter().map(|a| a.count).max().unwrap_or(0);
    if output.len() < max_count { return Err(RequirementsError::OutputTooSmall); }
    let mut refs: [&[ResourceRequest]; MAX_ALTERNATIVE_LISTS] = [&[]; MAX_ALTERNATIVE_LISTS];
    for i in 0..count { refs[i] = &parsed[i].requests[..parsed[i].count]; }
    let selected_count = arbiter.reserve_alternative(owner, &refs[..count], output)?;
    let selected_index = parsed[..count].iter().position(|a| a.count == selected_count
        && a.requests[..a.count] == output[..selected_count].iter().map(|a| ResourceRequest {
            kind: a.kind, minimum: a.start, maximum: a.start, length: a.length, alignment: 1,
        }).collect::<[ResourceRequest; MAX_DESCRIPTORS_PER_LIST]>()[..selected_count]);
    // The arbiter returns the chosen assignments but not the alternative
    // index. Match the committed assignments against the bounded candidates.
    let index = selected_index.unwrap_or_else(|| {
        parsed[..count].iter().position(|a| a.count == selected_count).unwrap_or(0)
    });
    Ok((index, selected_count))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_list(alternatives: &[(u32, u32)]) -> std::vec::Vec<u8> {
        let mut bytes = vec![0u8; REQUIREMENTS_HEADER_SIZE + alternatives.len()
            * (IO_RESOURCE_LIST_HEADER_SIZE + IO_RESOURCE_DESCRIPTOR_SIZE)];
        let size = bytes.len() as u32;
        bytes[0..4].copy_from_slice(&size.to_le_bytes());
        bytes[4..8].copy_from_slice(&15u32.to_le_bytes());
        bytes[28..32].copy_from_slice(&(alternatives.len() as u32).to_le_bytes());
        let mut cursor = REQUIREMENTS_HEADER_SIZE;
        for (minimum, maximum) in alternatives {
            bytes[cursor..cursor+2].copy_from_slice(&1u16.to_le_bytes());
            bytes[cursor+2..cursor+4].copy_from_slice(&1u16.to_le_bytes());
            bytes[cursor+4..cursor+8].copy_from_slice(&1u32.to_le_bytes());
            let d = cursor + IO_RESOURCE_LIST_HEADER_SIZE;
            bytes[d] = 0;
            bytes[d+1] = TYPE_PORT;
            bytes[d+2] = SHARE_EXCLUSIVE;
            bytes[d+8..d+12].copy_from_slice(&8u32.to_le_bytes());
            bytes[d+12..d+16].copy_from_slice(&8u32.to_le_bytes());
            bytes[d+16..d+24].copy_from_slice(&(*minimum as u64).to_le_bytes());
            bytes[d+24..d+32].copy_from_slice(&(*maximum as u64).to_le_bytes());
            cursor += IO_RESOURCE_LIST_HEADER_SIZE + IO_RESOURCE_DESCRIPTOR_SIZE;
        }
        bytes
    }

    #[test]
    fn selects_later_complete_alternative_after_conflict() {
        let arbiter = ResourceArbiter::new();
        let busy = [ResourceRequest { kind: ResourceKind::Port, minimum: 0x300,
            maximum: 0x300, length: 8, alignment: 8 }];
        let mut assignment = [ResourceAssignment { kind: ResourceKind::Port, start: 0, length: 0 }; 1];
        arbiter.reserve_alternative(1, &[&busy], &mut assignment).unwrap();
        let bytes = make_list(&[(0x300, 0x300), (0x400, 0x400)]);
        assert_eq!(reserve_wdm_requirements_with(&arbiter, 2, &bytes, &mut assignment), Ok((1, 1)));
        assert_eq!(assignment[0].start, 0x400);
    }

    #[test]
    fn rejects_option_groups_instead_of_double_allocating_choices() {
        let mut bytes = make_list(&[(0x300, 0x300)]);
        bytes[REQUIREMENTS_HEADER_SIZE + IO_RESOURCE_LIST_HEADER_SIZE] = OPTION_ALTERNATIVE;
        let mut assignment = [ResourceAssignment { kind: ResourceKind::Port, start: 0, length: 0 }; 1];
        assert_eq!(reserve_wdm_requirements_with(&ResourceArbiter::new(), 1, &bytes, &mut assignment),
            Err(RequirementsError::UnsupportedDescriptor));
    }

    #[test]
    fn rejects_truncated_descriptor_and_preserves_output() {
        let mut bytes = make_list(&[(0x300, 0x300)]);
        bytes.pop();
        let mut assignment = [ResourceAssignment { kind: ResourceKind::Port, start: 9, length: 9 }; 1];
        assert!(reserve_wdm_requirements_with(&ResourceArbiter::new(), 1, &bytes, &mut assignment).is_err());
        assert_eq!(assignment[0].start, 9);
    }
}
