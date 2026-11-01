//! A deliberately simple bytecode interpreter used only as an independent test oracle.

use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(Debug, PartialEq, Eq)]
struct ResultValue {
    output: Vec<u8>,
    status: i32,
}

fn interpret(source: &[u8], input: &[u8], tape_size: usize) -> ResultValue {
    let code: Vec<u8> = source
        .iter()
        .copied()
        .filter(|b| b"><+-.,[]".contains(b))
        .collect();
    let mut pairs = vec![0; code.len()];
    let mut stack = Vec::new();
    for (i, op) in code.iter().enumerate() {
        if *op == b'[' {
            stack.push(i);
        }
        if *op == b']' {
            let open = stack.pop().unwrap();
            pairs[open] = i;
            pairs[i] = open;
        }
    }
    assert!(stack.is_empty());
    let (mut pc, mut pointer, mut next_input) = (0, 0, 0);
    let mut tape = vec![0u8; tape_size];
    let mut output = Vec::new();
    let mut steps = 0usize;
    while pc < code.len() {
        steps += 1;
        assert!(steps < 1_000_000, "reference program exceeded step limit");
        match code[pc] {
            b'>' if pointer + 1 == tape_size => return ResultValue { output, status: 2 },
            b'<' if pointer == 0 => return ResultValue { output, status: 2 },
            b'>' => pointer += 1,
            b'<' => pointer -= 1,
            b'+' => tape[pointer] = tape[pointer].wrapping_add(1),
            b'-' => tape[pointer] = tape[pointer].wrapping_sub(1),
            b'.' => output.push(tape[pointer]),
            b',' => {
                tape[pointer] = input.get(next_input).copied().unwrap_or(0);
                next_input += 1;
            }
            b'[' if tape[pointer] == 0 => pc = pairs[pc],
            b']' if tape[pointer] != 0 => pc = pairs[pc],
            _ => {}
        }
        pc += 1;
    }
    ResultValue { output, status: 0 }
}

struct TestDir(std::path::PathBuf);
impl TestDir {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "bfcompiler-reference-{}-{stamp}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&dir).unwrap();
        Self(dir)
    }
}
impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn native_programs_match_reference_with_and_without_optimization() {
    let Some(host) = bfcompiler::Target::host_default() else {
        return;
    };
    let targets: Vec<&str> = match host {
        bfcompiler::Target::Aarch64Darwin => vec!["aarch64-apple-darwin"],
        bfcompiler::Target::X86_64Darwin => vec!["x86_64-apple-darwin"],
        bfcompiler::Target::Aarch64Linux => vec!["aarch64-unknown-linux-gnu"],
        bfcompiler::Target::X86_64Linux => vec!["x86_64-unknown-linux-gnu"],
        bfcompiler::Target::I386Linux => vec!["i686-unknown-linux-gnu"],
        bfcompiler::Target::Riscv64Linux => vec!["riscv64gc-unknown-linux-gnu"],
        bfcompiler::Target::X86_64WindowsGnu => vec!["x86_64-pc-windows-gnu"],
    };
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    let targets = {
        let mut targets = targets;
        if Command::new("arch")
            .args(["-x86_64", "/usr/bin/true"])
            .status()
            .is_ok_and(|s| s.success())
        {
            targets.push("x86_64-apple-darwin");
        }
        targets
    };
    let transfer_program = b",[->+<]>.<".repeat(256);
    let transfer_input: Vec<u8> = (0..=255).collect();
    let cases: &[(&[u8], &[u8])] = &[
        (b"", b""),
        (b"-.,.,.", &[0, 255]),
        (b"abc++[>+++<-]>.", b""),
        (b",[->+<]>.", &[0]),
        (b",[->+<]>.", &[255]),
        (b",[->++<]>.", &[129]),
        (b",[->-<]>.", &[2]),
        (b"[->+<]", b""),
        (b"+[->+<]>.", b""),
        (b">+[ - < - > ]<.", b""),
        (b">++[-<+>]<.", b""),
        (b"[-<+>]", b""),
        (b"+[ - < + > ]", b""),
        (b">>>[->+<]", b""),
        (b">>>+[->+<]", b""),
        (b">>>+[->-<]", b""),
        (b">+<,[->+<]>.", &[255]),
        (b">-<,[->-<]>.", &[255]),
        (b"[<]++.", b""),
        (b"+[<]", b""),
        (b"+[>+++<-]>.", b""),
        (b">>><>", b""),
        (b">>>><", b""),
        (b"<<<>>", b""),
        (b">++[<++[>+<-]>-]<.", b""),
        (&transfer_program, &transfer_input),
    ];
    let dir = TestDir::new();
    for target in targets {
        for optimization in ["-O0", "-O2"] {
            for (index, (source, input)) in cases.iter().enumerate() {
                let expected = interpret(source, input, 4);
                let source_path = dir.0.join("program.bf");
                let executable = dir.0.join(format!(
                    "program-{target}-{optimization}-{index}{}",
                    if cfg!(windows) { ".exe" } else { "" }
                ));
                std::fs::write(&source_path, source).unwrap();
                let compiled = Command::new(env!("CARGO_BIN_EXE_bfcompiler"))
                    .args([
                        "--target",
                        target,
                        optimization,
                        "--mode=extended",
                        "--tape-size=4",
                    ])
                    .arg(&source_path)
                    .arg("-o")
                    .arg(&executable)
                    .output()
                    .unwrap();
                assert!(
                    compiled.status.success(),
                    "case {index}: {}",
                    String::from_utf8_lossy(&compiled.stderr)
                );
                let mut child = Command::new(&executable)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::null())
                    .spawn()
                    .unwrap();
                child.stdin.take().unwrap().write_all(input).unwrap();
                let deadline = Instant::now() + Duration::from_secs(5);
                let status = loop {
                    if let Some(status) = child.try_wait().unwrap() {
                        break status;
                    }
                    if Instant::now() >= deadline {
                        child.kill().unwrap();
                        let _ = child.wait();
                        panic!("program timed out: {target} {optimization} case {index}");
                    }
                    std::thread::sleep(Duration::from_millis(5));
                };
                let mut output = Vec::new();
                child
                    .stdout
                    .take()
                    .unwrap()
                    .read_to_end(&mut output)
                    .unwrap();
                assert_eq!(
                    ResultValue {
                        output,
                        status: status.code().unwrap()
                    },
                    expected,
                    "{target} {optimization} case {index}: {:?}",
                    String::from_utf8_lossy(source)
                );
            }
        }
    }
}
