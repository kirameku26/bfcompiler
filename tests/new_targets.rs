use bfcompiler::{
    CellWidth, RuntimeConfig, Target, compile_machine_for, compile_machine_with_config,
    compile_object_for,
};

fn u32_at(data: &[u8], at: usize) -> usize {
    u32::from_le_bytes(data[at..at + 4].try_into().unwrap()) as usize
}
fn u64_at(data: &[u8], at: usize) -> usize {
    u64::from_le_bytes(data[at..at + 8].try_into().unwrap()) as usize
}

fn text_section(data: &[u8], target: Target) -> &[u8] {
    let (offset, size) = if target == Target::X86_64WindowsGnu {
        (u32_at(data, 40), u32_at(data, 36))
    } else if matches!(target, Target::Aarch64Darwin | Target::X86_64Darwin) {
        let mut command = 32;
        let mut found = None;
        for _ in 0..u32_at(data, 16) {
            if u32_at(data, command) == 0x19 {
                for index in 0..u32_at(data, command + 64) {
                    let section = command + 72 + index * 80;
                    if data[section..section + 16].starts_with(b"__text\0") {
                        found = Some((u32_at(data, section + 48), u64_at(data, section + 40)));
                    }
                }
            }
            command += u32_at(data, command + 4);
        }
        found.unwrap()
    } else {
        let is32 = data[4] == 1;
        let (table, strindex, count, stride) = if is32 {
            (
                u32_at(data, 32),
                u16::from_le_bytes(data[50..52].try_into().unwrap()) as usize,
                u16::from_le_bytes(data[48..50].try_into().unwrap()) as usize,
                40,
            )
        } else {
            (
                u64_at(data, 40),
                u16::from_le_bytes(data[62..64].try_into().unwrap()) as usize,
                u16::from_le_bytes(data[60..62].try_into().unwrap()) as usize,
                64,
            )
        };
        let string_section = table + strindex * stride;
        let strings = if is32 {
            u32_at(data, string_section + 16)
        } else {
            u64_at(data, string_section + 24)
        };
        let section = (0..count)
            .map(|i| table + i * stride)
            .find(|&i| data[strings + u32_at(data, i)..].starts_with(b".text\0"))
            .unwrap();
        if is32 {
            (u32_at(data, section + 16), u32_at(data, section + 20))
        } else {
            (u64_at(data, section + 24), u64_at(data, section + 32))
        }
    };
    &data[offset..offset + size]
}

#[test]
fn configured_objects_match_cross_assembled_output() {
    use std::{
        fs,
        process::Command,
        time::{SystemTime, UNIX_EPOCH},
    };
    let Some(clang) = std::env::var_os("BFC_TEST_CROSS_CLANG") else {
        return;
    };
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir =
        std::env::temp_dir().join(format!("bfcompiler-config-{}-{stamp}", std::process::id()));
    fs::create_dir(&dir).unwrap();
    for (target, triple) in [
        (Target::Aarch64Darwin, "aarch64-apple-darwin"),
        (Target::X86_64Darwin, "x86_64-apple-darwin"),
        (Target::Aarch64Linux, "aarch64-unknown-linux-gnu"),
        (Target::X86_64Linux, "x86_64-unknown-linux-gnu"),
        (Target::I386Linux, "i386-unknown-linux-gnu"),
        (Target::Riscv64Linux, "riscv64-unknown-linux-gnu"),
        (Target::X86_64WindowsGnu, "x86_64-pc-windows-gnu"),
    ] {
        for encoding in [
            bfcompiler::OutputEncoding::Utf8,
            bfcompiler::OutputEncoding::Utf16Be,
            bfcompiler::OutputEncoding::Utf32Le,
        ] {
            for width in [CellWidth::Bits8, CellWidth::Bits16, CellWidth::Bits32] {
                for optimize in [false, true] {
                    let code = bfcompiler::compile_machine_with_encoding(
                        b",[->+<]>>[-<->]<<[>+<-]>.[+]",
                        optimize,
                        target,
                        RuntimeConfig {
                            tape_size: 12345,
                            cell_width: width,
                            output_utf8: true,
                        },
                        encoding,
                    )
                    .unwrap();
                    let object = bfcompiler::write_machine_object(&code, target).unwrap();
                    assert_eq!(text_section(&object, target), code.section_bytes());
                    let asm = dir.join("program.s");
                    let assembled = dir.join("program.o");
                    fs::write(
                        &asm,
                        bfcompiler::backend::assembly::render_configured(&code, target),
                    )
                    .unwrap();
                    let result = Command::new(&clang)
                        .args(["-target", triple, "-c"])
                        .arg(&asm)
                        .arg("-o")
                        .arg(&assembled)
                        .output()
                        .unwrap();
                    assert!(
                        result.status.success(),
                        "{triple}: {}",
                        String::from_utf8_lossy(&result.stderr)
                    );
                    let data = fs::read(&assembled).unwrap();
                    let text = text_section(&data, target);
                    // COFF assemblers pad executable sections to their alignment.
                    assert_eq!(
                        &text[..code.section_bytes().len()],
                        code.section_bytes(),
                        "{triple} {width:?} optimized={optimize}"
                    );
                    assert!(
                        text[code.section_bytes().len()..]
                            .iter()
                            .all(|&b| b == 0 || b == 0x90)
                    );
                }
            }
        }
    }
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn elf_headers_and_call_relocations_match_target_abi() {
    for (target, class, machine, relocation) in [
        (Target::I386Linux, 1, 3, 2),
        (Target::Riscv64Linux, 2, 243, 19),
    ] {
        let object =
            compile_object_for(include_bytes!("fixtures/targets.bf"), false, target).unwrap();
        let code =
            compile_machine_for(include_bytes!("fixtures/targets.bf"), false, target).unwrap();
        assert_eq!(object[4], class);
        assert_eq!(
            u16::from_le_bytes(object[18..20].try_into().unwrap()),
            machine
        );
        if class == 1 {
            let sections = u32_at(&object, 32);
            let rel = u32_at(&object, sections + 2 * 40 + 16);
            assert_eq!(u32_at(&object, rel), code.relocations[0].offset as usize);
            assert_eq!(u32_at(&object, rel + 4) & 255, relocation);
            let off = code.relocations[0].offset as usize;
            assert_eq!(&code.bytes[off..off + 4], &(-4i32).to_le_bytes());
        } else {
            assert_eq!(u32_at(&object, 48), 4); // LP64D ABI
            let sections = u64_at(&object, 40);
            let rela = u64_at(&object, sections + 2 * 64 + 24);
            assert_eq!(u64_at(&object, rela), code.relocations[0].offset as usize);
            assert_eq!(u64_at(&object, rela + 8) & 0xffff_ffff, relocation);
        }
    }
}

#[test]
fn generated_assembly_matches_object_text_with_cross_clang() {
    use std::{
        fs,
        process::Command,
        time::{SystemTime, UNIX_EPOCH},
    };
    let Some(clang) = std::env::var_os("BFC_TEST_CROSS_CLANG") else {
        return;
    };
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "bfcompiler-new-targets-{}-{stamp}",
        std::process::id()
    ));
    fs::create_dir(&dir).unwrap();
    for (target, triple) in [
        (Target::I386Linux, "i386-unknown-linux-gnu"),
        (Target::Riscv64Linux, "riscv64-unknown-linux-gnu"),
    ] {
        for optimize in [false, true] {
            let code =
                compile_machine_for(b",[->+<]>>[-<->]<<[>+<-]>.[+]", optimize, target).unwrap();
            let assembly = match target {
                Target::I386Linux => bfcompiler::backend::i386::assembly(&code),
                _ => bfcompiler::backend::riscv64::assembly(&code),
            };
            let asm = dir.join("test.s");
            let object = dir.join("test.o");
            fs::write(&asm, assembly).unwrap();
            let result = Command::new(&clang)
                .args(["-target", triple, "-c"])
                .arg(&asm)
                .arg("-o")
                .arg(&object)
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "{triple}: {}",
                String::from_utf8_lossy(&result.stderr)
            );
            let data = fs::read(object).unwrap();
            let class = data[4];
            let sections = if class == 1 {
                u32_at(&data, 32)
            } else {
                u64_at(&data, 40)
            };
            let (entry_size, shstr_index, section_count) = if class == 1 {
                (
                    40,
                    u16::from_le_bytes(data[50..52].try_into().unwrap()) as usize,
                    u16::from_le_bytes(data[48..50].try_into().unwrap()) as usize,
                )
            } else {
                (
                    64,
                    u16::from_le_bytes(data[62..64].try_into().unwrap()) as usize,
                    u16::from_le_bytes(data[60..62].try_into().unwrap()) as usize,
                )
            };
            let str_section = sections + entry_size * shstr_index;
            let strings = if class == 1 {
                u32_at(&data, str_section + 16)
            } else {
                u64_at(&data, str_section + 24)
            };
            let text_section = (0..section_count)
                .map(|i| sections + entry_size * i)
                .find(|&s| data[strings + u32_at(&data, s)..].starts_with(b".text\0"))
                .unwrap();
            let (size, offset) = if class == 1 {
                (
                    u32_at(&data, text_section + 20),
                    u32_at(&data, text_section + 16),
                )
            } else {
                (
                    u64_at(&data, text_section + 32),
                    u64_at(&data, text_section + 24),
                )
            };
            assert_eq!(
                &data[offset..offset + size],
                code.bytes,
                "{triple}, optimize={optimize}"
            );
        }
    }
    fs::remove_dir_all(dir).unwrap();
}

// Exercise actual RV64 machine code without a Linux sysroot. QEMU's virt test
// device reports success/failure, while tiny stubs supply the runtime calls.
#[test]
fn riscv64_runs_on_qemu_system_when_requested() {
    use std::{
        fs,
        process::{Command, Stdio},
        time::{Duration, Instant, SystemTime, UNIX_EPOCH},
    };
    let Some(qemu) = std::env::var_os("BFC_TEST_QEMU_RISCV64") else {
        return;
    };
    fn word(out: &mut Vec<u8>, n: u32) {
        out.extend(n.to_le_bytes());
    }
    fn lui(rd: u32, n: u32) -> u32 {
        (n << 12) | (rd << 7) | 0x37
    }
    fn addi(rd: u32, rs: u32, n: i32) -> u32 {
        ((n as u32 & 4095) << 20) | (rs << 15) | (rd << 7) | 0x13
    }
    fn address(out: &mut Vec<u8>, rd: u32, upper: u32) {
        word(out, lui(rd, upper));
        word(out, (32 << 20) | (rd << 15) | (1 << 12) | (rd << 7) | 0x13);
        word(out, (32 << 20) | (rd << 15) | (5 << 12) | (rd << 7) | 0x13);
    }
    fn jal(rd: u32, from: usize, to: usize) -> u32 {
        let n = ((to as i64 - from as i64) as u32) & 0x1f_ffff;
        ((n >> 20) << 31)
            | (((n >> 1) & 1023) << 21)
            | (((n >> 11) & 1) << 20)
            | (((n >> 12) & 255) << 12)
            | (rd << 7)
            | 0x6f
    }
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "bfcompiler-riscv-qemu-{}-{stamp}",
        std::process::id()
    ));
    fs::create_dir(&dir).unwrap();
    let mut immediate_source = vec![b'+'; 4096];
    immediate_source.push(b'.');
    for width in [CellWidth::Bits8, CellWidth::Bits16, CellWidth::Bits32] {
        for (source, input, tape_size, result, output) in [
            (immediate_source.as_slice(), 0, 1, 0, Some(4096)),
            (b",[->+<]>.".as_slice(), 5, 2, 0, Some(5)),
            (b",.".as_slice(), -1, 1, 0, Some(0)),
            (b",.".as_slice(), -2, 1, 2, None),
            (b"-.".as_slice(), 0, 1, 0, Some(255)),
            (b",[->+<]".as_slice(), 5, 1, 2, None),
            (b"<".as_slice(), 0, 1, 2, None),
            (b">".as_slice(), 0, 1, 2, None),
        ] {
            for optimize in [false, true] {
                let output = if source == b"-." {
                    Some((width.modulus() - 1) as u32)
                } else {
                    output.map(|value: u32| value & (width.modulus() - 1) as u32)
                };
                let code = compile_machine_with_config(
                    source,
                    optimize,
                    Target::Riscv64Linux,
                    RuntimeConfig {
                        tape_size,
                        cell_width: width,
                        output_utf8: false,
                    },
                )
                .unwrap();
                let mut image = Vec::new();
                address(&mut image, 2, 0x80300);
                address(&mut image, 10, 0x80100);
                word(&mut image, addi(11, 0, (tape_size * width.bytes()) as i32));
                word(&mut image, (10 << 15) | (3 << 12) | 0x23);
                word(&mut image, (10 << 15) | (3 << 12) | (8 << 7) | 0x23); // sd zero,0(a0)
                address(&mut image, 5, 0x80200);
                word(&mut image, addi(6, 0, 255));
                word(&mut image, (6 << 20) | (5 << 15) | 0x23);
                let call = image.len();
                word(&mut image, 0);
                word(&mut image, addi(5, 0, result));
                // Skip the failure jump when the result agrees.
                word(&mut image, (5 << 20) | (10 << 15) | (4 << 8) | 0x63);
                let fail1 = image.len();
                word(&mut image, 0);
                let mut fail2 = None;
                if let Some(expected) = output {
                    address(&mut image, 5, 0x80200);
                    word(&mut image, (5 << 15) | (2 << 12) | (6 << 7) | 0x03); // lbu t1,0(t0)
                    word(
                        &mut image,
                        (expected.wrapping_add(0x800) & 0xfffff000) | (5 << 7) | 0x37,
                    );
                    word(
                        &mut image,
                        ((expected & 0xfff) << 20) | (5 << 15) | (5 << 7) | 0x1b,
                    );
                    word(&mut image, (5 << 20) | (6 << 15) | (4 << 8) | 0x63);
                    fail2 = Some(image.len());
                    word(&mut image, 0);
                }
                let success = image.len();
                word(&mut image, lui(5, 0x100));
                word(&mut image, lui(6, 5));
                word(&mut image, addi(6, 6, 0x555));
                word(&mut image, (6 << 20) | (5 << 15) | (2 << 12) | 0x23);
                word(&mut image, 0x0000006f);
                let fail = image.len();
                word(&mut image, lui(5, 0x100));
                word(&mut image, lui(6, 3));
                word(&mut image, addi(6, 6, 0x333));
                word(&mut image, (6 << 20) | (5 << 15) | (2 << 12) | 0x23);
                word(&mut image, 0x0000006f);
                let input_stub = image.len();
                word(&mut image, addi(10, 0, input));
                word(&mut image, 0x00008067);
                let output_stub = image.len();
                address(&mut image, 5, 0x80200);
                word(&mut image, (10 << 20) | (5 << 15) | (2 << 12) | 0x23);
                word(&mut image, addi(10, 0, 0));
                word(&mut image, 0x00008067);
                let program = image.len();
                image.extend(&code.bytes);
                image[call..call + 4].copy_from_slice(&jal(1, call, program).to_le_bytes());
                image[fail1..fail1 + 4].copy_from_slice(&jal(0, fail1, fail).to_le_bytes());
                if let Some(pos) = fail2 {
                    image[pos..pos + 4].copy_from_slice(&jal(0, pos, fail).to_le_bytes());
                }
                assert!(success < fail);
                for reloc in code.relocations {
                    let pos = program + reloc.offset as usize;
                    let target = if reloc.symbol == "_bf_input" {
                        input_stub
                    } else {
                        output_stub
                    };
                    image[pos..pos + 4].copy_from_slice(&jal(1, pos, target).to_le_bytes());
                    image[pos + 4..pos + 8].copy_from_slice(&0x00000013u32.to_le_bytes());
                }
                let path = dir.join("program.bin");
                fs::write(&path, image).unwrap();
                let mut child = Command::new(&qemu)
                    .args(["-machine", "virt", "-bios", "none", "-nographic", "-kernel"])
                    .arg(&path)
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                    .unwrap();
                let deadline = Instant::now() + Duration::from_secs(5);
                loop {
                    if let Some(status) = child.try_wait().unwrap() {
                        assert!(
                            status.success(),
                            "source={source:?} optimize={optimize}, status={status}"
                        );
                        break;
                    }
                    if Instant::now() > deadline {
                        child.kill().unwrap();
                        let _ = child.wait();
                        panic!("RISC-V QEMU timed out");
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
        }
    }
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn i386_runs_on_qemu_system_when_requested() {
    use std::{
        fs,
        process::{Command, Stdio},
        time::{Duration, Instant, SystemTime, UNIX_EPOCH},
    };
    let Some(qemu) = std::env::var_os("BFC_TEST_QEMU_I386") else {
        return;
    };
    fn put32(bytes: &mut Vec<u8>, value: u32) {
        bytes.extend(value.to_le_bytes());
    }
    fn rel32(bytes: &mut [u8], position: usize, target: usize) {
        bytes[position..position + 4]
            .copy_from_slice(&((target as i64 - position as i64 - 4) as i32).to_le_bytes());
    }
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "bfcompiler-i386-qemu-{}-{stamp}",
        std::process::id()
    ));
    fs::create_dir(&dir).unwrap();
    let mut immediate_source = vec![b'+'; 4096];
    immediate_source.push(b'.');
    for width in [CellWidth::Bits8, CellWidth::Bits16, CellWidth::Bits32] {
        for (source, input, tape_size, result, output) in [
            (immediate_source.as_slice(), 0, 1, 0, Some(4096)),
            (b",[->+<]>.".as_slice(), 5i32, 2u32, 0u8, Some(5u32)),
            (b",.".as_slice(), -1, 1, 0, Some(0)),
            (b",.".as_slice(), -2, 1, 2, None),
            (b"-.".as_slice(), 0, 1, 0, Some(255)),
            (b",[->+<]".as_slice(), 5, 1, 2, None),
            (b"<".as_slice(), 0, 1, 2, None),
            (b">".as_slice(), 0, 1, 2, None),
        ] {
            for optimize in [false, true] {
                let output = if source == b"-." {
                    Some((width.modulus() - 1) as u32)
                } else {
                    output.map(|value: u32| value & (width.modulus() - 1) as u32)
                };
                let code = compile_machine_with_config(
                    source,
                    optimize,
                    Target::I386Linux,
                    RuntimeConfig {
                        tape_size,
                        cell_width: width,
                        output_utf8: false,
                    },
                )
                .unwrap();
                // Multiboot a.out header: QEMU enters 32-bit protected mode at 1 MiB + 0x120.
                let mut image = vec![0; 0x100];
                for value in [
                    0x1badb002u32,
                    0x10000,
                    0xe4514ffe,
                    0x100100,
                    0x100000,
                    0,
                    0,
                    0x100120,
                ] {
                    put32(&mut image, value);
                }
                image.push(0xbc);
                put32(&mut image, 0x180000); // mov esp, stack
                for address in [0x120000, 0x120004, 0x120008, 0x12000c] {
                    image.extend([0xc7, 0x05]);
                    put32(&mut image, address);
                    put32(&mut image, 0);
                }
                image.extend([0xc7, 0x05]);
                put32(&mut image, 0x130000);
                put32(&mut image, 0xffff_ffff);
                image.extend([0x83, 0xec, 8, 0x68]);
                put32(&mut image, tape_size * width.bytes());
                image.push(0x68);
                put32(&mut image, 0x120000);
                image.push(0xe8);
                let call = image.len();
                put32(&mut image, 0);
                image.extend([0x83, 0xf8, result, 0x0f, 0x85]);
                let fail1 = image.len();
                put32(&mut image, 0);
                let mut fail2 = None;
                if let Some(expected) = output {
                    image.extend([0xa1, 0x00, 0x00, 0x13, 0x00, 0x3d]);
                    put32(&mut image, expected);
                    image.extend([0x0f, 0x85]);
                    fail2 = Some(image.len());
                    put32(&mut image, 0);
                }
                image.extend([0x66, 0xba, 0xf4, 0x00, 0xb8]);
                put32(&mut image, 0x10);
                image.extend([0xef, 0xf4]);
                let fail = image.len();
                image.extend([0x66, 0xba, 0xf4, 0x00, 0xef, 0xf4]);
                let fail_result = image.len();
                image.extend([0x66, 0xba, 0xf4, 0x00, 0xb8]);
                put32(&mut image, 0x12);
                image.extend([0xef, 0xf4]);
                let input_stub = image.len();
                image.push(0xb8);
                put32(&mut image, input as u32);
                image.push(0xc3);
                let output_stub = image.len();
                image.extend([
                    0x8b, 0x44, 0x24, 0x04, 0xa3, 0x00, 0x00, 0x13, 0x00, 0x31, 0xc0, 0xc3,
                ]);
                let program = image.len();
                image.extend(&code.bytes);
                rel32(&mut image, call, program);
                rel32(&mut image, fail1, fail_result);
                if let Some(pos) = fail2 {
                    rel32(&mut image, pos, fail);
                }
                for r in code.relocations {
                    let pos = program + r.offset as usize;
                    let target = if r.symbol == "_bf_input" {
                        input_stub
                    } else {
                        output_stub
                    };
                    rel32(&mut image, pos, target);
                }
                let path = dir.join("program.bin");
                fs::write(&path, image).unwrap();
                let mut child = Command::new(&qemu)
                    .args([
                        "-machine",
                        "pc",
                        "-m",
                        "32",
                        "-display",
                        "none",
                        "-no-reboot",
                        "-device",
                        "isa-debug-exit,iobase=0xf4,iosize=0x04",
                        "-kernel",
                    ])
                    .arg(&path)
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                    .unwrap();
                let deadline = Instant::now() + Duration::from_secs(5);
                loop {
                    if let Some(status) = child.try_wait().unwrap() {
                        assert_eq!(
                            status.code(),
                            Some(33),
                            "source={source:?} optimize={optimize}, status={status}"
                        );
                        break;
                    }
                    if Instant::now() > deadline {
                        child.kill().unwrap();
                        let _ = child.wait();
                        panic!("i386 QEMU timed out");
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
        }
    }
    fs::remove_dir_all(dir).unwrap();
}
