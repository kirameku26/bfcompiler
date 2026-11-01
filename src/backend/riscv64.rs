use crate::CellWidth;
use crate::backend::{MachineCode, Relocation};
use crate::ir::{Op, Program};

// RV64I registers: s0=current, s1=base, s2=end; t0/t1 are scratch.
struct Emitter {
    bytes: Vec<u8>,
    relocations: Vec<Relocation>,
    errors: Vec<usize>,
    width: CellWidth,
}
impl Emitter {
    fn load_cell(&mut self, rd: u32) {
        let f = match self.width {
            CellWidth::Bits8 => 4,
            CellWidth::Bits16 => 5,
            CellWidth::Bits32 => 6,
        };
        self.i(0x03, rd, f, 8, 0);
    }
    fn store_cell(&mut self, rs: u32) {
        let f = match self.width {
            CellWidth::Bits8 => 0,
            CellWidth::Bits16 => 1,
            CellWidth::Bits32 => 2,
        };
        self.store(rs, 8, 0, f);
    }
    fn word(&mut self, word: u32) {
        self.bytes.extend(word.to_le_bytes());
    }
    fn i(&mut self, opcode: u32, rd: u32, f: u32, rs: u32, imm: i32) {
        self.word(((imm as u32 & 0xfff) << 20) | (rs << 15) | (f << 12) | (rd << 7) | opcode);
    }
    fn r(&mut self, rd: u32, f: u32, a: u32, b: u32, funct7: u32) {
        self.word((funct7 << 25) | (b << 20) | (a << 15) | (f << 12) | (rd << 7) | 0x33);
    }
    fn store(&mut self, rs: u32, base: u32, imm: i32, f: u32) {
        let n = imm as u32 & 0xfff;
        self.word(
            ((n >> 5) << 25) | (rs << 20) | (base << 15) | (f << 12) | ((n & 31) << 7) | 0x23,
        );
    }
    fn branch8(&mut self, f: u32, a: u32, b: u32) {
        self.word((b << 20) | (a << 15) | (f << 12) | (4 << 8) | 0x63);
    }
    fn jump(&mut self) -> usize {
        let pos = self.bytes.len();
        self.word(0x6f);
        pos
    }
    fn patch(&mut self, pos: usize, dest: usize) -> Result<(), String> {
        let d = dest as i64 - pos as i64;
        if !(-1048576..=1048574).contains(&d) || d % 2 != 0 {
            return Err("RISC-V jump displacement out of range".into());
        }
        let n = d as u32 & 0x1f_ffff;
        let word = ((n >> 20) << 31)
            | (((n >> 1) & 0x3ff) << 21)
            | (((n >> 11) & 1) << 20)
            | (((n >> 12) & 0xff) << 12)
            | 0x6f;
        self.bytes[pos..pos + 4].copy_from_slice(&word.to_le_bytes());
        Ok(())
    }
    fn error_unless(&mut self, f: u32, a: u32, b: u32) {
        self.branch8(f, a, b);
        let pos = self.jump();
        self.errors.push(pos);
    }
    fn call(&mut self, symbol: &'static str) -> Result<(), String> {
        let offset = u32::try_from(self.bytes.len()).map_err(|_| "code too large")?;
        self.relocations.push(Relocation { offset, symbol });
        self.word(0x00000097);
        self.word(0x000080e7);
        Ok(()) // auipc ra,0; jalr ra,ra,0
    }
    fn ops(&mut self, ops: &[Op]) -> Result<(), String> {
        for op in ops {
            match op {
                Op::Move { amount, .. } => {
                    let mut remaining = amount.unsigned_abs();
                    while remaining > 0 {
                        let cells = remaining.min(2047 / u64::from(self.width.bytes()));
                        let chunk = cells as i32 * self.width.bytes() as i32;
                        if *amount > 0 {
                            self.r(5, 0, 18, 8, 0x20);
                        } else {
                            self.r(5, 0, 8, 9, 0x20);
                        }
                        self.i(0x13, 6, 0, 0, chunk);
                        if *amount > 0 {
                            self.error_unless(6, 6, 5);
                        } else {
                            self.error_unless(7, 5, 6);
                        }
                        self.i(0x13, 8, 0, 8, if *amount > 0 { chunk } else { -chunk });
                        remaining -= cells;
                    }
                }
                Op::Add { amount, .. } => {
                    let n = amount.rem_euclid(self.width.modulus()) as u32;
                    if n != 0 {
                        self.load_cell(5);
                        let signed = n as i32;
                        if (-2048..=2047).contains(&signed) {
                            self.i(0x13, 5, 0, 5, signed);
                        } else {
                            let high = n.wrapping_add(0x800) & 0xfffff000;
                            self.word(high | (6 << 7) | 0x37);
                            self.i(0x1b, 6, 0, 6, signed); // addiw sign-extends the 32-bit constant
                            self.r(5, 0, 5, 6, 0);
                        }
                        self.store_cell(5);
                    }
                }
                Op::Clear { .. } => self.store_cell(0),
                Op::Transfer {
                    offset,
                    factor,
                    pos,
                } => {
                    if !matches!(offset, -1 | 1) || !matches!(factor, -1 | 1) {
                        return Err("invalid transfer operation".into());
                    }
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
                    if self.width == CellWidth::Bits32 {
                        self.i(0x03, 10, 2, 8, 0);
                    } else {
                        self.load_cell(10);
                    }
                    self.call("_bf_output")?;
                    self.error_unless(0, 10, 0);
                }
                Op::Input { .. } => {
                    self.call("_bf_input")?;
                    self.i(0x13, 5, 0, 0, -2);
                    self.error_unless(1, 10, 5);
                    self.i(0x13, 5, 0, 0, -1);
                    self.branch8(1, 10, 5);
                    self.i(0x13, 10, 0, 0, 0);
                    self.store_cell(10);
                }
                Op::Loop { body, .. } => {
                    let start = self.bytes.len();
                    self.load_cell(5);
                    self.branch8(1, 5, 0);
                    let end = self.jump();
                    self.ops(body)?;
                    let back = self.jump();
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
    e.i(0x13, 2, 0, 2, -32);
    e.store(1, 2, 24, 3);
    e.store(8, 2, 16, 3);
    e.store(9, 2, 8, 3);
    e.store(18, 2, 0, 3);
    e.i(0x13, 8, 0, 10, 0);
    e.i(0x13, 9, 0, 10, 0);
    e.r(18, 0, 10, 11, 0);
    e.ops(program)?;
    e.i(0x13, 10, 0, 0, 0);
    let done = e.jump();
    let error = e.bytes.len();
    e.i(0x13, 10, 0, 0, 2);
    let epilogue = e.bytes.len();
    e.i(0x03, 18, 3, 2, 0);
    e.i(0x03, 9, 3, 2, 8);
    e.i(0x03, 8, 3, 2, 16);
    e.i(0x03, 1, 3, 2, 24);
    e.i(0x13, 2, 0, 2, 32);
    e.i(0x67, 0, 0, 1, 0);
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
    let mut text = String::from(
        ".option norvc\n.option norelax\n.text\n.p2align 2\n.globl bf_program\nbf_program:\n",
    );
    let mut i = 0;
    while i < code.bytes.len() {
        if let Some(r) = code.relocations.iter().find(|r| r.offset as usize == i) {
            writeln!(text, "    call {}", r.symbol.trim_start_matches('_')).unwrap();
            i += 8;
        } else {
            let word = u32::from_le_bytes(code.bytes[i..i + 4].try_into().unwrap());
            writeln!(text, "    .4byte 0x{word:08x}").unwrap();
            i += 4;
        }
    }
    text.push_str(".section .note.GNU-stack,\"\",@progbits\n");
    text
}
