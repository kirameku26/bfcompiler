use crate::backend::MachineCode;

fn u16le(out: &mut Vec<u8>, n: u16) {
    out.extend(n.to_le_bytes());
}
fn u32le(out: &mut Vec<u8>, n: u32) {
    out.extend(n.to_le_bytes());
}
fn align(out: &mut Vec<u8>, n: usize) {
    out.resize((out.len() + n - 1) & !(n - 1), 0);
}
#[allow(clippy::too_many_arguments)]
fn section(
    out: &mut Vec<u8>,
    name: u32,
    kind: u32,
    flags: u32,
    offset: u32,
    size: u32,
    link: u32,
    info: u32,
    alignment: u32,
    entsize: u32,
) {
    for n in [
        name, kind, flags, 0, offset, size, link, info, alignment, entsize,
    ] {
        u32le(out, n);
    }
}
fn sym(out: &mut Vec<u8>, name: u32, value: u32, size: u32, info: u8, section: u16) {
    u32le(out, name);
    u32le(out, value);
    u32le(out, size);
    out.push(info);
    out.push(0);
    u16le(out, section);
}
fn offset(n: usize) -> Result<u32, String> {
    u32::try_from(n).map_err(|_| "ELF32 object too large".into())
}
pub fn write_object(code: &MachineCode) -> Result<Vec<u8>, String> {
    let section_bytes = code.section_bytes();
    let mut out = vec![0; 52];
    let text = offset(out.len())?;
    out.extend(&section_bytes);
    align(&mut out, 4);
    let rel = offset(out.len())?;
    for r in &code.relocations {
        if r.offset as usize + 4 > code.bytes.len() {
            return Err("invalid relocation offset".into());
        }
        let symbol = match r.symbol {
            "_bf_input" => 2,
            "_bf_output" => 3,
            _ => return Err(format!("unknown symbol {}", r.symbol)),
        };
        u32le(&mut out, r.offset);
        u32le(&mut out, (symbol << 8) | 2);
    }
    let symtab = offset(out.len())?;
    sym(&mut out, 0, 0, 0, 0, 0);
    sym(&mut out, 1, 0, offset(code.bytes.len())?, 0x12, 1);
    sym(&mut out, 12, 0, 0, 0x10, 0);
    sym(&mut out, 21, 0, 0, 0x10, 0);
    if code.config.is_some() {
        sym(&mut out, 31, offset(code.config_offset())?, 16, 0x11, 1);
    }
    let strings = b"\0bf_program\0bf_input\0bf_output\0bf_config\0";
    let strtab = offset(out.len())?;
    out.extend(strings);
    let names = b"\0.text\0.rel.text\0.symtab\0.strtab\0.shstrtab\0.note.GNU-stack\0";
    let shstrtab = offset(out.len())?;
    out.extend(names);
    align(&mut out, 4);
    let shtab = offset(out.len())?;
    out.extend([0; 40]);
    section(
        &mut out,
        1,
        1,
        6,
        text,
        offset(section_bytes.len())?,
        0,
        0,
        1,
        0,
    );
    section(
        &mut out,
        7,
        9,
        0,
        rel,
        offset(
            code.relocations
                .len()
                .checked_mul(8)
                .ok_or("relocations too large")?,
        )?,
        3,
        1,
        4,
        8,
    );
    section(
        &mut out,
        17,
        2,
        0,
        symtab,
        if code.config.is_some() { 80 } else { 64 },
        4,
        1,
        4,
        16,
    );
    section(&mut out, 25, 3, 0, strtab, strings.len() as u32, 0, 0, 1, 0);
    section(&mut out, 33, 3, 0, shstrtab, names.len() as u32, 0, 0, 1, 0);
    section(&mut out, 43, 1, 0, shtab, 0, 0, 0, 1, 0);
    let mut h = Vec::new();
    h.extend(b"\x7fELF\x01\x01\x01\0\0\0\0\0\0\0\0\0");
    u16le(&mut h, 1);
    u16le(&mut h, 3);
    u32le(&mut h, 1);
    u32le(&mut h, 0);
    u32le(&mut h, 0);
    u32le(&mut h, shtab);
    u32le(&mut h, 0);
    u16le(&mut h, 52);
    u16le(&mut h, 0);
    u16le(&mut h, 0);
    u16le(&mut h, 40);
    u16le(&mut h, 7);
    u16le(&mut h, 5);
    out[..52].copy_from_slice(&h);
    Ok(out)
}
