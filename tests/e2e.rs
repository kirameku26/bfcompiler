#![cfg(all(target_os = "macos", target_arch = "aarch64"))]

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

struct TestDir(PathBuf);
static NEXT_DIR: AtomicU64 = AtomicU64::new(0);

impl TestDir {
    fn new() -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let sequence = NEXT_DIR.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "bfcompiler-test-{}-{stamp}-{sequence}",
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

fn compile(source: &[u8], extra: &[&str]) -> (TestDir, PathBuf) {
    let dir = TestDir::new();
    let source_path = dir.0.join("program.bf");
    let executable = dir.0.join("program");
    fs::write(&source_path, source).unwrap();
    let status = Command::new(env!("CARGO_BIN_EXE_bfcompiler"))
        .args(extra)
        .arg(&source_path)
        .arg("-o")
        .arg(&executable)
        .status()
        .unwrap();
    assert!(status.success());
    (dir, executable)
}

#[test]
fn compiles_and_runs_loops_and_byte_output() {
    let (_dir, executable) = compile(b"comment\n++++++++[>+++++++++<-]>.", &[]);
    let output = Command::new(executable).output().unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"H");
}

#[test]
fn eof_sets_the_current_cell_to_zero() {
    for options in [&[][..], &["--mode=standard"][..], &["--mode=extended"][..]] {
        let (_dir, executable) = compile(b"+,.+.", options);
        let output = Command::new(executable)
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout, [0, 1]);
    }
}

#[test]
fn pointer_underflow_is_a_runtime_error_with_or_without_optimization() {
    for options in [&[][..], &["--no-optimize"][..]] {
        let (_dir, executable) = compile(b"<>", options);
        let output = Command::new(executable).output().unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(String::from_utf8_lossy(&output.stderr).contains("tape boundary"));
    }
}

#[test]
fn syntax_error_does_not_replace_an_existing_output() {
    let dir = TestDir::new();
    let source = dir.0.join("bad.bf");
    let output = dir.0.join("keep");
    fs::write(&source, b"[").unwrap();
    fs::write(&output, b"keep me").unwrap();
    let status = Command::new(env!("CARGO_BIN_EXE_bfcompiler"))
        .arg(source)
        .arg("-o")
        .arg(&output)
        .status()
        .unwrap();
    assert!(!status.success());
    assert_eq!(fs::read(output).unwrap(), b"keep me");
}

#[test]
fn assembly_can_be_assembled_and_linked() {
    let dir = TestDir::new();
    let source = dir.0.join("sample.bf");
    let assembly = dir.0.join("sample.s");
    let executable = dir.0.join("sample");
    fs::write(&source, b",[>+<-]>.").unwrap();
    let status = Command::new(env!("CARGO_BIN_EXE_bfcompiler"))
        .args(["-S", "-O2"])
        .arg(&source)
        .arg("-o")
        .arg(&assembly)
        .status()
        .unwrap();
    assert!(status.success());
    let text = fs::read_to_string(&assembly).unwrap();
    assert!(text.contains("bl _bf_input"));
    assert!(text.contains("cbz w8"));
    let status = Command::new("cc")
        .arg(&assembly)
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/runtime/runtime.c"))
        .arg("-o")
        .arg(&executable)
        .status()
        .unwrap();
    assert!(status.success());
    let mut child = Command::new(executable)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    use std::io::Write;
    child.stdin.take().unwrap().write_all(b"A").unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(result.status.success());
    assert_eq!(result.stdout, b"A");
}

#[test]
fn optimization_options_and_invalid_options() {
    let dir = TestDir::new();
    let source = dir.0.join("test.bf");
    fs::write(&source, b"++++.").unwrap();
    let asm0 = dir.0.join("zero.s");
    let asm2 = dir.0.join("two.s");
    for (opt, path) in [("-O0", &asm0), ("-O2", &asm2)] {
        assert!(
            Command::new(env!("CARGO_BIN_EXE_bfcompiler"))
                .args(["-S", opt])
                .arg(&source)
                .arg("-o")
                .arg(path)
                .status()
                .unwrap()
                .success()
        );
    }
    assert!(fs::metadata(asm0).unwrap().len() > fs::metadata(asm2).unwrap().len());
    assert!(
        !Command::new(env!("CARGO_BIN_EXE_bfcompiler"))
            .args(["-O9"])
            .arg(source)
            .output()
            .unwrap()
            .status
            .success()
    );
}

#[test]
fn x86_64_target_runs_and_assembly_round_trips() {
    let dir = TestDir::new();
    let source = dir.0.join("x86.bf");
    fs::write(&source, b",[>+<-]>.").unwrap();
    for assembly in [false, true] {
        let executable = dir.0.join(if assembly { "from-asm" } else { "native" });
        let output = if assembly {
            dir.0.join("x86.s")
        } else {
            executable.clone()
        };
        let mut compiler = Command::new(env!("CARGO_BIN_EXE_bfcompiler"));
        compiler.args(["--target", "x86_64-apple-darwin"]);
        if assembly {
            compiler.arg("-S");
        }
        assert!(
            compiler
                .arg(&source)
                .arg("-o")
                .arg(&output)
                .status()
                .unwrap()
                .success()
        );
        if assembly {
            assert!(
                Command::new("cc")
                    .args(["-arch", "x86_64"])
                    .arg(&output)
                    .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/runtime/runtime.c"))
                    .arg("-o")
                    .arg(&executable)
                    .status()
                    .unwrap()
                    .success()
            );
        }
        let mut child = Command::new(&executable)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        use std::io::Write;
        child.stdin.take().unwrap().write_all(b"A").unwrap();
        let result = child.wait_with_output().unwrap();
        assert!(result.status.success());
        assert_eq!(result.stdout, b"A");
    }
}

#[test]
fn x86_64_checks_boundaries_and_eof() {
    for (source, expected, stdout) in [
        (b"<>".as_slice(), 2, b"".as_slice()),
        (b"+,.".as_slice(), 0, b"\0".as_slice()),
    ] {
        let (_dir, executable) = compile(source, &["--target=x86_64-apple-darwin"]);
        let result = Command::new(executable)
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(expected));
        assert_eq!(result.stdout, stdout);
    }
}

#[test]
fn x86_64_cell_wrap_and_right_boundary_match_optimization_modes() {
    for optimize in ["-O0", "-O2"] {
        let (_dir, executable) = compile(b"-.", &["--target=x86_64-apple-darwin", optimize]);
        let result = Command::new(executable).output().unwrap();
        assert!(result.status.success());
        assert_eq!(result.stdout, [255]);
        let source = vec![b'>'; 30_000];
        let (_dir, executable) = compile(
            &source,
            &[
                "--target=x86_64-apple-darwin",
                optimize,
                "--mode=extended",
                "--tape-size=30000",
            ],
        );
        assert_eq!(
            Command::new(executable).output().unwrap().status.code(),
            Some(2)
        );
    }
}

#[test]
fn output_encoding_preserves_bytes_or_encodes_cell_as_unicode() {
    use std::io::Write;
    for target in ["aarch64-apple-darwin", "x86_64-apple-darwin"] {
        for (encoding, expected) in [
            ("bytes", &[0, 127, 128, 255][..]),
            ("utf-8", &[0, 127, 0xc2, 0x80, 0xc3, 0xbf][..]),
        ] {
            let (_dir, executable) = compile(
                b",.,.,.,.",
                &[
                    "--target",
                    target,
                    "--mode",
                    "extended",
                    "--output-encoding",
                    encoding,
                ],
            );
            let mut child = Command::new(executable)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .spawn()
                .unwrap();
            child
                .stdin
                .take()
                .unwrap()
                .write_all(&[0, 127, 128, 255])
                .unwrap();
            let result = child.wait_with_output().unwrap();
            assert!(result.status.success());
            assert_eq!(result.stdout, expected);
        }
    }
}

#[test]
fn runtime_encodes_full_unicode_and_rejects_invalid_values() {
    let dir = TestDir::new();
    let harness = dir.0.join("unicode.c");
    let executable = dir.0.join("unicode");
    fs::write(
        &harness,
        r#"
#include <stdint.h>
#include <stddef.h>
extern int bf_output(uint32_t);
int bf_program(uint8_t *tape, size_t size) {
    (void)tape; (void)size;
    if (bf_output(0x41) || bf_output(0xe9) || bf_output(0x3042) || bf_output(0x1f600) ||
        bf_output(0xd7ff) || bf_output(0xe000) || bf_output(0x10ffff)) return 1;
    if (bf_output(0xd800) != 2 || bf_output(0xdfff) != 2 || bf_output(0x110000) != 2 ||
        bf_output(UINT32_MAX) != 2) return 1;
    return 0;
}
"#,
    )
    .unwrap();
    assert!(
        Command::new("cc")
            .arg("-DBFC_OUTPUT_UTF8=1")
            .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/runtime/runtime.c"))
            .arg(&harness)
            .arg("-o")
            .arg(&executable)
            .status()
            .unwrap()
            .success()
    );
    let result = Command::new(executable).output().unwrap();
    assert!(result.status.success());
    assert_eq!(result.stdout, "Aéあ😀\u{d7ff}\u{e000}\u{10ffff}".as_bytes());
}

#[test]
fn runtime_reports_invalid_unicode_as_execution_error() {
    let dir = TestDir::new();
    let harness = dir.0.join("invalid_unicode.c");
    let executable = dir.0.join("invalid_unicode");
    fs::write(
        &harness,
        r#"
#include <stdint.h>
#include <stddef.h>
extern int bf_output(uint32_t);
int bf_program(uint8_t *tape, size_t size) {
    (void)tape; (void)size;
    return bf_output(0xd800);
}
"#,
    )
    .unwrap();
    assert!(
        Command::new("cc")
            .arg("-DBFC_OUTPUT_UTF8=1")
            .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/runtime/runtime.c"))
            .arg(&harness)
            .arg("-o")
            .arg(&executable)
            .status()
            .unwrap()
            .success()
    );
    let result = Command::new(executable).output().unwrap();
    assert_eq!(result.status.code(), Some(2));
    assert!(result.stdout.is_empty());
}

#[test]
fn runtime_byte_output_uses_low_eight_bits() {
    let dir = TestDir::new();
    let harness = dir.0.join("bytes.c");
    let executable = dir.0.join("bytes");
    fs::write(
        &harness,
        r#"
#include <stdint.h>
#include <stddef.h>
extern int bf_output(uint32_t);
int bf_program(uint8_t *tape, size_t size) {
    (void)tape; (void)size;
    return bf_output(0x1234) || bf_output(UINT32_MAX);
}
"#,
    )
    .unwrap();
    assert!(
        Command::new("cc")
            .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/runtime/runtime.c"))
            .arg(&harness)
            .arg("-o")
            .arg(&executable)
            .status()
            .unwrap()
            .success()
    );
    let result = Command::new(executable).output().unwrap();
    assert!(result.status.success());
    assert_eq!(result.stdout, [0x34, 0xff]);
}
#[test]
fn tape_size_defaults_and_custom_boundary() {
    let source = vec![b'>'; 30_000];
    let (_dir, executable) = compile(&source[..29_999], &[]);
    assert!(Command::new(executable).status().unwrap().success());
    for options in [&[][..], &["--mode=standard"][..]] {
        let (_dir, executable) = compile(&source, options);
        assert_eq!(
            Command::new(executable).output().unwrap().status.code(),
            Some(2)
        );
    }
    let (_dir, executable) = compile(&source, &["--mode=extended"]);
    assert!(Command::new(executable).status().unwrap().success());
    for (source, expected) in [
        (b">".as_slice(), 2),
        (b"+<".as_slice(), 2),
        (b"+.".as_slice(), 0),
    ] {
        let (_dir, executable) = compile(source, &["--mode=extended", "--tape-size", "1"]);
        assert_eq!(
            Command::new(executable).output().unwrap().status.code(),
            Some(expected)
        );
    }
}

#[test]
fn manually_linked_standard_object_uses_standard_tape_size() {
    let dir = TestDir::new();
    let source = dir.0.join("boundary.bf");
    let object = dir.0.join("boundary.o");
    fs::write(&source, vec![b'>'; 30_000]).unwrap();
    assert!(
        Command::new(env!("CARGO_BIN_EXE_bfcompiler"))
            .args(["--mode", "standard", "-c"])
            .arg(&source)
            .arg("-o")
            .arg(&object)
            .status()
            .unwrap()
            .success()
    );
    for (extra, expected) in [(&[][..], 2), (&["-DBFC_TAPE_SIZE=1000000"][..], 0)] {
        let executable = dir.0.join(if extra.is_empty() {
            "standard"
        } else {
            "extended"
        });
        assert!(
            Command::new("cc")
                .args(extra)
                .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/runtime/runtime.c"))
                .arg(&object)
                .arg("-o")
                .arg(&executable)
                .status()
                .unwrap()
                .success()
        );
        assert_eq!(
            Command::new(executable).output().unwrap().status.code(),
            Some(expected)
        );
    }
}

#[test]
fn extended_object_and_assembly_link_with_runtime_settings() {
    let dir = TestDir::new();
    let source = dir.0.join("extended.bf");
    fs::write(&source, b">-.").unwrap();
    for format in ["-c", "-S"] {
        let output = dir.0.join(if format == "-c" {
            "program.o"
        } else {
            "program.s"
        });
        let executable = dir
            .0
            .join(if format == "-c" { "object" } else { "assembly" });
        let result = Command::new(env!("CARGO_BIN_EXE_bfcompiler"))
            .args([
                "--mode=extended",
                "--tape-size=2",
                "--output-encoding=utf-8",
                format,
            ])
            .arg(&source)
            .arg("-o")
            .arg(&output)
            .output()
            .unwrap();
        assert!(result.status.success());
        assert!(result.stderr.is_empty());
        assert!(
            Command::new("cc")
                .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/runtime/runtime.c"))
                .arg(&output)
                .arg("-o")
                .arg(&executable)
                .status()
                .unwrap()
                .success()
        );
        let result = Command::new(executable).output().unwrap();
        assert!(result.status.success());
        assert_eq!(result.stdout, [0xc3, 0xbf]);
    }
}

#[test]
fn invalid_runtime_options_are_rejected() {
    let dir = TestDir::new();
    let source = dir.0.join("test.bf");
    fs::write(&source, b".").unwrap();
    for args in [
        &["--mode=extended", "--tape-size=0"][..],
        &["--mode=extended", "--tape-size=-1"][..],
        &["--mode=extended", "--tape-size=2147483648"][..],
        &["--mode=extended", "--output-encoding=bad"][..],
        &["--mode=standard", "--output-encoding=utf8"][..],
        &["--tape-size=30000"][..],
        &["--output-encoding=bytes"][..],
        &["--mode=unknown"][..],
    ] {
        assert!(
            !Command::new(env!("CARGO_BIN_EXE_bfcompiler"))
                .args(args)
                .arg(&source)
                .output()
                .unwrap()
                .status
                .success()
        );
    }
}

#[test]
fn clear_loops_match_unoptimized_programs() {
    use std::io::Write;
    for target in ["aarch64-apple-darwin", "x86_64-apple-darwin"] {
        for optimize in ["-O0", "-O2"] {
            for value in [0, 1, 127, 255] {
                let (_dir, executable) = compile(b",[-].,[+].", &["--target", target, optimize]);
                let mut child = Command::new(executable)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .spawn()
                    .unwrap();
                child
                    .stdin
                    .take()
                    .unwrap()
                    .write_all(&[value, value])
                    .unwrap();
                let output = child.wait_with_output().unwrap();
                assert!(output.status.success());
                assert_eq!(output.stdout, [0, 0]);
            }
        }
    }
}

#[test]
fn copied_compiler_runs_from_another_directory() {
    let dir = TestDir::new();
    let compiler = dir.0.join("bfcompiler");
    fs::copy(env!("CARGO_BIN_EXE_bfcompiler"), &compiler).unwrap();
    fs::write(
        dir.0.join("test.bf"),
        b"+++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++++.",
    )
    .unwrap();
    let result = Command::new(&compiler)
        .current_dir(&dir.0)
        .args(["test.bf", "-o", "test"])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let output = Command::new(dir.0.join("test")).output().unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"A");
}

#[test]
fn assembly_round_trip_handles_optimized_transfers_and_branches() {
    use std::io::Write;
    let dir = TestDir::new();
    let source = dir.0.join("transfer.bf");
    fs::write(&source, b",[->+<]>. ").unwrap();
    for (target, arch) in [
        ("aarch64-apple-darwin", "arm64"),
        ("x86_64-apple-darwin", "x86_64"),
    ] {
        for opt in ["-O0", "-O2"] {
            let asm = dir.0.join(format!("{arch}-{opt}.s"));
            let executable = dir.0.join(format!("{arch}-{opt}"));
            assert!(
                Command::new(env!("CARGO_BIN_EXE_bfcompiler"))
                    .args(["--target", target, "-S", opt])
                    .arg(&source)
                    .arg("-o")
                    .arg(&asm)
                    .status()
                    .unwrap()
                    .success()
            );
            let rendered = fs::read_to_string(&asm).unwrap();
            assert!(!rendered.contains(if arch == "arm64" { ".long" } else { ".byte" }));
            assert!(
                Command::new("cc")
                    .args(["-arch", arch])
                    .arg(&asm)
                    .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/runtime/runtime.c"))
                    .arg("-o")
                    .arg(&executable)
                    .status()
                    .unwrap()
                    .success()
            );
            for value in [0, 255] {
                let mut child = Command::new(&executable)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .spawn()
                    .unwrap();
                child.stdin.take().unwrap().write_all(&[value]).unwrap();
                let result = child.wait_with_output().unwrap();
                assert!(result.status.success());
                assert_eq!(result.stdout, [value]);
            }
        }
    }
}

#[test]
fn grouped_moves_preserve_boundaries_around_aarch64_immediate_limit() {
    for target in ["aarch64-apple-darwin", "x86_64-apple-darwin"] {
        for (distance, tape, expected) in [
            (4094, 4095, 0),
            (4095, 4095, 2),
            (4095, 4096, 0),
            (4096, 4096, 2),
            (4096, 4097, 0),
        ] {
            let mut source = vec![b'>'; distance];
            source.extend(vec![b'<'; distance]);
            let (_dir, executable) = compile(
                &source,
                &[
                    "--target",
                    target,
                    "--mode=extended",
                    "--tape-size",
                    &tape.to_string(),
                ],
            );
            assert_eq!(
                Command::new(executable).output().unwrap().status.code(),
                Some(expected),
                "{target}: distance {distance}, tape {tape}"
            );
        }
    }
}

#[test]
fn linux_and_windows_assembly_can_be_cross_assembled() {
    let dir = TestDir::new();
    let source = dir.0.join("cross.bf");
    fs::write(&source, b",[->+<]>>[-<->]<<[>+<-]>.").unwrap();
    for target in [
        "aarch64-unknown-linux-gnu",
        "x86_64-unknown-linux-gnu",
        "x86_64-pc-windows-gnu",
    ] {
        for opt in ["-O0", "-O2"] {
            let assembly = dir.0.join(format!("{target}-{opt}.s"));
            let object = dir.0.join(format!("{target}-{opt}.o"));
            let compiled = Command::new(env!("CARGO_BIN_EXE_bfcompiler"))
                .args(["--target", target, "-S", opt])
                .arg(&source)
                .arg("-o")
                .arg(&assembly)
                .output()
                .unwrap();
            assert!(
                compiled.status.success(),
                "{}",
                String::from_utf8_lossy(&compiled.stderr)
            );
            let assembled = Command::new("clang")
                .args(["-target", target, "-c"])
                .arg(&assembly)
                .arg("-o")
                .arg(&object)
                .output()
                .unwrap();
            assert!(
                assembled.status.success(),
                "{target} {opt}: {}",
                String::from_utf8_lossy(&assembled.stderr)
            );
        }
    }
}

#[test]
fn even_step_loop_remains_nonterminating() {
    use std::time::{Duration, Instant};
    for target in ["aarch64-apple-darwin", "x86_64-apple-darwin"] {
        for opt in ["-O0", "-O2"] {
            let (_dir, executable) = compile(b"+[--]", &["--target", target, opt]);
            let mut child = Command::new(executable)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap();
            let deadline = Instant::now() + Duration::from_millis(750);
            while Instant::now() < deadline {
                if let Some(status) = child.try_wait().unwrap() {
                    panic!("{target} {opt}: nonterminating loop exited with {status}");
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            child.kill().unwrap();
            child.wait().unwrap();
        }
    }
}

#[test]
fn transfer_boundary_failure_preserves_preceding_cell_decrement() {
    let dir = TestDir::new();
    let harness = dir.0.join("harness.c");
    fs::write(&harness, include_str!("transfer_error_harness.c")).unwrap();
    for (target, arch) in [
        (bfcompiler::Target::Aarch64Darwin, "arm64"),
        (bfcompiler::Target::X86_64Darwin, "x86_64"),
    ] {
        for optimize in [false, true] {
            for (index, source) in [b"+[-<+>]".as_slice(), b"+[->+<]".as_slice()]
                .iter()
                .enumerate()
            {
                let object = dir.0.join(format!("{arch}-{optimize}-{index}.o"));
                let executable = dir.0.join(format!("{arch}-{optimize}-{index}"));
                fs::write(
                    &object,
                    bfcompiler::compile_object_for(source, optimize, target).unwrap(),
                )
                .unwrap();
                let linked = Command::new("cc")
                    .args(["-arch", arch])
                    .arg(&harness)
                    .arg(&object)
                    .arg("-o")
                    .arg(&executable)
                    .output()
                    .unwrap();
                assert!(
                    linked.status.success(),
                    "{}",
                    String::from_utf8_lossy(&linked.stderr)
                );
                assert!(
                    Command::new(executable).status().unwrap().success(),
                    "{arch} optimized={optimize} case={index}"
                );
            }
        }
    }
}
