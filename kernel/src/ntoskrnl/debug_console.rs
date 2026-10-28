/// Non-blocking UART debugger console for NTOSKRNL.
/// Commands: help, status, trace [count], filter <component> <level> <on|off>.
/// This is a polled console, not a remote KD transport; callers must poll at a
/// safe point where serial input is expected.
use crate::ntoskrnl::debug;

const INPUT_CAPACITY: usize = 96;
static mut INPUT: [u8; INPUT_CAPACITY] = [0; INPUT_CAPACITY];
static mut INPUT_LEN: usize = 0;

#[inline]
unsafe fn uart_read_byte() -> Option<u8> {
    let status: u8;
    unsafe {
        core::arch::asm!("in al, dx", in("dx") 0x3fdu16, out("al") status,
            options(nomem, nostack, preserves_flags));
    }
    if status & 1 == 0 { return None; }
    let value: u8;
    unsafe {
        core::arch::asm!("in al, dx", in("dx") 0x3f8u16, out("al") value,
            options(nomem, nostack, preserves_flags));
    }
    Some(value)
}

fn write(bytes: &[u8]) { unsafe { crate::serial_write(bytes); } }
fn hex(value: u64) { unsafe { crate::serial_write_hex(value); } }

fn parse_u32(bytes: &[u8]) -> Option<u32> {
    if bytes.is_empty() { return None; }
    let mut value = 0u32;
    for &b in bytes {
        let digit = match b {
            b'0'..=b'9' => (b - b'0') as u32,
            b'a'..=b'f' => (b - b'a' + 10) as u32,
            b'A'..=b'F' => (b - b'A' + 10) as u32,
            _ => return None,
        };
        value = value.checked_mul(16)?.checked_add(digit)?;
    }
    Some(value)
}

fn words(line: &[u8]) -> [Option<&[u8]>; 4] {
    let mut out = [None; 4];
    let mut count = 0usize;
    let mut start = 0usize;
    for i in 0..=line.len() {
        if i == line.len() || line[i] == b' ' || line[i] == b'\t' {
            if i > start && count < out.len() {
                out[count] = Some(&line[start..i]);
                count += 1;
            }
            start = i + 1;
        }
    }
    out
}

fn command(line: &[u8]) {
    let w = words(line);
    match w[0] {
        Some(b"help") => write(b"debug commands: help | status | trace [count-hex] | filter <component-hex> <level-hex> <on|off> | persist | restore\r\n"),
        Some(b"status") => {
            write(b"trace latest sequence=");
            hex(debug::latest_sequence());
            write(b" capacity=");
            hex(debug::TRACE_CAPACITY as u64);
            write(b" UART=COM1 polled-console=active remote-KD=not-configured\r\n");
        }
        Some(b"trace") => {
            let requested = w[1].and_then(parse_u32).unwrap_or(32) as usize;
            debug::dump_serial(requested.min(debug::TRACE_CAPACITY));
        }
        Some(b"persist") => {
            if debug::persist_to_storage() { write(b"trace snapshot persisted to reserved GPT partition\\r\\n"); }
            else { write(b"persist failed: dedicated trace partition unavailable or I/O failed\\r\\n"); }
        }
        Some(b"restore") => {
            if debug::restore_from_storage() { write(b"validated trace snapshot restored\\r\\n"); }
            else { write(b"restore failed: no valid snapshot or storage unavailable\\r\\n"); }
        }
        Some(b"filter") => {
            let component = w[1].and_then(parse_u32);
            let level = w[2].and_then(parse_u32);
            let state = w[3];
            match (component, level, state) {
                (Some(c), Some(l), Some(b"on")) if debug::set_filter(c, l, true) => write(b"filter enabled\r\n"),
                (Some(c), Some(l), Some(b"off")) if debug::set_filter(c, l, false) => write(b"filter disabled\r\n"),
                (Some(c), Some(l), _) if l >= 32 => write(b"error: level must be 0..1f (hex)\r\n"),
                _ => write(b"usage: filter <component-hex> <level-hex> <on|off>\r\n"),
            }
        }
        Some(_) => write(b"unknown command; type help\r\n"),
        None => {}
    }
}

/// Poll COM1 without waiting. Call from a safe, single-owner console context.
/// Input is bounded, line-oriented, and does not allocate.
pub fn poll() {
    loop {
        let byte = unsafe { uart_read_byte() };
        let Some(byte) = byte else { break; };
        match byte {
            b'\r' | b'\n' => {
                let len = unsafe { INPUT_LEN };
                if len != 0 {
                    write(b"\r\n");
                    let mut line = [0u8; INPUT_CAPACITY];
                    unsafe {
                        core::ptr::copy_nonoverlapping(core::ptr::addr_of!(INPUT).cast::<u8>(), line.as_mut_ptr(), len);
                        INPUT_LEN = 0;
                    }
                    command(&line[..len]);
                }
                write(b"cognacos-kd> ");
            }
            8 | 127 => unsafe {
                if INPUT_LEN != 0 { INPUT_LEN -= 1; write(b"\x08 \x08"); }
            },
            0x20..=0x7e => unsafe {
                if INPUT_LEN < INPUT_CAPACITY {
                    core::ptr::write(core::ptr::addr_of_mut!(INPUT).cast::<u8>().add(INPUT_LEN), byte);
                    INPUT_LEN += 1;
                    write(&[byte]);
                }
            },
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_hex_numbers_without_overflow() {
        assert_eq!(parse_u32(b"0"), Some(0));
        assert_eq!(parse_u32(b"ff"), Some(255));
        assert_eq!(parse_u32(b"FFFFFFFF"), Some(u32::MAX));
        assert_eq!(parse_u32(b"100000000"), None);
        assert_eq!(parse_u32(b"0x10"), None);
    }
    #[test]
    fn splits_commands_into_bounded_words() {
        let w = words(b"filter 1a 03 on");
        assert_eq!(w[0], Some(&b"filter"[..]));
        assert_eq!(w[1], Some(&b"1a"[..]));
        assert_eq!(w[2], Some(&b"03"[..]));
        assert_eq!(w[3], Some(&b"on"[..]));
    }
}
