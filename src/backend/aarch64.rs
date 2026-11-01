use super::{MachineCode, Relocation};
use crate::CellWidth;
use crate::ir::{Op, Program};

struct Emitter {
    words: Vec<u32>,
    relocations: Vec<Relocation>,
    error_branches: Vec<usize>,
    width: CellWidth,
}

impl Emitter {
    fn load(&mut self, register: u32) {
        let base = match self.width {
            CellWidth::Bits8 => 0x3940_0260,
            CellWidth::Bits16 => 0x7940_0260,
            CellWidth::Bits32 => 0xb940_0260,
        };
        self.emit(base | register);
    }
    fn store(&mut self, register: u32) {
        let base = match self.width {
            CellWidth::Bits8 => 0x3900_0260,
            CellWidth::Bits16 => 0x7900_0260,
            CellWidth::Bits32 => 0xb900_0260,
        };
        self.emit(base | register);
    }
    fn emit(&mut self, word: u32) -> usize {
        self.words.push(word);
        self.words.len() - 1
    }
    fn offset(&self) -> usize {
        self.words.len() * 4
    }

    fn patch_imm26(&mut self, at: usize, target: usize) -> Result<(), String> {
        let delta = target as i64 - (at * 4) as i64;
        if delta % 4 != 0 || !(-(1_i64 << 27)..(1_i64 << 27)).contains(&delta) {
            return Err("branch is outside the AArch64 B range".into());
        }
        self.words[at] = (self.words[at] & 0xfc00_0000) | ((delta / 4) as u32 & 0x03ff_ffff);
        Ok(())
    }

    fn patch_imm19(&mut self, at: usize, target: usize) -> Result<(), String> {
        let delta = target as i64 - (at * 4) as i64;
        if delta % 4 != 0 || !(-(1_i64 << 20)..(1_i64 << 20)).contains(&delta) {
            return Err("loop branch is outside the AArch64 conditional-branch range".into());
        }
        self.words[at] = (self.words[at] & !0x00ff_ffe0) | (((delta / 4) as u32 & 0x7ffff) << 5);
        Ok(())
    }

    fn branch_error(&mut self) {
        let at = self.emit(0x1400_0000); // b error
        self.error_branches.push(at);
    }

    fn call(&mut self, symbol: &'static str) {
        let offset = self.offset() as u32;
        self.emit(0x9400_0000); // linker fills BRANCH26
        self.relocations.push(Relocation { offset, symbol });
    }

    fn emit_ops(&mut self, ops: &[Op]) -> Result<(), String> {
        for op in ops {
            match op {
                Op::Move { amount, .. } => {
                    let mut remaining = amount.unsigned_abs();
                    while remaining != 0 {
                        let cells = remaining.min(4095 / u64::from(self.width.bytes()));
                        let chunk = cells as u32 * self.width.bytes();
                        if *amount > 0 {
                            self.emit(0xcb13_02a8); // sub x8, x21, x19 (space to end)
                        } else {
                            self.emit(0xcb14_0268); // sub x8, x19, x20 (space to base)
                        }
                        self.emit(0xf100_011f | (chunk << 10)); // cmp x8, #chunk
                        self.emit(if *amount > 0 {
                            0x5400_0048
                        } else {
                            0x5400_0042
                        }); // b.hi / b.hs +8
                        self.branch_error();
                        self.emit(
                            (if *amount > 0 {
                                0x9100_0273
                            } else {
                                0xd100_0273
                            }) | (chunk << 10),
                        );
                        remaining -= cells;
                    }
                }
                Op::Add { amount, .. } => {
                    let value = amount.rem_euclid(self.width.modulus()) as u32;
                    if value != 0 {
                        self.load(8);
                        if value <= 4095 {
                            self.emit(0x1100_0108 | (value << 10));
                        } else {
                            self.emit(0x5280_0009 | ((value & 0xffff) << 5));
                            self.emit(0x72a0_0009 | ((value >> 16) << 5));
                            self.emit(0x0b09_0108); // add w8,w8,w9
                        }
                        self.store(8);
                    }
                }
                Op::Clear { .. } => {
                    self.store(31);
                } // strb wzr, [x19]
                Op::Transfer {
                    offset,
                    factor,
                    pos,
                } => {
                    if !matches!(offset, -1 | 1) || !matches!(factor, -1 | 1) {
                        return Err("invalid transfer operation".into());
                    }
                    self.load(10);
                    let skip = self.emit(0x3400_000a); // cbz w10, done
                    self.emit(0x5100_0548); // sub w8, w10, #1
                    self.store(8); // preserve the first decrement if movement fails
                    self.emit_ops(&[Op::Move {
                        amount: i64::from(*offset),
                        pos: *pos,
                    }])?;
                    self.load(8);
                    self.emit(if *factor == 1 {
                        0x0b0a_0108
                    } else {
                        0x4b0a_0108
                    }); // add/sub w8, w8, w10
                    self.store(8);
                    self.emit_ops(&[Op::Move {
                        amount: -i64::from(*offset),
                        pos: *pos,
                    }])?;
                    self.store(31);
                    self.patch_imm19(skip, self.offset())?;
                }
                Op::Output { .. } => {
                    self.load(0);
                    self.call("_bf_output");
                    self.emit(0x7100_001f); // cmp w0, #0
                    self.emit(0x5400_0040); // b.eq +8
                    self.branch_error();
                }
                Op::Input { .. } => {
                    self.call("_bf_input");
                    self.emit(0x3100_081f); // cmn w0, #2 (I/O error)
                    self.emit(0x5400_0041); // b.ne +8
                    self.branch_error();
                    self.emit(0x3100_041f); // cmn w0, #1 (EOF)
                    let not_eof = self.emit(0x5400_0001); // b.ne store
                    self.emit(0x5280_0000); // mov w0, #0
                    let store = self.words.len() * 4;
                    self.patch_imm19(not_eof, store)?;
                    self.store(0);
                }
                Op::Loop { body, .. } => {
                    let test = self.words.len();
                    self.load(8);
                    let exit = self.emit(0x3400_0008); // cbz w8, exit
                    self.emit_ops(body)?;
                    let back = self.emit(0x1400_0000); // b test
                    self.patch_imm26(back, test * 4)?;
                    let after = self.words.len() * 4;
                    self.patch_imm19(exit, after)?;
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
        words: Vec::new(),
        relocations: Vec::new(),
        error_branches: Vec::new(),
        width,
    };
    e.emit(0xa9bd_7bfd); // stp x29, x30, [sp, #-48]!
    e.emit(0xa901_53f3); // stp x19, x20, [sp, #16]
    e.emit(0xf900_13f5); // str x21, [sp, #32]
    e.emit(0x9100_03fd); // mov x29, sp
    e.emit(0xaa00_03f3); // mov x19, x0 (current)
    e.emit(0xaa00_03f4); // mov x20, x0 (base)
    e.emit(0x8b01_0015); // add x21, x0, x1 (end)
    e.emit_ops(program)?;
    e.emit(0x5280_0000); // mov w0, #0
    let success_to_epilogue = e.emit(0x1400_0000);
    let error = e.words.len() * 4;
    e.emit(0x5280_0040); // mov w0, #2
    let epilogue = e.words.len() * 4;
    e.emit(0xf940_13f5); // ldr x21, [sp, #32]
    e.emit(0xa941_53f3); // ldp x19, x20, [sp, #16]
    e.emit(0xa8c3_7bfd); // ldp x29, x30, [sp], #48
    e.emit(0xd65f_03c0); // ret
    e.patch_imm26(success_to_epilogue, epilogue)?;
    for at in std::mem::take(&mut e.error_branches) {
        e.patch_imm26(at, error)?;
    }

    let bytes = e.words.into_iter().flat_map(u32::to_le_bytes).collect();
    Ok(MachineCode {
        bytes,
        relocations: e.relocations,
        config: None,
        output_encoding: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{frontend, optimizer};

    #[test]
    fn emits_direct_calls_as_relocations() {
        let p = optimizer::optimize(frontend::parse(b",.").unwrap());
        let code = compile(&p).unwrap();
        assert_eq!(
            code.relocations
                .iter()
                .map(|r| r.symbol)
                .collect::<Vec<_>>(),
            ["_bf_input", "_bf_output"]
        );
        assert!(
            code.bytes
                .windows(4)
                .any(|w| w == 0x9400_0000u32.to_le_bytes())
        );
    }
}
