//! sekken の CLI。変換・サーバー・分かち書きの確認を 1 つのバイナリにまとめる。

use anyhow::Result;
use clap::{Parser, Subcommand};

mod commands;

#[derive(Parser)]
#[command(name = "sekken", about = "SKK 風一括変換エンジン")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// ローマ字入力を変換して候補を表示する
    Henkan(commands::henkan::Args),
    /// 過去に書いた文（1 行 1 文、標準入力）を本人の n-gram に数える。server を止めてから使う
    Learn(commands::learn::Args),
    /// Emacs 向けに stdio 上で JSON-RPC を話す常駐サーバー
    Server(commands::server::Args),
    /// 文を分かち書きして表示する（モデルの挙動を調べる用）
    Tokenize(commands::tokenize::Args),
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Henkan(a) => commands::henkan::run(a),
        Command::Learn(a) => commands::learn::run(a),
        Command::Server(a) => commands::server::run(a),
        Command::Tokenize(a) => commands::tokenize::run(a),
    }
}
