# bfcompile

Compiles Brainfuck directly to ARM64 or x86-64 machine code. It can emit objects or assembly and produce executables using the target platform's toolchain.

English | [日本語](README.ja.md)

## Features

- Direct machine-code and Mach-O, ELF, or COFF object generation.
- Standard and extended modes, with configurable tape size and output encoding in extended mode.
- Arithmetic, pointer-movement, and loop optimizations, plus assembly output.

## Requirements

- Build and installation: Rust and Cargo with Rust 2024 edition support.
- Executable generation: a C compiler and linker for the target platform.

## Installation

Install from the project directory:

```sh
cargo install --path .
bfcompile --version
```

Add Cargo's bin directory to your `PATH` (normally `~/.cargo/bin`, or the Cargo bin directory in your Windows user profile). The installed compiler can be used without the source tree.

## Usage

Compile and run `hello.bf`:

```sh
bfcompile hello.bf -o hello
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
| `x86_64-pc-windows-gnu` | x86-64 | Windows (MinGW-w64) | COFF |

Linux and Windows objects have been inspected and cross-assembled; final linking and execution on those operating systems remain unverified. Windows objects do not support stack unwinding through generated functions.

### Object and assembly output

Use `-c` for an object file or `-S` for assembly, without linking:

```sh
bfcompile -c hello.bf -o hello.o
bfcompile -S -O2 hello.bf -o hello.s
```

Assembly uses instruction mnemonics and branch labels. From the source tree, assemble and link host-target output with:

```sh
cc hello.s runtime/runtime.c -o hello
```

### Select a target

```sh
bfcompile --target x86_64-unknown-linux-gnu hello.bf -o hello-linux
bfcompile --target aarch64-unknown-linux-gnu hello.bf -o hello-linux-arm64
bfcompile --target x86_64-pc-windows-gnu hello.bf -o hello.exe
```

Cross-compiling an executable requires a C compiler, linker, C runtime, and headers for the target. The default C compilers are:

| Cross-compilation target | C compiler |
| --- | --- |
| Linux x86-64 | `x86_64-linux-gnu-gcc` |
| Linux ARM64 | `aarch64-linux-gnu-gcc` |
| Windows x86-64 | `x86_64-w64-mingw32-gcc` |

Set `BFC_CC` to choose a different compiler path. The `-c` and `-S` options do not require a cross-toolchain.

Running an Intel Mac executable on an Apple Silicon Mac requires Rosetta.

## Configuration

### Language modes

| Setting | `standard` (default) | `extended` |
| --- | --- | --- |
| Tape | Fixed 30,000 cells | Default 1,000,000 cells; configurable |
| Output | Bytes | Bytes or UTF-8 encoding of each cell value |
| Output formats | Executable, object, assembly | Executable |

Both modes share these semantics:

- Cells are zero-initialized unsigned 8-bit integers that wrap modulo 256.
- Bytes other than the eight instructions are ignored as comments; unmatched brackets are errors.
- Input reads bytes and sets the current cell to zero on EOF. Windows uses binary standard I/O.
- Out-of-bounds pointer movement is a runtime error with exit code 2.

Select extended mode to change the tape size or output encoding:

```sh
bfcompile --mode extended --tape-size 2000000 --output-encoding utf-8 hello.bf -o hello
```

`--tape-size` accepts 1 to 2,147,483,647 cells. `--output-encoding utf-8` encodes each cell value as U+0000 through U+00FF; for example, 255 becomes `C3 BF`. A single cell cannot represent Japanese characters. Use the default `bytes` mode for programs that output a UTF-8 byte sequence.

Extension options require `--mode extended`, which cannot be combined with `-c` or `-S`. For manual linking, pass `-DBFC_TAPE_SIZE=1000000` and optionally `-DBFC_OUTPUT_UTF8=1` to the C compiler to configure the runtime. Without overrides, the runtime uses standard-mode settings.

### Options

| Option | Description |
| --- | --- |
| `-o <path>` | Set the output file |
| `-c` | Emit an object file |
| `-S` | Emit assembly |
| `--mode <standard\|extended>` | Select language mode (default: `standard`) |
| `--tape-size <cells>` | Set tape length in extended mode |
| `--output-encoding <bytes\|utf-8>` | Select output encoding for `.` in extended mode (default: `bytes`) |
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

The binary is `target/release/bfcompile` (`bfcompile.exe` on Windows). Tests compare generated programs against a reference interpreter. Run the transfer-loop timing sample with `cargo test --test performance -- --ignored --nocapture`; it includes process startup.

## License

[MIT](LICENSE)
