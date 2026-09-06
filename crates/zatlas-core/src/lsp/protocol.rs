use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::error::{CoreError, Result};

const STDERR_KEPT: usize = 24;

pub struct Transport {
    child: Child,
    stdin: ChildStdin,
    rx: Receiver<Value>,
    stderr: Arc<Mutex<Vec<String>>>,
    next_id: i64,
}

impl Transport {
    pub fn spawn(mut command: Command) -> Result<Self> {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| CoreError::other(format!("could not start language server: {e}")))?;

        let stdin = child.stdin.take().expect("stdin was piped");
        let stdout = child.stdout.take().expect("stdout was piped");
        let stderr_pipe = child.stderr.take().expect("stderr was piped");

        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || read_messages(stdout, tx));

        let stderr = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&stderr);
        std::thread::spawn(move || {
            for line in BufReader::new(stderr_pipe)
                .lines()
                .map_while(std::result::Result::ok)
            {
                let mut kept = sink.lock().unwrap_or_else(|e| e.into_inner());
                if kept.len() < STDERR_KEPT {
                    kept.push(line);
                }
            }
        });

        Ok(Self {
            child,
            stdin,
            rx,
            stderr,
            next_id: 0,
        })
    }

    pub fn stderr_tail(&self) -> String {
        self.stderr
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .join("\n")
    }

    pub fn notify(&mut self, method: &str, params: Value) -> Result<()> {
        self.send(&json!({ "jsonrpc": "2.0", "method": method, "params": params }))
    }

    pub fn request(&mut self, method: &str, params: Value, timeout: Duration) -> Result<Value> {
        self.next_id += 1;
        let id = self.next_id;
        self.send(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        }))?;
        self.await_response(id, timeout)
    }

    fn await_response(&mut self, id: i64, timeout: Duration) -> Result<Value> {
        let deadline = Instant::now() + timeout;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(CoreError::other(format!(
                    "language server did not answer {id} within {}ms",
                    timeout.as_millis()
                )));
            }
            let message = match self.rx.recv_timeout(left) {
                Ok(m) => m,
                Err(RecvTimeoutError::Timeout) => continue,
                Err(RecvTimeoutError::Disconnected) => {
                    let tail = self.stderr_tail();
                    return Err(CoreError::other(if tail.is_empty() {
                        "language server exited without answering".to_owned()
                    } else {
                        format!("language server exited without answering:\n{tail}")
                    }));
                }
            };

            match classify(&message) {
                Kind::Response(got) if got == id => {
                    if let Some(error) = message.get("error") {
                        let text = error
                            .get("message")
                            .and_then(Value::as_str)
                            .unwrap_or("unknown error");
                        return Err(CoreError::other(format!("language server: {text}")));
                    }
                    return Ok(message.get("result").cloned().unwrap_or(Value::Null));
                }

                Kind::Response(_) => continue,
                Kind::ServerRequest(server_id) => {
                    self.send(&json!({
                        "jsonrpc": "2.0",
                        "id": server_id,
                        "result": Value::Null,
                    }))?;
                }
                Kind::Notification => continue,
            }
        }
    }

    fn send(&mut self, message: &Value) -> Result<()> {
        let body = serde_json::to_vec(message)
            .map_err(|e| CoreError::other(format!("encoding an LSP message: {e}")))?;

        write!(self.stdin, "Content-Length: {}\r\n\r\n", body.len())
            .and_then(|()| self.stdin.write_all(&body))
            .and_then(|()| self.stdin.flush())
            .map_err(|e| CoreError::other(format!("writing to the language server: {e}")))
    }
}

impl Drop for Transport {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

enum Kind {
    Response(i64),
    ServerRequest(Value),
    Notification,
}

fn classify(message: &Value) -> Kind {
    match (message.get("id"), message.get("method")) {
        (Some(id), Some(_)) => Kind::ServerRequest(id.clone()),
        (Some(id), None) => Kind::Response(id.as_i64().unwrap_or(i64::MIN)),
        _ => Kind::Notification,
    }
}

fn read_messages(stdout: impl Read, tx: mpsc::Sender<Value>) {
    let mut reader = BufReader::new(stdout);
    loop {
        let mut length: Option<usize> = None;

        loop {
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) => return,
                Ok(_) => {}
                Err(_) => return,
            }
            let trimmed = line.trim_end_matches(['\r', '\n']);
            if trimmed.is_empty() {
                break;
            }
            if let Some(value) = trimmed
                .split_once(':')
                .filter(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                .and_then(|(_, value)| value.trim().parse::<usize>().ok())
            {
                length = Some(value);
            }
        }

        let Some(length) = length else {
            return;
        };
        let mut body = vec![0u8; length];
        if reader.read_exact(&mut body).is_err() {
            return;
        }
        match serde_json::from_slice::<Value>(&body) {
            Ok(value) => {
                if tx.send(value).is_err() {
                    return;
                }
            }

            Err(_) => continue,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(body: &str) -> Vec<u8> {
        format!("Content-Length: {}\r\n\r\n{body}", body.len()).into_bytes()
    }

    #[test]
    fn framed_messages_are_read_back_in_order() {
        let stream = [
            frame(r#"{"id":1,"result":"a"}"#),
            frame(r#"{"id":2,"result":"b"}"#),
        ]
        .concat();
        let (tx, rx) = mpsc::channel();
        read_messages(std::io::Cursor::new(stream), tx);
        assert_eq!(rx.recv().unwrap()["result"], "a");
        assert_eq!(rx.recv().unwrap()["result"], "b");
        assert!(rx.recv().is_err(), "the stream should end");
    }

    #[test]
    fn extra_headers_and_case_differences_do_not_break_framing() {
        let body = r#"{"id":7,"result":null}"#;
        let raw = format!(
            "content-length: {}\r\nContent-Type: application/vscode-jsonrpc\r\n\r\n{body}",
            body.len()
        );
        let (tx, rx) = mpsc::channel();
        read_messages(std::io::Cursor::new(raw.into_bytes()), tx);
        assert_eq!(rx.recv().unwrap()["id"], 7);
    }

    #[test]
    fn a_body_with_multibyte_characters_is_read_by_bytes_not_characters() {
        let body = r#"{"id":1,"result":"café"}"#;
        let (tx, rx) = mpsc::channel();
        read_messages(std::io::Cursor::new(frame(body)), tx);
        assert_eq!(rx.recv().unwrap()["result"], "café");
    }

    #[test]
    fn a_truncated_frame_ends_the_reader_rather_than_emitting_garbage() {
        let (tx, rx) = mpsc::channel();
        read_messages(
            std::io::Cursor::new(b"Content-Length: 99\r\n\r\n{}".to_vec()),
            tx,
        );
        assert!(rx.recv().is_err());
    }

    #[test]
    fn messages_are_classified_by_the_id_and_method_pair() {
        assert!(matches!(
            classify(&json!({"id": 1, "result": null})),
            Kind::Response(1)
        ));
        assert!(matches!(
            classify(&json!({"id": 2, "method": "window/workDoneProgress/create"})),
            Kind::ServerRequest(_)
        ));
        assert!(matches!(
            classify(&json!({"method": "$/progress"})),
            Kind::Notification
        ));
    }
}
