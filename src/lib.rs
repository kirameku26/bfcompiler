pub mod backend;
pub mod frontend;
pub mod ir;
pub mod linker;
pub mod object;
pub mod optimizer;

pub use optimizer::CellWidth;
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RuntimeConfig {
    /// Number of cells, rather than bytes.
    pub tape_size: u32,
    pub cell_width: CellWidth,
    /// Legacy bytes/UTF-8 selection. Use `compile_machine_with_encoding` or
    /// `compile_object_with_encoding` for an explicit output encoding.
    pub output_utf8: bool,
}

/// Output byte order is explicit and independent of the host or target CPU.
/// Unicode encodings do not emit a byte-order mark automatically.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum OutputEncoding {
    Bytes = 0,
    Utf8 = 1,
    Utf16Le = 2,
    Utf16Be = 3,
    Utf32Le = 4,
    Utf32Be = 5,
}

impl OutputEncoding {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "bytes" => Some(Self::Bytes),
            "utf-8" | "utf8" => Some(Self::Utf8),
            "utf-16" | "utf16" | "utf-16le" | "utf16le" => Some(Self::Utf16Le),
            "utf-16be" | "utf16be" => Some(Self::Utf16Be),
            "utf-32" | "utf32" | "utf-32le" | "utf32le" => Some(Self::Utf32Le),
            "utf-32be" | "utf32be" => Some(Self::Utf32Be),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub enum CompileError {
    Syntax(frontend::SyntaxError),
    Codegen(String),
    Io(std::io::Error),
    Link(String),
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Syntax(e) => write!(f, "{e}"),
            Self::Codegen(e) => write!(f, "code generation error: {e}"),
            Self::Io(e) => write!(f, "I/O error: {e}"),
            Self::Link(e) => write!(f, "link error: {e}"),
        }
    }
}

impl std::error::Error for CompileError {}

impl From<std::io::Error> for CompileError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Target {
    Aarch64Darwin,
    X86_64Darwin,
    Aarch64Linux,
    X86_64Linux,
    I386Linux,
    Riscv64Linux,
    X86_64WindowsGnu,
}

impl Target {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "aarch64-apple-darwin" => Some(Self::Aarch64Darwin),
            "x86_64-apple-darwin" => Some(Self::X86_64Darwin),
            "aarch64-unknown-linux-gnu" => Some(Self::Aarch64Linux),
            "x86_64-unknown-linux-gnu" => Some(Self::X86_64Linux),
            "i386-unknown-linux-gnu" | "i686-unknown-linux-gnu" => Some(Self::I386Linux),
            "riscv64gc-unknown-linux-gnu" | "riscv64-unknown-linux-gnu" => Some(Self::Riscv64Linux),
            "x86_64-pc-windows-gnu" => Some(Self::X86_64WindowsGnu),
            _ => None,
        }
    }
    pub fn host_default() -> Option<Self> {
        if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
            Some(Self::X86_64Darwin)
        } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            Some(Self::Aarch64Darwin)
        } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
            Some(Self::X86_64Linux)
        } else if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
            Some(Self::Aarch64Linux)
        } else if cfg!(all(target_os = "linux", target_arch = "x86")) {
            Some(Self::I386Linux)
        } else if cfg!(all(target_os = "linux", target_arch = "riscv64")) {
            Some(Self::Riscv64Linux)
        } else if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
            Some(Self::X86_64WindowsGnu)
        } else {
            None
        }
    }
}

pub fn compile_machine_for(
    source: &[u8],
    optimize: bool,
    target: Target,
) -> Result<backend::MachineCode, CompileError> {
    compile_with_width(source, optimize, target, CellWidth::Bits8)
}

fn compile_with_width(
    source: &[u8],
    optimize: bool,
    target: Target,
    width: CellWidth,
) -> Result<backend::MachineCode, CompileError> {
    let mut program = frontend::parse(source).map_err(CompileError::Syntax)?;
    if optimize {
        program = optimizer::optimize_with_width(program, width);
    }
    match target {
        Target::Aarch64Darwin | Target::Aarch64Linux => {
            backend::aarch64::compile_with_width(&program, width)
        }
        Target::X86_64Darwin | Target::X86_64Linux => {
            backend::x86_64::compile_with_width(&program, false, width)
        }
        Target::X86_64WindowsGnu => backend::x86_64::compile_with_width(&program, true, width),
        Target::I386Linux => backend::i386::compile_with_width(&program, width),
        Target::Riscv64Linux => backend::riscv64::compile_with_width(&program, width),
    }
    .map_err(CompileError::Codegen)
}

/// Compile an extended program and retain its runtime configuration.
/// The generated `bf_program` receives a zero-initialized, suitably aligned tape
/// and its length in bytes. Object writers export the configuration as `bf_config`.
pub fn compile_machine_with_config(
    source: &[u8],
    optimize: bool,
    target: Target,
    config: RuntimeConfig,
) -> Result<backend::MachineCode, CompileError> {
    if config.tape_size == 0 || config.tape_size > i32::MAX as u32 {
        return Err(CompileError::Codegen(
            "tape size must be from 1 to 2147483647".into(),
        ));
    }
    if target == Target::I386Linux
        && u64::from(config.tape_size) * u64::from(config.cell_width.bytes()) > u64::from(u32::MAX)
    {
        return Err(CompileError::Codegen(
            "tape size and cell width exceed the i386 address space".into(),
        ));
    }
    let mut code = compile_with_width(source, optimize, target, config.cell_width)?;
    code.config = Some(config);
    Ok(code)
}

/// The explicit encoding overrides the legacy `RuntimeConfig::output_utf8` flag.
pub fn compile_machine_with_encoding(
    source: &[u8],
    optimize: bool,
    target: Target,
    config: RuntimeConfig,
    encoding: OutputEncoding,
) -> Result<backend::MachineCode, CompileError> {
    let mut code = compile_machine_with_config(source, optimize, target, config)?;
    code.output_encoding = Some(encoding);
    Ok(code)
}

pub fn compile_machine(
    source: &[u8],
    optimize: bool,
) -> Result<backend::MachineCode, CompileError> {
    compile_machine_for(source, optimize, Target::Aarch64Darwin)
}

pub fn compile_object(source: &[u8], optimize: bool) -> Result<Vec<u8>, CompileError> {
    let machine = compile_machine(source, optimize)?;
    object::macho::write_object(&machine).map_err(CompileError::Codegen)
}

pub fn compile_object_for(
    source: &[u8],
    optimize: bool,
    target: Target,
) -> Result<Vec<u8>, CompileError> {
    let machine = compile_machine_for(source, optimize, target)?;
    write_machine_object(&machine, target)
}

pub fn compile_object_with_config(
    source: &[u8],
    optimize: bool,
    target: Target,
    config: RuntimeConfig,
) -> Result<Vec<u8>, CompileError> {
    let machine = compile_machine_with_config(source, optimize, target, config)?;
    write_machine_object(&machine, target)
}

pub fn compile_object_with_encoding(
    source: &[u8],
    optimize: bool,
    target: Target,
    config: RuntimeConfig,
    encoding: OutputEncoding,
) -> Result<Vec<u8>, CompileError> {
    let machine = compile_machine_with_encoding(source, optimize, target, config, encoding)?;
    write_machine_object(&machine, target)
}

pub fn write_machine_object(
    machine: &backend::MachineCode,
    target: Target,
) -> Result<Vec<u8>, CompileError> {
    match target {
        Target::Aarch64Darwin | Target::X86_64Darwin => {
            object::macho::write_object_for(machine, target == Target::X86_64Darwin)
        }
        Target::Aarch64Linux | Target::X86_64Linux => {
            object::elf::write_object(machine, target == Target::Aarch64Linux)
        }
        Target::X86_64WindowsGnu => object::coff::write_object(machine),
        Target::I386Linux => object::elf32::write_object(machine),
        Target::Riscv64Linux => object::elf::write_object_for(machine, 243),
    }
    .map_err(CompileError::Codegen)
}
