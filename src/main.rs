use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

const MAGIC_T2B: u32 = 0x6232_7401;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ValueType {
    String = 0,
    Integer = 1,
    FloatingPoint = 2,
}

#[derive(Debug, Clone, Copy)]
enum ValueLength {
    Int = 4,
    Long = 8,
}

#[derive(Debug, Clone, Copy)]
enum StringEncoding {
    Sjis,
    Utf8,
}

#[derive(Debug, Clone)]
enum ValueData {
    Str(Option<String>),
    Int(i64),
    Float(f64),
}

#[derive(Debug, Clone)]
struct ValueField {
    typ: ValueType,
    data: ValueData,
    offset: usize,
}

#[derive(Debug, Clone)]
struct Entry {
    name: String,
    values: Vec<ValueField>,
}

#[derive(Debug, Clone)]
struct ParsedT2b {
    bytes: Vec<u8>,
    value_length: ValueLength,
    entries: Vec<Entry>,
}

fn main() {
    let mut raw_args = std::env::args();
    let bin_name = raw_args
        .next()
        .and_then(|p| std::path::Path::new(&p).file_name().map(|s| s.to_string_lossy().to_string()))
        .unwrap_or_else(|| "cpk_size_sync".into());
    let args = raw_args.collect::<Vec<_>>();

    if args.iter().any(|a| a == "-v" || a == "--version") {
        print_version(&bin_name);
        std::process::exit(0);
    }

    if args.is_empty() || args.iter().any(|a| a == "-h" || a == "--help") {
        print_usage(&bin_name);
        std::process::exit(0);
    }

    if args.len() != 3 {
        eprintln!("Error: requires exactly 3 arguments.");
        print_usage(&bin_name);
        std::process::exit(1);
    }

    let path_a = PathBuf::from(&args[0]);
    let path_b = PathBuf::from(&args[1]);
    let path_c = PathBuf::from(&args[2]);

    if !path_a.exists() {
        eprintln!("Original file not found: {}", path_a.display());
        std::process::exit(1);
    }
    if !path_b.exists() {
        eprintln!("Modified file not found: {}", path_b.display());
        std::process::exit(1);
    }

    match run(&path_a, &path_b, &path_c) {
        Ok(updated) => {
            println!(
                "Updated {} entries. Output: {}",
                updated,
                path_c.display()
            );
        }
        Err(err) => {
            eprintln!("Failed: {err}");
            std::process::exit(1);
        }
    }
}

fn print_usage(bin_name: &str) {
    eprintln!("Synchronize file size entries in LEVEL5 cpk_list.cfg.bin tables.");
    eprintln!();
    eprintln!("Usage:");
    eprintln!("  {bin_name} <original.bin> <patched.bin> <output.bin>");
    eprintln!();
    eprintln!("Arguments:");
    eprintln!("  original.bin   Source table whose size fields will be updated");
    eprintln!("  patched.bin    Patched table that already contains correct sizes");
    eprintln!("  output.bin     Required output path for the synchronized table");
    eprintln!();
    eprintln!("Examples:");
    eprintln!("  {bin_name} original.bin patched.bin synced.bin");
    eprintln!();
    eprintln!("Environment:");
    eprintln!("  CPK_DEBUG=1    Print debug info about parsed entries");
}

fn print_version(bin_name: &str) {
    eprintln!("{bin_name} {}", env!("CARGO_PKG_VERSION"));
}

fn run(path_a: &PathBuf, path_b: &PathBuf, path_c: &PathBuf) -> Result<u32, String> {
    let debug = std::env::var("CPK_DEBUG").is_ok();

    let parsed_a = parse_t2b(path_a).map_err(|e| format!("parse original: {e}"))?;
    let parsed_b = parse_t2b(path_b).map_err(|e| format!("parse modified: {e}"))?;

    const B_PRIMARY_SIZE_INDEX: usize = 2; // B의 3번째 줄 (패치된 항목만)
    const A_PRIMARY_SIZE_INDEX: usize = 4; // A에서 기본 5번째 줄

    // Build size map from B (size: require numeric at index 2, and only when suffix is empty).
    let mut size_map: HashMap<String, (i64, ValueLength)> = HashMap::new();
    for entry in &parsed_b.entries {
        if entry.name != "CPK_ITEM" {
            continue;
        }

        let key = path_key(entry);
        if key.is_none() {
            continue;
        }
        let (prefix, suffix) = key.unwrap();

        // Only consider patched entries (suffix empty or quotes treated as empty).
        if !suffix.trim_matches('"').is_empty() {
            continue;
        }

        let full_path = prefix + &suffix;

        let size_field = entry
            .values
            .get(B_PRIMARY_SIZE_INDEX)
            .ok_or_else(|| format!("B missing size field (index {}) for {}", B_PRIMARY_SIZE_INDEX, full_path))?;

        let size_val = match &size_field.data {
            ValueData::Int(n) => Some(*n),
            ValueData::Str(Some(s)) => s.trim_matches('"').parse::<i64>().ok(),
            _ => None,
        };

        if let Some(n) = size_val {
            size_map.insert(full_path, (n, parsed_b.value_length));
        }
    }

    if debug {
        eprintln!(
            "B entries: total={}, CPK_ITEM={}",
            parsed_b.entries.len(),
            parsed_b
                .entries
                .iter()
                .filter(|e| e.name.starts_with("CPK_ITEM"))
                .count()
        );
        for (i, entry) in parsed_b.entries.iter().take(3).enumerate() {
            eprintln!(
                "B entry[{i}] name={} values={} types={:?} vals={:?}",
                entry.name,
                entry.values.len(),
                entry
                    .values
                    .iter()
                    .map(|v| v.typ as u8)
                    .collect::<Vec<_>>(),
                entry
                    .values
                    .iter()
                    .map(|v| match &v.data {
                        ValueData::Str(s) => s.clone().unwrap_or_default(),
                        ValueData::Int(n) => n.to_string(),
                        ValueData::Float(f) => f.to_string(),
                    })
                    .collect::<Vec<_>>()
            );
        }
    }

    if size_map.is_empty() {
        return Err("No patched CPK_ITEM entries found in B (needs empty second field and numeric third field)".into());
    }

    // Work on mutable copy of A bytes.
    let mut out_bytes = parsed_a.bytes.clone();
    let mut updated = 0u32;

    for entry in &parsed_a.entries {
        if entry.name != "CPK_ITEM" {
            continue;
        }
        let key = path_key(entry);
        if key.is_none() {
            continue;
        }
        let (prefix, suffix) = key.unwrap();
        let full_key = prefix + &suffix;

        let Some((size_val, _)) = size_map.get(&full_key) else {
            continue;
        };

        let target_field = entry
            .values
            .get(A_PRIMARY_SIZE_INDEX)
            .or_else(|| entry.values.last());
        let Some(target_field) = target_field else { continue };
        if target_field.typ != ValueType::Integer {
            continue;
        }

        // Write using A's value length to avoid corruption.
        let len_bytes = parsed_a.value_length as usize;
        let offset = target_field.offset;
        if offset + len_bytes > out_bytes.len() {
            continue;
        }

        match parsed_a.value_length {
            ValueLength::Int => {
                let v = *size_val as i32;
                out_bytes[offset..offset + 4].copy_from_slice(&v.to_le_bytes());
            }
            ValueLength::Long => {
                let v = *size_val as i64;
                out_bytes[offset..offset + 8].copy_from_slice(&v.to_le_bytes());
            }
        }

        updated += 1;
    }

    fs::write(path_c, &out_bytes).map_err(|e| format!("write output: {e}"))?;

    Ok(updated)
}

fn path_key(entry: &Entry) -> Option<(String, String)> {
    if entry.values.len() < 2 {
        return None;
    }
    let prefix = match &entry.values[0].data {
        ValueData::Str(Some(s)) => s.clone(),
        _ => return None,
    };
    let suffix = match &entry.values[1].data {
        ValueData::Str(Some(s)) => s.clone(),
        ValueData::Str(None) => String::new(),
        _ => String::new(),
    };
    Some((prefix, suffix))
}

fn parse_t2b(path: &PathBuf) -> Result<ParsedT2b, String> {
    let bytes = fs::read(path).map_err(|e| format!("read file: {e}"))?;
    if bytes.len() < 0x30 {
        return Err("file too small".into());
    }

    let footer_pos = bytes.len() - 0x10;
    let magic = read_u32(&bytes, footer_pos).ok_or("footer read failed")?;
    if magic != MAGIC_T2B {
        return Err("invalid magic".into());
    }
    let encoding_raw = read_i16(&bytes, footer_pos + 6).ok_or("footer encoding")?;
    let encoding = match encoding_raw {
        0 => StringEncoding::Sjis,
        1 | 256 | 257 => StringEncoding::Utf8,
        _ => return Err(format!("unknown encoding {encoding_raw}")),
    };

    // Entry header
    let entry_count = read_u32(&bytes, 0).ok_or("entryCount")? as usize;
    let string_data_offset = read_u32(&bytes, 4).ok_or("stringDataOffset")? as usize;
    let string_data_length = read_u32(&bytes, 8).ok_or("stringDataLength")? as usize;

    // Detect value length
    let value_length = detect_value_length(&bytes, entry_count, string_data_offset)
        .ok_or("failed to detect value length")?;

    let (entries_raw, entries_end_pos) =
        parse_entries(&bytes, entry_count, string_data_offset, value_length)
            .ok_or("failed to parse entries")?;

    if string_data_offset + string_data_length > bytes.len() {
        return Err("string data out of range".into());
    }
    let value_string_data = &bytes[string_data_offset..string_data_offset + string_data_length];

    let checksum_pos = align_up(string_data_offset + string_data_length, 0x10);
    if checksum_pos + 0x10 > bytes.len() {
        return Err("checksum header out of range".into());
    }
    let _checksum_size = read_u32(&bytes, checksum_pos).ok_or("checksum size")? as usize;
    let checksum_count = read_u32(&bytes, checksum_pos + 4).ok_or("checksum count")? as usize;
    let checksum_string_offset =
        read_u32(&bytes, checksum_pos + 8).ok_or("checksum string offset")? as usize;
    let checksum_string_size =
        read_u32(&bytes, checksum_pos + 12).ok_or("checksum string size")? as usize;

    let checksum_entries_pos = checksum_pos + 0x10;
    let checksum_strings_pos = checksum_pos + checksum_string_offset;

    if checksum_entries_pos + checksum_count * 8 > bytes.len()
        || checksum_strings_pos + checksum_string_size > bytes.len()
    {
        return Err("checksum section out of range".into());
    }

    let mut checksum_entries = Vec::with_capacity(checksum_count);
    for i in 0..checksum_count {
        let p = checksum_entries_pos + i * 8;
        let crc = read_u32(&bytes, p).ok_or("checksum entry crc")?;
        let str_off = read_u32(&bytes, p + 4).ok_or("checksum entry offset")?;
        checksum_entries.push((crc, str_off));
    }

    let checksum_string_data =
        &bytes[checksum_strings_pos..checksum_strings_pos + checksum_string_size];

    // Map crc -> name offset (relative to first string offset)
    let base_offset = checksum_entries
        .first()
        .map(|e| e.1)
        .ok_or("no checksum entries")?;
    let mut crc_to_name_offset = HashMap::new();
    for (crc, off) in &checksum_entries {
        crc_to_name_offset.insert(*crc, (*off as i64 - base_offset as i64) as usize);
    }

    let mut entries = Vec::with_capacity(entries_raw.len());
    for raw in entries_raw {
        let name_offset = *crc_to_name_offset
            .get(&raw.crc32)
            .ok_or("missing name offset")?;
        let name = read_string(checksum_string_data, name_offset, encoding)
            .ok_or("name read failed")?;

        let mut values = Vec::with_capacity(raw.types.len());
        for (idx, typ) in raw.types.iter().enumerate() {
            let offset = raw.value_offsets[idx];
            let val = match typ {
                ValueType::String => {
                    let val_off = raw.values[idx];
                    if val_off < 0 {
                        ValueData::Str(None)
                    } else {
                        let v = read_string(
                            value_string_data,
                            val_off as usize,
                            encoding,
                        );
                        ValueData::Str(v)
                    }
                }
                ValueType::Integer => {
                    ValueData::Int(raw.values[idx])
                }
                ValueType::FloatingPoint => match value_length {
                    ValueLength::Int => {
                        let bits = raw.values[idx] as u32;
                        ValueData::Float(f32::from_bits(bits) as f64)
                    }
                    ValueLength::Long => {
                        let bits = raw.values[idx] as u64;
                        ValueData::Float(f64::from_bits(bits))
                    }
                },
            };
            values.push(ValueField {
                typ: *typ,
                data: val,
                offset,
            });
        }

        entries.push(Entry { name, values });
    }

    // entries_end_pos check optional
    let _ = entries_end_pos;

    Ok(ParsedT2b {
        bytes,
        value_length,
        entries,
    })
}

#[derive(Debug)]
struct RawEntry {
    crc32: u32,
    types: Vec<ValueType>,
    values: Vec<i64>,
    value_offsets: Vec<usize>,
}

fn detect_value_length(
    bytes: &[u8],
    entry_count: usize,
    string_offset: usize,
) -> Option<ValueLength> {
    if try_parse_entries(bytes, entry_count, string_offset, ValueLength::Int).is_some() {
        return Some(ValueLength::Int);
    }
    if try_parse_entries(bytes, entry_count, string_offset, ValueLength::Long).is_some() {
        return Some(ValueLength::Long);
    }
    None
}

fn parse_entries(
    bytes: &[u8],
    entry_count: usize,
    string_offset: usize,
    value_length: ValueLength,
) -> Option<(Vec<RawEntry>, usize)> {
    try_parse_entries(bytes, entry_count, string_offset, value_length)
}

fn try_parse_entries(
    bytes: &[u8],
    entry_count: usize,
    string_offset: usize,
    value_length: ValueLength,
) -> Option<(Vec<RawEntry>, usize)> {
    let mut pos = 0x10; // after entry header
    let mut entries = Vec::with_capacity(entry_count);

    for _ in 0..entry_count {
        if pos + 5 > bytes.len() || pos + 5 > string_offset {
            return None;
        }
        let crc32 = read_u32(bytes, pos)?;
        pos += 4;
        let value_count = bytes.get(pos)?; // entryCount
        pos += 1;

        let mut types = Vec::with_capacity(*value_count as usize);
        for j in (0..*value_count).step_by(4) {
            if pos >= bytes.len() || pos >= string_offset {
                return None;
            }
            let type_chunk = *bytes.get(pos)?;
            pos += 1;
            for h in 0..4 {
                if j + h >= *value_count {
                    break;
                }
                let t = (type_chunk >> (h * 2)) & 0x3;
                if t == 3 {
                    return None;
                }
                types.push(match t {
                    0 => ValueType::String,
                    1 => ValueType::Integer,
                    2 => ValueType::FloatingPoint,
                    _ => return None,
                });
            }
        }

        pos = align_up(pos, 4);

        let mut values = Vec::with_capacity(types.len());
        let mut value_offsets = Vec::with_capacity(types.len());
        for _ in 0..types.len() {
            if pos + value_length as usize > bytes.len()
                || pos + value_length as usize > string_offset
            {
                return None;
            }
            value_offsets.push(pos);
            let v = match value_length {
                ValueLength::Int => read_i32(bytes, pos)? as i64,
                ValueLength::Long => read_i64(bytes, pos)?,
            };
            values.push(v);
            pos += value_length as usize;
        }

        entries.push(RawEntry {
            crc32,
            types,
            values,
            value_offsets,
        });
    }

    if pos > string_offset || string_offset.saturating_sub(pos) >= 0x10 {
        return None;
    }

    Some((entries, pos))
}

fn read_string(data: &[u8], offset: usize, enc: StringEncoding) -> Option<String> {
    if offset >= data.len() {
        return None;
    }
    let mut end = offset;
    while end < data.len() && data[end] != 0 {
        end += 1;
    }
    let slice = &data[offset..end];
    match enc {
        StringEncoding::Utf8 => std::str::from_utf8(slice).ok().map(|s| s.to_string()),
        // Fallback: treat SJIS bytes as lossless Latin-1-ish to keep ASCII paths readable.
        StringEncoding::Sjis => Some(slice.iter().map(|b| *b as char).collect()),
    }
}

fn align_up(pos: usize, align: usize) -> usize {
    (pos + (align - 1)) & !(align - 1)
}

fn read_u32(data: &[u8], offset: usize) -> Option<u32> {
    if offset + 4 > data.len() {
        None
    } else {
        Some(u32::from_le_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]))
    }
}

fn read_i32(data: &[u8], offset: usize) -> Option<i32> {
    read_u32(data, offset).map(|v| v as i32)
}

fn read_i16(data: &[u8], offset: usize) -> Option<i16> {
    if offset + 2 > data.len() {
        None
    } else {
        Some(i16::from_le_bytes([data[offset], data[offset + 1]]))
    }
}

fn read_i64(data: &[u8], offset: usize) -> Option<i64> {
    if offset + 8 > data.len() {
        None
    } else {
        Some(i64::from_le_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
            data[offset + 4],
            data[offset + 5],
            data[offset + 6],
            data[offset + 7],
        ]))
    }
}
