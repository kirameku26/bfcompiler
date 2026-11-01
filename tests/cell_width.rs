use bfcompiler::{CellWidth, Target};
use std::{
    fs,
    path::PathBuf,
    process::{Command, Stdio},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

struct TestDir(PathBuf);
impl TestDir {
    fn new() -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("bfcompiler-width-{}-{stamp}", std::process::id()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn standard_objects_keep_legacy_runtime_defaults_when_manually_linked() {
    if Target::host_default().is_none() {
        return;
    }
    let dir = TestDir::new();
    let input = dir.0.join("source.bf");
    fs::write(&input, b"-.>").unwrap();
    let runtime = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("runtime/runtime.c");
    for format in ["-c", "-S"] {
        let object = dir.0.join(if format == "-c" {
            "source.o"
        } else {
            "source.s"
        });
        let compile = Command::new(env!("CARGO_BIN_EXE_bfcompiler"))
            .arg(format)
            .arg(&input)
            .arg("-o")
            .arg(&object)
            .output()
            .unwrap();
        assert!(compile.status.success());
        for optimize in ["-O0", "-O2", "-O3"] {
            for runtime_first in [false, true] {
                let executable = dir.0.join(if cfg!(windows) { "test.exe" } else { "test" });
                let inputs = if runtime_first {
                    [&runtime, &object]
                } else {
                    [&object, &runtime]
                };
                let link = Command::new(std::env::var_os("BFC_CC").unwrap_or_else(|| "cc".into()))
                    .args([optimize, "-DBFC_TAPE_SIZE=1", "-DBFC_OUTPUT_UTF8=1"])
                    .args(inputs)
                    .arg("-o")
                    .arg(&executable)
                    .output()
                    .unwrap();
                assert!(
                    link.status.success(),
                    "{}",
                    String::from_utf8_lossy(&link.stderr)
                );
                let output = Command::new(&executable).output().unwrap();
                assert_eq!(
                    output.status.code(),
                    Some(2),
                    "{format} {optimize} runtime_first={runtime_first}: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
                assert_eq!(output.stdout, [0xc3, 0xbf]);
            }
        }
    }
}

#[test]
fn extended_widths_run_and_survive_manual_linking() {
    let Some(host) = Target::host_default() else {
        return;
    };
    let mut targets = vec![host];
    if host == Target::Aarch64Darwin
        && Command::new("arch")
            .args(["-x86_64", "/usr/bin/true"])
            .status()
            .is_ok_and(|s| s.success())
    {
        targets.push(Target::X86_64Darwin);
    }
    let mut unicode = vec![b'+'; 256];
    unicode.extend_from_slice(b"[->+<]>.");
    let mut japanese = vec![b'+'; 0x3042];
    japanese.push(b'.');
    let cases: Vec<(&[u8], &str, &[u8], i32)> = vec![
        (&unicode, "utf-8", &[0xc4, 0x80], 0),
        (&japanese, "utf-8", "あ".as_bytes(), 0),
        (b"-.+.", "bytes", &[0xff, 0], 0),
        (b"-,.", "bytes", &[0], 0),
        (b"+[->+<]", "bytes", &[], 2),
        (b"[->+<]", "bytes", &[], 0),
    ];
    for target in targets {
        let triple = match target {
            Target::Aarch64Darwin => "aarch64-apple-darwin",
            Target::X86_64Darwin => "x86_64-apple-darwin",
            Target::Aarch64Linux => "aarch64-unknown-linux-gnu",
            Target::X86_64Linux => "x86_64-unknown-linux-gnu",
            Target::I386Linux => "i686-unknown-linux-gnu",
            Target::Riscv64Linux => "riscv64gc-unknown-linux-gnu",
            Target::X86_64WindowsGnu => "x86_64-pc-windows-gnu",
        };
        for bits in ["16", "32"] {
            for optimize in ["-O0", "-O2"] {
                for format in ["exe", "-c", "-S"] {
                    for (index, (source, encoding, expected, status)) in cases.iter().enumerate() {
                        let dir = TestDir::new();
                        let input = dir.0.join("source.bf");
                        let object = dir.0.join(if format == "-S" {
                            "source.s"
                        } else {
                            "source.o"
                        });
                        let executable = dir.0.join(if cfg!(windows) {
                            "program.exe"
                        } else {
                            "program"
                        });
                        fs::write(&input, source).unwrap();
                        let mut compiler = Command::new(env!("CARGO_BIN_EXE_bfcompiler"));
                        compiler.args([
                            "--target",
                            triple,
                            "--mode=extended",
                            "--cell-bits",
                            bits,
                            "--output-encoding",
                            encoding,
                            optimize,
                            "--tape-size",
                            if index >= 4 { "1" } else { "2" },
                        ]);
                        if format != "exe" {
                            compiler.arg(format);
                        }
                        let output = compiler
                            .arg(&input)
                            .arg("-o")
                            .arg(if format == "exe" {
                                &executable
                            } else {
                                &object
                            })
                            .output()
                            .unwrap();
                        assert!(
                            output.status.success(),
                            "{triple} {bits} {optimize} {format}: {}",
                            String::from_utf8_lossy(&output.stderr)
                        );
                        if format != "exe" {
                            let mut linker = Command::new(
                                std::env::var_os("BFC_CC").unwrap_or_else(|| "cc".into()),
                            );
                            linker.arg("-O2");
                            if matches!(target, Target::Aarch64Darwin | Target::X86_64Darwin) {
                                linker.args([
                                    "-arch",
                                    if target == Target::Aarch64Darwin {
                                        "arm64"
                                    } else {
                                        "x86_64"
                                    },
                                ]);
                            }
                            let output = linker
                                .arg(&object)
                                .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/runtime/runtime.c"))
                                .arg("-o")
                                .arg(&executable)
                                .output()
                                .unwrap();
                            assert!(
                                output.status.success(),
                                "{}",
                                String::from_utf8_lossy(&output.stderr)
                            );
                        }
                        let mut child = Command::new(&executable)
                            .stdin(Stdio::null())
                            .stdout(Stdio::piped())
                            .stderr(Stdio::piped())
                            .spawn()
                            .unwrap();
                        let deadline = Instant::now() + Duration::from_secs(5);
                        while child.try_wait().unwrap().is_none() {
                            if Instant::now() > deadline {
                                child.kill().unwrap();
                                let _ = child.wait();
                                panic!(
                                    "execution timed out: {triple} {bits} {optimize} {format} case {index}"
                                );
                            }
                            std::thread::sleep(Duration::from_millis(5));
                        }
                        let output = child.wait_with_output().unwrap();
                        assert_eq!(
                            output.status.code(),
                            Some(*status),
                            "{triple} {bits} {optimize} {format} case {index}: {}",
                            String::from_utf8_lossy(&output.stderr)
                        );
                        assert_eq!(
                            output.stdout, *expected,
                            "{triple} {bits} {optimize} {format} case {index}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn cell_width_validation() {
    for (bits, valid) in [
        ("8", true),
        ("16", true),
        ("32", true),
        ("0", false),
        ("64", false),
        ("abc", false),
    ] {
        let dir = TestDir::new();
        let input = dir.0.join("empty.bf");
        fs::write(&input, b"").unwrap();
        let result = Command::new(env!("CARGO_BIN_EXE_bfcompiler"))
            .args(["--mode=extended", "--cell-bits", bits, "-c"])
            .arg(&input)
            .arg("-o")
            .arg(dir.0.join("test.o"))
            .output()
            .unwrap();
        assert_eq!(result.status.success(), valid);
        let result = Command::new(env!("CARGO_BIN_EXE_bfcompiler"))
            .args(["--cell-bits", bits, "-c"])
            .arg(&input)
            .output()
            .unwrap();
        assert!(!result.status.success());
    }
    let result = bfcompiler::compile_machine_with_config(
        b"",
        true,
        Target::I386Linux,
        bfcompiler::RuntimeConfig {
            tape_size: i32::MAX as u32,
            cell_width: CellWidth::Bits32,
            output_utf8: false,
        },
    );
    assert!(result.is_err());
}

#[test]
fn wide_unicode_wraparound_and_invalid_scalars_execute() {
    for bits in ["16", "32"] {
        for optimize in ["-O0", "-O2"] {
            for value in [0x10000u32, 0x1f600, 0xd800] {
                let dir = TestDir::new();
                let input = dir.0.join("unicode.bf");
                let executable = dir.0.join(if cfg!(windows) {
                    "unicode.exe"
                } else {
                    "unicode"
                });
                let mut source = vec![b'+'; value as usize];
                source.push(b'.');
                fs::write(&input, source).unwrap();
                let compile = Command::new(env!("CARGO_BIN_EXE_bfcompiler"))
                    .args([
                        "--mode=extended",
                        "--tape-size=1",
                        "--cell-bits",
                        bits,
                        "--output-encoding=utf-8",
                        optimize,
                    ])
                    .arg(&input)
                    .arg("-o")
                    .arg(&executable)
                    .output()
                    .unwrap();
                assert!(
                    compile.status.success(),
                    "{}",
                    String::from_utf8_lossy(&compile.stderr)
                );
                let output = Command::new(executable).output().unwrap();
                let scalar = if bits == "16" { value & 0xffff } else { value };
                if let Some(ch) = char::from_u32(scalar) {
                    assert!(output.status.success());
                    assert_eq!(output.stdout, ch.to_string().as_bytes());
                } else {
                    assert_eq!(output.status.code(), Some(2));
                    assert!(output.stdout.is_empty());
                }
            }
        }
    }
    let dir = TestDir::new();
    let input = dir.0.join("invalid.bf");
    fs::write(&input, b"-.").unwrap();
    let executable = dir.0.join(if cfg!(windows) {
        "invalid.exe"
    } else {
        "invalid"
    });
    let result = Command::new(env!("CARGO_BIN_EXE_bfcompiler"))
        .args([
            "--mode=extended",
            "--cell-bits=32",
            "--output-encoding=utf-8",
        ])
        .arg(input)
        .arg("-o")
        .arg(&executable)
        .output()
        .unwrap();
    assert!(result.status.success());
    let result = Command::new(executable).output().unwrap();
    assert_eq!(result.status.code(), Some(2));
    assert!(result.stdout.is_empty());
}
