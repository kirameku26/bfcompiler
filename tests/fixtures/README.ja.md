# Brainfuckのテスト用コード

[English](README.md) | 日本語

## 必要な環境

コマンドはリポジトリのルートで実行してください。テストに使うコンパイラはCargoがビルドするため、別途 `cargo install` を実行する必要はありません。

- Rust 2024エディションに対応したstableのRustとCargo
- ホスト環境のCコンパイラ、リンカ、Cランタイム、開発用ヘッダー:

| ホスト | 必要なツールと環境構築 |
| --- | --- |
| Linux | GCC (`cc`)、リンカ、libc開発用ヘッダー。Ubuntu/Debianでは `sudo apt-get install -y build-essential` |
| macOS | Xcode Command Line Tools (Apple Clang、リンカ、macOS SDK)。`xcode-select --install` でインストール |
| Windows x86-64 | MinGW-w64 GCCとRust GNUツールチェーン。下記のWindows向け手順を参照 |

別のCコンパイラを使う場合は `BFC_CC` を指定します。クロスターゲットでの実行には、後述の追加ツールが必要です。

## 実行方法

Linux・macOSでは、サブディレクトリを含むすべての `.bf` を次のコマンドで実行します。

```sh
cargo test --test fixtures -- --nocapture
```

各プログラムを `-O0`・`-O2` の両方で実行し、実行時間の上限はそれぞれ5秒です。
失敗時はファイル名と最適化レベルを表示し、残りのプログラムも検証します。

### Windowsの環境構築

MSYS2をインストールし、MINGW64ターミナルでGCCをインストールします。

```sh
pacman -S --needed mingw-w64-x86_64-gcc
```

Rustupをインストールした状態で、リポジトリのルートからPowerShellで次を実行します。
`PATH` の例はMSYS2を `C:\msys64` にインストールした場合です。実際のインストール先に合わせて変更してください。

```powershell
rustup toolchain install stable-x86_64-pc-windows-gnu --profile minimal
$env:PATH = "C:\msys64\mingw64\bin;$env:PATH"
$env:BFC_CC = "gcc"
$env:CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER = "gcc"
cargo +stable-x86_64-pc-windows-gnu test --test fixtures -- --nocapture
```

### Linuxのクロスターゲット実行

i686・RISC-V 64向けには、QEMUユーザーモード、クロスコンパイラ、対象のCランタイム開発用パッケージを用意します。Ubuntu/Debianでの例です。

```sh
sudo apt-get install -y qemu-user gcc-i686-linux-gnu libc6-dev-i386-cross gcc-riscv64-linux-gnu libc6-dev-riscv64-cross
BFC_TEST_LINUX_CROSS=i686 cargo test --test fixtures -- --nocapture
BFC_TEST_LINUX_CROSS=riscv64 cargo test --test fixtures -- --nocapture
```

テストは `qemu-i386` と `/usr/i686-linux-gnu`、または `qemu-riscv64` と `/usr/riscv64-linux-gnu` を使います。これらのパスとコンパイラ名はCIの設定に対応しています。

## テスト用コードの追加

`.bf` と同じ名前の補助ファイルを追加します。

| 拡張子 | 内容 |
| --- | --- |
| `.out` | 必須の期待標準出力。バイト列を完全一致で比較し、出力しない場合は空ファイルにする |
| `.in` | 任意の標準入力バイト列。省略すると最初からEOFになる |
| `.args` | 任意のコンパイラオプション。`--mode=extended --cell-bits=16` のように空白で区切り、入出力ファイル名・ターゲット・最適化オプションは含めない |
| `.status` | 任意の期待終了コード。既定は `0` |

例えば `echo.bf`、`echo.in`、`echo.out` で1つのテストケースになります。
Rustコードを編集せずに、新しいプログラムが自動で検出されます。
同じディレクトリの `.gitattributes` により、Windowsでも入出力のバイト列を保持します。

`standard/` はコメント・算術演算・ループ・バイト入力・EOF・改行入力を扱います。
`extended/` はセル幅の拡張とUnicode出力、`errors/` は実行時エラーを検証します。
`targets.bf` はクロスターゲットのオブジェクト検証にも使用します。

## CI

Linux・Windows・macOSのCIジョブは `cargo test` でこれらのコードを検証します。
Linuxのクロスジョブでも `BFC_TEST_LINUX_CROSS=i686` または `riscv64` を設定し、クロスコンパイラとQEMUユーザーモードで実行します。
