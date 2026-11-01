use crate::ir::{Op, Program};

pub fn optimize(program: Program) -> Program {
    let mut out: Program = Vec::with_capacity(program.len());
    for op in program {
        let op = match op {
            Op::Loop { body, pos } => {
                let body = optimize(body);
                if matches!(body.as_slice(), [Op::Add { amount, .. }] if amount.rem_euclid(256) == 1 || amount.rem_euclid(256) == 255)
                {
                    Op::Clear { pos }
                } else if let [
                    Op::Add { amount: source, .. },
                    Op::Move { amount: offset, .. },
                    Op::Add { amount: dest, .. },
                    Op::Move { amount: back, .. },
                ] = body.as_slice()
                    && source.rem_euclid(256) == 255
                    && (*offset == 1 || *offset == -1)
                    && *back == -*offset
                    && (dest.rem_euclid(256) == 1 || dest.rem_euclid(256) == 255)
                {
                    Op::Transfer {
                        offset: *offset as i8,
                        factor: if dest.rem_euclid(256) == 1 { 1 } else { -1 },
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
    out.retain(|op| !matches!(op, Op::Add { amount, .. } if amount.rem_euclid(256) == 0));
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
}
