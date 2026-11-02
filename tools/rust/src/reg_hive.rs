//! Build-time converter from Windows Registry Editor .reg files to native REGF hives.
//! This uses the same REGF writer/parser as the kernel. It intentionally supports
//! a bounded, documented subset of .reg syntax and rejects unsupported constructs
//! instead of silently generating a different registry image.

#[path = "../../../ntoskrnl/src/cm/hive.rs"]
mod hive;
#[path = "../../../ntoskrnl/src/cm/hive_writer.rs"]
mod hive_writer;

use std::{fs, path::Path};

#[derive(Debug, Clone, PartialEq, Eq)]
struct RegValue {
    key: String,
    name: String,
    kind: u32,
    data: Vec<u8>,
}

fn err(line: usize, message: impl AsRef<str>) -> String {
    format!("line {line}: {}", message.as_ref())
}

fn parse_quoted(input: &str, line: usize) -> Result<(String, &str), String> {
    let mut chars = input.char_indices();
    if chars.next().map(|(_, c)| c) != Some('"') {
        return Err(err(line, "expected quoted string"));
    }
    let mut out = String::new();
    while let Some((index, ch)) = chars.next() {
        match ch {
            '"' => return Ok((out, &input[index + 1..])),
            '\\' => {
                let (_, escaped) = chars.next().ok_or_else(|| err(line, "trailing escape"))?;
                match escaped {
                    '\\' => out.push('\\'),
                    '"' => out.push('"'),
                    other => {
                        out.push('\\');
                        out.push(other);
                    }
                }
            }
            other => out.push(other),
        }
    }
    Err(err(line, "unterminated quoted string"))
}

fn split_assignment(input: &str) -> Option<(&str, &str)> {
    let mut quoted = false;
    let mut escaped = false;
    for (index, ch) in input.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if quoted && ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == '"' {
            quoted = !quoted;
        } else if ch == '=' && !quoted {
            return Some((&input[..index], &input[index + 1..]));
        }
    }
    None
}

fn normalize_key(path: &str, line: usize) -> Result<String, String> {
    let mut parts = path.split('\\');
    let root = parts.next().unwrap_or("");
    let hive = parts.next().unwrap_or("");
    if !(root.eq_ignore_ascii_case("HKEY_LOCAL_MACHINE") || root.eq_ignore_ascii_case("HKLM"))
        || !hive.eq_ignore_ascii_case("SYSTEM")
    {
        return Err(err(line, "only HKLM\\SYSTEM keys can be written to a SYSTEM hive"));
    }
    let mut normalized = Vec::new();
    for part in parts {
        if part.is_empty() {
            return Err(err(line, "empty registry path component"));
        }
        normalized.push(if part.eq_ignore_ascii_case("CurrentControlSet") {
            "ControlSet001".to_owned()
        } else {
            part.to_owned()
        });
    }
    Ok(normalized.join("\\"))
}

fn parse_hex_bytes(input: &str, line: usize) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    for token in input.split(',') {
        let token = token.trim();
        if token.is_empty() {
            return Err(err(line, "empty byte in hex value"));
        }
        if token.contains('\\') {
            return Err(err(line, "line-continuation hex values are not supported; keep hex data on one line"));
        }
        let byte = u8::from_str_radix(token, 16)
            .map_err(|_| err(line, format!("invalid hexadecimal byte {token:?}")))?;
        out.push(byte);
    }
    Ok(out)
}

fn parse_data(input: &str, line: usize) -> Result<(u32, Vec<u8>), String> {
    let input = input.trim();
    if input.starts_with('"') {
        let (value, tail) = parse_quoted(input, line)?;
        if !tail.trim().is_empty() {
            return Err(err(line, "unexpected text after quoted string"));
        }
        let mut bytes = Vec::new();
        for unit in value.encode_utf16().chain(std::iter::once(0)) {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        return Ok((1, bytes)); // REG_SZ
    }
    if let Some(value) = input.strip_prefix("dword:") {
        if value.len() != 8 {
            return Err(err(line, "DWORD must contain exactly eight hexadecimal digits"));
        }
        let number = u32::from_str_radix(value, 16)
            .map_err(|_| err(line, "invalid DWORD hexadecimal value"))?;
        return Ok((4, number.to_le_bytes().to_vec())); // REG_DWORD
    }
    if let Some(hex) = input.strip_prefix("hex") {
        let (kind, data) = if let Some(rest) = hex.strip_prefix('(') {
            let close = rest.find("):").ok_or_else(|| err(line, "expected hex(type):"))?;
            let kind = u32::from_str_radix(&rest[..close], 16)
                .map_err(|_| err(line, "invalid registry type in hex(type)"))?;
            (kind, &rest[close + 2..])
        } else if let Some(data) = hex.strip_prefix(':') {
            (3, data) // REG_BINARY
        } else {
            return Err(err(line, "unsupported hex value syntax"));
        };
        return Ok((kind, if data.trim().is_empty() { Vec::new() } else { parse_hex_bytes(data, line)? }));
    }
    Err(err(line, "supported data forms are quoted strings, dword: and hex/hex(type):"))
}

fn parse_reg(source: &str) -> Result<Vec<RegValue>, String> {
    let mut saw_header = false;
    let mut current_key: Option<String> = None;
    let mut values = Vec::new();
    for (zero_line, raw) in source.lines().enumerate() {
        let line_no = zero_line + 1;
        let mut line = raw.trim().trim_start_matches('\u{feff}').trim();
        if line.is_empty() || line.starts_with(';') {
            continue;
        }
        if !saw_header {
            if line != "Windows Registry Editor Version 5.00" {
                return Err(err(line_no, "expected Windows Registry Editor Version 5.00 header"));
            }
            saw_header = true;
            continue;
        }
        if line.starts_with('[') {
            if !line.ends_with(']') || line.starts_with("[-") {
                return Err(err(line_no, "malformed/deletion registry key sections are unsupported"));
            }
            line = &line[1..line.len() - 1];
            current_key = Some(normalize_key(line, line_no)?);
            continue;
        }
        let key = current_key.clone().ok_or_else(|| err(line_no, "value appears before a key section"))?;
        let (raw_name, raw_data) = split_assignment(line)
            .ok_or_else(|| err(line_no, "expected registry value assignment"))?;
        let name = if raw_name.trim() == "@" {
            String::new()
        } else {
            let (name, tail) = parse_quoted(raw_name.trim(), line_no)?;
            if !tail.trim().is_empty() {
                return Err(err(line_no, "unexpected text after value name"));
            }
            name
        };
        let (kind, data) = parse_data(raw_data, line_no)?;
        values.push(RegValue { key, name, kind, data });
    }
    if !saw_header {
        return Err("missing Registry Editor Version 5.00 header".to_owned());
    }
    if values.is_empty() {
        return Err("no registry values found".to_owned());
    }
    Ok(values)
}

fn path_components(path: &str) -> Vec<&str> {
    if path.is_empty() { Vec::new() } else { path.split('\\').collect() }
}

fn ensure_key(bytes: &mut [u8], path: &str) -> Result<u32, String> {
    let components = path_components(path);
    let mut prefix = Vec::<String>::new();
    for component in components {
        let current = {
            let length = 4096usize + u32::from_le_bytes(bytes[0x28..0x2c].try_into().unwrap()) as usize;
            let view = hive::Hive::open(&bytes[..length]).map_err(|e| format!("REGF validation failed: {e:?}"))?;
            let mut search = prefix.clone();
            search.push(component.to_owned());
            let utf16: Vec<u16> = search.join("\\").encode_utf16().collect();
            view.find_path(&utf16).ok().map(|key| key.cell_index())
        };
        if current.is_none() {
            let parent = {
                let length = 4096usize + u32::from_le_bytes(bytes[0x28..0x2c].try_into().unwrap()) as usize;
                let view = hive::Hive::open(&bytes[..length]).map_err(|e| format!("REGF validation failed: {e:?}"))?;
                let utf16: Vec<u16> = prefix.join("\\").encode_utf16().collect();
                if prefix.is_empty() {
                    view.info.root_cell
                } else {
                    view.find_path(&utf16).map_err(|e| format!("parent key {:?} not found: {e:?}", prefix.join("\\")))?.cell_index()
                }
            };
            let name: Vec<u16> = component.encode_utf16().collect();
            let mut writer = hive_writer::HiveWriter::open(bytes).map_err(|e| format!("REGF writer open failed: {e:?}"))?;
            writer.create_subkey(parent, &name).map_err(|e| format!("cannot create key {}: {e:?}", path))?;
            let sequence = u32::from_le_bytes(writer.bytes()[4..8].try_into().unwrap()).wrapping_add(1).max(1);
            writer.update_base_block(sequence).map_err(|e| format!("cannot finalize hive header: {e:?}"))?;
        }
        prefix.push(component.to_owned());
    }
    let length = 4096usize + u32::from_le_bytes(bytes[0x28..0x2c].try_into().unwrap()) as usize;
    let view = hive::Hive::open(&bytes[..length]).map_err(|e| format!("final REGF validation failed: {e:?}"))?;
    let utf16: Vec<u16> = path.encode_utf16().collect();
    view.find_path(&utf16).map(|key| key.cell_index()).map_err(|e| format!("cannot resolve key {path}: {e:?}"))
}

fn set_value(bytes: &mut [u8], key: u32, name: &str, kind: u32, data: &[u8]) -> Result<(), String> {
    let name: Vec<u16> = name.encode_utf16().collect();
    let mut writer = hive_writer::HiveWriter::open(bytes).map_err(|e| format!("REGF writer open failed: {e:?}"))?;
    writer.set_value(key, &name, kind, data).map_err(|e| format!("cannot set registry value: {e:?}"))?;
    let sequence = u32::from_le_bytes(writer.bytes()[4..8].try_into().unwrap()).wrapping_add(1).max(1);
    writer.update_base_block(sequence).map_err(|e| format!("cannot finalize hive header: {e:?}"))?;
    Ok(())
}

pub fn convert_file(input: &Path, output: &Path) -> Result<(), String> {
    convert_files(&[input.to_path_buf()], output)
}

pub fn convert_files(inputs: &[std::path::PathBuf], output: &Path) -> Result<(), String> {
    if inputs.is_empty() { return Err("at least one .reg input is required".to_owned()); }
    let mut values = Vec::new();
    for input in inputs {
        let source = fs::read_to_string(input).map_err(|e| format!("{}: {e}", input.display()))?;
        values.extend(parse_reg(&source)?);
    }
    // Capacity is a limit, not the final file size. The REGF writer grows HBINs
    // within this buffer and the result is truncated to the validated hive size.
    const CAPACITY: usize = 16 * 1024 * 1024;
    let mut bytes = vec![0u8; CAPACITY];
    hive::create_minimal_hive(&mut bytes).map_err(|e| format!("cannot create base REGF hive: {e:?}"))?;
    for value in &values {
        let key = ensure_key(&mut bytes, &value.key)?;
        set_value(&mut bytes, key, &value.name, value.kind, &value.data)?;
    }
    let length = 4096usize + u32::from_le_bytes(bytes[0x28..0x2c].try_into().unwrap()) as usize;
    let view = hive::Hive::open(&bytes[..length]).map_err(|e| format!("generated hive is invalid: {e:?}"))?;
    let select: Vec<u16> = "Select".encode_utf16().collect();
    let current: Vec<u16> = "Current".encode_utf16().collect();
    let select_key = view.find_path(&select).map_err(|_| "SYSTEM hive must define Select\\Current".to_owned())?;
    let current_value = select_key.find_value(&current).map_err(|_| "SYSTEM hive must define Select\\Current".to_owned())?;
    let mut raw = [0u8; 4];
    if current_value.kind != 4 || current_value.data(&mut raw).map_err(|e| format!("bad Select\\Current: {e:?}"))? != 4 {
        return Err("Select\\Current must be a REG_DWORD".to_owned());
    }
    let parent = output.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    fs::write(output, &bytes[..length]).map_err(|e| format!("{}: {e}", output.display()))?;
    println!("Converted {} registry values from {} .reg source(s) into validated REGF hive ({} bytes): {}",
        values.len(), inputs.len(), length, output.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_driver_services_and_normalizes_current_control_set() {
        let source = r#"Windows Registry Editor Version 5.00

[HKEY_LOCAL_MACHINE\SYSTEM\Select]
"Current"=dword:00000001

[HKEY_LOCAL_MACHINE\SYSTEM\CurrentControlSet\Services\USBXHCI]
"Type"=dword:00000001
"Start"=dword:00000000
"ErrorControl"=dword:00000001
"ImagePath"="\\SystemRoot\\System32\\drivers\\usbxhci.sys"
"#;
        let values = parse_reg(source).unwrap();
        assert_eq!(values[1].key, "ControlSet001\\Services\\USBXHCI");
        let dir = std::env::temp_dir().join(format!("cognacos-reg-hive-{}", std::process::id()));
        let input = dir.with_extension("reg");
        let output = dir.with_extension("hive");
        fs::write(&input, source).unwrap();
        convert_file(&input, &output).unwrap();
        let bytes = fs::read(&output).unwrap();
        let view = hive::Hive::open(&bytes).unwrap();
        let path: Vec<u16> = "ControlSet001\\Services\\USBXHCI".encode_utf16().collect();
        let key = view.find_path(&path).unwrap();
        let start: Vec<u16> = "Start".encode_utf16().collect();
        let value = key.find_value(&start).unwrap();
        let mut raw = [0u8; 4];
        assert_eq!(value.kind, 4);
        value.data(&mut raw).unwrap();
        assert_eq!(u32::from_le_bytes(raw), 0);
        let _ = fs::remove_file(input);
        let _ = fs::remove_file(output);
    }

    #[test]
    fn rejects_non_system_hives_and_unknown_syntax() {
        let source = "Windows Registry Editor Version 5.00\n[HKEY_LOCAL_MACHINE\\SOFTWARE\\CognacOS]\n\"X\"=dword:00000001\n";
        assert!(parse_reg(source).is_err());
        let source = "Windows Registry Editor Version 5.00\n[HKEY_LOCAL_MACHINE\\SYSTEM\\X]\n\"X\"=-\n";
        assert!(parse_reg(source).is_err());
    }
}
