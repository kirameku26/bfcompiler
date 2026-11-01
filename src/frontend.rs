use crate::ir::{Op, Program, SourcePos};
use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SyntaxError {
    pub message: &'static str,
    pub pos: SourcePos,
}

impl fmt::Display for SyntaxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "syntax error at {}:{} (byte {}): {}",
            self.pos.line, self.pos.column, self.pos.byte, self.message
        )
    }
}

pub fn parse(source: &[u8]) -> Result<Program, SyntaxError> {
    let mut stack: Vec<(Vec<Op>, SourcePos)> = Vec::new();
    let mut current = Vec::new();
    let (mut line, mut column) = (1, 1);

    for (byte, ch) in source.iter().copied().enumerate() {
        let pos = SourcePos { byte, line, column };
        match ch {
            b'>' => current.push(Op::Move { amount: 1, pos }),
            b'<' => current.push(Op::Move { amount: -1, pos }),
            b'+' => current.push(Op::Add { amount: 1, pos }),
            b'-' => current.push(Op::Add { amount: -1, pos }),
            b'.' => current.push(Op::Output { pos }),
            b',' => current.push(Op::Input { pos }),
            b'[' => {
                stack.push((current, pos));
                current = Vec::new();
            }
            b']' => {
                let Some((mut parent, open_pos)) = stack.pop() else {
                    return Err(SyntaxError {
                        message: "unmatched ']'",
                        pos,
                    });
                };
                parent.push(Op::Loop {
                    body: current,
                    pos: open_pos,
                });
                current = parent;
            }
            _ => {}
        }
        if ch == b'\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    if let Some((_, pos)) = stack.last() {
        return Err(SyntaxError {
            message: "unmatched '['",
            pos: *pos,
        });
    }
    Ok(current)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignores_comments_and_builds_nested_loops() {
        let p = parse(b"note +[->+[<]]").unwrap();
        assert_eq!(p.len(), 2);
        let Op::Loop { body, .. } = &p[1] else {
            panic!("expected loop")
        };
        assert!(body.iter().any(|op| matches!(op, Op::Loop { .. })));
    }

    #[test]
    fn reports_unmatched_bracket_position() {
        let e = parse(b"x\n  ]").unwrap_err();
        assert_eq!((e.pos.line, e.pos.column, e.pos.byte), (2, 3, 4));
    }
}
