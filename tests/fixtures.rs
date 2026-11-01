use std::{
    fs::{self, File},
    io,
    path::{Path, PathBuf},
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
            "bfcompiler-fixtures-{}-{stamp}",
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

fn discover(directory: &Path, sources: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_dir() {
            discover(&entry.path(), sources)?;
        } else if kind.is_file() && entry.path().extension().is_some_and(|ext| ext == "bf") {
            sources.push(entry.path());
        }
    }
    Ok(())
}

fn optional_bytes(path: &Path) -> io::Result<Vec<u8>> {
    match fs::read(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
        result => result,
    }
}

struct Fixture {
    source: PathBuf,
    expected: Vec<u8>,
    options: Vec<String>,
    status: i32,
}

impl Fixture {
    fn load(source: &Path) -> Result<Self, String> {
        let output = source.with_extension("out");
        let expected = fs::read(&output)
            .map_err(|error| format!("cannot read required {}: {error}", output.display()))?;
        let options = optional_bytes(&source.with_extension("args")).map_err(|e| e.to_string())?;
        let options = String::from_utf8(options).map_err(|e| e.to_string())?;
        let status = optional_bytes(&source.with_extension("status")).map_err(|e| e.to_string())?;
        let status = String::from_utf8(status).map_err(|e| e.to_string())?;
        let status = if status.is_empty() {
            0
        } else {
            status.trim().parse::<i32>().map_err(|e| e.to_string())?
        };
        Ok(Self {
            source: source.to_owned(),
            expected,
            options: options.split_whitespace().map(str::to_owned).collect(),
            status,
        })
    }
}

struct CrossTarget {
    triple: &'static str,
    compiler: &'static str,
    emulator: &'static str,
    sysroot: &'static str,
}

fn cross_target() -> Option<CrossTarget> {
    std::env::var_os("BFC_TEST_LINUX_CROSS").map(|value| match value.to_str() {
        Some("i686") => CrossTarget {
            triple: "i686-unknown-linux-gnu",
            compiler: "i686-linux-gnu-gcc",
            emulator: "qemu-i386",
            sysroot: "/usr/i686-linux-gnu",
        },
        Some("riscv64") => CrossTarget {
            triple: "riscv64gc-unknown-linux-gnu",
            compiler: "riscv64-linux-gnu-gcc",
            emulator: "qemu-riscv64",
            sysroot: "/usr/riscv64-linux-gnu",
        },
        _ => panic!("BFC_TEST_LINUX_CROSS must be i686 or riscv64"),
    })
}

fn run_fixture(
    fixture: &Fixture,
    optimization: &str,
    directory: &Path,
    cross: Option<&CrossTarget>,
) -> Result<(), String> {
    let executable = directory.join(if cfg!(windows) {
        "program.exe"
    } else {
        "program"
    });
    let mut compiler = Command::new(env!("CARGO_BIN_EXE_bfcompiler"));
    if let Some(target) = cross {
        compiler
            .args(["--target", target.triple])
            .env("BFC_CC", target.compiler);
    }
    let compiled = compiler
        .args(&fixture.options)
        .arg(optimization)
        .arg(&fixture.source)
        .arg("-o")
        .arg(&executable)
        .output()
        .map_err(|e| e.to_string())?;
    if !compiled.status.success() {
        return Err(format!(
            "compilation failed ({}): {}",
            compiled.status,
            String::from_utf8_lossy(&compiled.stderr)
        ));
    }
    let input = match File::open(fixture.source.with_extension("in")) {
        Ok(file) => Stdio::from(file),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Stdio::null(),
        Err(error) => return Err(error.to_string()),
    };
    // Files keep large inputs and outputs from deadlocking on pipe capacity.
    let output_path = directory.join("stdout");
    let error_path = directory.join("stderr");
    let output_file = File::create(&output_path).map_err(|e| e.to_string())?;
    let error_file = File::create(&error_path).map_err(|e| e.to_string())?;
    let mut command = if let Some(target) = cross {
        let mut command = Command::new(target.emulator);
        command.args(["-L", target.sysroot]).arg(&executable);
        command
    } else {
        Command::new(&executable)
    };
    let mut child = command
        .stdin(input)
        .stdout(output_file)
        .stderr(error_file)
        .spawn()
        .map_err(|e| e.to_string())?;
    let deadline = Instant::now() + Duration::from_secs(5);
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err("execution timed out after 5 seconds".into());
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    let stderr = fs::read(&error_path).map_err(|e| e.to_string())?;
    if status.code() != Some(fixture.status) {
        return Err(format!(
            "expected exit {}, got {status}: {}",
            fixture.status,
            String::from_utf8_lossy(&stderr)
        ));
    }
    let output = fs::read(output_path).map_err(|e| e.to_string())?;
    if output != fixture.expected {
        return Err(format!(
            "stdout differs: expected {:?}, got {output:?}",
            fixture.expected
        ));
    }
    Ok(())
}

#[test]
fn all_brainfuck_fixtures() {
    let cross = cross_target();
    if cross.is_none() && bfcompiler::Target::host_default().is_none() {
        eprintln!("host target is not supported; fixture execution skipped");
        return;
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut sources = Vec::new();
    discover(&root, &mut sources).unwrap();
    sources.sort();
    assert!(!sources.is_empty(), "no Brainfuck fixtures found");
    let directory = TestDir::new();
    let mut failures = Vec::new();
    for source in &sources {
        let name = source.strip_prefix(&root).unwrap().display();
        let fixture = match Fixture::load(source) {
            Ok(fixture) => fixture,
            Err(error) => {
                failures.push(format!("{name}: {error}"));
                continue;
            }
        };
        for optimization in ["-O0", "-O2"] {
            match run_fixture(&fixture, optimization, &directory.0, cross.as_ref()) {
                Ok(()) => eprintln!("{name} {optimization}: ok"),
                Err(error) => failures.push(format!("{name} {optimization}: {error}")),
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    eprintln!("{} fixtures passed at -O0 and -O2", sources.len());
}
