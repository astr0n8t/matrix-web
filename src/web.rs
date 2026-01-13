use axum::{
    extract::State,
    http::StatusCode,
    response::{sse::Event, Html, IntoResponse, Sse},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::sync::Arc;
use tokio_stream::{wrappers::BroadcastStream, StreamExt};
use tracing::info;

use crate::bot::MatrixBot;

#[derive(Clone)]
pub struct AppState {
    pub bot: MatrixBot,
}

#[derive(Deserialize)]
pub struct SendMessageRequest {
    pub message: String,
}

#[derive(Serialize)]
pub struct SendMessageResponse {
    pub success: bool,
    pub error: Option<String>,
}

pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/", get(index_handler))
        .route("/api/messages", post(send_message_handler))
        .route("/api/stream", get(stream_messages_handler))
        .with_state(Arc::new(state))
}

async fn index_handler() -> Html<&'static str> {
    Html(include_str!("../static/index.html"))
}

async fn send_message_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<SendMessageRequest>,
) -> impl IntoResponse {
    if payload.message.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(SendMessageResponse {
                success: false,
                error: Some("Message cannot be empty".to_string()),
            }),
        );
    }

    match state.bot.send_message(&payload.message).await {
        Ok(_) => (
            StatusCode::OK,
            Json(SendMessageResponse {
                success: true,
                error: None,
            }),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(SendMessageResponse {
                success: false,
                error: Some(e.to_string()),
            }),
        ),
    }
}

async fn stream_messages_handler(
    State(state): State<Arc<AppState>>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let rx = state.bot.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|msg| match msg {
        Ok(message) => Some(Ok(Event::default().data(message))),
        Err(e) => {
            tracing::warn!("Broadcast stream error: {}", e);
            None
        }
    });

    Sse::new(stream)
}

pub async fn start_server(host: &str, port: u16, state: AppState) -> anyhow::Result<()> {
    let app = create_router(state);
    let addr = format!("{}:{}", host, port);
    info!("Starting web server on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
