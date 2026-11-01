use bfcompiler::{CellWidth, OutputEncoding, RuntimeConfig, Target};
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
        let path = std::env::temp_dir().join(format!(
            "bfcompiler-encoding-{}-{stamp}",
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

fn expected(scalars: &[u32], encoding: OutputEncoding) -> Vec<u8> {
    let mut bytes = vec![];
    for &scalar in scalars {
        let ch = char::from_u32(scalar).unwrap();
        match encoding {
            OutputEncoding::Bytes => bytes.push(scalar as u8),
            OutputEncoding::Utf8 => bytes.extend(ch.encode_utf8(&mut [0; 4]).as_bytes()),
            OutputEncoding::Utf16Le | OutputEncoding::Utf16Be => {
                for &unit in ch.encode_utf16(&mut [0; 2]).iter() {
                    bytes.extend(if encoding == OutputEncoding::Utf16Le {
                        unit.to_le_bytes()
                    } else {
                        unit.to_be_bytes()
                    });
                }
            }
            OutputEncoding::Utf32Le | OutputEncoding::Utf32Be => {
                bytes.extend(if encoding == OutputEncoding::Utf32Le {
                    scalar.to_le_bytes()
                } else {
                    scalar.to_be_bytes()
                });
            }
        }
    }
    bytes
}

fn encodings() -> [(OutputEncoding, &'static str); 4] {
    [
        (OutputEncoding::Utf16Le, "utf-16le"),
        (OutputEncoding::Utf16Be, "utf-16be"),
        (OutputEncoding::Utf32Le, "utf-32le"),
        (OutputEncoding::Utf32Be, "utf-32be"),
    ]
}

#[test]
fn utf16_and_utf32_execute_and_retain_settings_when_linked_manually() {
    for bits in ["8", "16", "32"] {
        let scalars: &[u32] = if bits == "8" {
            &[0, 65, 233]
        } else {
            &[0, 65, 233, 0x3042]
        };
        let mut source = vec![];
        for &value in scalars {
            source.extend(vec![b'+'; value as usize]);
            source.extend_from_slice(b".[-]");
        }
        let mut scalars = scalars.to_vec();
        if bits == "32" {
            // 128 * 1004 = U+1F600. Keep the source small for every backend.
            source.extend(vec![b'+'; 128]);
            source.extend_from_slice(b"[>");
            source.extend(vec![b'+'; 1004]);
            source.extend_from_slice(b"<-]>.");
            scalars.push(0x1f600);
        }
        for (encoding, name) in encodings() {
            for format in ["exe", "-c", "-S"] {
                for optimize in ["-O0", "-O2"] {
                    let dir = TestDir::new();
                    let input = dir.0.join("source.bf");
                    fs::write(&input, &source).unwrap();
                    let object = dir.0.join(if format == "-S" {
                        "program.s"
                    } else {
                        "program.o"
                    });
                    let executable = dir.0.join(if cfg!(windows) {
                        "program.exe"
                    } else {
                        "program"
                    });
                    let mut compiler = Command::new(env!("CARGO_BIN_EXE_bfcompiler"));
                    compiler.args([
                        "--mode=extended",
                        "--cell-bits",
                        bits,
                        "--tape-size=2",
                        "--output-encoding",
                        name,
                        optimize,
                    ]);
                    if format != "exe" {
                        compiler.arg(format);
                    }
                    let result = compiler
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
                        result.status.success(),
                        "{bits} {name} {format} {optimize}: {}",
                        String::from_utf8_lossy(&result.stderr)
                    );
                    if format != "exe" {
                        let result =
                            Command::new(std::env::var_os("BFC_CC").unwrap_or_else(|| "cc".into()))
                                .arg("-O2")
                                .arg(&object)
                                .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/runtime/runtime.c"))
                                .arg("-o")
                                .arg(&executable)
                                .output()
                                .unwrap();
                        assert!(
                            result.status.success(),
                            "{}",
                            String::from_utf8_lossy(&result.stderr)
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
                            panic!("{bits} {name} {format} {optimize}: timed out");
                        }
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    let result = child.wait_with_output().unwrap();
                    assert!(
                        result.status.success(),
                        "{}",
                        String::from_utf8_lossy(&result.stderr)
                    );
                    assert_eq!(
                        result.stdout,
                        expected(&scalars, encoding),
                        "{bits} {name} {format} {optimize}"
                    );
                }
            }
        }
    }
}

#[test]
fn encodings_are_serialized_for_every_target_and_keep_legacy_configs() {
    for target in [
        Target::Aarch64Darwin,
        Target::X86_64Darwin,
        Target::Aarch64Linux,
        Target::X86_64Linux,
        Target::I386Linux,
        Target::Riscv64Linux,
        Target::X86_64WindowsGnu,
    ] {
        let config = RuntimeConfig {
            tape_size: 17,
            cell_width: CellWidth::Bits32,
            output_utf8: true,
        };
        let legacy = bfcompiler::compile_machine_with_config(b".", true, target, config).unwrap();
        assert_eq!(legacy.config_words(), Some([1, 17, 32, 1]));
        for (encoding, _) in encodings() {
            let code =
                bfcompiler::compile_machine_with_encoding(b".", true, target, config, encoding)
                    .unwrap();
            assert_eq!(code.config_words(), Some([2, 17, 32, encoding as u32]));
            let words = [2u32, 17, 32, encoding as u32]
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>();
            let object =
                bfcompiler::compile_object_with_encoding(b".", true, target, config, encoding)
                    .unwrap();
            assert!(object.windows(words.len()).any(|bytes| bytes == words));
            let assembly = bfcompiler::backend::assembly::render_configured(&code, target);
            assert!(assembly.contains(&format!(".long 2, 17, 32, {}", encoding as u32)));
        }
    }
}

#[test]
fn encoding_aliases_and_standard_mode_validation() {
    for (name, encoding) in [
        ("utf-16", OutputEncoding::Utf16Le),
        ("utf16", OutputEncoding::Utf16Le),
        ("utf16le", OutputEncoding::Utf16Le),
        ("utf16be", OutputEncoding::Utf16Be),
        ("utf-32", OutputEncoding::Utf32Le),
        ("utf32", OutputEncoding::Utf32Le),
        ("utf32le", OutputEncoding::Utf32Le),
        ("utf32be", OutputEncoding::Utf32Be),
    ] {
        assert_eq!(OutputEncoding::parse(name), Some(encoding));
        let dir = TestDir::new();
        let input = dir.0.join("empty.bf");
        fs::write(&input, b"").unwrap();
        let executable = dir
            .0
            .join(if cfg!(windows) { "empty.exe" } else { "empty" });
        let result = Command::new(env!("CARGO_BIN_EXE_bfcompiler"))
            .args(["--mode=extended", "--output-encoding", name])
            .arg(&input)
            .arg("-o")
            .arg(&executable)
            .output()
            .unwrap();
        assert!(result.status.success());
        let result = Command::new(executable).output().unwrap();
        assert!(result.status.success());
        assert!(result.stdout.is_empty());
        let result = Command::new(env!("CARGO_BIN_EXE_bfcompiler"))
            .args(["--output-encoding", name, "-c"])
            .arg(&input)
            .output()
            .unwrap();
        assert!(!result.status.success());
    }
    assert_eq!(OutputEncoding::parse("utf-64"), None);
}

#[test]
fn unicode_boundaries_and_invalid_scalars_in_all_encodings() {
    let scalars = [
        0, 0x7f, 0x80, 0x7ff, 0x800, 0xd7ff, 0xe000, 0xfeff, 0xffff, 0x10000, 0x1f600, 0x10ffff,
    ];
    for (encoding, name) in encodings() {
        let dir = TestDir::new();
        let harness = dir.0.join("harness.c");
        let executable = dir.0.join(if cfg!(windows) {
            "harness.exe"
        } else {
            "harness"
        });
        let values = scalars
            .iter()
            .map(|v| format!("0x{v:x}"))
            .collect::<Vec<_>>()
            .join(",");
        fs::write(
            &harness,
            format!(
                r#"
#include <stdint.h>
#include <stddef.h>
#include <stdlib.h>
extern int bf_output(uint32_t);
int bf_program(uint8_t *tape, size_t size) {{
    (void)tape; (void)size;
    const char *invalid = getenv("BF_TEST_INVALID_SCALAR");
    if (invalid) return bf_output((uint32_t)strtoul(invalid, 0, 16));
    uint32_t values[] = {{{values}}};
    for (size_t i=0; i<sizeof(values)/sizeof(values[0]); ++i) if (bf_output(values[i])) return 2;
    return 0;
}}
"#
            ),
        )
        .unwrap();
        let result = Command::new(std::env::var_os("BFC_CC").unwrap_or_else(|| "cc".into()))
            .arg(format!("-DBFC_OUTPUT_ENCODING={}", encoding as u32))
            .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/runtime/runtime.c"))
            .arg(&harness)
            .arg("-o")
            .arg(&executable)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let result = Command::new(&executable)
            .env_remove("BF_TEST_INVALID_SCALAR")
            .output()
            .unwrap();
        assert!(result.status.success());
        assert_eq!(result.stdout, expected(&scalars, encoding), "{name}");
        for invalid in ["d800", "dfff", "110000", "ffffffff"] {
            let result = Command::new(&executable)
                .env("BF_TEST_INVALID_SCALAR", invalid)
                .output()
                .unwrap();
            assert_eq!(result.status.code(), Some(2), "{name}: {invalid}");
            assert!(result.stdout.is_empty());
        }
    }
}
