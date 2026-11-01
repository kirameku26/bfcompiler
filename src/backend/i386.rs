use crate::CellWidth;
use crate::backend::{MachineCode, Relocation};
use crate::ir::{Op, Program};

struct Emitter {
    bytes: Vec<u8>,
    relocations: Vec<Relocation>,
    errors: Vec<usize>,
    width: CellWidth,
}

impl Emitter {
    fn prefix(&mut self) {
        if self.width == CellWidth::Bits16 {
            self.emit(&[0x66]);
        }
    }
    fn load(&mut self) {
        match self.width {
            CellWidth::Bits8 => self.emit(&[0x0f, 0xb6, 0x03]),
            CellWidth::Bits16 => self.emit(&[0x0f, 0xb7, 0x03]),
            CellWidth::Bits32 => self.emit(&[0x8b, 0x03]),
        }
    }
    fn emit(&mut self, bytes: &[u8]) {
        self.bytes.extend_from_slice(bytes);
    }
    fn branch(&mut self, opcode: &[u8]) -> usize {
        self.emit(opcode);
        let pos = self.bytes.len();
        self.emit(&[0; 4]);
        pos
    }
    fn patch(&mut self, pos: usize, dest: usize) -> Result<(), String> {
        let delta = i32::try_from(dest as i64 - pos as i64 - 4)
            .map_err(|_| "i386 branch displacement out of range")?;
        self.bytes[pos..pos + 4].copy_from_slice(&delta.to_le_bytes());
        Ok(())
    }
    fn error_if(&mut self, opcode: &[u8]) {
        let pos = self.branch(opcode);
        self.errors.push(pos);
    }
    fn call(&mut self, symbol: &'static str) -> Result<(), String> {
        let pos = self.branch(&[0xe8]);
        self.bytes[pos..pos + 4].copy_from_slice(&(-4i32).to_le_bytes());
        self.relocations.push(Relocation {
            offset: u32::try_from(pos).map_err(|_| "code too large")?,
            symbol,
        });
        Ok(())
    }
    fn ops(&mut self, ops: &[Op]) -> Result<(), String> {
        for op in ops {
            match op {
                Op::Move { amount, .. } => {
                    let mut remaining = amount.unsigned_abs();
                    while remaining > 0 {
                        let cells = remaining.min(i32::MAX as u64 / u64::from(self.width.bytes()));
                        let chunk = cells as u32 * self.width.bytes();
                        self.emit(if *amount > 0 {
                            &[0x89, 0xf8, 0x29, 0xd8]
                        } else {
                            &[0x89, 0xd8, 0x29, 0xf0]
                        });
                        self.emit(&[0x3d]);
                        self.emit(&chunk.to_le_bytes());
                        self.error_if(if *amount > 0 {
                            &[0x0f, 0x86]
                        } else {
                            &[0x0f, 0x82]
                        });
                        self.emit(if *amount > 0 {
                            &[0x81, 0xc3]
                        } else {
                            &[0x81, 0xeb]
                        });
                        self.emit(&chunk.to_le_bytes());
                        remaining -= cells;
                    }
                }
                Op::Add { amount, .. } => {
                    let value = amount.rem_euclid(self.width.modulus()) as u32;
                    if value != 0 {
                        self.prefix();
                        self.emit(&[
                            if self.width == CellWidth::Bits8 {
                                0x80
                            } else {
                                0x81
                            },
                            0x03,
                        ]);
                        self.emit(&value.to_le_bytes()[..self.width.bytes() as usize]);
                    }
                }
                Op::Clear { .. } => {
                    self.prefix();
                    self.emit(&[
                        if self.width == CellWidth::Bits8 {
                            0xc6
                        } else {
                            0xc7
                        },
                        0x03,
                    ]);
                    self.emit(&[0; 4][..self.width.bytes() as usize]);
                }
                Op::Transfer {
                    offset,
                    factor,
                    pos,
                } => {
                    if !matches!(offset, -1 | 1) || !matches!(factor, -1 | 1) {
                        return Err("invalid transfer operation".into());
                    }
                    // Reconstruct the original loop so boundary errors retain their timing.
                    self.ops(&[Op::Loop {
                        pos: *pos,
                        body: vec![
                            Op::Add {
                                amount: -1,
                                pos: *pos,
                            },
                            Op::Move {
                                amount: i64::from(*offset),
                                pos: *pos,
                            },
                            Op::Add {
                                amount: i64::from(*factor),
                                pos: *pos,
                            },
                            Op::Move {
                                amount: -i64::from(*offset),
                                pos: *pos,
                            },
                        ],
                    }])?;
                }
                Op::Output { .. } => {
                    self.load();
                    self.emit(&[0x89, 0x04, 0x24]);
                    self.call("_bf_output")?;
                    self.emit(&[0x85, 0xc0]);
                    self.error_if(&[0x0f, 0x85]);
                }
                Op::Input { .. } => {
                    self.call("_bf_input")?;
                    self.emit(&[0x83, 0xf8, 0xfe]);
                    self.error_if(&[0x0f, 0x84]);
                    self.emit(&[0x83, 0xf8, 0xff]);
                    let non_eof = self.branch(&[0x0f, 0x85]);
                    self.emit(&[0x31, 0xc0]);
                    self.patch(non_eof, self.bytes.len())?;
                    self.prefix();
                    self.emit(&[
                        if self.width == CellWidth::Bits8 {
                            0x88
                        } else {
                            0x89
                        },
                        0x03,
                    ]);
                }
                Op::Loop { body, .. } => {
                    let start = self.bytes.len();
                    self.prefix();
                    self.emit(&[
                        if self.width == CellWidth::Bits8 {
                            0x80
                        } else {
                            0x83
                        },
                        0x3b,
                        0,
                    ]);
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
    compile_with_width(program, CellWidth::Bits8)
}

pub fn compile_with_width(program: &Program, width: CellWidth) -> Result<MachineCode, String> {
    let mut e = Emitter {
        bytes: vec![],
        relocations: vec![],
        errors: vec![],
        width,
    };
    // Save callee-saved registers and align the stack, reserving outgoing argument space.
    e.emit(&[0x55, 0x53, 0x56, 0x57, 0x83, 0xec, 0x1c]);
    e.emit(&[
        0x8b, 0x5c, 0x24, 0x30, 0x89, 0xde, 0x89, 0xdf, 0x03, 0x7c, 0x24, 0x34,
    ]);
    e.ops(program)?;
    e.emit(&[0x31, 0xc0]);
    let done = e.branch(&[0xe9]);
    let error = e.bytes.len();
    e.emit(&[0xb8, 2, 0, 0, 0]);
    let epilogue = e.bytes.len();
    e.emit(&[0x83, 0xc4, 0x1c, 0x5f, 0x5e, 0x5b, 0x5d, 0xc3]);
    e.patch(done, epilogue)?;
    for pos in std::mem::take(&mut e.errors) {
        e.patch(pos, error)?;
    }
    Ok(MachineCode {
        bytes: e.bytes,
        relocations: e.relocations,
        config: None,
        output_encoding: None,
    })
}

pub fn assembly(code: &MachineCode) -> String {
    use std::fmt::Write;
    let mut text = String::from(".text\n.globl bf_program\nbf_program:\n");
    let mut index = 0;
    while index < code.bytes.len() {
        if let Some(r) = code
            .relocations
            .iter()
            .find(|r| r.offset as usize == index + 1)
        {
            writeln!(text, "    call {}", r.symbol.trim_start_matches('_')).unwrap();
            index += 5;
        } else {
            writeln!(text, "    .byte 0x{:02x}", code.bytes[index]).unwrap();
            index += 1;
        }
    }
    text.push_str(".section .note.GNU-stack,\"\",@progbits\n");
    text
}
