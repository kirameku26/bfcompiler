# bfcompiler

BrainfuckのコードをARM64・x86-64・i386・RISC-V 64の機械語へコンパイルします。オブジェクトやアセンブリの出力に加え、対象環境のツールチェーンで実行ファイルを生成できます。

[English](README.md) | 日本語

## 主な機能

- Brainfuckコードから機械語とMach-O・ELF・COFFオブジェクトを生成
- 標準・拡張モードの切り替えと、拡張モードでのテープ長・セル幅・出力方式の指定
- 算術演算・ポインタ移動・ループの最適化とアセンブリ出力

## 必要な環境

- ビルド・インストール: Rust 2024エディションに対応したstableのRustとCargo
- 実行ファイルの生成・E2Eテスト: 対象環境のCコンパイラ、リンカ、Cランタイム、開発用ヘッダー

| ホスト | 必要なツールチェーン |
| --- | --- |
| Linux | GCC (`cc`)、リンカ、libc開発用ヘッダー。Ubuntu/Debianでは `build-essential` |
| macOS | Xcode Command Line Tools (Apple Clang、リンカ、macOS SDK) |
| Windows x86-64 | MinGW-w64 GCCとRust GNUツールチェーン (`stable-x86_64-pc-windows-gnu`)。`gcc` を `PATH` に追加し、`BFC_CC=gcc` を設定 |

テストの実行方法とWindowsのリンカ設定は、[fixturesの環境構築手順](tests/fixtures/README.ja.md) を参照してください。

## インストール

プロジェクトのディレクトリから、環境に応じてインストールします。

Linux・macOS:

```sh
cargo install --path .
bfcompiler --version
```

Windows (PowerShell)では、[MinGW-w64とRust GNUの環境構築](tests/fixtures/README.ja.md#windowsの環境構築)を済ませてから実行します。

```powershell
$env:BFC_CC = "gcc"
$env:CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER = "gcc"
cargo +stable-x86_64-pc-windows-gnu install --path .
bfcompiler --version
```

Cargoのbinディレクトリを `PATH` に追加してください(通常はLinux・macOSで `~/.cargo/bin`、Windowsで `%USERPROFILE%\.cargo\bin`)。インストール後はソースツリーなしで利用できます。

## 使い方

`Hello World!` を出力するプログラムを作成し、コンパイルして実行します。Linux・macOSでの例です。

```sh
printf '++++++++++[>+++++++>++++++++++>+++>+<<<<-]>++.>+.+++++++..+++.>++.<<+++++++++++++++.>.+++.------.--------.>+.>.' > hello.bf
bfcompiler hello.bf -o hello
./hello
```

Windows (PowerShell)では、リポジトリのルートからサンプルをコピーして実行します。

```powershell
Copy-Item tests/fixtures/standard/hello_world.bf hello.bf
bfcompiler hello.bf -o hello.exe
.\hello.exe
```

`--target` の既定はホスト環境です。

### 対応ターゲット

| ターゲット | CPU | OS | オブジェクト形式 |
| --- | --- | --- | --- |
| `aarch64-apple-darwin` | ARM64 | macOS | Mach-O |
| `x86_64-apple-darwin` | x86-64 | macOS | Mach-O |
| `aarch64-unknown-linux-gnu` | ARM64 | Linux | ELF |
| `x86_64-unknown-linux-gnu` | x86-64 | Linux | ELF |
| `i686-unknown-linux-gnu` | i386 | Linux | ELF32 |
| `riscv64gc-unknown-linux-gnu` | RISC-V 64(LP64D) | Linux | ELF64 |
| `x86_64-pc-windows-gnu` | x86-64 | Windows(MinGW-w64) | COFF |

`i386-unknown-linux-gnu` と `riscv64-unknown-linux-gnu` も別名として指定できます。

Windows向けの生成関数を通じたスタックのアンワインドはサポートしません。RISC-Vの分岐距離が約±1 MiBを超える場合はコンパイルエラーになります。

### オブジェクト・アセンブリの出力

`-c` でオブジェクト、`-S` でアセンブリを出力し、リンクを省略します。

```sh
bfcompiler -c hello.bf -o hello.o
bfcompiler -S -O2 hello.bf -o hello.s
```

i386・RISC-Vおよび拡張モードのアセンブリでは、命令を数値ディレクティブで、関数呼び出しをシンボルで出力します。ホスト向け出力をランタイムと手動リンクする場合は、ソースツリーから次を実行します。

```sh
# オブジェクトの場合
cc hello.o runtime/runtime.c -o hello
# アセンブリの場合
cc hello.s runtime/runtime.c -o hello
```

Windowsでは `cc` を `gcc`、出力名を `hello.exe` に置き換えてください。拡張モードのオブジェクト・アセンブリはテープ長・セル幅・出力方式を保持するため、追加のランタイム定義は不要です。

### ターゲットの指定

```sh
bfcompiler --target x86_64-unknown-linux-gnu hello.bf -o hello-linux
bfcompiler --target aarch64-unknown-linux-gnu hello.bf -o hello-linux-arm64
bfcompiler --target i686-unknown-linux-gnu hello.bf -o hello-i386
bfcompiler --target riscv64gc-unknown-linux-gnu hello.bf -o hello-riscv64
bfcompiler --target x86_64-pc-windows-gnu hello.bf -o hello.exe
```

実行ファイルのクロス生成には、対象向けのCコンパイラ、リンカ、Cランタイム、ヘッダーが必要です。既定のCコンパイラは次のとおりです。

| クロス生成先 | Cコンパイラ |
| --- | --- |
| Linux x86-64 | `x86_64-linux-gnu-gcc` |
| Linux ARM64 | `aarch64-linux-gnu-gcc` |
| Linux i386 | `i686-linux-gnu-gcc` |
| Linux RISC-V 64 | `riscv64-linux-gnu-gcc` |
| Windows x86-64 | `x86_64-w64-mingw32-gcc` |

`BFC_CC` でコンパイラのパスを変更できます。`-c`・`-S` にはクロスツールチェーンは不要です。

Apple Silicon MacでIntel Mac向けの実行ファイルを実行するにはRosettaが必要です。

## 設定

### 言語仕様モード

テープ長・セル幅・出力方式を変更する場合は、`--mode extended` を指定してください。

| 設定 | `standard`(既定) | `extended` |
| --- | --- | --- |
| テープ | 30,000セル | 既定1,000,000セル、変更可能 |
| セル幅 | 8ビット | 8(既定)・16・32ビット |
| 出力 | バイト列 | バイト列、またはセル値のUTF-8・UTF-16・UTF-32変換 |
| 出力形式 | 実行ファイル、オブジェクト、アセンブリ | 実行ファイル、オブジェクト、アセンブリ |

両モードの共通動作:

- セルはゼロ初期化した符号なし整数で、値は2のセル幅乗を法として循環します。
- 8命令以外はコメントとして無視され、対応しない括弧はエラーになります。
- 入力はセル幅にかかわらず1バイトずつ読み込み、EOF時はセルを0にします。Windowsでは標準入出力をバイナリモードにします。
- 範囲外へのポインタ移動は、終了コード2の実行時エラーになります。

拡張モードでテープ長を2,000,000セル、セル幅を32ビット、出力方式をUTF-8に設定し、`hello.bf` を `hello` にコンパイルする例です。

```sh
bfcompiler --mode extended --tape-size 2000000 --cell-bits 32 --output-encoding utf-8 hello.bf -o hello
```

`--tape-size` の指定範囲は1〜2,147,483,647セルで、対象環境のアドレス空間と利用可能メモリにも制約されます。
`--cell-bits` はテープ全体で共通の幅を指定します。

### 出力エンコーディング

Unicode出力モードはセル値をUnicodeのスカラー値として変換します。サロゲートとU+10FFFF超の値は終了コード2の実行時エラーになります。エンコード済みのバイト列を生成するプログラムには `bytes` を使用してください。

| `--output-encoding` | 出力 |
| --- | --- |
| `bytes` | 下位8ビット(既定) |
| `utf-8` | UTF-8 |
| `utf-16`、`utf-16le` | UTF-16、リトルエンディアン |
| `utf-16be` | UTF-16、ビッグエンディアン |
| `utf-32`、`utf-32le` | UTF-32、リトルエンディアン |
| `utf-32be` | UTF-32、ビッグエンディアン |

BOMは自動付与しません。
バイト順は対象CPUに依存しません。
UTF-16では補助文字をサロゲートペアに変換します。

また、U+FFFFを超える値には32ビットセルを使用してください。

拡張モードでセル幅を32ビット、出力方式をUTF-16LEに設定し、`hello.bf` を `hello` にコンパイルする例です。

```sh
bfcompiler --mode extended --cell-bits 32 --output-encoding utf-16le hello.bf -o hello
```

### オプション

| オプション | 説明 |
| --- | --- |
| `-o <path>` | 出力ファイルを指定 |
| `-c` | オブジェクトファイルを出力 |
| `-S` | アセンブリを出力 |
| `--mode <standard\|extended>` | 言語仕様モードを選択(既定: `standard`) |
| `--tape-size <cells>` | 拡張モードでテープのセル数を指定 |
| `--cell-bits <8\|16\|32>` | 拡張モードで全セルの幅を指定(既定: `8`) |
| `--output-encoding <encoding>` | 拡張モードで `.` の出力方式を指定(上表参照) |
| `-O0`、`--no-optimize` | IR最適化を無効化 |
| `-O`、`-O1`、`-O2`、`-O3`、`-Os`、`-Oz` | 基本的なIR最適化を有効化 |
| `--target <triple>` | 対象のCPUとOSを指定 |
| `-L<dir>`、`-l<name>`、`-Wl,<arg>,...` | ライブラリやリンカの引数を指定 |
| `-v` | リンクコマンドを表示 |
| `-h`、`--help` | ヘルプを表示 |
| `--version` | バージョンを表示 |

最適化は既定で有効です。算術演算の統合、同方向の移動の集約、セル消去(`[-]`、`[+]`)、隣接セルへの転送(`[->+<]`、`[->-<]` と左向きの同等パターン)を適用し、境界エラーとゼロセルでのループのスキップを維持します。`-O0` 以外の各レベルは同じ最適化を行います。

`-o` を省略すると、実行ファイルは入力ファイルの拡張子を除いた名前になります。Windows向け実行ファイルは `.exe`、`-c` は `.o`、`-S` は `.s` に拡張子を置き換えます。

## 開発

プロジェクトのディレクトリでビルドとチェックを実行します。

```sh
cargo build --release
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

実行ファイルは `target/release/bfcompiler`(Windowsでは `bfcompiler.exe`)です。テストでは生成プログラムを参照インタプリタと比較します。

[Brainfuckのテスト用コード](tests/fixtures/README.ja.md) は `cargo test --test fixtures -- --nocapture` で一括実行できます。
`tests/fixtures` 以下に `.bf` と対応する `.out` を追加すると、`-O0`・`-O2` の両方で自動的に検証します。

転送ループの実行時間サンプルは `cargo test --test performance -- --ignored --nocapture` で測定でき、プロセス起動時間を含みます。

## ライセンス

[MIT](LICENSE)
