use axum::{
    routing::get,
    Router,
    response::Html,
    extract::State,
};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::services::ServeDir;
use serde::Serialize;

// Shared state between temperature server and web server
#[derive(Clone)]
pub struct WebState {
    pub server_state: Arc<RwLock<ServerState>>,
}

#[derive(Default, Clone, Serialize)]
pub struct ServerState {
    pub bedroom: RoomState,
    pub kids_bedroom: RoomState,
}

#[derive(Default, Clone, Serialize)]
pub struct RoomState {
    pub sensor_available: bool,
    pub current_temp: f64,
    pub target_temp: f64,
    pub relay_available: bool,
    pub relay_state: bool,
}

pub async fn create_web_server(server_state: Arc<RwLock<ServerState>>) {
    let app_state = WebState { server_state };

    let app = Router::new()
        .route("/", get(serve_status_page))
        .route("/api/status", get(get_status))
        .nest_service("/static", ServeDir::new("static"))
        .with_state(app_state);

    println!("Starting web server on http://localhost:8080");
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn serve_status_page() -> Html<&'static str> {
    // For now, we'll serve the HTML directly from the binary
    // Later we can move it to a static file
    Html(include_str!("status.html"))
}

async fn get_status(
    State(state): State<WebState>,
) -> axum::Json<ServerState> {
    let server_state = state.server_state.read().await;
    axum::Json((*server_state).clone())
} 