use bfcompiler::{
    CellWidth, CompileError, OutputEncoding, RuntimeConfig, Target, compile_machine_for,
    compile_machine_with_encoding, write_machine_object,
};
use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

struct Options {
    input: PathBuf,
    output: Option<PathBuf>,
    mode: Mode,
    optimize: bool,
    linker_args: Vec<OsString>,
    verbose: bool,
    target: Target,
    tape_size: u32,
    output_utf8: bool,
    output_encoding: OutputEncoding,
    config: Option<RuntimeConfig>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Executable,
    Object,
    Assembly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LanguageMode {
    Standard,
    Extended,
}

fn parse_language_mode(value: &str) -> Result<LanguageMode, String> {
    match value {
        "standard" => Ok(LanguageMode::Standard),
        "extended" => Ok(LanguageMode::Extended),
        _ => Err(format!(
            "unsupported language mode: {value} (expected standard or extended)"
        )),
    }
}

fn usage() -> &'static str {
    "usage: bfcompiler [OPTIONS] <input.bf>\n\n  -o <path>           output path\n  -c                  emit a target object\n  -S                  emit assembly\n  -O0                 disable optimization\n  -O, -O1, -O2, -O3, -Os, -Oz  enable available optimizations\n  --no-optimize       disable optimization (alias for -O0)\n  --mode <standard|extended>  language mode (default: standard)\n  --tape-size <cells> tape length (extended mode only; default: 1000000)\n  --output-encoding <encoding>  bytes, utf-8, utf-16[le|be], utf-32[le|be]\n                      extended mode only; utf-16/utf-32 default to LE; no BOM\n  --cell-bits <8|16|32>  cell width (extended mode only; default: 8)\n  -L<dir>, -l<name>   pass library search path or library to linker\n  -Wl,<arg>,...       pass arguments to linker\n  --target <triple>   aarch64-apple-darwin, x86_64-apple-darwin,\n                      aarch64-unknown-linux-gnu, x86_64-unknown-linux-gnu,\n                      i686-unknown-linux-gnu, riscv64gc-unknown-linux-gnu,\n                      x86_64-pc-windows-gnu\n  -v                  show link command\n  --version           show version\n  -h, --help          show this help"
}

fn parse_args() -> Result<Option<Options>, String> {
    let mut args = env::args_os().skip(1);
    let (mut input, mut output) = (None, None);
    let (mut mode, mut optimize, mut verbose) = (Mode::Executable, true, false);
    let mut linker_args = Vec::new();
    let mut target = Target::host_default();
    let mut tape_size = None;
    let mut output_encoding = None;
    let mut cell_width = None;
    let mut language_mode = LanguageMode::Standard;
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("-h" | "--help") => {
                println!("{}", usage());
                return Ok(None);
            }
            Some("--version") => {
                println!("bfcompiler {}", env!("CARGO_PKG_VERSION"));
                return Ok(None);
            }
            Some("-c") => mode = Mode::Object,
            Some("-S") => mode = Mode::Assembly,
            Some("-O0" | "--no-optimize") => optimize = false,
            Some("-O" | "-O1" | "-O2" | "-O3" | "-Os" | "-Oz") => optimize = true,
            Some("-v") => verbose = true,
            Some("--mode") => {
                let value = args.next().ok_or("--mode requires standard or extended")?;
                language_mode = parse_language_mode(&value.to_string_lossy())?;
            }
            Some(s) if s.starts_with("--mode=") => {
                language_mode = parse_language_mode(&s[7..])?;
            }
            Some("--tape-size") => {
                let value = args
                    .next()
                    .ok_or("--tape-size requires a positive cell count")?;
                tape_size = Some(parse_tape_size(&value.to_string_lossy())?);
            }
            Some(s) if s.starts_with("--tape-size=") => {
                tape_size = Some(parse_tape_size(&s[12..])?)
            }
            Some("--output-encoding") => {
                let value = args
                    .next()
                    .ok_or("--output-encoding requires an encoding name")?;
                output_encoding = Some(parse_encoding(&value.to_string_lossy())?);
            }
            Some("--cell-bits") => {
                let value = args.next().ok_or("--cell-bits requires 8, 16, or 32")?;
                cell_width = Some(parse_cell_width(&value.to_string_lossy())?);
            }
            Some(s) if s.starts_with("--cell-bits=") => {
                cell_width = Some(parse_cell_width(&s[12..])?);
            }
            Some(s) if s.starts_with("--output-encoding=") => {
                output_encoding = Some(parse_encoding(&s[18..])?)
            }
            Some("-o") => output = Some(PathBuf::from(args.next().ok_or("-o requires a path")?)),
            Some("-L" | "-l") => {
                linker_args.push(arg.clone());
                linker_args.push(args.next().ok_or("library option requires an argument")?);
            }
            Some("--target") => {
                let name = args.next().ok_or("--target requires a triple")?;
                let name = name.to_string_lossy();
                target = Some(
                    Target::parse(&name).ok_or_else(|| format!("unsupported target: {name}"))?,
                );
            }
            Some(s) if s.starts_with("--target=") => {
                target = Some(
                    Target::parse(&s[9..])
                        .ok_or_else(|| format!("unsupported target: {}", &s[9..]))?,
                );
            }
            Some(s) if s.starts_with("-o") && s.len() > 2 => output = Some(PathBuf::from(&s[2..])),
            Some(s) if (s.starts_with("-L") || s.starts_with("-l")) && s.len() > 2 => {
                linker_args.push(arg)
            }
            Some(s) if s.starts_with("-Wl,") && s.len() > 4 => linker_args.push(arg),
            Some(s) if s.starts_with('-') => return Err(format!("unknown option: {s}")),
            _ if input.is_none() => input = Some(PathBuf::from(arg)),
            _ => return Err("more than one input file was provided".into()),
        }
    }
    if language_mode == LanguageMode::Standard
        && (tape_size.is_some() || output_encoding.is_some() || cell_width.is_some())
    {
        return Err(
            "--tape-size, --output-encoding, and --cell-bits require --mode extended".into(),
        );
    }
    let resolved_tape_size = tape_size.unwrap_or(if language_mode == LanguageMode::Standard {
        30_000
    } else {
        1_000_000
    });
    let config = (language_mode == LanguageMode::Extended).then_some(RuntimeConfig {
        tape_size: resolved_tape_size,
        cell_width: cell_width.unwrap_or(CellWidth::Bits8),
        output_utf8: output_encoding == Some(OutputEncoding::Utf8),
    });
    Ok(Some(Options {
        input: input.ok_or("no input file was provided")?,
        output,
        mode,
        optimize,
        linker_args,
        verbose,
        target: target.ok_or("unsupported host; specify --target")?,
        tape_size: resolved_tape_size,
        output_utf8: output_encoding == Some(OutputEncoding::Utf8),
        output_encoding: output_encoding.unwrap_or(OutputEncoding::Bytes),
        config,
    }))
}

fn parse_cell_width(value: &str) -> Result<CellWidth, String> {
    match value {
        "8" => Ok(CellWidth::Bits8),
        "16" => Ok(CellWidth::Bits16),
        "32" => Ok(CellWidth::Bits32),
        _ => Err("--cell-bits must be 8, 16, or 32".into()),
    }
}

fn parse_tape_size(value: &str) -> Result<u32, String> {
    let count = value
        .parse::<u32>()
        .map_err(|_| "--tape-size must be an integer from 1 to 2147483647".to_string())?;
    if count == 0 || count > i32::MAX as u32 {
        return Err("--tape-size must be an integer from 1 to 2147483647".into());
    }
    Ok(count)
}

fn parse_encoding(value: &str) -> Result<OutputEncoding, String> {
    OutputEncoding::parse(value).ok_or_else(||format!(
        "unsupported output encoding: {value} (expected bytes, utf-8, utf-16, utf-16le, utf-16be, utf-32, utf-32le, or utf-32be)"
    ))
}

fn default_output(input: &Path, mode: Mode, target: Target) -> PathBuf {
    match mode {
        Mode::Object => input.with_extension("o"),
        Mode::Assembly => input.with_extension("s"),
        Mode::Executable => input.with_extension(if target == Target::X86_64WindowsGnu {
            "exe"
        } else {
            ""
        }),
    }
}

fn temporary_output(output: &Path, stamp: u128) -> PathBuf {
    let name = output.file_name().unwrap_or_default().to_string_lossy();
    output.with_file_name(format!(".{name}.bfcompiler-{}-{stamp}", std::process::id()))
}

fn atomic_write(output: &Path, contents: &[u8], stamp: u128) -> Result<(), std::io::Error> {
    let temporary = temporary_output(output, stamp);
    let result = (|| {
        fs::write(&temporary, contents)?;
        fs::rename(&temporary, output)
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

fn run(options: Options) -> Result<(), CompileError> {
    let source = fs::read(&options.input)?;
    let output = options
        .output
        .unwrap_or_else(|| default_output(&options.input, options.mode, options.target));
    if output == options.input
        || (output.exists() && fs::canonicalize(&output)? == fs::canonicalize(&options.input)?)
    {
        return Err(CompileError::Codegen(
            "output path refers to the input file".into(),
        ));
    }
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let machine = if let Some(config) = options.config {
        compile_machine_with_encoding(
            &source,
            options.optimize,
            options.target,
            config,
            options.output_encoding,
        )?
    } else {
        compile_machine_for(&source, options.optimize, options.target)?
    };
    if options.mode == Mode::Assembly {
        let assembly = if options.config.is_some() {
            bfcompiler::backend::assembly::render_configured(&machine, options.target)
        } else {
            match options.target {
                Target::Aarch64Darwin => bfcompiler::backend::assembly::render(&machine),
                Target::Aarch64Linux => {
                    bfcompiler::backend::assembly::render(&machine).replace("_bf_", "bf_")
                        + ".section .note.GNU-stack,\"\",%progbits\n"
                }
                Target::X86_64Darwin => bfcompiler::backend::x86_64::assembly_for(&machine, true),
                Target::X86_64Linux => {
                    bfcompiler::backend::x86_64::assembly_for(&machine, false)
                        + ".section .note.GNU-stack,\"\",@progbits\n"
                }
                Target::X86_64WindowsGnu => {
                    bfcompiler::backend::x86_64::assembly_for(&machine, false)
                }
                Target::I386Linux => bfcompiler::backend::i386::assembly(&machine),
                Target::Riscv64Linux => bfcompiler::backend::riscv64::assembly(&machine),
            }
        };
        atomic_write(&output, assembly.as_bytes(), stamp)?;
        return Ok(());
    }
    let object = write_machine_object(&machine, options.target)?;
    if options.mode == Mode::Object {
        atomic_write(&output, &object, stamp)?;
        return Ok(());
    }

    let temp_dir = env::temp_dir().join(format!("bfcompiler-{}-{stamp}", std::process::id()));
    fs::create_dir(&temp_dir)?;
    let object_path = temp_dir.join("program.o");
    let runtime_path = temp_dir.join("runtime.c");
    let mut executable_path = temporary_output(&output, stamp);
    if options.target == Target::X86_64WindowsGnu {
        let mut name = executable_path.into_os_string();
        name.push(".exe");
        executable_path = name.into();
    }
    let result = (|| {
        fs::write(&object_path, object)?;
        fs::write(&runtime_path, include_str!("../runtime/runtime.c"))?;
        bfcompiler::linker::link(
            &object_path,
            &executable_path,
            &options.linker_args,
            options.verbose,
            options.target,
            options.tape_size,
            options.output_utf8,
        )?;
        fs::rename(&executable_path, &output)?;
        Ok(())
    })();
    let _ = fs::remove_file(&object_path);
    let _ = fs::remove_file(&runtime_path);
    if result.is_err() {
        let _ = fs::remove_file(&executable_path);
    }
    let _ = fs::remove_dir(&temp_dir);
    result
}

fn main() -> ExitCode {
    let options = match parse_args() {
        Ok(Some(options)) => options,
        Ok(None) => return ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("bfcompiler: {e}\n{}", usage());
            return ExitCode::from(2);
        }
    };
    match run(options) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("bfcompiler: {e}");
            ExitCode::from(1)
        }
    }
}
