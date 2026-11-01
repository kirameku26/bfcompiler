//! End-to-end Linux cross-linking and execution under QEMU user mode.
//! Set BFC_TEST_LINUX_CROSS to `i686` or `riscv64` to enable one target.

use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, ExitStatus, Stdio},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

struct TestDir(PathBuf);
impl TestDir {
    fn new() -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "bfcompiler-linux-cross-{}-{stamp}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct CrossTarget {
    triple: &'static str,
    compiler: &'static str,
    emulator: &'static str,
    sysroot: &'static str,
}

fn selected_target() -> Option<CrossTarget> {
    let value = std::env::var_os("BFC_TEST_LINUX_CROSS")?;
    let target = match value.to_str() {
        Some("i686") => CrossTarget {
            triple: "i686-unknown-linux-gnu",
            compiler: "i686-linux-gnu-gcc",
            emulator: "qemu-i386",
            sysroot: "/usr/i686-linux-gnu",
        },
        Some("riscv64") => CrossTarget {
            triple: "riscv64gc-unknown-linux-gnu",
            compiler: "riscv64-linux-gnu-gcc",
            emulator: "qemu-riscv64",
            sysroot: "/usr/riscv64-linux-gnu",
        },
        _ => panic!("BFC_TEST_LINUX_CROSS must be i686 or riscv64"),
    };
    assert!(
        Path::new(target.sysroot).is_dir(),
        "cross sysroot {} is missing",
        target.sysroot
    );
    Some(target)
}

struct Case {
    name: &'static str,
    source: Vec<u8>,
    input: &'static [u8],
    options: &'static [&'static str],
    output: &'static [u8],
    status: i32,
}

fn compile(target: &CrossTarget, dir: &Path, case: &Case, optimize: &str, format: &str) -> PathBuf {
    let source = dir.join("program.bf");
    let executable = dir.join("program");
    let artifact = dir.join(if format == "-S" {
        "program.s"
    } else {
        "program.o"
    });
    fs::write(&source, &case.source).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_bfcompiler"));
    command
        .env("BFC_CC", target.compiler)
        .args(["--target", target.triple, optimize])
        .args(case.options);
    if format != "exe" {
        command.arg(format);
    }
    let result = command
        .arg(&source)
        .arg("-o")
        .arg(if format == "exe" {
            &executable
        } else {
            &artifact
        })
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{} {} {} {}: {}",
        target.triple,
        case.name,
        optimize,
        format,
        String::from_utf8_lossy(&result.stderr)
    );
    if format != "exe" {
        let result = Command::new(target.compiler)
            .arg("-O2")
            .arg(&artifact)
            .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/runtime/runtime.c"))
            .arg("-o")
            .arg(&executable)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{} {} {} {} manual link: {}",
            target.triple,
            case.name,
            optimize,
            format,
            String::from_utf8_lossy(&result.stderr)
        );
    }
    executable
}

fn run(
    target: &CrossTarget,
    executable: &Path,
    input: &[u8],
    timeout: Duration,
) -> Option<(ExitStatus, Vec<u8>, Vec<u8>)> {
    let mut child = Command::new(target.emulator)
        .args(["-L", target.sysroot])
        .arg(executable)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            let mut output = Vec::new();
            let mut stderr = Vec::new();
            child
                .stdout
                .take()
                .unwrap()
                .read_to_end(&mut output)
                .unwrap();
            child
                .stderr
                .take()
                .unwrap()
                .read_to_end(&mut stderr)
                .unwrap();
            return Some((status, output, stderr));
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            let _ = child.wait();
            return None;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn linux_cross_compiled_programs_run_under_qemu() {
    let Some(target) = selected_target() else {
        eprintln!("BFC_TEST_LINUX_CROSS is not set; Linux cross tests skipped");
        return;
    };
    let mut transfer = vec![b'+'; 256];
    transfer.extend_from_slice(b"[->+<]>.");
    let mut supplementary = vec![b'+'; 128];
    supplementary.extend_from_slice(b"[>");
    supplementary.extend(vec![b'+'; 1004]);
    supplementary.extend_from_slice(b"<-]>.");
    let cases = [
        Case {
            name: "standard_input_eof",
            source: b",.,.,.".to_vec(),
            input: &[0, 255],
            options: &[],
            output: &[0, 255, 0],
            status: 0,
        },
        Case {
            name: "standard_transfer",
            source: b",[->+<]>.".to_vec(),
            input: &[17],
            options: &[],
            output: &[17],
            status: 0,
        },
        Case {
            name: "standard_wraparound",
            source: b"-+.".to_vec(),
            input: &[],
            options: &[],
            output: &[0],
            status: 0,
        },
        Case {
            name: "wide_transfer_utf8",
            source: transfer,
            input: &[],
            options: &[
                "--mode=extended",
                "--cell-bits=16",
                "--tape-size=2",
                "--output-encoding=utf-8",
            ],
            output: &[0xc4, 0x80],
            status: 0,
        },
        Case {
            name: "supplementary_utf16be",
            source: supplementary.clone(),
            input: &[],
            options: &[
                "--mode=extended",
                "--cell-bits=32",
                "--tape-size=2",
                "--output-encoding=utf-16be",
            ],
            output: &[0xd8, 0x3d, 0xde, 0x00],
            status: 0,
        },
        Case {
            name: "supplementary_utf32le",
            source: supplementary,
            input: &[],
            options: &[
                "--mode=extended",
                "--cell-bits=32",
                "--tape-size=2",
                "--output-encoding=utf-32le",
            ],
            output: &[0x00, 0xf6, 0x01, 0x00],
            status: 0,
        },
        Case {
            name: "left_boundary",
            source: b"<".to_vec(),
            input: &[],
            options: &["--mode=extended", "--cell-bits=32", "--tape-size=1"],
            output: &[],
            status: 2,
        },
        Case {
            name: "right_boundary",
            source: b">".to_vec(),
            input: &[],
            options: &["--mode=extended", "--cell-bits=16", "--tape-size=1"],
            output: &[],
            status: 2,
        },
        Case {
            name: "zero_skips_boundary",
            source: b"[->+<]".to_vec(),
            input: &[],
            options: &["--mode=extended", "--cell-bits=32", "--tape-size=1"],
            output: &[],
            status: 0,
        },
        Case {
            name: "invalid_unicode",
            source: b"-.".to_vec(),
            input: &[],
            options: &[
                "--mode=extended",
                "--cell-bits=32",
                "--output-encoding=utf-8",
            ],
            output: &[],
            status: 2,
        },
    ];
    let dir = TestDir::new();
    for case in &cases {
        for optimize in ["-O0", "-O2"] {
            for format in ["exe", "-c", "-S"] {
                let executable = compile(&target, &dir.0, case, optimize, format);
                let (status, output, stderr) =
                    run(&target, &executable, case.input, Duration::from_secs(10)).unwrap_or_else(
                        || {
                            panic!(
                                "{} {} {} {} timed out",
                                target.triple, case.name, optimize, format
                            )
                        },
                    );
                assert_eq!(
                    status.code(),
                    Some(case.status),
                    "{} {} {} {}: {}",
                    target.triple,
                    case.name,
                    optimize,
                    format,
                    String::from_utf8_lossy(&stderr)
                );
                assert_eq!(
                    output, case.output,
                    "{} {} {} {}",
                    target.triple, case.name, optimize, format
                );
            }
        }
    }
    let nonterminating = Case {
        name: "nonterminating_even_step",
        source: b"+[--]".to_vec(),
        input: &[],
        options: &[],
        output: &[],
        status: 0,
    };
    for optimize in ["-O0", "-O2"] {
        let executable = compile(&target, &dir.0, &nonterminating, optimize, "exe");
        assert!(
            run(&target, &executable, &[], Duration::from_millis(500)).is_none(),
            "{} {optimize}: [--] unexpectedly terminated",
            target.triple
        );
    }
}
