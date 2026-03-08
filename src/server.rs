use std::sync::Arc;

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router as AxumRouter};
use serde_json::{Value, json};

use crate::Result;
use crate::discord::DiscordClient;
use crate::events::{IncomingEvent, normalize_event};
use crate::router::Router;

#[derive(Clone)]
struct AppState {
    router: Arc<Router>,
    discord: Arc<DiscordClient>,
}

pub async fn serve(
    addr: std::net::SocketAddr,
    router: Arc<Router>,
    discord: Arc<DiscordClient>,
) -> Result<()> {
    let app_state = AppState { router, discord };

    let app = AxumRouter::new()
        .route("/health", get(health))
        .route("/events", post(post_events))
        .route("/github", post(post_github))
        .with_state(app_state);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    println!("clawhip listening on http://{}", listener.local_addr()?);
    axum::serve(listener, app).await?;
    Ok(())
}

async fn health() -> impl IntoResponse {
    Json(json!({ "ok": true }))
}

async fn post_events(
    State(state): State<AppState>,
    Json(event): Json<IncomingEvent>,
) -> impl IntoResponse {
    let event = normalize_event(event);
    match state.router.dispatch(&event, state.discord.as_ref()).await {
        Ok(()) => (
            StatusCode::ACCEPTED,
            Json(json!({ "ok": true, "type": event.kind })),
        )
            .into_response(),
        Err(error) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "ok": false, "error": error.to_string() })),
        )
            .into_response(),
    }
}

async fn post_github(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> impl IntoResponse {
    let event_name = headers
        .get("x-github-event")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    let action = payload
        .get("action")
        .and_then(Value::as_str)
        .unwrap_or_default();

    let dispatch_result = match event_name {
        "issues" if action == "opened" => {
            let repo = payload
                .pointer("/repository/full_name")
                .and_then(Value::as_str)
                .unwrap_or("unknown/unknown")
                .to_string();
            let number = payload
                .pointer("/issue/number")
                .and_then(Value::as_u64)
                .unwrap_or_default();
            let title = payload
                .pointer("/issue/title")
                .and_then(Value::as_str)
                .unwrap_or("Untitled issue")
                .to_string();
            let event = IncomingEvent::github_issue_opened(repo, number, title, None);
            state.router.dispatch(&event, state.discord.as_ref()).await
        }
        "pull_request" => {
            let repo = payload
                .pointer("/repository/full_name")
                .and_then(Value::as_str)
                .unwrap_or("unknown/unknown")
                .to_string();
            let number = payload
                .pointer("/pull_request/number")
                .or_else(|| payload.pointer("/number"))
                .and_then(Value::as_u64)
                .unwrap_or_default();
            let title = payload
                .pointer("/pull_request/title")
                .and_then(Value::as_str)
                .unwrap_or("Untitled pull request")
                .to_string();
            let url = payload
                .pointer("/pull_request/html_url")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let merged = payload
                .pointer("/pull_request/merged")
                .and_then(Value::as_bool)
                .unwrap_or(false);

            let transition = match action {
                "opened" => Some(("<new>".to_string(), "open".to_string())),
                "reopened" => Some(("closed".to_string(), "open".to_string())),
                "closed" if merged => Some(("open".to_string(), "merged".to_string())),
                "closed" => Some(("open".to_string(), "closed".to_string())),
                _ => None,
            };

            if let Some((old_status, new_status)) = transition {
                let event = IncomingEvent::git_pr_status_changed(
                    repo, number, title, old_status, new_status, url, None,
                );
                state.router.dispatch(&event, state.discord.as_ref()).await
            } else {
                return (
                    StatusCode::ACCEPTED,
                    Json(json!({ "ok": true, "ignored": true, "reason": "unsupported pull_request action" })),
                )
                    .into_response();
            }
        }
        _ => {
            return (
                StatusCode::ACCEPTED,
                Json(json!({ "ok": true, "ignored": true, "reason": "unsupported event" })),
            )
                .into_response();
        }
    };

    match dispatch_result {
        Ok(()) => (StatusCode::ACCEPTED, Json(json!({ "ok": true }))).into_response(),
        Err(error) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "ok": false, "error": error.to_string() })),
        )
            .into_response(),
    }
}
