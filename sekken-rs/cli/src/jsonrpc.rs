//! LSP と同じ `Content-Length` ヘッダで区切る JSON-RPC 2.0 の読み書き。
//! Emacs の jsonrpc.el（`jsonrpc-process-connection`）がこの枠組みを使う。

use std::io::{BufRead, Write};

use anyhow::{Context as _, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize)]
pub struct Request {
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Serialize)]
pub struct Response {
    pub jsonrpc: &'static str,
    pub id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

#[derive(Debug, Serialize)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
}

impl Response {
    pub fn ok(id: Value, result: Value) -> Response {
        Response {
            jsonrpc: "2.0",
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn err(id: Value, code: i64, message: impl Into<String>) -> Response {
        Response {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(RpcError {
                code,
                message: message.into(),
            }),
        }
    }
}

/// 1 メッセージ読む。入力が閉じていれば `None`。
pub fn read_message(reader: &mut impl BufRead) -> Result<Option<Request>> {
    let mut length: Option<usize> = None;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).context("read header")? == 0 {
            return Ok(None);
        }
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            break;
        }
        if let Some(v) = line.strip_prefix("Content-Length:") {
            length = Some(v.trim().parse().context("parse Content-Length")?);
        }
    }
    let Some(length) = length else {
        bail!("Content-Length header is missing");
    };
    let mut body = vec![0; length];
    reader.read_exact(&mut body).context("read body")?;
    let req = serde_json::from_slice(&body).context("parse request")?;
    Ok(Some(req))
}

pub fn write_message(writer: &mut impl Write, response: &Response) -> Result<()> {
    let body = serde_json::to_vec(response).context("serialize response")?;
    write!(writer, "Content-Length: {}\r\n\r\n", body.len())?;
    writer.write_all(&body)?;
    writer.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn content_length_で区切られたリクエストを読む() {
        let body = r#"{"jsonrpc":"2.0","id":1,"method":"henkan","params":{"input":"Neko"}}"#;
        let msg = format!("Content-Length: {}\r\n\r\n{body}", body.len());
        let mut reader = msg.as_bytes();
        let req = read_message(&mut reader).unwrap().unwrap();
        assert_eq!(req.method, "henkan");
        assert_eq!(req.id, Some(json!(1)));
        assert_eq!(req.params["input"], "Neko");
        assert!(read_message(&mut reader).unwrap().is_none());
    }

    #[test]
    fn レスポンスに_content_length_を付けて書く() {
        let mut out = Vec::new();
        write_message(&mut out, &Response::ok(json!(1), json!(["猫"]))).unwrap();
        let s = String::from_utf8(out).unwrap();
        let (header, body) = s.split_once("\r\n\r\n").unwrap();
        assert_eq!(header, format!("Content-Length: {}", body.len()));
        assert_eq!(body, r#"{"jsonrpc":"2.0","id":1,"result":["猫"]}"#);
    }
}
