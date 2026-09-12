//! A minimal local HTTP server for tests.
//!
//! Hand-rolled on a tokio `TcpListener` rather than pulling in a mock-server dependency: this
//! workspace is deliberately dependency-light with fast CI, and the tests need only enough HTTP to
//! read one request and write one canned response.
//!
//! It never talks to a real Fleet Core, so the suite stays offline and deterministic.

use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::TcpListener;

/// What the mock should do when a request arrives.
#[derive(Clone, Debug)]
pub enum Behaviour {
    /// Reply with this status and body.
    Respond { status: u16, body: String },
    /// Reply with a body far larger than the client's configured bound.
    Oversized { bytes: usize },
    /// Declare a huge Content-Length without sending it, to exercise the header-based bound.
    LyingContentLength { declared: u64, body: String },
    /// Accept the connection, then never answer, to exercise the client timeout.
    Hang,
}

/// A running mock server. Dropping the handle stops accepting.
#[derive(Debug)]
pub struct MockCore {
    pub base_url: String,
    captured: Arc<Mutex<Vec<Captured>>>,
    _task: tokio::task::JoinHandle<()>,
}

/// One captured request.
///
/// Fields are read selectively per test, so unused-field analysis is suppressed rather than
/// deleting assertions' worth of captured detail.
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct Captured {
    pub path: String,
    pub body: Vec<u8>,
    pub content_type: Option<String>,
}

impl MockCore {
    /// Starts a mock on an ephemeral loopback port.
    pub async fn start(behaviour: Behaviour) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let address = listener.local_addr().expect("addr");
        let captured = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&captured);

        let task = tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    return;
                };
                let behaviour = behaviour.clone();
                let sink = Arc::clone(&sink);
                tokio::spawn(async move {
                    let mut buffer = Vec::new();
                    let mut chunk = [0u8; 4096];

                    // Read headers, then exactly Content-Length bytes of body.
                    let (head_end, content_length) = loop {
                        match stream.read(&mut chunk).await {
                            Ok(0) => return,
                            Ok(n) => buffer.extend_from_slice(&chunk[..n]),
                            Err(_) => return,
                        }
                        if let Some(position) = find_header_end(&buffer) {
                            let head = String::from_utf8_lossy(&buffer[..position]).to_string();
                            let length = head
                                .lines()
                                .find_map(|line| {
                                    let (name, value) = line.split_once(':')?;
                                    name.eq_ignore_ascii_case("content-length")
                                        .then(|| value.trim().parse::<usize>().ok())?
                                })
                                .unwrap_or(0);
                            break (position, length);
                        }
                    };

                    while buffer.len() < head_end + content_length {
                        match stream.read(&mut chunk).await {
                            Ok(0) => break,
                            Ok(n) => buffer.extend_from_slice(&chunk[..n]),
                            Err(_) => return,
                        }
                    }

                    let head = String::from_utf8_lossy(&buffer[..head_end]).to_string();
                    let path = head
                        .lines()
                        .next()
                        .and_then(|line| line.split_whitespace().nth(1))
                        .unwrap_or("/")
                        .to_owned();
                    let content_type = head.lines().find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("content-type")
                            .then(|| value.trim().to_owned())
                    });
                    sink.lock().expect("lock").push(Captured {
                        path,
                        body: buffer[head_end..].to_vec(),
                        content_type,
                    });

                    let response = match behaviour {
                        Behaviour::Respond { status, body } => http_response(status, &body, None),
                        Behaviour::Oversized { bytes } => {
                            http_response(200, &"x".repeat(bytes), None)
                        }
                        Behaviour::LyingContentLength { declared, body } => {
                            http_response(200, &body, Some(declared))
                        }
                        Behaviour::Hang => {
                            // Hold the connection open without answering.
                            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                            return;
                        }
                    };
                    let _ = stream.write_all(response.as_bytes()).await;
                    let _ = stream.flush().await;
                });
            }
        });

        Self {
            base_url: format!("http://{address}"),
            captured,
            _task: task,
        }
    }

    /// Requests captured so far.
    pub fn captured(&self) -> Vec<Captured> {
        self.captured.lock().expect("lock").clone()
    }
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|position| position + 4)
}

fn http_response(status: u16, body: &str, declared_length: Option<u64>) -> String {
    let length = declared_length.unwrap_or(body.len() as u64);
    format!(
        "HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n{body}"
    )
}
