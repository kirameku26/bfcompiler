# bfcompiler

Compiles Brainfuck directly to ARM64, x86-64, i386, or RISC-V 64 machine code. It can emit objects or assembly and produce executables using the target platform's toolchain.

English | [日本語](README.ja.md)

## Features

- Direct machine-code and Mach-O, ELF, or COFF object generation.
- Standard and extended modes, with configurable tape length, cell width, and output encoding in extended mode.
- Arithmetic, pointer-movement, and loop optimizations, plus assembly output.

## Requirements

- Build and installation: Rust and Cargo with Rust 2024 edition support.
- Executable generation: a C compiler and linker for the target platform.

## Installation

Install from the project directory:

```sh
cargo install --path .
bfcompiler --version
```

Add Cargo's bin directory to your `PATH` (normally `~/.cargo/bin`, or the Cargo bin directory in your Windows user profile). The installed compiler can be used without the source tree.

## Usage

Create a program that prints `A`, then compile and run it:

```sh
printf '++++++++[>++++++++<-]>+.' > hello.bf
bfcompiler hello.bf -o hello
./hello
```

If `--target` is omitted, the compiler selects the host target. On Windows, use `-o hello.exe` and run `hello.exe`.

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

- Windows objects do not support stack unwinding through generated functions.
- RISC-V jumps beyond approximately ±1 MiB produce a compilation error.

### Object and assembly output

Use `-c` for an object file or `-S` for assembly, without linking:

```sh
bfcompiler -c hello.bf -o hello.o
bfcompiler -S -O2 hello.bf -o hello.s
```

Assembly for i386, RISC-V, and extended mode uses encoded instruction directives and symbolic calls. From the source tree, assemble and link host-target output with:

```sh
cc hello.s runtime/runtime.c -o hello
```

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

| Setting | `standard` (default) | `extended` |
| --- | --- | --- |
| Tape | Fixed 30,000 cells | Default 1,000,000 cells; configurable |
| Cell width | 8 bits | 8 (default), 16, or 32 bits |
| Output | Bytes | Bytes, UTF-8, UTF-16, or UTF-32 encoding of each cell value |
| Output formats | Executable, object, assembly | Executable, object, assembly |

Both modes share these semantics:

- Cells are zero-initialized unsigned integers that wrap modulo 2 to the power of the cell width.
- Bytes other than the eight instructions are ignored as comments; unmatched brackets are errors.
- Input reads bytes and sets the current cell to zero on EOF. Windows uses binary standard I/O.
- Out-of-bounds pointer movement is a runtime error with exit code 2.

Select extended mode to change the tape length, cell width, or output encoding:

```sh
bfcompiler --mode extended --tape-size 2000000 --cell-bits 32 --output-encoding utf-8 hello.bf -o hello
```

`--tape-size` accepts 1 to 2,147,483,647 cells, subject to the target address space and available memory. `--cell-bits` selects a shared width for the entire tape; `,` still reads one byte into a cell.

In `bytes` mode, `.` outputs the low 8 bits. Unicode output modes encode the cell value as a Unicode scalar value. Surrogates and values above U+10FFFF cause a runtime error with exit code 2. Use `bytes` for programs that already produce encoded byte sequences.

| `--output-encoding` | Output |
| --- | --- |
| `bytes` | Low 8 bits (default) |
| `utf-8` | UTF-8 |
| `utf-16`, `utf-16le` | UTF-16, little-endian |
| `utf-16be` | UTF-16, big-endian |
| `utf-32`, `utf-32le` | UTF-32, little-endian |
| `utf-32be` | UTF-32, big-endian |

No BOM is added automatically. Byte order is independent of the target CPU. UTF-16 encodes supplementary characters as surrogate pairs; use 32-bit cells for values above U+FFFF. For example:

```sh
bfcompiler --mode extended --cell-bits 32 --output-encoding utf-16le hello.bf -o hello
```

Extension options require `--mode extended`. Executables, objects, and assembly retain the tape length, cell width, and output encoding. For manual linking, use `cc hello.o runtime/runtime.c -o hello` (or `hello.s`); no extra runtime defines are needed for extended output.

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

The binary is `target/release/bfcompiler` (`bfcompiler.exe` on Windows). Tests compare generated programs against a reference interpreter. See [AGENTS.md](AGENTS.md) for development guidelines and optional cross-target checks.

Run the transfer-loop timing sample with `cargo test --test performance -- --ignored --nocapture`; it includes process startup.

## License

[MIT](LICENSE)
