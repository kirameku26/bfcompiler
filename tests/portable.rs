use bfcompiler::{Target, compile_machine_for, compile_object_for};

#[test]
fn cross_target_object_headers_and_relocations() {
    for (target, magic, machine) in [
        (Target::X86_64Linux, b"\x7fELF".as_slice(), 62u16),
        (Target::Aarch64Linux, b"\x7fELF".as_slice(), 183u16),
        (Target::I386Linux, b"\x7fELF".as_slice(), 3u16),
        (Target::Riscv64Linux, b"\x7fELF".as_slice(), 243u16),
        (Target::X86_64WindowsGnu, b"\x64\x86".as_slice(), 0x8664u16),
    ] {
        let object = compile_object_for(b",[>+<-]>.", true, target).unwrap();
        assert!(object.starts_with(magic));
        let machine_offset = if target == Target::X86_64WindowsGnu {
            0
        } else {
            18
        };
        assert_eq!(
            u16::from_le_bytes(
                object[machine_offset..machine_offset + 2]
                    .try_into()
                    .unwrap()
            ),
            machine
        );
        assert!(object.windows(9).any(|w| w == b"bf_input\0"));
        assert!(object.windows(10).any(|w| w == b"bf_output\0"));
        let code = compile_machine_for(b",.", true, target).unwrap();
        assert_eq!(code.relocations.len(), 2);
    }
}

#[test]
fn clear_loops_generate_smaller_code_for_all_targets() {
    for target in [
        Target::Aarch64Darwin,
        Target::X86_64Darwin,
        Target::Aarch64Linux,
        Target::X86_64Linux,
        Target::I386Linux,
        Target::Riscv64Linux,
        Target::X86_64WindowsGnu,
    ] {
        let source = b"[-][+]";
        let optimized = compile_machine_for(source, true, target).unwrap();
        let unoptimized = compile_machine_for(source, false, target).unwrap();
        println!(
            "{target:?}: {} -> {} bytes",
            unoptimized.bytes.len(),
            optimized.bytes.len()
        );
        assert!(
            optimized.bytes.len() < unoptimized.bytes.len(),
            "{target:?}"
        );
    }
}

#[test]
fn grouped_pointer_moves_reduce_generated_code_size() {
    for target in [
        Target::Aarch64Darwin,
        Target::X86_64Darwin,
        Target::Aarch64Linux,
        Target::X86_64Linux,
        Target::I386Linux,
        Target::Riscv64Linux,
        Target::X86_64WindowsGnu,
    ] {
        let source = vec![b'>'; 10_000];
        let optimized = compile_machine_for(&source, true, target).unwrap();
        let unoptimized = compile_machine_for(&source, false, target).unwrap();
        println!(
            "{target:?}: {} -> {} bytes",
            unoptimized.bytes.len(),
            optimized.bytes.len()
        );
        assert!(optimized.bytes.len() < unoptimized.bytes.len() / 10);
    }
}

#[test]
fn rendered_x86_assembly_shows_generated_instructions() {
    for target in [
        Target::X86_64Darwin,
        Target::X86_64Linux,
        Target::X86_64WindowsGnu,
    ] {
        for optimize in [false, true] {
            let code = compile_machine_for(b",[->+<]>>[-<->]<<[>+<-]>.", optimize, target).unwrap();
            let assembly =
                bfcompiler::backend::x86_64::assembly_for(&code, target == Target::X86_64Darwin);
            assert!(!assembly.contains(".byte"), "{target:?}:\n{assembly}");
            assert!(assembly.contains("movzbl"));
            assert!(assembly.contains(".Lbf_"));
        }
    }
}

#[test]
fn grouped_move_assembly_renders_large_immediates() {
    let mut source = vec![b'>'; 4_100];
    source.extend(vec![b'<'; 4_100]);
    for target in [Target::Aarch64Darwin, Target::X86_64Darwin] {
        let code = compile_machine_for(&source, true, target).unwrap();
        let assembly = match target {
            Target::Aarch64Darwin => bfcompiler::backend::assembly::render(&code),
            _ => bfcompiler::backend::x86_64::assembly_for(&code, true),
        };
        assert!(
            !assembly.contains(".long") && !assembly.contains(".byte"),
            "{target:?}:\n{assembly}"
        );
        assert!(assembly.contains("4095") || assembly.contains("4100"));
    }
}

#[test]
fn mode_validation_works_for_cross_target_object_and_assembly_output() {
    use std::{
        fs,
        process::Command,
        time::{SystemTime, UNIX_EPOCH},
    };
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("bfcompiler-modes-{}-{stamp}", std::process::id()));
    fs::create_dir(&dir).unwrap();
    let source = dir.join("test.bf");
    fs::write(&source, b"+").unwrap();
    for target in [
        "aarch64-apple-darwin",
        "x86_64-apple-darwin",
        "aarch64-unknown-linux-gnu",
        "x86_64-unknown-linux-gnu",
        "i686-unknown-linux-gnu",
        "riscv64gc-unknown-linux-gnu",
        "x86_64-pc-windows-gnu",
    ] {
        for format in ["-c", "-S"] {
            let path = dir.join(format!("{target}-{}", &format[1..]));
            let result = Command::new(env!("CARGO_BIN_EXE_bfcompiler"))
                .args(["--target", target, "--mode=standard", format])
                .arg(&source)
                .arg("-o")
                .arg(&path)
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "{target}: {}",
                String::from_utf8_lossy(&result.stderr)
            );
            assert!(path.exists());
            let result = Command::new(env!("CARGO_BIN_EXE_bfcompiler"))
                .args(["--target", target, "--mode=extended", format])
                .arg(&source)
                .arg("-o")
                .arg(&path)
                .output()
                .unwrap();
            assert!(result.status.success());
            assert!(result.stderr.is_empty());
        }
    }
    let _ = fs::remove_dir_all(dir);
}

#[cfg(any(target_os = "linux", all(target_os = "windows", target_env = "gnu")))]
#[test]
fn native_compilation_and_execution() {
    use std::{
        fs,
        io::Write,
        process::{Command, Stdio},
        time::{SystemTime, UNIX_EPOCH},
    };
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir =
        std::env::temp_dir().join(format!("bfcompiler-native-{}-{stamp}", std::process::id()));
    fs::create_dir(&dir).unwrap();
    let source = dir.join("test.bf");
    let executable = dir.join(if cfg!(windows) { "test.exe" } else { "test" });
    fs::write(&source, b",[>+<-]>.").unwrap();
    assert!(
        Command::new(env!("CARGO_BIN_EXE_bfcompiler"))
            .arg(&source)
            .arg("-o")
            .arg(&executable)
            .status()
            .unwrap()
            .success()
    );
    let mut child = Command::new(&executable)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(&[13]).unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(result.status.success());
    assert_eq!(result.stdout, [13]);
    let _ = fs::remove_dir_all(dir);
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn windows_x64_calling_convention_runs_under_rosetta() {
    use std::{
        fs,
        process::Command,
        time::{SystemTime, UNIX_EPOCH},
    };
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("bfcompiler-abi-{}-{stamp}", std::process::id()));
    fs::create_dir(&dir).unwrap();
    for width in [
        bfcompiler::CellWidth::Bits8,
        bfcompiler::CellWidth::Bits16,
        bfcompiler::CellWidth::Bits32,
    ] {
        let mut source = vec![b','];
        source.extend(vec![b'+'; 256]);
        source.extend_from_slice(b"[->+<]>.");
        let code = bfcompiler::compile_machine_with_config(
            &source,
            true,
            Target::X86_64WindowsGnu,
            bfcompiler::RuntimeConfig {
                tape_size: 2,
                cell_width: width,
                output_utf8: false,
            },
        )
        .unwrap();
        let object = bfcompiler::object::macho::write_object_for(&code, true).unwrap();
        let object_path = dir.join("test.o");
        let harness = dir.join("harness.c");
        let executable = dir.join("test");
        fs::write(&object_path, object).unwrap();
        fs::write(&harness, include_str!("windows_abi_harness.c")).unwrap();
        assert!(
            Command::new("cc")
                .args(["-arch", "x86_64"])
                .arg(if width == bfcompiler::CellWidth::Bits8 {
                    "-DEXPECTED=65"
                } else {
                    "-DEXPECTED=321"
                })
                .arg(&harness)
                .arg(&object_path)
                .arg("-o")
                .arg(&executable)
                .status()
                .unwrap()
                .success()
        );
        assert!(Command::new(executable).status().unwrap().success());
    }
    let _ = fs::remove_dir_all(dir);
}
