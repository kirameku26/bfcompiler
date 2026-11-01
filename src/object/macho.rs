use crate::backend::MachineCode;
use std::collections::BTreeMap;

fn u32le(out: &mut Vec<u8>, v: u32) {
    out.extend(v.to_le_bytes());
}
fn u64le(out: &mut Vec<u8>, v: u64) {
    out.extend(v.to_le_bytes());
}
fn name16(out: &mut Vec<u8>, name: &[u8]) {
    let mut buf = [0u8; 16];
    buf[..name.len()].copy_from_slice(name);
    out.extend(buf);
}
fn align4(v: usize) -> usize {
    (v + 3) & !3
}

pub fn write_object(machine: &MachineCode) -> Result<Vec<u8>, String> {
    write_object_for(machine, false)
}

pub fn write_object_for(machine: &MachineCode, x86: bool) -> Result<Vec<u8>, String> {
    let section_bytes = machine.section_bytes();
    let extra = u32::from(machine.config.is_some());
    let mut symbols = BTreeMap::new();
    symbols.insert("_bf_input", 2u32 + extra);
    symbols.insert("_bf_output", 3u32 + extra);
    for r in &machine.relocations {
        if !symbols.contains_key(r.symbol) {
            return Err(format!("unknown external symbol {}", r.symbol));
        }
        if r.offset as usize + 4 > machine.bytes.len() || (!x86 && r.offset % 4 != 0) {
            return Err("invalid relocation offset".into());
        }
    }

    const HEADER_AND_COMMANDS: usize = 312;
    let code_offset = HEADER_AND_COMMANDS;
    let reloc_offset = align4(code_offset + section_bytes.len());
    let reloc_size = machine.relocations.len() * 8;
    let sym_offset = reloc_offset + reloc_size;
    let symbol_count = 4usize + extra as usize;
    let string_offset = sym_offset + symbol_count * 16;
    let strings = b"\0ltmp0\0_bf_program\0_bf_input\0_bf_output\0_bf_config\0";

    let mut out = Vec::with_capacity(string_offset + strings.len());
    // mach_header_64
    u32le(&mut out, 0xfeed_facf);
    u32le(&mut out, if x86 { 0x0100_0007 } else { 0x0100_000c });
    u32le(&mut out, if x86 { 3 } else { 0 });
    u32le(&mut out, 1);
    u32le(&mut out, 4);
    u32le(&mut out, 280);
    u32le(&mut out, 0);
    u32le(&mut out, 0);
    // LC_SEGMENT_64 + one section
    u32le(&mut out, 0x19);
    u32le(&mut out, 152);
    name16(&mut out, b"");
    u64le(&mut out, 0);
    u64le(&mut out, section_bytes.len() as u64);
    u64le(&mut out, code_offset as u64);
    u64le(&mut out, section_bytes.len() as u64);
    u32le(&mut out, 7);
    u32le(&mut out, 7);
    u32le(&mut out, 1);
    u32le(&mut out, 0);
    name16(&mut out, b"__text");
    name16(&mut out, b"__TEXT");
    u64le(&mut out, 0);
    u64le(&mut out, section_bytes.len() as u64);
    u32le(&mut out, code_offset as u32);
    u32le(&mut out, 2);
    u32le(&mut out, reloc_offset as u32);
    u32le(&mut out, machine.relocations.len() as u32);
    u32le(&mut out, 0x8000_0400);
    u32le(&mut out, 0);
    u32le(&mut out, 0);
    u32le(&mut out, 0);
    // LC_BUILD_VERSION (macOS 11.0; valid baseline for arm64)
    u32le(&mut out, 0x32);
    u32le(&mut out, 24);
    u32le(&mut out, 1);
    u32le(&mut out, 11 << 16);
    u32le(&mut out, 0);
    u32le(&mut out, 0);
    // LC_SYMTAB
    u32le(&mut out, 0x2);
    u32le(&mut out, 24);
    u32le(&mut out, sym_offset as u32);
    u32le(&mut out, symbol_count as u32);
    u32le(&mut out, string_offset as u32);
    u32le(&mut out, strings.len() as u32);
    // LC_DYSYMTAB
    u32le(&mut out, 0xb);
    u32le(&mut out, 80);
    for v in [0, 1, 1, 1 + extra, 2 + extra, 2] {
        u32le(&mut out, v);
    }
    for _ in 0..12 {
        u32le(&mut out, 0);
    }
    debug_assert_eq!(out.len(), HEADER_AND_COMMANDS);
    out.extend(&section_bytes);
    out.resize(reloc_offset, 0);
    // ld expects section relocations in descending address order.
    let mut relocs = machine.relocations.clone();
    relocs.sort_by_key(|r| std::cmp::Reverse(r.offset));
    for r in relocs {
        u32le(&mut out, r.offset);
        let word = symbols[r.symbol] | (1 << 24) | (2 << 25) | (1 << 27) | (2 << 28);
        u32le(&mut out, word);
    }
    // nlist_64: local section symbol, external definition, two undefined symbols.
    let mut entries = vec![(1, 0x0e, 1, 0u64), (7, 0x0f, 1, 0)];
    if machine.config.is_some() {
        entries.push((40, 0x0f, 1, machine.config_offset() as u64));
    }
    entries.extend([(19, 0x01, 0, 0), (29, 0x01, 0, 0)]);
    for (strx, kind, sect, value) in entries {
        u32le(&mut out, strx);
        out.push(kind);
        out.push(sect);
        out.extend(0u16.to_le_bytes());
        u64le(&mut out, value);
    }
    out.extend(strings);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{backend::aarch64, frontend};

    #[test]
    fn writes_arm64_macho_header() {
        let code = aarch64::compile(&frontend::parse(b"+").unwrap()).unwrap();
        let obj = write_object(&code).unwrap();
        assert_eq!(&obj[..4], &0xfeed_facfu32.to_le_bytes());
        assert_eq!(&obj[4..8], &0x0100_000cu32.to_le_bytes());
    }
}
