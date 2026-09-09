//! Emacs から常駐プロセスとして使われる JSON-RPC サーバー。

use anyhow::Result;
use serde_json::{Value, json};

use sekken_cli::engine::{Engine, EngineArgs};
use sekken_cli::jsonrpc::{Request, Response, read_message, write_message};

#[derive(clap::Args)]
pub struct Args {
    #[command(flatten)]
    engine: EngineArgs,
}

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn run(args: Args) -> Result<()> {
    let engine = args.engine.build()?;
    let mut stdin = std::io::stdin().lock();
    let mut stdout = std::io::stdout().lock();
    while let Some(req) = read_message(&mut stdin)? {
        let Some(id) = req.id.clone() else {
            // 通知には応答しない。
            if req.method == "exit" {
                // モデルと辞書の解放に時間がかかり、jsonrpc.el が待つ猶予
                // （0.3 秒）を超えて kill されるため、解放せずに終了する。
                std::process::exit(0);
            }
            continue;
        };
        write_message(&mut stdout, &handle(&engine, id, &req))?;
    }
    Ok(())
}

pub fn handle(engine: &Engine, id: Value, req: &Request) -> Response {
    match req.method.as_str() {
        "version" => Response::ok(id, json!({ "version": VERSION })),
        "henkan" => {
            let Some(input) = req.params["input"].as_str() else {
                return Response::err(id, -32602, "params.input must be a string");
            };
            let top = req.params["top"].as_u64().unwrap_or(10) as usize;
            Response::ok(id, json!({ "candidates": engine.henkan(input, top) }))
        }
        "shutdown" => Response::ok(id, Value::Null),
        other => Response::err(id, -32601, format!("unknown method: {other}")),
    }
}
