pub mod backend;
pub mod frontend;
pub mod ir;
pub mod linker;
pub mod object;
pub mod optimizer;

use std::fmt;

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
    X86_64WindowsGnu,
}

impl Target {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "aarch64-apple-darwin" => Some(Self::Aarch64Darwin),
            "x86_64-apple-darwin" => Some(Self::X86_64Darwin),
            "aarch64-unknown-linux-gnu" => Some(Self::Aarch64Linux),
            "x86_64-unknown-linux-gnu" => Some(Self::X86_64Linux),
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
    let mut program = frontend::parse(source).map_err(CompileError::Syntax)?;
    if optimize {
        program = optimizer::optimize(program);
    }
    match target {
        Target::Aarch64Darwin | Target::Aarch64Linux => backend::aarch64::compile(&program),
        Target::X86_64Darwin | Target::X86_64Linux => backend::x86_64::compile_for(&program, false),
        Target::X86_64WindowsGnu => backend::x86_64::compile_for(&program, true),
    }
    .map_err(CompileError::Codegen)
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
    match target {
        Target::Aarch64Darwin | Target::X86_64Darwin => {
            object::macho::write_object_for(&machine, target == Target::X86_64Darwin)
        }
        Target::Aarch64Linux | Target::X86_64Linux => {
            object::elf::write_object(&machine, target == Target::Aarch64Linux)
        }
        Target::X86_64WindowsGnu => object::coff::write_object(&machine),
    }
    .map_err(CompileError::Codegen)
}
