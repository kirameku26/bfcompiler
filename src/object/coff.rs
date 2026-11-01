use crate::backend::MachineCode;
fn u16le(out: &mut Vec<u8>, v: u16) {
    out.extend(v.to_le_bytes());
}
fn u32le(out: &mut Vec<u8>, v: u32) {
    out.extend(v.to_le_bytes());
}

pub fn write_object(code: &MachineCode) -> Result<Vec<u8>, String> {
    let section_bytes = code.section_bytes();
    let count = u16::try_from(code.relocations.len())
        .map_err(|_| "too many COFF relocations".to_string())?;
    let text_offset = 60usize;
    let reloc_offset = text_offset
        .checked_add(section_bytes.len())
        .ok_or("object too large")?;
    let symbol_offset = reloc_offset
        .checked_add(code.relocations.len() * 10)
        .ok_or("object too large")?;
    let symbol_offset = u32::try_from(symbol_offset).map_err(|_| "object too large")?;
    let mut out = Vec::new();
    u16le(&mut out, 0x8664);
    u16le(&mut out, 1);
    u32le(&mut out, 0);
    u32le(&mut out, symbol_offset);
    u32le(&mut out, if code.config.is_some() { 4 } else { 3 });
    u16le(&mut out, 0);
    u16le(&mut out, 0);
    out.extend(b".text\0\0\0");
    u32le(&mut out, 0);
    u32le(&mut out, 0);
    u32le(
        &mut out,
        u32::try_from(section_bytes.len()).map_err(|_| "object too large")?,
    );
    u32le(&mut out, text_offset as u32);
    u32le(
        &mut out,
        u32::try_from(reloc_offset).map_err(|_| "object too large")?,
    );
    u32le(&mut out, 0);
    u16le(&mut out, count);
    u16le(&mut out, 0);
    u32le(&mut out, 0x6050_0020);
    out.extend(&section_bytes);
    for r in &code.relocations {
        if r.offset as usize + 4 > code.bytes.len() {
            return Err("invalid relocation offset".into());
        }
        let symbol = match r.symbol {
            "_bf_input" => 1,
            "_bf_output" => 2,
            _ => return Err(format!("unknown symbol {}", r.symbol)),
        };
        u32le(&mut out, r.offset);
        u32le(&mut out, symbol);
        u16le(&mut out, 4); // IMAGE_REL_AMD64_REL32
    }
    for (offset, section) in [(4, 1), (15, 0), (24, 0)] {
        u32le(&mut out, 0);
        u32le(&mut out, offset);
        u32le(&mut out, 0);
        u16le(&mut out, section);
        u16le(&mut out, 0x20);
        out.push(2);
        out.push(0);
    }
    let strings = b"bf_program\0bf_input\0bf_output\0bf_config\0";
    if code.config.is_some() {
        u32le(&mut out, 0);
        u32le(&mut out, 34);
        u32le(
            &mut out,
            u32::try_from(code.config_offset()).map_err(|_| "object too large")?,
        );
        u16le(&mut out, 1);
        u16le(&mut out, 0);
        out.extend([2, 0]);
    }
    u32le(&mut out, (4 + strings.len()) as u32);
    out.extend(strings);
    Ok(out)
}
