//! Emacs から常駐プロセスとして使われる JSON-RPC サーバー。
//!
//! モデルの読み込みには数秒かかるので別スレッドで進め、読み込み中でも
//! `version` と `shutdown` には即応答する。`henkan` は読み込みの完了を待つ。
//!
//! `henkan` の params は `{"pieces": [{"kind": "kana" | "convert" | "literal" | "abbrev",
//! "text": "...", "prefix": bool, "suffix": bool}, ...], "top": n}`。エディタがローマ字を
//! かなにし、区間の種類を決めて送る。`prefix` `suffix` は `>` の印で、省略すれば偽。
//!
//! `register` の params は `{"yomi": "...", "surface": "..."}`。送りなしの語として
//! ユーザー辞書に足し、`--user-jisyo` のファイルに書く。
//!
//! 入力中の補完は打鍵ごとに `henkan` を送り、Emacs 側は打鍵で待つのをやめる。
//! 変換は 1 つずつしか処理できないので、溜まった `henkan` のうち後ろに
//! 別の `henkan` があるものは変換せずに捨て、最新の入力だけを変換する。

use std::io::Write as _;
use std::path::Path;
use std::sync::mpsc;
use std::thread::JoinHandle;

use anyhow::{Result, anyhow};
use serde_json::{Value, json};

use sekken_cli::engine::{Engine, EngineArgs};
use sekken_cli::jsonrpc::{Request, Response, read_message, write_message};
use sekken_core::input::{Input, Kind, Piece};

#[derive(clap::Args)]
pub struct Args {
    #[command(flatten)]
    engine: EngineArgs,
}

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// エンジンの読み込みに失敗したときの JSON-RPC エラーコード。
/// Emacs 側はこのコードでプロセスの生死に関係なく異常終了として数える。
pub const LOAD_FAILED: i64 = -32000;

/// 後から来た `henkan` に置き換えられて変換しなかったときの JSON-RPC エラーコード。
pub const SUPERSEDED: i64 = -32001;

/// ユーザー辞書への登録に失敗したときの JSON-RPC エラーコード。
/// `--user-jisyo` が無い、語が形式を壊す、ファイルに書けない、のどれも
/// エンジンは生きているので Emacs 側は異常終了として数えない。
pub const REGISTER_FAILED: i64 = -32002;

/// 別スレッドで読み込み中のエンジン。初回利用時に完了を待つ。
pub enum EngineLoader {
    Loading(JoinHandle<Result<Engine>>),
    Loaded(Box<Result<Engine>>),
}

impl EngineLoader {
    pub fn spawn(build: impl FnOnce() -> Result<Engine> + Send + 'static) -> EngineLoader {
        EngineLoader::Loading(std::thread::spawn(move || {
            let engine = build();
            if let Err(err) = &engine {
                // stderr が閉じていても panic せず、理由を JSON-RPC 側に残す。
                let _ = writeln!(std::io::stderr(), "sekken: failed to load engine: {err:#}");
            }
            engine
        }))
    }

    /// 読み込みの完了を待って結果を返す。
    pub fn engine(&mut self) -> &mut Result<Engine> {
        if let EngineLoader::Loading(_) = self {
            let EngineLoader::Loading(handle) = std::mem::replace(
                self,
                EngineLoader::Loaded(Box::new(Err(anyhow!("unreachable")))),
            ) else {
                unreachable!()
            };
            let engine = match handle.join() {
                Ok(engine) => engine,
                Err(payload) => {
                    let reason = payload
                        .downcast_ref::<String>()
                        .map(String::as_str)
                        .or_else(|| payload.downcast_ref::<&str>().copied())
                        .unwrap_or("unknown panic");
                    Err(anyhow!("engine loader thread panicked: {reason}"))
                }
            };
            *self = EngineLoader::Loaded(Box::new(engine));
        }
        let EngineLoader::Loaded(engine) = self else {
            unreachable!()
        };
        engine
    }
}

/// 要求への応答と、応答を書いた後にプロセスを終了するかどうか。
pub struct Reply {
    pub response: Response,
    pub exit: bool,
}

impl Reply {
    fn ok(id: Value, result: Value) -> Reply {
        Reply {
            response: Response::ok(id, result),
            exit: false,
        }
    }

    fn err(id: Value, code: i64, message: impl Into<String>) -> Reply {
        Reply {
            response: Response::err(id, code, message),
            exit: false,
        }
    }
}

pub fn run(args: Args) -> Result<()> {
    let user_jisyo = args.engine.user_jisyo.clone();
    let mut loader = EngineLoader::spawn(move || args.engine.build());
    let mut stdout = std::io::stdout().lock();
    // 変換の間も要求を読み進め、溜まった分をまとめて見られるように読みを分ける。
    let (tx, rx) = mpsc::channel::<Result<Request>>();
    std::thread::spawn(move || {
        let mut stdin = std::io::stdin().lock();
        loop {
            match read_message(&mut stdin) {
                Ok(Some(req)) => {
                    if tx.send(Ok(req)).is_err() {
                        break;
                    }
                }
                Ok(None) => break,
                Err(err) => {
                    let _ = tx.send(Err(err));
                    break;
                }
            }
        }
    });
    while let Ok(first) = rx.recv() {
        let mut batch = vec![first?];
        while let Ok(req) = rx.try_recv() {
            batch.push(req?);
        }
        let (batch, stale) = drop_stale(batch);
        for response in stale {
            write_message(&mut stdout, &response)?;
        }
        for req in batch {
            let Some(id) = req.id.clone() else {
                // 通知には応答しない。
                if req.method == "exit" {
                    // モデルと辞書の解放に時間がかかり、jsonrpc.el が待つ猶予
                    // （0.3 秒）を超えて kill されるため、解放せずに終了する。
                    std::process::exit(0);
                }
                continue;
            };
            let reply = handle(&mut loader, user_jisyo.as_deref(), id, &req);
            write_message(&mut stdout, &reply.response)?;
            if reply.exit {
                std::process::exit(1);
            }
        }
    }
    Ok(())
}

/// 溜まった要求のうち、後ろに別の `henkan` がある `henkan` を捨てる。
/// 残す要求と、捨てた要求への応答を返す。
pub fn drop_stale(batch: Vec<Request>) -> (Vec<Request>, Vec<Response>) {
    let is_henkan = |req: &Request| req.method == "henkan" && req.id.is_some();
    let Some(last) = batch.iter().rposition(is_henkan) else {
        return (batch, Vec::new());
    };
    let mut keep = Vec::with_capacity(batch.len());
    let mut stale = Vec::new();
    for (i, req) in batch.into_iter().enumerate() {
        if i < last && is_henkan(&req) {
            stale.push(Response::err(
                req.id.clone().unwrap_or(Value::Null),
                SUPERSEDED,
                "superseded by a later henkan request",
            ));
        } else {
            keep.push(req);
        }
    }
    (keep, stale)
}

pub fn handle(
    loader: &mut EngineLoader,
    user_jisyo: Option<&Path>,
    id: Value,
    req: &Request,
) -> Reply {
    match req.method.as_str() {
        "version" => Reply::ok(id, json!({ "version": VERSION })),
        "henkan" => {
            let input = match parse_pieces(&req.params["pieces"]) {
                Ok(input) => input,
                Err(message) => return Reply::err(id, -32602, message),
            };
            let top = req.params["top"].as_u64().unwrap_or(10) as usize;
            match loader.engine() {
                Ok(engine) => Reply::ok(id, json!({ "candidates": engine.henkan(&input, top) })),
                Err(err) => Reply {
                    response: Response::err(
                        id,
                        LOAD_FAILED,
                        format!("failed to load engine: {err:#}"),
                    ),
                    exit: true,
                },
            }
        }
        "register" => {
            let (yomi, surface) = match parse_word(&req.params) {
                Ok(word) => word,
                Err(message) => return Reply::err(id, -32602, message),
            };
            let Some(path) = user_jisyo else {
                return Reply::err(id, REGISTER_FAILED, "user dictionary is not configured");
            };
            match loader.engine() {
                Ok(engine) => match register(engine, path, yomi, surface) {
                    Ok(()) => Reply::ok(id, Value::Null),
                    Err(err) => Reply::err(id, REGISTER_FAILED, format!("{err:#}")),
                },
                Err(err) => Reply {
                    response: Response::err(
                        id,
                        LOAD_FAILED,
                        format!("failed to load engine: {err:#}"),
                    ),
                    exit: true,
                },
            }
        }
        "shutdown" => Reply::ok(id, Value::Null),
        other => Reply::err(id, -32601, format!("unknown method: {other}")),
    }
}

/// `params.yomi` と `params.surface` を読む。
pub fn parse_word(params: &Value) -> Result<(&str, &str), String> {
    let Some(yomi) = params["yomi"].as_str() else {
        return Err("params.yomi must be a string".to_string());
    };
    let Some(surface) = params["surface"].as_str() else {
        return Err("params.surface must be a string".to_string());
    };
    Ok((yomi, surface))
}

/// 語をユーザー辞書に足してファイルに書く。書けなければメモリにも残さない。
fn register(engine: &mut Engine, path: &Path, yomi: &str, surface: &str) -> Result<()> {
    let mut user = engine.user.clone();
    user.register_okuri_nasi(yomi, surface)?;
    user.save(path)?;
    engine.user = user;
    Ok(())
}

/// `params.pieces` を読む。種類と文字列が揃っていなければ理由を返す。
pub fn parse_pieces(pieces: &Value) -> Result<Input, String> {
    let Some(pieces) = pieces.as_array() else {
        return Err("params.pieces must be an array".to_string());
    };
    pieces
        .iter()
        .map(|piece| {
            let kind = match piece["kind"].as_str() {
                Some("kana") => Kind::Kana,
                Some("convert") => Kind::Convert,
                Some("literal") => Kind::Literal,
                Some("abbrev") => Kind::Abbrev,
                _ => {
                    return Err(
                        "params.pieces[].kind must be kana, convert, literal or abbrev".to_string(),
                    );
                }
            };
            let Some(text) = piece["text"].as_str() else {
                return Err("params.pieces[].text must be a string".to_string());
            };
            Ok(Piece::new(kind, text).with_marks(
                piece["prefix"].as_bool().unwrap_or(false),
                piece["suffix"].as_bool().unwrap_or(false),
            ))
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Input::new)
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use anyhow::Context as _;

    use super::*;

    fn request(method: &str, params: Value) -> Request {
        Request {
            id: Some(json!(1)),
            method: method.to_string(),
            params,
        }
    }

    #[test]
    fn 溜まった_henkan_は最後の_1_つだけ残す() {
        let mut batch = vec![
            request(
                "henkan",
                json!({ "pieces": [{ "kind": "convert", "text": "n" }] }),
            ),
            request("version", Value::Null),
            request(
                "henkan",
                json!({ "pieces": [{ "kind": "convert", "text": "ね" }] }),
            ),
            request(
                "henkan",
                json!({ "pieces": [{ "kind": "convert", "text": "ねk" }] }),
            ),
            request("shutdown", Value::Null),
        ];
        batch[0].id = Some(json!(10));
        batch[2].id = Some(json!(12));
        let (keep, stale) = drop_stale(batch);
        let methods: Vec<&str> = keep.iter().map(|r| r.method.as_str()).collect();
        assert_eq!(methods, ["version", "henkan", "shutdown"]);
        assert_eq!(keep[1].params["pieces"][0]["text"], "ねk");
        let ids: Vec<&Value> = stale.iter().map(|r| &r.id).collect();
        assert_eq!(ids, [&json!(10), &json!(12)]);
        assert!(
            stale
                .iter()
                .all(|r| r.error.as_ref().unwrap().code == SUPERSEDED)
        );
    }

    #[test]
    fn henkan_が_1_つなら捨てない() {
        let batch = vec![
            request("version", Value::Null),
            request("henkan", json!({ "pieces": [] })),
        ];
        let (keep, stale) = drop_stale(batch);
        assert_eq!(keep.len(), 2);
        assert!(stale.is_empty());
    }

    #[test]
    fn pieces_を種類付きの区間として読む() {
        let input = parse_pieces(&json!([
            { "kind": "kana", "text": "きょう" },
            { "kind": "convert", "text": "は" },
            { "kind": "literal", "text": "Emacs" },
        ]))
        .unwrap();
        assert_eq!(
            input.pieces,
            [
                Piece::kana("きょう"),
                Piece::convert("は"),
                Piece::literal("Emacs"),
            ]
        );
        assert!(parse_pieces(&json!([])).unwrap().pieces.is_empty());
    }

    #[test]
    fn abbrev_の区間と山括弧の印を読む() {
        let input = parse_pieces(&json!([
            { "kind": "abbrev", "text": "emacs" },
            { "kind": "convert", "text": "お", "prefix": true },
            { "kind": "convert", "text": "かい", "suffix": true },
        ]))
        .unwrap();
        assert_eq!(
            input.pieces,
            [
                Piece::abbrev("emacs"),
                Piece::convert("お").with_marks(true, false),
                Piece::convert("かい").with_marks(false, true),
            ]
        );
    }

    #[test]
    fn pieces_の形が違えば理由を返す() {
        assert!(parse_pieces(&Value::Null).unwrap_err().contains("array"));
        assert!(
            parse_pieces(&json!([{ "kind": "roman", "text": "neko" }]))
                .unwrap_err()
                .contains("kind")
        );
        assert!(
            parse_pieces(&json!([{ "kind": "kana" }]))
                .unwrap_err()
                .contains("text")
        );
        // 読み込みを待たずに params の誤りを返す。
        let (_tx, rx) = mpsc::channel::<()>();
        let mut loader = EngineLoader::spawn(move || {
            let _ = rx.recv();
            Err(anyhow!("never loaded"))
        });
        let reply = handle(
            &mut loader,
            None,
            json!(1),
            &request("henkan", json!({ "input": "Neko" })),
        );
        assert_eq!(reply.response.error.unwrap().code, -32602);
        assert!(!reply.exit);
    }

    #[test]
    fn register_は読み込みを待たずに_params_の誤りと辞書の未設定を返す() {
        let (_tx, rx) = mpsc::channel::<()>();
        let mut loader = EngineLoader::spawn(move || {
            let _ = rx.recv();
            Err(anyhow!("never loaded"))
        });
        let reply = handle(
            &mut loader,
            Some(Path::new("/tmp/user-jisyo")),
            json!(1),
            &request("register", json!({ "yomi": "ねこ" })),
        );
        let error = reply.response.error.unwrap();
        assert_eq!(error.code, -32602);
        assert!(error.message.contains("surface"), "{}", error.message);
        assert!(!reply.exit);
        let reply = handle(
            &mut loader,
            None,
            json!(1),
            &request("register", json!({ "yomi": "ねこ", "surface": "根子" })),
        );
        let error = reply.response.error.unwrap();
        assert_eq!(error.code, REGISTER_FAILED);
        assert!(
            error.message.contains("not configured"),
            "{}",
            error.message
        );
        assert!(!reply.exit);
    }

    #[test]
    fn 読み込み中でも_version_には即応答する() {
        let (_tx, rx) = mpsc::channel::<()>();
        let mut loader = EngineLoader::spawn(move || {
            let _ = rx.recv();
            Err(anyhow!("never loaded"))
        });
        let reply = handle(
            &mut loader,
            None,
            json!(1),
            &request("version", Value::Null),
        );
        assert_eq!(reply.response.result, Some(json!({ "version": VERSION })));
        assert!(!reply.exit);
        let reply = handle(
            &mut loader,
            None,
            json!(1),
            &request("shutdown", Value::Null),
        );
        assert_eq!(reply.response.result, Some(Value::Null));
        assert!(!reply.exit);
    }

    #[test]
    fn 読み込みに失敗したら_henkan_は理由を返して終了する() {
        let mut loader =
            EngineLoader::spawn(|| Err(anyhow!("No such file")).context("load SKK dictionary"));
        let reply = handle(
            &mut loader,
            None,
            json!(1),
            &request(
                "henkan",
                json!({ "pieces": [{ "kind": "convert", "text": "ねこ" }] }),
            ),
        );
        let error = reply.response.error.expect("error response");
        assert_eq!(error.code, LOAD_FAILED);
        assert!(
            error.message.contains("load SKK dictionary"),
            "{}",
            error.message
        );
        assert!(error.message.contains("No such file"), "{}", error.message);
        assert!(reply.exit);
    }

    #[test]
    fn 読み込みスレッドの_panic_も読み込み失敗として扱う() {
        let mut loader = EngineLoader::spawn(|| panic!("boom"));
        let reply = handle(
            &mut loader,
            None,
            json!(1),
            &request(
                "henkan",
                json!({ "pieces": [{ "kind": "convert", "text": "ねこ" }] }),
            ),
        );
        let error = reply.response.error.expect("error response");
        assert_eq!(error.code, LOAD_FAILED);
        assert!(error.message.contains("boom"), "{}", error.message);
        assert!(reply.exit);
    }
}
