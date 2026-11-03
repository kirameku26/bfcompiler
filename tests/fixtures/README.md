# Brainfuck fixtures

English | [日本語](README.ja.md)

## Requirements

Run commands from the repository root. Cargo builds the compiler used by the tests; a separate `cargo install` is unnecessary.

- Stable Rust and Cargo with Rust 2024 edition support.
- A C compiler, linker, C runtime, and development headers for the host platform:

| Host | Required tools and setup |
| --- | --- |
| Linux | GCC (`cc`), linker, and libc development headers. On Ubuntu/Debian: `sudo apt-get install -y build-essential` |
| macOS | Xcode Command Line Tools (Apple Clang, linker, and macOS SDK): `xcode-select --install` |
| Windows x86-64 | MinGW-w64 GCC and the Rust GNU toolchain. Use the Windows setup below |

Use `BFC_CC` to select a different C compiler. Cross-target execution requires the additional tools described below.

## Run

On Linux or macOS, run all `.bf` files, including subdirectories, with:

```sh
cargo test --test fixtures -- --nocapture
```

Each program runs at `-O0` and `-O2` with a five-second execution timeout.
Failures list the filename and optimization level; the remaining programs still run.

### Windows setup

Install MSYS2, then install GCC in its MINGW64 terminal:

```sh
pacman -S --needed mingw-w64-x86_64-gcc
```

With Rustup installed, run the following in PowerShell from the repository root.
The `PATH` example assumes MSYS2 is installed at `C:\msys64`; adjust it to your installation.

```powershell
rustup toolchain install stable-x86_64-pc-windows-gnu --profile minimal
$env:PATH = "C:\msys64\mingw64\bin;$env:PATH"
$env:BFC_CC = "gcc"
$env:CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER = "gcc"
cargo +stable-x86_64-pc-windows-gnu test --test fixtures -- --nocapture
```

### Linux cross-target execution

For i686 and RISC-V 64, install QEMU user mode, the cross-compilers, and the target C runtime development packages. On Ubuntu/Debian:

```sh
sudo apt-get install -y qemu-user gcc-i686-linux-gnu libc6-dev-i386-cross gcc-riscv64-linux-gnu libc6-dev-riscv64-cross
BFC_TEST_LINUX_CROSS=i686 cargo test --test fixtures -- --nocapture
BFC_TEST_LINUX_CROSS=riscv64 cargo test --test fixtures -- --nocapture
```

The runner uses `qemu-i386` with `/usr/i686-linux-gnu`, or `qemu-riscv64` with `/usr/riscv64-linux-gnu`. These paths and the compiler names match the CI configuration.

## Add a fixture

Add a `.bf` file and companion files with the same name:

| Extension | Contents |
| --- | --- |
| `.out` | Required expected stdout, compared as exact bytes; use an empty file for no output |
| `.in` | Optional stdin bytes; omitted means immediate EOF |
| `.args` | Optional whitespace-separated compiler options, such as `--mode=extended --cell-bits=16`; omit input/output filenames, target, and optimization options |
| `.status` | Optional expected process exit code; defaults to `0` |

For example, `echo.bf`, `echo.in`, and `echo.out` form one test case.
New programs are discovered automatically without editing Rust code.
The local `.gitattributes` preserves input/output bytes on Windows.

`standard/` covers comments, arithmetic, loops, byte input, EOF, and newline input.
`extended/` covers wider cells and Unicode output; `errors/` checks runtime errors.
`targets.bf` is also used by the cross-target object tests.

## CI

The Linux, Windows, and macOS CI jobs run these fixtures through `cargo test`.
The Linux cross jobs also run them with `BFC_TEST_LINUX_CROSS=i686` or `riscv64` using the cross-compilers and QEMU user mode.
