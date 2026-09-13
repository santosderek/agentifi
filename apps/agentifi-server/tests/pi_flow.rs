//! End-to-end server tests against a mock `pi --mode rpc` process.
//!
//! The mock speaks the documented RPC protocol (https://pi.dev/docs/latest/rpc):
//! JSONL commands on stdin, `type: "response"` frames correlated by id, and
//! asynchronous agent events on stdout.

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use std::{fs, path::PathBuf, sync::mpsc};
use tower::ServiceExt;
use uuid::Uuid;

/// Serializes tests: they mutate process-wide environment variables.
static TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

const MOCK_PI: &str = r#"
import sys, json

def send(obj):
    print(json.dumps(obj), flush=True)

for line in sys.stdin:
    try:
        command = json.loads(line)
    except Exception:
        continue
    kind = command.get("type")
    request_id = command.get("id")
    if kind == "get_state":
        send({"id": request_id, "type": "response", "command": "get_state", "success": True,
              "data": {"isStreaming": False, "sessionId": "mock", "sessionName": "mock session",
                       "messageCount": 2}})
    elif kind == "get_messages":
        send({"id": request_id, "type": "response", "command": "get_messages", "success": True,
              "data": {"messages": [
                  {"role": "user", "content": "Stored prompt"},
                  {"role": "assistant", "content": [{"type": "text", "text": "Stored answer"}]}
              ]}})
    elif kind == "prompt":
        send({"id": request_id, "type": "response", "command": "prompt", "success": True})
        send({"type": "agent_start"})
        send({"type": "message_end", "message": {"role": "assistant",
              "content": [{"type": "text", "text": "Mock reply"}]}})
        # Deliberately no agent_settled yet: the session stays "running"
        # until the test aborts, which proves live status tracking.
    elif kind == "abort":
        send({"id": request_id, "type": "response", "command": "abort", "success": True})
        send({"type": "agent_settled"})
    else:
        send({"id": request_id, "type": "response", "command": kind, "success": True})
"#;

const SESSION_ID: &str = "11111111-2222-3333-4444-555555555555";

struct Fixture {
    _guard: mpsc::Sender<()>,
    _root: PathBuf,
    router: axum::Router,
    state: std::sync::Arc<agentifi_server::AppState>,
}

/// Builds a server router pointed at a temp session dir and the mock Pi.
fn fixture() -> Fixture {
    let root = std::env::temp_dir().join(format!("agentifi-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&root).expect("create session root");
    let script = root.join("mock-pi.py");
    fs::write(&script, MOCK_PI).expect("write mock pi script");
    let mock = root.join("mock-pi.sh");
    fs::write(
        &mock,
        format!("#!/bin/sh\nexec python3 {}\n", script.display()),
    )
    .expect("write mock pi");
    make_executable(&mock);
    fs::write(
        root.join("20260912-220000-session.jsonl"),
        format!(
            "{{\"type\":\"session\",\"version\":3,\"id\":\"{SESSION_ID}\",\"timestamp\":\"2026-09-12T22:00:00Z\",\"cwd\":\"/work/agentifi\"}}\n\
             {{\"type\":\"message\",\"id\":\"e1\",\"parentId\":null,\"timestamp\":\"2026-09-12T22:00:01Z\",\"message\":{{\"role\":\"user\",\"content\":\"Restore the SSE stream\"}}}}\n\
             {{\"type\":\"message\",\"id\":\"e2\",\"parentId\":\"e1\",\"timestamp\":\"2026-09-12T22:00:05Z\",\"message\":{{\"role\":\"assistant\",\"content\":[{{\"type\":\"text\",\"text\":\"Done\"}}]}}}}\n"
        ),
    )
    .expect("write session");

    std::env::set_var("PI_CODING_AGENT_SESSION_DIR", &root);
    std::env::set_var("AGENTIFI_PI_COMMAND", mock.display().to_string());
    let state = agentifi_server::build_state();
    Fixture {
        _guard: drop_guard(),
        _root: root,
        router: agentifi_server::build_router(std::sync::Arc::clone(&state)),
        state,
    }
}

fn make_executable(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(path).expect("stat mock").permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).expect("chmod mock");
}

fn drop_guard() -> mpsc::Sender<()> {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = receiver.recv();
    });
    sender
}

async fn rpc(router: &axum::Router, id: u64, method: &str, params: Value) -> Value {
    let response = router
        .clone()
        .oneshot(
            Request::post("/api/v1/rpc")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(&json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "method": method,
                        "params": params,
                    }))
                    .expect("serialize request"),
                ))
                .expect("build request"),
        )
        .await
        .expect("send rpc");
    assert_eq!(response.status(), StatusCode::OK);
    let body = response
        .into_body()
        .collect()
        .await
        .expect("read body")
        .to_bytes();
    serde_json::from_slice(&body).expect("parse rpc response")
}

async fn list(router: &axum::Router) -> Vec<Value> {
    let response = router
        .clone()
        .oneshot(
            Request::get("/api/v1/sessions")
                .body(Body::empty())
                .expect("build request"),
        )
        .await
        .expect("list sessions");
    assert_eq!(response.status(), StatusCode::OK);
    let body = response
        .into_body()
        .collect()
        .await
        .expect("read body")
        .to_bytes();
    serde_json::from_slice(&body).expect("parse sessions")
}

fn field<'a>(session: &'a Value, name: &str) -> &'a Value {
    session.get(name).unwrap_or(&Value::Null)
}

#[tokio::test]
async fn attach_prompt_and_abort_flow_reports_live_state() {
    let _guard = TEST_LOCK.lock().await;
    let fixture = fixture();
    let router = &fixture.router;

    // Before attach: discovered from disk, resumable.
    let sessions = list(router).await;
    assert_eq!(sessions.len(), 1);
    assert_eq!(field(&sessions[0], "id").as_str().unwrap(), SESSION_ID);
    assert_eq!(
        field(&sessions[0], "attachment").as_str().unwrap(),
        "resumable"
    );
    assert_ne!(field(&sessions[0], "status").as_str().unwrap(), "active");

    // Attach: the supervisor proves readiness with get_state.
    let response = rpc(
        router,
        1,
        "sessions.attach",
        json!({ "session_id": SESSION_ID }),
    )
    .await;
    assert!(response.get("error").is_none(), "attach failed: {response}");
    assert_eq!(
        response.pointer("/result/state").and_then(Value::as_str),
        Some("attached")
    );
    assert_eq!(
        response
            .pointer("/result/pi/sessionName")
            .and_then(Value::as_str),
        Some("mock session")
    );

    // The catalog now reports the session as attached.
    let sessions = list(router).await;
    assert_eq!(
        field(&sessions[0], "attachment").as_str().unwrap(),
        "attached"
    );
    assert_eq!(field(&sessions[0], "status").as_str().unwrap(), "idle");

    // Live messages come from the attached process.
    let response = rpc(
        router,
        2,
        "sessions.messages",
        json!({ "session_id": SESSION_ID }),
    )
    .await;
    let messages = response.pointer("/result/messages").expect("messages");
    assert_eq!(messages.as_array().map(Vec::len), Some(2));
    assert_eq!(
        messages.pointer("/0/content").and_then(Value::as_str),
        Some("Stored prompt")
    );

    // Prompt: agent_start marks the session active.
    let response = rpc(
        router,
        3,
        "sessions.prompt",
        json!({ "session_id": SESSION_ID, "message": "Run the tests" }),
    )
    .await;
    assert!(response.get("error").is_none(), "prompt failed: {response}");
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    let sessions = list(router).await;
    assert_eq!(field(&sessions[0], "status").as_str().unwrap(), "active");

    // Abort: agent_settled marks it idle again.
    let response = rpc(
        router,
        4,
        "sessions.abort",
        json!({ "session_id": SESSION_ID }),
    )
    .await;
    assert!(response.get("error").is_none(), "abort failed: {response}");
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    let sessions = list(router).await;
    assert_eq!(field(&sessions[0], "status").as_str().unwrap(), "idle");

    // Detach returns the session to resumable.
    let response = rpc(
        router,
        5,
        "sessions.detach",
        json!({ "session_id": SESSION_ID }),
    )
    .await;
    assert!(response.get("error").is_none(), "detach failed: {response}");
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    let sessions = list(router).await;
    assert_eq!(
        field(&sessions[0], "attachment").as_str().unwrap(),
        "resumable"
    );

    // Without an attachment, messages fall back to the stored transcript.
    let response = rpc(
        router,
        6,
        "sessions.messages",
        json!({ "session_id": SESSION_ID }),
    )
    .await;
    let messages = response
        .pointer("/result/messages")
        .expect("stored messages");
    assert_eq!(messages.as_array().map(Vec::len), Some(2));
    assert_eq!(
        messages.pointer("/0/text").and_then(Value::as_str),
        Some("Restore the SSE stream")
    );

    drop(fixture);
}

#[tokio::test]
async fn session_events_are_broadcast() {
    let _guard = TEST_LOCK.lock().await;
    let fixture = fixture();
    let router = &fixture.router;
    let mut stream = fixture.state.events.subscribe();

    let response = rpc(
        router,
        1,
        "sessions.attach",
        json!({ "session_id": SESSION_ID }),
    )
    .await;
    assert!(response.get("error").is_none(), "attach failed: {response}");
    let _ = rpc(
        router,
        2,
        "sessions.prompt",
        json!({ "session_id": SESSION_ID, "message": "Run the tests" }),
    )
    .await;

    // Attach and prompt produce the expected event sequence on the bus.
    let mut names = Vec::new();
    for _ in 0..8 {
        match tokio::time::timeout(std::time::Duration::from_secs(2), stream.recv()).await {
            Ok(Ok(event)) => names.push(event.event),
            _ => break,
        }
    }
    assert!(names.contains(&"session.attached".to_owned()));
    assert!(names.contains(&"session.updated".to_owned()));
    assert!(names.contains(&"pi.event".to_owned()));

    drop(fixture);
}

#[tokio::test]
async fn unknown_sessions_and_methods_report_errors() {
    let _guard = TEST_LOCK.lock().await;
    let fixture = fixture();
    let router = &fixture.router;

    let response = rpc(
        router,
        1,
        "sessions.attach",
        json!({ "session_id": Uuid::new_v4().to_string() }),
    )
    .await;
    assert_eq!(
        response.pointer("/error/code").and_then(Value::as_str),
        Some("SESSION_NOT_FOUND")
    );

    let response = rpc(router, 2, "sessions.nonsense", json!({})).await;
    assert_eq!(
        response.pointer("/error/code").and_then(Value::as_str),
        Some("METHOD_NOT_FOUND")
    );

    let response = rpc(
        router,
        3,
        "sessions.steer",
        json!({ "session_id": SESSION_ID, "message": "x" }),
    )
    .await;
    assert_eq!(
        response.pointer("/error/code").and_then(Value::as_str),
        Some("PI_COMMAND_FAILED")
    );

    drop(fixture);
}
