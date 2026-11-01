# bfcompile

BrainfuckソースをARM64・x86-64の機械語へ直接コンパイルします。オブジェクトやアセンブリの出力に加え、対象環境のツールチェーンで実行ファイルを生成できます。

[English](README.md) | 日本語

## 主な機能

- 機械語とMach-O・ELF・COFFオブジェクトの直接生成
- 標準・拡張モードの切り替えと、拡張モードでのテープ長・出力方式の指定
- 算術演算・ポインタ移動・ループの最適化とアセンブリ出力

## 必要な環境

- ビルド・インストール: Rust 2024エディションに対応したRustとCargo
- 実行ファイルの生成: 対象環境のCコンパイラとリンカ

## インストール

プロジェクトのディレクトリからインストールします。

```sh
cargo install --path .
bfcompile --version
```

Cargoのbinディレクトリを `PATH` に追加してください（通常は `~/.cargo/bin`、Windowsではユーザープロファイル内のCargo binディレクトリ）。インストール後はソースツリーなしで利用できます。

## 使い方

`hello.bf` をコンパイルして実行します。

```sh
bfcompile hello.bf -o hello
./hello
```

`--target` の既定はホスト環境です。Windowsでは `-o hello.exe` を指定し、`hello.exe` を実行します。

### 対応ターゲット

| ターゲット | CPU | OS | オブジェクト形式 |
| --- | --- | --- | --- |
| `aarch64-apple-darwin` | ARM64 | macOS | Mach-O |
| `x86_64-apple-darwin` | x86-64 | macOS | Mach-O |
| `aarch64-unknown-linux-gnu` | ARM64 | Linux | ELF |
| `x86_64-unknown-linux-gnu` | x86-64 | Linux | ELF |
| `x86_64-pc-windows-gnu` | x86-64 | Windows（MinGW-w64） | COFF |

Linux・Windows向けはオブジェクト検査とクロスアセンブルまで確認し、対象OSでの最終的なリンク・実行は未検証です。Windows向けの生成関数を通じたスタックのアンワインドはサポートしません。

### オブジェクト・アセンブリの出力

`-c` でオブジェクト、`-S` でアセンブリを出力し、リンクを省略します。

```sh
bfcompile -c hello.bf -o hello.o
bfcompile -S -O2 hello.bf -o hello.s
```

アセンブリには命令ニーモニックと分岐ラベルを表示します。ソースツリーからホスト向け出力を再アセンブル・リンクするには:

```sh
cc hello.s runtime/runtime.c -o hello
```

### ターゲットの指定

```sh
bfcompile --target x86_64-unknown-linux-gnu hello.bf -o hello-linux
bfcompile --target aarch64-unknown-linux-gnu hello.bf -o hello-linux-arm64
bfcompile --target x86_64-pc-windows-gnu hello.bf -o hello.exe
```

実行ファイルのクロス生成には、対象向けのCコンパイラ、リンカ、Cランタイム、ヘッダーが必要です。既定のCコンパイラは次のとおりです。

| クロス生成先 | Cコンパイラ |
| --- | --- |
| Linux x86-64 | `x86_64-linux-gnu-gcc` |
| Linux ARM64 | `aarch64-linux-gnu-gcc` |
| Windows x86-64 | `x86_64-w64-mingw32-gcc` |

`BFC_CC` でコンパイラのパスを変更できます。`-c`・`-S` にはクロスツールチェーンは不要です。

Apple Silicon MacでIntel Mac向けの実行ファイルを実行するにはRosettaが必要です。

## 設定

### 言語仕様モード

| 設定 | `standard`（既定） | `extended` |
| --- | --- | --- |
| テープ | 固定30,000セル | 既定1,000,000セル、変更可能 |
| 出力 | バイト列 | バイト列、またはセル値のUTF-8変換 |
| 出力形式 | 実行ファイル、オブジェクト、アセンブリ | 実行ファイル |

両モード共通の動作:

- セルはゼロ初期化した8ビット符号なし整数で、値は256を法として循環します。
- 8命令以外のバイトはコメントとして無視し、対応しない括弧はエラーにします。
- 入力はバイト単位で、EOF時はセルを0にします。Windowsでは標準入出力をバイナリモードにします。
- 範囲外へのポインタ移動は、終了コード2の実行時エラーになります。

テープ長や出力方式を変更するには、拡張モードを指定します。

```sh
bfcompile --mode extended --tape-size 2000000 --output-encoding utf-8 hello.bf -o hello
```

`--tape-size` の範囲は1〜2,147,483,647セルです。`--output-encoding utf-8` はセル値をU+0000〜U+00FFとして変換し、例えば255は `C3 BF` になります。1セルで日本語は表せません。UTF-8のバイト列を出力するプログラムには、既定の `bytes` を使用してください。

拡張オプションには `--mode extended` が必要で、`-c`・`-S` とは併用できません。手動リンク時はCコンパイラに `-DBFC_TAPE_SIZE=1000000`、必要に応じて `-DBFC_OUTPUT_UTF8=1` を渡してランタイムを設定できます。指定しなければ標準モードの設定になります。

### オプション

| オプション | 説明 |
| --- | --- |
| `-o <path>` | 出力ファイルを指定 |
| `-c` | オブジェクトファイルを出力 |
| `-S` | アセンブリを出力 |
| `--mode <standard\|extended>` | 言語仕様モードを選択（既定: `standard`） |
| `--tape-size <cells>` | 拡張モードでテープのセル数を指定 |
| `--output-encoding <bytes\|utf-8>` | 拡張モードで `.` の出力方式を指定（既定: `bytes`） |
| `-O0`、`--no-optimize` | IR最適化を無効化 |
| `-O`、`-O1`、`-O2`、`-O3`、`-Os`、`-Oz` | 基本的なIR最適化を有効化 |
| `--target <triple>` | 対象のCPUとOSを指定 |
| `-L<dir>`、`-l<name>`、`-Wl,<arg>,...` | ライブラリやリンカの引数を指定 |
| `-v` | リンクコマンドを表示 |
| `-h`、`--help` | ヘルプを表示 |
| `--version` | バージョンを表示 |

最適化は既定で有効です。算術演算の統合、同方向の移動の集約、セル消去（`[-]`、`[+]`）、隣接セルへの転送（`[->+<]`、`[->-<]` と左向きの同等パターン）を適用し、境界エラーとゼロセルでのループのスキップを維持します。`-O0` 以外の各レベルは同じ最適化を行います。

`-o` を省略すると、実行ファイルは入力ファイルの拡張子を除いた名前になります。Windows向け実行ファイルは `.exe`、`-c` は `.o`、`-S` は `.s` に拡張子を置き換えます。

## 開発

プロジェクトのディレクトリでビルドとチェックを実行します。

```sh
cargo build --release
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

実行ファイルは `target/release/bfcompile`（Windowsでは `bfcompile.exe`）です。テストでは生成プログラムを参照インタプリタと比較します。転送ループの実行時間サンプルは `cargo test --test performance -- --ignored --nocapture` で測定でき、プロセス起動時間を含みます。

## ライセンス

[MIT](LICENSE)
