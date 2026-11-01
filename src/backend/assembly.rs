use super::MachineCode;
use std::fmt::Write;

/// Render configured programs without changing encoded instruction sizes.
pub fn render_configured(code: &MachineCode, target: crate::Target) -> String {
    use crate::Target;
    let darwin = matches!(target, Target::Aarch64Darwin | Target::X86_64Darwin);
    let arm = matches!(target, Target::Aarch64Darwin | Target::Aarch64Linux);
    let riscv = target == Target::Riscv64Linux;
    let prefix = if darwin { "_" } else { "" };
    let mut out = String::new();
    if riscv {
        out.push_str(".option norvc\n.option norelax\n");
    }
    writeln!(
        out,
        ".text\n.p2align 2\n.globl {prefix}bf_program\n{prefix}bf_program:"
    )
    .unwrap();
    let mut offset = 0;
    while offset < code.bytes.len() {
        let call_offset = if arm || riscv { offset } else { offset + 1 };
        if let Some(r) = code
            .relocations
            .iter()
            .find(|r| r.offset as usize == call_offset)
        {
            writeln!(
                out,
                "    {} {prefix}{}",
                if arm { "bl" } else { "call" },
                r.symbol.trim_start_matches('_')
            )
            .unwrap();
            offset += if arm {
                4
            } else if riscv {
                8
            } else {
                5
            };
        } else if arm || riscv {
            let word = u32::from_le_bytes(code.bytes[offset..offset + 4].try_into().unwrap());
            writeln!(out, "    .long 0x{word:08x}").unwrap();
            offset += 4;
        } else {
            writeln!(out, "    .byte 0x{:02x}", code.bytes[offset]).unwrap();
            offset += 1;
        }
    }
    if let Some([version, tape_size, cell_bits, encoding]) = code.config_words() {
        writeln!(
            out,
            ".p2align 2, 0\n.globl {prefix}bf_config\n{prefix}bf_config:\n    .long {version}, {tape_size}, {cell_bits}, {encoding}"
        )
        .unwrap();
    }
    if !darwin && target != Target::X86_64WindowsGnu {
        writeln!(
            out,
            ".section .note.GNU-stack,\"\",{}progbits",
            if arm { "%" } else { "@" }
        )
        .unwrap();
    }
    out
}

/// Render the actual encoded instructions, preserving external relocations.
pub fn render(code: &MachineCode) -> String {
    let mut out = String::from(".text\n.p2align 2\n.globl _bf_program\n_bf_program:\n");
    for (index, bytes) in code.bytes.as_chunks::<4>().0.iter().enumerate() {
        let offset = index * 4;
        let word = u32::from_le_bytes(*bytes);
        writeln!(out, ".Lbf_{offset}:").unwrap();
        let instruction = if let Some(reloc) = code
            .relocations
            .iter()
            .find(|r| r.offset as usize == offset)
        {
            format!("bl {}", reloc.symbol)
        } else if word & 0xfc00_0000 == 0x1400_0000 {
            let delta = ((word & 0x03ff_ffff) as i32) << 6 >> 4;
            format!("b .Lbf_{}", offset as i64 + i64::from(delta))
        } else if word & 0xff00_0010 == 0x5400_0000 {
            let delta = (((word >> 5) & 0x7ffff) as i32) << 13 >> 11;
            let cond = match word & 15 {
                0 => "eq",
                1 => "ne",
                2 => "hs",
                3 => "lo",
                8 => "hi",
                _ => unreachable!("unknown generated condition"),
            };
            format!("b.{cond} .Lbf_{}", offset as i64 + i64::from(delta))
        } else if word & 0xff00_001f == 0x3400_0008 || word & 0xff00_001f == 0x3400_000a {
            let delta = (((word >> 5) & 0x7ffff) as i32) << 13 >> 11;
            format!(
                "cbz w{}, .Lbf_{}",
                word & 31,
                offset as i64 + i64::from(delta)
            )
        } else if word & !0x003f_fc00 == 0x1100_0108 {
            format!("add w8, w8, #{}", (word >> 10) & 0xfff)
        } else if word & !0x003f_fc00 == 0xf100_011f {
            format!("cmp x8, #{}", (word >> 10) & 0xfff)
        } else if word & !0x003f_fc00 == 0x9100_0273 {
            format!("add x19, x19, #{}", (word >> 10) & 0xfff)
        } else if word & !0x003f_fc00 == 0xd100_0273 {
            format!("sub x19, x19, #{}", (word >> 10) & 0xfff)
        } else {
            match word {
                0xa9bd_7bfd => "stp x29, x30, [sp, #-48]!",
                0xa901_53f3 => "stp x19, x20, [sp, #16]",
                0xf900_13f5 => "str x21, [sp, #32]",
                0x9100_03fd => "mov x29, sp",
                0xaa00_03f3 => "mov x19, x0",
                0xaa00_03f4 => "mov x20, x0",
                0x8b01_0015 => "add x21, x0, x1",
                0x9100_0673 => "add x19, x19, #1",
                0xd100_0673 => "sub x19, x19, #1",
                0xeb15_027f => "cmp x19, x21",
                0xeb14_027f => "cmp x19, x20",
                0xcb13_02a8 => "sub x8, x21, x19",
                0xcb14_0268 => "sub x8, x19, x20",
                0x3940_0268 => "ldrb w8, [x19]",
                0x3940_026a => "ldrb w10, [x19]",
                0x0b0a_0108 => "add w8, w8, w10",
                0x4b0a_0108 => "sub w8, w8, w10",
                0x5100_0548 => "sub w8, w10, #1",
                0x3900_0268 => "strb w8, [x19]",
                0x3900_027f => "strb wzr, [x19]",
                0x3940_0260 => "ldrb w0, [x19]",
                0x3900_0260 => "strb w0, [x19]",
                0x7100_001f => "cmp w0, #0",
                0x3100_081f => "cmn w0, #2",
                0x3100_041f => "cmn w0, #1",
                0x5280_0000 => "mov w0, #0",
                0x5280_0040 => "mov w0, #2",
                0xf940_13f5 => "ldr x21, [sp, #32]",
                0xa941_53f3 => "ldp x19, x20, [sp, #16]",
                0xa8c3_7bfd => "ldp x29, x30, [sp], #48",
                0xd65f_03c0 => "ret",
                _ => {
                    writeln!(out, "    .long 0x{word:08x}").unwrap();
                    continue;
                }
            }
            .to_owned()
        };
        writeln!(out, "    {instruction}").unwrap();
    }
    writeln!(out, ".Lbf_{}:", code.bytes.len()).unwrap();
    out
}
