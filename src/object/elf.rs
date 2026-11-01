use crate::backend::MachineCode;

fn u16le(out: &mut Vec<u8>, v: u16) {
    out.extend(v.to_le_bytes());
}
fn u32le(out: &mut Vec<u8>, v: u32) {
    out.extend(v.to_le_bytes());
}
fn u64le(out: &mut Vec<u8>, v: u64) {
    out.extend(v.to_le_bytes());
}
fn align(out: &mut Vec<u8>, a: usize) {
    out.resize((out.len() + a - 1) & !(a - 1), 0);
}
#[allow(clippy::too_many_arguments)] // ELF section headers have independent fields.
fn section(
    out: &mut Vec<u8>,
    name: u32,
    kind: u32,
    flags: u64,
    offset: u64,
    size: u64,
    link: u32,
    info: u32,
    alignment: u64,
    entsize: u64,
) {
    u32le(out, name);
    u32le(out, kind);
    u64le(out, flags);
    u64le(out, 0);
    u64le(out, offset);
    u64le(out, size);
    u32le(out, link);
    u32le(out, info);
    u64le(out, alignment);
    u64le(out, entsize);
}
fn sym(out: &mut Vec<u8>, name: u32, info: u8, section: u16, value: u64, size: u64) {
    u32le(out, name);
    out.push(info);
    out.push(0);
    u16le(out, section);
    u64le(out, value);
    u64le(out, size);
}

pub fn write_object(code: &MachineCode, arm64: bool) -> Result<Vec<u8>, String> {
    let mut out = vec![0; 64];
    let text = out.len();
    out.extend_from_slice(&code.bytes);
    align(&mut out, 8);
    let rela = out.len();
    for r in &code.relocations {
        if r.offset as usize + 4 > code.bytes.len() {
            return Err("invalid relocation offset".into());
        }
        let sym = match r.symbol {
            "_bf_input" => 2u64,
            "_bf_output" => 3,
            _ => return Err(format!("unknown symbol {}", r.symbol)),
        };
        u64le(&mut out, r.offset.into());
        u64le(&mut out, (sym << 32) | if arm64 { 283 } else { 4 });
        u64le(&mut out, if arm64 { 0 } else { (-4i64) as u64 });
    }
    let symtab = out.len();
    sym(&mut out, 0, 0, 0, 0, 0);
    sym(&mut out, 1, 0x12, 1, 0, code.bytes.len() as u64);
    sym(&mut out, 12, 0x10, 0, 0, 0);
    sym(&mut out, 21, 0x10, 0, 0, 0);
    let strings = b"\0bf_program\0bf_input\0bf_output\0";
    let strtab = out.len();
    out.extend_from_slice(strings);
    let names = b"\0.text\0.rela.text\0.symtab\0.strtab\0.shstrtab\0.note.GNU-stack\0";
    let shstrtab = out.len();
    out.extend_from_slice(names);
    align(&mut out, 8);
    let shtab = out.len();
    out.extend([0; 64]);
    section(
        &mut out,
        1,
        1,
        6,
        text as u64,
        code.bytes.len() as u64,
        0,
        0,
        if arm64 { 4 } else { 1 },
        0,
    );
    section(
        &mut out,
        7,
        4,
        0,
        rela as u64,
        (code.relocations.len() * 24) as u64,
        3,
        1,
        8,
        24,
    );
    section(&mut out, 18, 2, 0, symtab as u64, 96, 4, 1, 8, 24);
    section(
        &mut out,
        26,
        3,
        0,
        strtab as u64,
        strings.len() as u64,
        0,
        0,
        1,
        0,
    );
    section(
        &mut out,
        34,
        3,
        0,
        shstrtab as u64,
        names.len() as u64,
        0,
        0,
        1,
        0,
    );
    section(&mut out, 44, 1, 0, shtab as u64, 0, 0, 0, 1, 0);
    let mut header = Vec::new();
    header.extend(b"\x7fELF\x02\x01\x01\0\0\0\0\0\0\0\0\0");
    u16le(&mut header, 1);
    u16le(&mut header, if arm64 { 183 } else { 62 });
    u32le(&mut header, 1);
    u64le(&mut header, 0);
    u64le(&mut header, 0);
    u64le(&mut header, shtab as u64);
    u32le(&mut header, 0);
    u16le(&mut header, 64);
    u16le(&mut header, 0);
    u16le(&mut header, 0);
    u16le(&mut header, 64);
    u16le(&mut header, 7);
    u16le(&mut header, 5);
    out[..64].copy_from_slice(&header);
    Ok(out)
}
