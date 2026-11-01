use crate::ir::{Op, Program};

pub fn optimize(program: Program) -> Program {
    optimize_with_width(program, CellWidth::Bits8)
}

/// Width of each wrapping unsigned tape cell.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CellWidth {
    Bits8,
    Bits16,
    Bits32,
}

impl CellWidth {
    pub fn bits(self) -> u32 {
        match self {
            Self::Bits8 => 8,
            Self::Bits16 => 16,
            Self::Bits32 => 32,
        }
    }

    pub fn bytes(self) -> u32 {
        self.bits() / 8
    }

    pub fn modulus(self) -> i64 {
        match self {
            Self::Bits8 => 1 << 8,
            Self::Bits16 => 1 << 16,
            Self::Bits32 => 1i64 << 32,
        }
    }
}

pub fn optimize_with_width(program: Program, width: CellWidth) -> Program {
    let modulus = width.modulus();
    let mut out: Program = Vec::with_capacity(program.len());
    for op in program {
        let op = match op {
            Op::Loop { body, pos } => {
                let body = optimize_with_width(body, width);
                if matches!(body.as_slice(), [Op::Add { amount, .. }] if amount.rem_euclid(modulus) == 1 || amount.rem_euclid(modulus) == modulus - 1)
                {
                    Op::Clear { pos }
                } else if let [
                    Op::Add { amount: source, .. },
                    Op::Move { amount: offset, .. },
                    Op::Add { amount: dest, .. },
                    Op::Move { amount: back, .. },
                ] = body.as_slice()
                    && source.rem_euclid(modulus) == modulus - 1
                    && (*offset == 1 || *offset == -1)
                    && *back == -*offset
                    && (dest.rem_euclid(modulus) == 1 || dest.rem_euclid(modulus) == modulus - 1)
                {
                    Op::Transfer {
                        offset: *offset as i8,
                        factor: if dest.rem_euclid(modulus) == 1 { 1 } else { -1 },
                        pos,
                    }
                } else {
                    Op::Loop { body, pos }
                }
            }
            other => other,
        };
        match op {
            Op::Add { amount, pos } => {
                if let Some(Op::Add {
                    amount: previous, ..
                }) = out.last_mut()
                {
                    *previous += amount;
                } else {
                    out.push(Op::Add { amount, pos });
                }
            }
            Op::Move { amount, pos } => {
                // Only same-direction moves are combined. Cancelling opposite moves
                // could hide an intermediate tape-boundary error.
                if let Some(Op::Move {
                    amount: previous, ..
                }) = out.last_mut()
                    && previous.signum() == amount.signum()
                {
                    *previous += amount;
                } else {
                    out.push(Op::Move { amount, pos });
                }
            }
            other => out.push(other),
        }
    }
    out.retain(|op| !matches!(op, Op::Add { amount, .. } if amount.rem_euclid(modulus) == 0));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::parse;

    #[test]
    fn combines_arithmetic_modulo_cell_width() {
        let p = optimize(parse(b"++--+").unwrap());
        assert!(matches!(p.as_slice(), [Op::Add { amount: 1, .. }]));
    }

    #[test]
    fn does_not_cancel_pointer_motion() {
        let p = optimize(parse(b"><").unwrap());
        assert_eq!(p.len(), 2);
    }

    #[test]
    fn clears_only_terminating_unit_step_loops() {
        for source in [b"[-]".as_slice(), b"[+]".as_slice(), b"[++-]".as_slice()] {
            assert!(matches!(
                optimize(parse(source).unwrap()).as_slice(),
                [Op::Clear { .. }]
            ));
        }
        for source in [b"[--]".as_slice(), b"[]".as_slice()] {
            assert!(matches!(
                optimize(parse(source).unwrap()).as_slice(),
                [Op::Loop { .. }]
            ));
        }
    }

    #[test]
    fn identifies_only_safe_adjacent_transfer_loops() {
        for source in [b"[->+<]".as_slice(), b"[-<->]".as_slice()] {
            assert!(matches!(
                optimize(parse(source).unwrap()).as_slice(),
                [Op::Transfer { .. }]
            ));
        }
        for source in [
            b"[-- >+<]".as_slice(),
            b"[->++<]".as_slice(),
            b"[->+>+<<]".as_slice(),
        ] {
            assert!(matches!(
                optimize(parse(source).unwrap()).as_slice(),
                [Op::Loop { .. }]
            ));
        }
    }

    #[test]
    fn wider_cells_preserve_arithmetic_and_loop_semantics() {
        let source = vec![b'+'; 256];
        assert!(optimize_with_width(parse(&source).unwrap(), CellWidth::Bits8).is_empty());
        for width in [CellWidth::Bits16, CellWidth::Bits32] {
            assert!(matches!(
                optimize_with_width(parse(&source).unwrap(), width).as_slice(),
                [Op::Add { amount: 256, .. }]
            ));
            let mut loop_source = vec![b'['];
            loop_source.extend(vec![b'+'; 255]);
            loop_source.push(b']');
            assert!(matches!(
                optimize_with_width(parse(&loop_source).unwrap(), width).as_slice(),
                [Op::Loop { .. }]
            ));
        }
    }
}
