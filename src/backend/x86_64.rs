use crate::backend::{MachineCode, Relocation};
use crate::ir::{Op, Program};

struct Emitter {
    bytes: Vec<u8>,
    relocations: Vec<Relocation>,
    errors: Vec<usize>,
    windows: bool,
}

impl Emitter {
    fn emit(&mut self, bytes: &[u8]) {
        self.bytes.extend_from_slice(bytes);
    }
    fn branch(&mut self, opcode: &[u8]) -> usize {
        self.emit(opcode);
        let pos = self.bytes.len();
        self.emit(&[0; 4]);
        pos
    }
    fn patch(&mut self, pos: usize, target: usize) -> Result<(), String> {
        let delta = i32::try_from(target as i64 - (pos as i64 + 4))
            .map_err(|_| "x86-64 branch displacement out of range".to_string())?;
        self.bytes[pos..pos + 4].copy_from_slice(&delta.to_le_bytes());
        Ok(())
    }
    fn error_if(&mut self, opcode: &[u8]) {
        let pos = self.branch(opcode);
        self.errors.push(pos);
    }
    fn call(&mut self, symbol: &'static str) {
        let pos = self.branch(&[0xe8]);
        self.relocations.push(Relocation {
            offset: pos as u32,
            symbol,
        });
    }
    fn ops(&mut self, ops: &[Op]) -> Result<(), String> {
        for op in ops {
            match op {
                Op::Move { amount, .. } => {
                    let mut remaining = amount.unsigned_abs();
                    while remaining != 0 {
                        let chunk = remaining.min(i32::MAX as u64) as u32;
                        if *amount > 0 {
                            self.emit(&[0x4c, 0x89, 0xf0, 0x4c, 0x29, 0xe0]); // rax = end - current
                        } else {
                            self.emit(&[0x4c, 0x89, 0xe0, 0x4c, 0x29, 0xe8]); // rax = current - base
                        }
                        self.emit(&[0x48, 0x3d]); // cmp rax, imm32
                        self.emit(&chunk.to_le_bytes());
                        self.error_if(if *amount > 0 {
                            &[0x0f, 0x86]
                        } else {
                            &[0x0f, 0x82]
                        }); // jbe / jb
                        self.emit(if *amount > 0 {
                            &[0x49, 0x81, 0xc4]
                        } else {
                            &[0x49, 0x81, 0xec]
                        });
                        self.emit(&chunk.to_le_bytes());
                        remaining -= u64::from(chunk);
                    }
                }
                Op::Add { amount, .. } => {
                    let value = amount.rem_euclid(256) as u8;
                    if value != 0 {
                        self.emit(&[0x41, 0x80, 0x04, 0x24, value]);
                    }
                }
                Op::Clear { .. } => self.emit(&[0x41, 0xc6, 0x04, 0x24, 0x00]),
                Op::Transfer {
                    offset,
                    factor,
                    pos,
                } => {
                    if !matches!(offset, -1 | 1) || !matches!(factor, -1 | 1) {
                        return Err("invalid transfer operation".into());
                    }
                    self.emit(&[0x41, 0x0f, 0xb6, 0x0c, 0x24]); // movzbl (r12), ecx
                    self.emit(&[0x85, 0xc9]); // test ecx, ecx
                    let skip = self.branch(&[0x0f, 0x84]);
                    self.emit(&[0x41, 0x80, 0x2c, 0x24, 0x01]); // preserve first decrement if movement fails
                    self.ops(&[Op::Move {
                        amount: i64::from(*offset),
                        pos: *pos,
                    }])?;
                    self.emit(if *factor == 1 {
                        &[0x41, 0x00, 0x0c, 0x24]
                    } else {
                        &[0x41, 0x28, 0x0c, 0x24]
                    });
                    self.ops(&[Op::Move {
                        amount: -i64::from(*offset),
                        pos: *pos,
                    }])?;
                    self.emit(&[0x41, 0xc6, 0x04, 0x24, 0x00]);
                    self.patch(skip, self.bytes.len())?;
                }
                Op::Output { .. } => {
                    if self.windows {
                        self.emit(&[0x41, 0x0f, 0xb6, 0x0c, 0x24]); // movzx ecx, byte [r12]
                    } else {
                        self.emit(&[0x41, 0x0f, 0xb6, 0x3c, 0x24]); // movzx edi, byte [r12]
                    }
                    self.call("_bf_output");
                    self.emit(&[0x85, 0xc0]); // test eax, eax
                    self.error_if(&[0x0f, 0x85]);
                }
                Op::Input { .. } => {
                    self.call("_bf_input");
                    self.emit(&[0x83, 0xf8, 0xfe]); // cmp eax, -2
                    self.error_if(&[0x0f, 0x84]);
                    self.emit(&[0x83, 0xf8, 0xff]); // cmp eax, -1
                    let non_eof = self.branch(&[0x0f, 0x85]);
                    self.emit(&[0x31, 0xc0]); // xor eax, eax
                    self.patch(non_eof, self.bytes.len())?;
                    self.emit(&[0x41, 0x88, 0x04, 0x24]); // mov [r12], al
                }
                Op::Loop { body, .. } => {
                    let start = self.bytes.len();
                    self.emit(&[0x41, 0x80, 0x3c, 0x24, 0x00]); // cmp byte [r12], 0
                    let end = self.branch(&[0x0f, 0x84]);
                    self.ops(body)?;
                    let back = self.branch(&[0xe9]);
                    self.patch(back, start)?;
                    self.patch(end, self.bytes.len())?;
                }
            }
        }
        Ok(())
    }
}

pub fn compile(program: &Program) -> Result<MachineCode, String> {
    compile_for(program, false)
}

pub fn compile_for(program: &Program, windows: bool) -> Result<MachineCode, String> {
    let mut e = Emitter {
        bytes: Vec::new(),
        relocations: Vec::new(),
        errors: Vec::new(),
        windows,
    };
    // Four pushes leave RSP misaligned on entry, so reserve eight bytes before calls.
    e.emit(&[
        0x55,
        0x41,
        0x54,
        0x41,
        0x55,
        0x41,
        0x56,
        0x48,
        0x83,
        0xec,
        if windows { 0x28 } else { 0x08 },
    ]);
    if windows {
        e.emit(&[0x49, 0x89, 0xcc, 0x49, 0x89, 0xcd]); // r12 = r13 = rcx
        e.emit(&[0x4c, 0x8d, 0x34, 0x11]); // lea r14, [rcx + rdx]
    } else {
        e.emit(&[0x49, 0x89, 0xfc, 0x49, 0x89, 0xfd]);
        e.emit(&[0x4c, 0x8d, 0x34, 0x37]);
    }
    e.ops(program)?;
    e.emit(&[0x31, 0xc0]); // xor eax, eax
    let done = e.branch(&[0xe9]);
    let error = e.bytes.len();
    e.emit(&[0xb8, 2, 0, 0, 0]);
    let epilogue = e.bytes.len();
    e.emit(&[
        0x48,
        0x83,
        0xc4,
        if windows { 0x28 } else { 0x08 },
        0x41,
        0x5e,
        0x41,
        0x5d,
        0x41,
        0x5c,
        0x5d,
        0xc3,
    ]);
    e.patch(done, epilogue)?;
    for pos in std::mem::take(&mut e.errors) {
        e.patch(pos, error)?;
    }
    Ok(MachineCode {
        bytes: e.bytes,
        relocations: e.relocations,
    })
}

pub fn assembly(code: &MachineCode) -> String {
    assembly_for(code, true)
}

pub fn assembly_for(code: &MachineCode, darwin: bool) -> String {
    use std::fmt::Write;
    let name = if darwin { "_bf_program" } else { "bf_program" };
    let mut text = format!(".text\n.globl {name}\n{name}:\n");
    let mut index = 0;
    while index < code.bytes.len() {
        writeln!(text, ".Lbf_{index}:").unwrap();
        let bytes = &code.bytes[index..];
        if let Some(reloc) = code
            .relocations
            .iter()
            .find(|r| r.offset as usize == index + 1)
            && bytes.len() >= 5
            && bytes[0] == 0xe8
        {
            let symbol = if darwin {
                reloc.symbol
            } else {
                reloc.symbol.trim_start_matches('_')
            };
            writeln!(text, "    call {symbol}").unwrap();
            index += 5;
            continue;
        }
        let fixed: &[(&[u8], &str)] = &[
            (&[0x55], "pushq %rbp"),
            (&[0x41, 0x54], "pushq %r12"),
            (&[0x41, 0x55], "pushq %r13"),
            (&[0x41, 0x56], "pushq %r14"),
            (&[0x41, 0x5e], "popq %r14"),
            (&[0x41, 0x5d], "popq %r13"),
            (&[0x41, 0x5c], "popq %r12"),
            (&[0x5d], "popq %rbp"),
            (&[0xc3], "retq"),
            (&[0x49, 0x89, 0xfc], "movq %rdi, %r12"),
            (&[0x49, 0x89, 0xfd], "movq %rdi, %r13"),
            (&[0x49, 0x89, 0xcc], "movq %rcx, %r12"),
            (&[0x49, 0x89, 0xcd], "movq %rcx, %r13"),
            (&[0x4c, 0x8d, 0x34, 0x37], "leaq (%rdi,%rsi), %r14"),
            (&[0x4c, 0x8d, 0x34, 0x11], "leaq (%rcx,%rdx), %r14"),
            (&[0x4c, 0x89, 0xf0], "movq %r14, %rax"),
            (&[0x4c, 0x29, 0xe0], "subq %r12, %rax"),
            (&[0x4c, 0x89, 0xe0], "movq %r12, %rax"),
            (&[0x4c, 0x29, 0xe8], "subq %r13, %rax"),
            (&[0x41, 0xc6, 0x04, 0x24, 0x00], "movb $0, (%r12)"),
            (&[0x41, 0x0f, 0xb6, 0x3c, 0x24], "movzbl (%r12), %edi"),
            (&[0x41, 0x0f, 0xb6, 0x0c, 0x24], "movzbl (%r12), %ecx"),
            (&[0x41, 0x88, 0x04, 0x24], "movb %al, (%r12)"),
            (&[0x41, 0x00, 0x0c, 0x24], "addb %cl, (%r12)"),
            (&[0x41, 0x28, 0x0c, 0x24], "subb %cl, (%r12)"),
            (&[0x41, 0x80, 0x3c, 0x24, 0x00], "cmpb $0, (%r12)"),
            (&[0x41, 0x80, 0x2c, 0x24, 0x01], "subb $1, (%r12)"),
            (&[0x85, 0xc0], "testl %eax, %eax"),
            (&[0x85, 0xc9], "testl %ecx, %ecx"),
            (&[0x83, 0xf8, 0xfe], "cmpl $-2, %eax"),
            (&[0x83, 0xf8, 0xff], "cmpl $-1, %eax"),
            (&[0x31, 0xc0], "xorl %eax, %eax"),
            (&[0xb8, 2, 0, 0, 0], "movl $2, %eax"),
        ];
        if let Some((encoded, mnemonic)) =
            fixed.iter().find(|(encoded, _)| bytes.starts_with(encoded))
        {
            writeln!(text, "    {mnemonic}").unwrap();
            index += encoded.len();
            continue;
        }
        if bytes.len() >= 6 && bytes[0] == 0x0f && matches!(bytes[1], 0x82..=0x86) {
            let displacement = i32::from_le_bytes(bytes[2..6].try_into().unwrap());
            let dest = index as i64 + 6 + i64::from(displacement);
            let mnemonic = match bytes[1] {
                0x82 => "jb",
                0x83 => "jae",
                0x84 => "je",
                0x85 => "jne",
                _ => "jbe",
            };
            writeln!(text, "    {mnemonic} .Lbf_{dest}").unwrap();
            index += 6;
        } else if bytes.len() >= 5 && bytes[0] == 0xe9 {
            let displacement = i32::from_le_bytes(bytes[1..5].try_into().unwrap());
            writeln!(
                text,
                "    jmp .Lbf_{}",
                index as i64 + 5 + i64::from(displacement)
            )
            .unwrap();
            index += 5;
        } else if bytes.len() >= 6 && bytes.starts_with(&[0x48, 0x3d]) {
            let imm = i32::from_le_bytes(bytes[2..6].try_into().unwrap());
            writeln!(text, "    cmpq ${imm}, %rax").unwrap();
            index += 6;
        } else if bytes.len() >= 7
            && (bytes.starts_with(&[0x49, 0x81, 0xc4]) || bytes.starts_with(&[0x49, 0x81, 0xec]))
        {
            let imm = i32::from_le_bytes(bytes[3..7].try_into().unwrap());
            writeln!(
                text,
                "    {}q ${imm}, %r12",
                if bytes[2] == 0xc4 { "add" } else { "sub" }
            )
            .unwrap();
            index += 7;
        } else if bytes.len() >= 4
            && (bytes.starts_with(&[0x48, 0x83, 0xec]) || bytes.starts_with(&[0x48, 0x83, 0xc4]))
        {
            writeln!(
                text,
                "    {}q ${}, %rsp",
                if bytes[2] == 0xec { "sub" } else { "add" },
                bytes[3]
            )
            .unwrap();
            index += 4;
        } else if bytes.len() >= 5 && bytes.starts_with(&[0x41, 0x80, 0x04, 0x24]) {
            writeln!(text, "    addb ${}, (%r12)", bytes[4]).unwrap();
            index += 5;
        } else {
            writeln!(text, "    .byte 0x{:02x}", bytes[0]).unwrap();
            index += 1;
        }
    }
    writeln!(text, ".Lbf_{index}:").unwrap();
    text
}
