use crate::CompileError;
use std::ffi::OsString;
use std::path::Path;
use std::process::Command;

pub fn link(
    object: &Path,
    output: &Path,
    args: &[OsString],
    verbose: bool,
    target: crate::Target,
    tape_size: u32,
    output_utf8: bool,
) -> Result<(), CompileError> {
    let runtime = object.with_file_name("runtime.c");
    let compiler = std::env::var_os("BFC_CC").unwrap_or_else(|| {
        let native = crate::Target::host_default() == Some(target);
        let name = match target {
            crate::Target::Aarch64Darwin | crate::Target::X86_64Darwin
                if cfg!(target_os = "macos") =>
            {
                "cc"
            }
            _ if native => "cc",
            crate::Target::X86_64Linux => "x86_64-linux-gnu-gcc",
            crate::Target::Aarch64Linux => "aarch64-linux-gnu-gcc",
            crate::Target::X86_64WindowsGnu => "x86_64-w64-mingw32-gcc",
            _ => "unavailable-darwin-cross-compiler",
        };
        name.into()
    });
    let mut command = Command::new(&compiler);
    if matches!(
        target,
        crate::Target::Aarch64Darwin | crate::Target::X86_64Darwin
    ) {
        command
            .arg("-arch")
            .arg(if target == crate::Target::Aarch64Darwin {
                "arm64"
            } else {
                "x86_64"
            });
    }
    command
        .arg(format!("-DBFC_TAPE_SIZE={tape_size}"))
        .arg(format!("-DBFC_OUTPUT_UTF8={}", u8::from(output_utf8)))
        .arg(&runtime)
        .arg(object)
        .args(args)
        .arg("-o")
        .arg(output);
    if verbose {
        eprintln!("bfcompile: {command:?}");
    }
    let result = command.output().map_err(|e| {
        CompileError::Link(format!(
            "cannot run {:?}: {e}; install the target toolchain or set BFC_CC",
            compiler
        ))
    })?;
    if !result.status.success() {
        let diagnostic = String::from_utf8_lossy(&result.stderr).trim().to_owned();
        return Err(CompileError::Link(if diagnostic.is_empty() {
            format!("cc exited with {}", result.status)
        } else {
            diagnostic
        }));
    }
    Ok(())
}
