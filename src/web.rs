use axum::{
    extract::State,
    http::{Request, StatusCode},
    middleware::{self, Next},
    response::{sse::Event, Html, IntoResponse, Response, Sse},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::sync::Arc;
use tokio_stream::{wrappers::BroadcastStream, StreamExt};
use tracing::{info, warn};

use crate::bot::MatrixBot;
use crate::config::{AuthConfig, hash_value};

#[derive(Clone)]
pub struct AppState {
    pub bot: MatrixBot,
    pub auth: Option<AuthConfig>,
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

#[derive(Serialize)]
pub struct MessageHistoryResponse {
    pub messages: Vec<String>,
}

#[derive(Deserialize)]
pub struct LoginRequest {
    pub passphrase: String,
}

#[derive(Serialize)]
pub struct LoginResponse {
    pub success: bool,
    pub error: Option<String>,
}

#[derive(Serialize)]
pub struct StatusResponse {
    pub connected: bool,
}

pub fn create_router(state: AppState) -> Router {
    let router = Router::new()
        .route("/", get(index_handler))
        .route("/api/login", post(login_handler))
        .route("/api/logout", post(logout_handler))
        .route("/api/status", get(status_handler))
        .route("/api/messages", post(send_message_handler))
        .route("/api/history", get(get_message_history_handler))
        .route("/api/stream", get(stream_messages_handler));

    // Apply authentication middleware if configured
    if state.auth.is_some() {
        router
            .layer(middleware::from_fn_with_state(
                Arc::new(state.clone()),
                auth_middleware,
            ))
            .with_state(Arc::new(state))
    } else {
        router.with_state(Arc::new(state))
    }
}

async fn auth_middleware(
    State(state): State<Arc<AppState>>,
    request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    if let Some(ref auth_config) = state.auth {
        let headers = request.headers();
        
        if let Some(header_value) = headers.get(&auth_config.header_name) {
            if let Ok(value_str) = header_value.to_str() {
                // Hash the incoming header value and compare with stored hash
                let incoming_hash = hash_value(value_str);
                if incoming_hash == auth_config.header_value_hash {
                    return Ok(next.run(request).await);
                }
            }
        }
        
        warn!("Authentication failed: invalid or missing header");
        return Err(StatusCode::UNAUTHORIZED);
    }
    
    Ok(next.run(request).await)
}

async fn index_handler() -> Html<&'static str> {
    Html(include_str!("../static/index.html"))
}

async fn status_handler(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let connected = state.bot.is_connected().await;
    Json(StatusResponse { connected })
}

async fn login_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<LoginRequest>,
) -> impl IntoResponse {
    if state.bot.is_connected().await {
        return (
            StatusCode::OK,
            Json(LoginResponse {
                success: true,
                error: None,
            }),
        );
    }

    match state.bot.connect(&payload.passphrase).await {
        Ok(_) => {
            info!("Bot connected successfully");
            (
                StatusCode::OK,
                Json(LoginResponse {
                    success: true,
                    error: None,
                }),
            )
        }
        Err(e) => {
            warn!("Login failed: {}", e);
            (
                StatusCode::UNAUTHORIZED,
                Json(LoginResponse {
                    success: false,
                    error: Some(format!("Failed to connect: {}", e)),
                }),
            )
        }
    }
}

async fn logout_handler(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    match state.bot.disconnect().await {
        Ok(_) => {
            info!("Bot disconnected successfully");
            (
                StatusCode::OK,
                Json(LoginResponse {
                    success: true,
                    error: None,
                }),
            )
        }
        Err(e) => {
            warn!("Logout failed: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(LoginResponse {
                    success: false,
                    error: Some(format!("Failed to disconnect: {}", e)),
                }),
            )
        }
    }
}

async fn get_message_history_handler(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let messages = state.bot.get_message_history().await;
    Json(MessageHistoryResponse { messages })
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
