pub mod aarch64;
pub mod assembly;
pub mod x86_64;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Relocation {
    pub offset: u32,
    pub symbol: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MachineCode {
    pub bytes: Vec<u8>,
    pub relocations: Vec<Relocation>,
}
