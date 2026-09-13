//! JSON-RPC surface for the desktop.
//!
//! Command methods map onto Pi's documented RPC commands
//! (https://pi.dev/docs/latest/rpc): `prompt`, `steer`, `follow_up`, `abort`,
//! `clear_queue`, `get_state`, `get_messages`, `get_entries`, `set_model`.
//! Reading without an attachment falls back to the stored JSONL transcript.

use crate::{catalog::Catalog, supervisor::PiSupervisor};
use agentifi_domain::AgentSession;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct RpcRequest {
    #[allow(dead_code)]
    pub jsonrpc: String,
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Serialize)]
pub struct RpcResponse {
    pub jsonrpc: &'static str,
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

#[derive(Debug, Serialize)]
pub struct RpcError {
    pub code: &'static str,
    pub message: String,
}

pub struct RpcContext {
    pub catalog: Arc<Catalog>,
    pub supervisor: PiSupervisor,
}

pub async fn dispatch(context: &RpcContext, request: RpcRequest) -> RpcResponse {
    match handle(context, &request).await {
        Ok(result) => RpcResponse {
            jsonrpc: "2.0",
            id: request.id,
            result: Some(result),
            error: None,
        },
        Err((code, message)) => error_response(request.id, code, message),
    }
}

async fn handle(
    context: &RpcContext,
    request: &RpcRequest,
) -> Result<Value, (&'static str, String)> {
    let session_id = request
        .params
        .get("session_id")
        .and_then(Value::as_str)
        .map(str::to_owned);

    match request.method.as_str() {
        "sessions.list" => Ok(json!(context.catalog.list().await.map_err(internal)?)),
        "sessions.get" => {
            let id = parse_id(&session_id)?;
            let session = find(context, id).await?;
            Ok(serde_json::to_value(session)
                .map_err(|error| ("INTERNAL_ERROR", error.to_string()))?)
        }
        "sessions.attach" => {
            let id = parse_id(&session_id)?;
            let session = find(context, id).await?;
            let path = session.source_path.ok_or((
                "SESSION_SOURCE_MISSING",
                "Session has no Pi source path".into(),
            ))?;
            let state = context
                .supervisor
                .attach(&id.to_string(), &path)
                .await
                .map_err(pi_command_failed)?;
            Ok(json!({
                "session_id": id.to_string(),
                "state": "attached",
                "pi": state,
            }))
        }
        "sessions.detach" => {
            let id = parse_id(&session_id)?;
            context
                .supervisor
                .detach(&id.to_string())
                .await
                .map_err(|_| ("NOT_ATTACHED", "Session is not attached".into()))?;
            Ok(json!({ "session_id": id.to_string(), "state": "detached" }))
        }
        "sessions.state" => {
            let id = parse_id(&session_id)?;
            pi(context, &id, json!({ "type": "get_state" })).await
        }
        "sessions.messages" => {
            let id = parse_id(&session_id)?;
            if context
                .supervisor
                .snapshot()
                .await
                .contains_key(&id.to_string())
            {
                // Live conversation from the attached process.
                pi(context, &id, json!({ "type": "get_messages" })).await
            } else {
                // Stored conversation from the JSONL transcript.
                let messages = context.catalog.transcript(id).await.map_err(internal)?;
                Ok(json!({ "messages": messages }))
            }
        }
        "sessions.entries" => {
            let id = parse_id(&session_id)?;
            let mut command = json!({ "type": "get_entries" });
            if let Some(since) = request.params.get("since").and_then(Value::as_str) {
                command["since"] = json!(since);
            }
            pi(context, &id, command).await
        }
        "sessions.prompt" => {
            let id = parse_id(&session_id)?;
            pi(context, &id, message_command("prompt", request)).await
        }
        "sessions.steer" => {
            let id = parse_id(&session_id)?;
            pi(context, &id, message_command("steer", request)).await
        }
        "sessions.follow_up" => {
            let id = parse_id(&session_id)?;
            pi(context, &id, message_command("follow_up", request)).await
        }
        "sessions.abort" => {
            let id = parse_id(&session_id)?;
            pi(context, &id, json!({ "type": "abort" })).await
        }
        "sessions.clear_queue" => {
            let id = parse_id(&session_id)?;
            pi(context, &id, json!({ "type": "clear_queue" })).await
        }
        "sessions.set_model" => {
            let id = parse_id(&session_id)?;
            let provider = request
                .params
                .get("provider")
                .and_then(Value::as_str)
                .ok_or(("INVALID_PARAMS", "provider is required".into()))?;
            let model_id = request
                .params
                .get("model_id")
                .or_else(|| request.params.get("modelId"))
                .and_then(Value::as_str)
                .ok_or(("INVALID_PARAMS", "model_id is required".into()))?;
            pi(
                context,
                &id,
                json!({ "type": "set_model", "provider": provider, "modelId": model_id }),
            )
            .await
        }
        other => Err(("METHOD_NOT_FOUND", format!("Unsupported method: {other}"))),
    }
}

fn message_command(kind: &str, request: &RpcRequest) -> Value {
    let message = request
        .params
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or_default();
    json!({ "type": kind, "message": message })
}

async fn pi(
    context: &RpcContext,
    id: &Uuid,
    command: Value,
) -> Result<Value, (&'static str, String)> {
    context
        .supervisor
        .request(&id.to_string(), command)
        .await
        .map_err(pi_command_failed)
}

fn pi_command_failed(error: anyhow::Error) -> (&'static str, String) {
    ("PI_COMMAND_FAILED", error.to_string())
}

fn internal(error: anyhow::Error) -> (&'static str, String) {
    ("INTERNAL_ERROR", error.to_string())
}

fn parse_id(session_id: &Option<String>) -> Result<Uuid, (&'static str, String)> {
    session_id
        .as_deref()
        .and_then(|id| Uuid::parse_str(id).ok())
        .ok_or(("INVALID_PARAMS", "a valid session_id is required".into()))
}

async fn find(context: &RpcContext, id: Uuid) -> Result<AgentSession, (&'static str, String)> {
    context
        .catalog
        .list()
        .await
        .map_err(internal)?
        .into_iter()
        .find(|session| session.id == id)
        .ok_or(("SESSION_NOT_FOUND", "Session not found".into()))
}

fn error_response(id: Option<Value>, code: &'static str, message: String) -> RpcResponse {
    RpcResponse {
        jsonrpc: "2.0",
        id,
        result: None,
        error: Some(RpcError {
            code,
            message: message.to_owned(),
        }),
    }
}
