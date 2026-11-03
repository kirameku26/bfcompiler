# bfcompiler

[![crates.io](https://img.shields.io/crates/v/bfcompiler.svg)](https://crates.io/crates/bfcompiler)
[![CI](https://github.com/kirameku26/bfcompiler/actions/workflows/ci.yml/badge.svg)](https://github.com/kirameku26/bfcompiler/actions/workflows/ci.yml)

Compiles Brainfuck directly to ARM64, x86-64, i386, or RISC-V 64 machine code. It can emit objects or assembly and produce executables using the target platform's toolchain.

English | [日本語](README.ja.md)

## Features

- Machine-code and Mach-O, ELF, or COFF object generation from Brainfuck code.
- Standard and extended modes, with configurable tape length, cell width, and output encoding in extended mode.
- Arithmetic, pointer-movement, and loop optimizations, plus assembly output.

## Requirements

- Build and installation: stable Rust and Cargo with Rust 2024 edition support.
- Executable generation and E2E tests: a C compiler, linker, C runtime, and development headers for the target platform.

| Host | Required toolchain |
| --- | --- |
| Linux | GCC (`cc`), linker, and libc development headers; `build-essential` on Ubuntu/Debian |
| macOS | Xcode Command Line Tools (Apple Clang, linker, and macOS SDK) |
| Windows x86-64 | MinGW-w64 GCC and the Rust GNU toolchain (`stable-x86_64-pc-windows-gnu`); put `gcc` on `PATH` and set `BFC_CC=gcc` |

See the [fixture setup instructions](tests/fixtures/README.md) for test commands and Windows linker settings.

## Installation

Install the published package from [crates.io](https://crates.io/crates/bfcompiler). Cargo downloads and builds it; a local checkout is not required.

Linux/macOS:

```sh
cargo install bfcompiler --locked
bfcompiler --version
```

Windows (PowerShell), after completing the [MinGW-w64 and Rust GNU setup](tests/fixtures/README.md#windows-setup):

```powershell
$env:BFC_CC = "gcc"
$env:CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER = "gcc"
cargo +stable-x86_64-pc-windows-gnu install bfcompiler --locked
bfcompiler --version
```

Add Cargo's bin directory to your `PATH` (normally `~/.cargo/bin` on Linux/macOS or `%USERPROFILE%\.cargo\bin` on Windows). The installed compiler can be used without the source tree.

### From source

To install from a local checkout, run from the project directory:

```sh
cargo install --path . --locked
```

On Windows, use `cargo +stable-x86_64-pc-windows-gnu install --path . --locked` with the GNU linker settings above.

## Usage

Create a program that prints `Hello World!`, then compile and run it. On Linux/macOS:

```sh
printf '++++++++++[>+++++++>++++++++++>+++>+<<<<-]>++.>+.+++++++..+++.>++.<<+++++++++++++++.>.+++.------.--------.>+.>.' > hello.bf
bfcompiler hello.bf -o hello
./hello
```

On Windows (PowerShell), create the same program and run it:

```powershell
Set-Content -Path hello.bf -Encoding ascii -Value '++++++++++[>+++++++>++++++++++>+++>+<<<<-]>++.>+.+++++++..+++.>++.<<+++++++++++++++.>.+++.------.--------.>+.>.'
bfcompiler hello.bf -o hello.exe
.\hello.exe
```

If `--target` is omitted, the compiler selects the host target.

### Supported targets

| Target | CPU | OS | Object format |
| --- | --- | --- | --- |
| `aarch64-apple-darwin` | ARM64 | macOS | Mach-O |
| `x86_64-apple-darwin` | x86-64 | macOS | Mach-O |
| `aarch64-unknown-linux-gnu` | ARM64 | Linux | ELF |
| `x86_64-unknown-linux-gnu` | x86-64 | Linux | ELF |
| `i686-unknown-linux-gnu` | i386 | Linux | ELF32 |
| `riscv64gc-unknown-linux-gnu` | RISC-V 64 (LP64D) | Linux | ELF64 |
| `x86_64-pc-windows-gnu` | x86-64 | Windows (MinGW-w64) | COFF |

The aliases `i386-unknown-linux-gnu` and `riscv64-unknown-linux-gnu` are also accepted.

Windows objects do not support stack unwinding through generated functions. RISC-V jumps beyond approximately ±1 MiB produce a compilation error.

### Object and assembly output

Use `-c` for an object file or `-S` for assembly, without linking:

```sh
bfcompiler -c hello.bf -o hello.o
bfcompiler -S -O2 hello.bf -o hello.s
```

Assembly for i386, RISC-V, and extended mode uses encoded instruction directives and symbolic calls. To manually link host-target output with the runtime, run from the source tree:

```sh
# Object
cc hello.o runtime/runtime.c -o hello
# Assembly
cc hello.s runtime/runtime.c -o hello
```

On Windows, replace `cc` with `gcc` and the output name with `hello.exe`. Extended objects and assembly retain the tape length, cell width, and output encoding; no extra runtime defines are needed.

### Select a target

```sh
bfcompiler --target x86_64-unknown-linux-gnu hello.bf -o hello-linux
bfcompiler --target aarch64-unknown-linux-gnu hello.bf -o hello-linux-arm64
bfcompiler --target i686-unknown-linux-gnu hello.bf -o hello-i386
bfcompiler --target riscv64gc-unknown-linux-gnu hello.bf -o hello-riscv64
bfcompiler --target x86_64-pc-windows-gnu hello.bf -o hello.exe
```

Cross-compiling an executable requires a C compiler, linker, C runtime, and headers for the target. The default C compilers are:

| Cross-compilation target | C compiler |
| --- | --- |
| Linux x86-64 | `x86_64-linux-gnu-gcc` |
| Linux ARM64 | `aarch64-linux-gnu-gcc` |
| Linux i386 | `i686-linux-gnu-gcc` |
| Linux RISC-V 64 | `riscv64-linux-gnu-gcc` |
| Windows x86-64 | `x86_64-w64-mingw32-gcc` |

Set `BFC_CC` to choose a different compiler path. The `-c` and `-S` options do not require a cross-toolchain.

Running an Intel Mac executable on an Apple Silicon Mac requires Rosetta.

## Configuration

### Language modes

Use `--mode extended` to change the tape length, cell width, or output encoding.

| Setting | `standard` (default) | `extended` |
| --- | --- | --- |
| Tape | 30,000 cells | Default 1,000,000 cells; configurable |
| Cell width | 8 bits | 8 (default), 16, or 32 bits |
| Output | Bytes | Bytes, UTF-8, UTF-16, or UTF-32 encoding of each cell value |
| Output formats | Executable, object, assembly | Executable, object, assembly |

Both modes share these semantics:

- Cells are zero-initialized unsigned integers that wrap modulo 2 to the power of the cell width.
- Bytes other than the eight instructions are ignored as comments; unmatched brackets are errors.
- Input reads one byte regardless of cell width and sets the current cell to zero on EOF. Windows uses binary standard I/O.
- Out-of-bounds pointer movement is a runtime error with exit code 2.

Compile `hello.bf` to `hello` in extended mode with 2,000,000 cells, 32-bit cells, and UTF-8 output:

```sh
bfcompiler --mode extended --tape-size 2000000 --cell-bits 32 --output-encoding utf-8 hello.bf -o hello
```

`--tape-size` accepts 1 to 2,147,483,647 cells, subject to the target address space and available memory.
`--cell-bits` selects a shared width for the entire tape.

### Output encoding

Unicode output modes encode the cell value as a Unicode scalar value. Surrogates and values above U+10FFFF cause a runtime error with exit code 2. Use `bytes` for programs that already produce encoded byte sequences.

| `--output-encoding` | Output |
| --- | --- |
| `bytes` | Low 8 bits (default) |
| `utf-8` | UTF-8 |
| `utf-16`, `utf-16le` | UTF-16, little-endian |
| `utf-16be` | UTF-16, big-endian |
| `utf-32`, `utf-32le` | UTF-32, little-endian |
| `utf-32be` | UTF-32, big-endian |

No BOM is added automatically.
Byte order is independent of the target CPU.
UTF-16 encodes supplementary characters as surrogate pairs.

Use 32-bit cells for values above U+FFFF.

Compile `hello.bf` to `hello` in extended mode with 32-bit cells and UTF-16LE output:

```sh
bfcompiler --mode extended --cell-bits 32 --output-encoding utf-16le hello.bf -o hello
```

### Options

| Option | Description |
| --- | --- |
| `-o <path>` | Set the output file |
| `-c` | Emit an object file |
| `-S` | Emit assembly |
| `--mode <standard\|extended>` | Select language mode (default: `standard`) |
| `--tape-size <cells>` | Set tape length in extended mode |
| `--cell-bits <8\|16\|32>` | Set the width of all cells in extended mode (default: `8`) |
| `--output-encoding <encoding>` | Select output encoding for `.` in extended mode (see table above) |
| `-O0`, `--no-optimize` | Disable IR optimization |
| `-O`, `-O1`, `-O2`, `-O3`, `-Os`, `-Oz` | Enable basic IR optimization |
| `--target <triple>` | Select the target CPU and OS |
| `-L<dir>`, `-l<name>`, `-Wl,<arg>,...` | Specify libraries and linker arguments |
| `-v` | Show the link command |
| `-h`, `--help` | Show help |
| `--version` | Show the version |

Optimization is enabled by default: arithmetic merging, grouped pointer moves, clear loops (`[-]`, `[+]`), and adjacent-cell transfers (`[->+<]`, `[->-<]` and their leftward equivalents). Boundary checks and zero-cell loop skipping are preserved. All nonzero optimization levels currently apply the same optimizations.

If `-o` is omitted, the executable name is the input filename without its extension. Windows executables use `.exe`; `-c` and `-S` replace the input extension with `.o` and `.s`, respectively.

## Development

Build and check from the project directory:

```sh
cargo build --release
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

The binary is `target/release/bfcompiler` (`bfcompiler.exe` on Windows). Tests compare generated programs against a reference interpreter.

Run all [Brainfuck fixtures](tests/fixtures/README.md) with `cargo test --test fixtures -- --nocapture`.
New `.bf` files with matching `.out` files under `tests/fixtures` are tested automatically at `-O0` and `-O2`.

Run the transfer-loop timing sample with `cargo test --test performance -- --ignored --nocapture`; it includes process startup.

## License

[MIT](LICENSE)
