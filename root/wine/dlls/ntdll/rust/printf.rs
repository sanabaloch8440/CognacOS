//! Source-first Rust port of the format-specifier parser used by Wine's
//! ntdll printf formatter.
//!
//! This ports format grammar and field metadata without claiming to implement
//! C va_list decoding or the complete ANSI/UTF-16 formatter. Those require
//! target-specific ABI integration and Wine's Unicode conversion routines.
//! It is not linked into the freestanding PE32+ build yet.

#![allow(non_camel_case_types)]

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FormatFlags {
    pub left_align: bool,
    pub alternate: bool,
    pub zero_pad: bool,
    /// '+' or ' '; zero means no explicit sign.
    pub sign: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Length { Default, Short, Long, LongLong, Native }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FormatSpec {
    pub flags: FormatFlags,
    pub width: Option<usize>,
    pub precision: Option<usize>,
    pub length: Length,
    pub wide_string: bool,
    pub conversion: u8,
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseError {
    TrailingPercent { offset: usize },
    InvalidWidth { offset: usize },
    InvalidPrecision { offset: usize },
    UnsupportedPositionalParameter { offset: usize },
    MissingConversion { offset: usize },
}

#[inline]
fn is_integer_conversion(c: u8) -> bool {
    matches!(c, b'd' | b'i' | b'o' | b'u' | b'x' | b'X')
}

fn parse_decimal(bytes: &[u8], mut pos: usize) -> Result<(usize, usize), ParseError> {
    let start = pos;
    let mut value = 0usize;
    while let Some(&b) = bytes.get(pos) {
        if !b.is_ascii_digit() { break; }
        value = value.checked_mul(10)
            .and_then(|v| v.checked_add((b - b'0') as usize))
            .ok_or(ParseError::InvalidWidth { offset: start })?;
        pos += 1;
    }
    Ok((value, pos))
}

/// Parse one conversion beginning at start, which must point to percent.
pub fn parse_spec(format: &[u8], start: usize) -> Result<FormatSpec, ParseError> {
    if format.get(start) != Some(&b'%') {
        return Err(ParseError::MissingConversion { offset: start });
    }
    let mut pos = start + 1;
    if pos >= format.len() {
        return Err(ParseError::TrailingPercent { offset: start });
    }
    if format[pos] == b'%' {
        return Ok(FormatSpec {
            flags: FormatFlags::default(), width: None, precision: None,
            length: Length::Default, wide_string: false, conversion: b'%',
            start, end: pos + 1,
        });
    }

    let mut flags = FormatFlags::default();
    loop {
        match format.get(pos).copied() {
            Some(b'+') | Some(b' ') => {
                let c = format[pos];
                if flags.sign != b'+' { flags.sign = c; }
                pos += 1;
            }
            Some(b'-') => { flags.left_align = true; pos += 1; }
            Some(b'0') => { flags.zero_pad = true; pos += 1; }
            Some(b'#') => { flags.alternate = true; pos += 1; }
            _ => break,
        }
    }

    let width = if format.get(pos) == Some(&b'*') {
        pos += 1;
        None
    } else if format.get(pos).is_some_and(u8::is_ascii_digit) {
        let (v, next) = parse_decimal(format, pos)?;
        pos = next;
        Some(v)
    } else { None };

    let precision = if format.get(pos) == Some(&b'.') {
        pos += 1;
        if format.get(pos) == Some(&b'*') {
            pos += 1;
            None
        } else {
            let (v, next) = parse_decimal(format, pos)
                .map_err(|_| ParseError::InvalidPrecision { offset: pos })?;
            pos = next;
            Some(v)
        }
    } else { None };

    let mut length = Length::Default;
    let mut wide_string = false;
    loop {
        match format.get(pos).copied() {
            Some(b'l') if format.get(pos + 1) == Some(&b'l') => {
                length = Length::LongLong; pos += 2;
            }
            Some(b'l') => { if length != Length::Short { length = Length::Long; } wide_string = true; pos += 1; }
            Some(b'h') => { length = Length::Short; pos += 1; }
            Some(b'L') | Some(b'F') => { pos += 1; }
            Some(b'I') if format.get(pos + 1..pos + 3) == Some(b"64") => {
                length = Length::LongLong; pos += 3;
            }
            Some(b'I') if format.get(pos + 1..pos + 3) == Some(b"32") => { pos += 3; }
            Some(b'I') if format.get(pos + 1).is_some_and(|c| is_integer_conversion(*c)) => {
                length = Length::Native; pos += 1;
            }
            Some(b'z' | b't') if format.get(pos + 1).is_some_and(|c| is_integer_conversion(*c)) => {
                length = Length::Native; pos += 1;
            }
            Some(b'j') => { length = Length::LongLong; pos += 1; }
            Some(b'w') => { wide_string = true; pos += 1; }
            _ => break,
        }
    }

    let conversion = *format.get(pos).ok_or(ParseError::TrailingPercent { offset: start })?;
    if conversion == b'$' {
        return Err(ParseError::UnsupportedPositionalParameter { offset: pos });
    }
    Ok(FormatSpec { flags, width, precision, length, wide_string, conversion, start, end: pos + 1 })
}

/// Parse all conversion specifications, skipping literal text.
pub fn parse_format(format: &[u8]) -> Result<Vec<FormatSpec>, ParseError> {
    let mut specs = Vec::new();
    let mut pos = 0;
    while pos < format.len() {
        if format[pos] != b'%' { pos += 1; continue; }
        let spec = parse_spec(format, pos)?;
        pos = spec.end;
        specs.push(spec);
    }
    Ok(specs)
}

/// Decimal digits for a signed value, including i64::MIN without overflow.
pub fn signed_decimal(value: i64) -> ([u8; 20], usize, bool) {
    let negative = value < 0;
    let mut n = value.unsigned_abs();
    let mut buf = [0u8; 20];
    let mut len = 0;
    loop {
        buf[len] = b'0' + (n % 10) as u8;
        len += 1;
        n /= 10;
        if n == 0 { break; }
    }
    buf[..len].reverse();
    (buf, len, negative)
}

/// Render an unsigned integer in a base from 2 through 16.
pub fn unsigned_digits(mut value: u64, base: u32, uppercase: bool) -> Option<([u8; 64], usize)> {
    if !(2..=16).contains(&base) { return None; }
    let alphabet = if uppercase { b"0123456789ABCDEF" } else { b"0123456789abcdef" };
    let mut buf = [0u8; 64];
    let mut len = 0;
    loop {
        buf[len] = alphabet[(value % base as u64) as usize];
        len += 1;
        value /= base as u64;
        if value == 0 { break; }
    }
    buf[..len].reverse();
    Some((buf, len))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_flags_width_precision_and_integer_lengths() {
        let s = parse_spec(b"%#0-+12.5I64X", 0).unwrap();
        assert_eq!(s.flags, FormatFlags { left_align: true, alternate: true, zero_pad: true, sign: b'+' });
        assert_eq!(s.width, Some(12));
        assert_eq!(s.precision, Some(5));
        assert_eq!(s.length, Length::LongLong);
        assert_eq!(s.conversion, b'X');
    }

    #[test]
    fn parses_dynamic_fields_and_wide_strings() {
        let s = parse_spec(b"%*.*ls", 0).unwrap();
        assert_eq!(s.width, None);
        assert_eq!(s.precision, None);
        assert!(s.wide_string);
        assert_eq!(s.conversion, b's');
    }

    #[test]
    fn escaped_percent_and_multiple_specs() {
        let specs = parse_format(b"hello %% %08x %s").unwrap();
        assert_eq!(specs.len(), 3);
        assert_eq!(specs[0].conversion, b'%');
        assert_eq!(specs[1].width, Some(8));
        assert_eq!(specs[2].conversion, b's');
    }

    #[test]
    fn malformed_and_positional_formats_are_rejected() {
        assert_eq!(parse_spec(b"%", 0), Err(ParseError::TrailingPercent { offset: 0 }));
        assert_eq!(parse_spec(b"%1$d", 0), Err(ParseError::UnsupportedPositionalParameter { offset: 2 }));
    }

    #[test]
    fn integer_conversion_handles_minimum_and_base_edges() {
        let (digits, len, neg) = signed_decimal(i64::MIN);
        assert_eq!(&digits[..len], b"9223372036854775808");
        assert!(neg);
        let (hex, len) = unsigned_digits(0xfeed, 16, true).unwrap();
        assert_eq!(&hex[..len], b"FEED");
        assert!(unsigned_digits(7, 1, false).is_none());
    }
}
