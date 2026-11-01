pub mod aarch64;
pub mod assembly;
pub mod i386;
pub mod riscv64;
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
    pub config: Option<crate::RuntimeConfig>,
    /// An explicit encoding takes precedence over the legacy UTF-8 flag.
    pub output_encoding: Option<crate::OutputEncoding>,
}

impl MachineCode {
    pub fn config_words(&self) -> Option<[u32; 4]> {
        self.config.map(|config| {
            let encoding = self.output_encoding.unwrap_or(if config.output_utf8 {
                crate::OutputEncoding::Utf8
            } else {
                crate::OutputEncoding::Bytes
            }) as u32;
            // Old runtimes reject the new encoding values rather than using a
            // different output encoding silently. Existing bytes/UTF-8 keep v1.
            [
                if encoding > 1 { 2 } else { 1 },
                config.tape_size,
                config.cell_width.bits(),
                encoding,
            ]
        })
    }
    pub fn config_offset(&self) -> usize {
        (self.bytes.len() + 3) & !3
    }

    pub fn section_bytes(&self) -> Vec<u8> {
        let mut bytes = self.bytes.clone();
        if let Some(words) = self.config_words() {
            bytes.resize(self.config_offset(), 0);
            for value in words {
                bytes.extend(value.to_le_bytes());
            }
        }
        bytes
    }
}
