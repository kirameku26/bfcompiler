# Brainfuck fixtures

Run all `.bf` files, including subdirectories, with:

```sh
cargo test --test fixtures -- --nocapture
```

Each program runs at `-O0` and `-O2` with a five-second execution timeout.
Failures list the filename and optimization level; the remaining programs still run.

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

The normal Linux, Windows, and macOS CI jobs run these fixtures through `cargo test`.
The Linux cross jobs also run them with `BFC_TEST_LINUX_CROSS=i686` or `riscv64`
using the existing cross-compilers and QEMU user-mode configuration.
