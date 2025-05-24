use axum::{
    routing::{get, post},
    Router,
    response::{Html, IntoResponse, Response},
    extract::{State, Json, Query},
    http::{StatusCode, Uri}, // Added Uri
};
use std::sync::Arc;
use tower_http::services::ServeDir;
use tower_http::compression::CompressionLayer;
use tokio::sync::RwLock; // Keep tokio RwLock
use serde::{Serialize, Deserialize};
use temperature_protocol::relay::set_relay;
use chrono::Local;
use std::path::PathBuf; // Added PathBuf
use tokio::fs; // Added tokio::fs for reading index.html

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
    pub temperature_history: Vec<TemperaturePoint>,
    pub disabled_until: Option<i64>, // Timestamp when disabled state expires
}

#[derive(Default, Clone, Serialize)]
pub struct TemperaturePoint {
    pub timestamp: i64,
    pub temperature: f64,
    pub target: f64,
    pub heater_on: bool,
    pub is_disabled: bool,
}

#[derive(Deserialize)]
pub struct RelayControlRequest {
    room: String,
    state: bool,
}

#[derive(Deserialize)]
pub struct StatusQuery {
    last_update: Option<i64>,
}

#[derive(Deserialize)]
pub struct DisableHeaterRequest {
    room: String,
    disable: bool, // true to disable, false to restore
}


pub async fn create_web_server(server_state: Arc<RwLock<ServerState>>) {
    let app_state = WebState { server_state };

    // Path to the React app's dist directory - adjust if server runs from different location
    let react_dist_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent().unwrap().parent().unwrap() // Navigate from apps/server/Cargo.toml to repo root
        .join("temperature-react-ui/dist");

    let assets_path = react_dist_path.join("assets");

    let spa_router = Router::new()
        .nest_service("/assets", ServeDir::new(assets_path.clone())) // Serve static assets (JS, CSS)
        .nest_service("/favicon.ico", tower_http::services::ServeFile::new(react_dist_path.join("favicon.ico")))
        .nest_service("/manifest.json", tower_http::services::ServeFile::new(react_dist_path.join("manifest.json")))
        // Add other specific public files if needed (e.g., favicons, svgs)
        // These were moved to temperature-react-ui/public and should be in dist after build
        .nest_service("/favicon.png", tower_http::services::ServeFile::new(react_dist_path.join("favicon.png")))
        .nest_service("/favicon-192.png", tower_http::services::ServeFile::new(react_dist_path.join("favicon-192.png")))
        .nest_service("/favicon-512.png", tower_http::services::ServeFile::new(react_dist_path.join("favicon-512.png")))
        .nest_service("/power.svg", tower_http::services::ServeFile::new(react_dist_path.join("power.svg")))
        .nest_service("/thermometer.svg", tower_http::services::ServeFile::new(react_dist_path.join("thermometer.svg")))
        .nest_service("/vite.svg", tower_http::services::ServeFile::new(react_dist_path.join("vite.svg"))) // Vite's default icon
        .fallback(get(serve_react_app_index)); // Fallback to serving index.html for SPA routing


    let app = Router::new()
        // API routes (ensure they are matched before SPA fallback)
        .route("/api/status", get(get_status))
        .route("/api/relay", post(control_relay))
        .route("/api/disable", post(disable_heater))
        // Mount the SPA router (serving static files and index.html)
        // IMPORTANT: This should generally be the last thing if it has a broad fallback
        .merge(spa_router) 
        .layer(CompressionLayer::new())
        .with_state(app_state);

    println!("Starting web server on http://localhost:8080");
    println!("React app should be served from: {}", react_dist_path.display());
    println!("Assets should be served from: {}", assets_path.display());
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

// Serves the index.html for the React SPA
async fn serve_react_app_index(uri: Uri) -> impl IntoResponse {
    println!("Fallback route hit for URI: {}", uri); // Log which URI is hitting fallback
    let react_dist_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent().unwrap().parent().unwrap()
        .join("temperature-react-ui/dist");
    let index_html_path = react_dist_path.join("index.html");

    match fs::read_to_string(index_html_path).await {
        Ok(content) => Html(content).into_response(),
        Err(e) => {
            eprintln!("Error reading React index.html: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to load React app: {}", e)).into_response()
        }
    }
}

async fn get_status(
    State(state): State<WebState>,
    Query(query): Query<StatusQuery>,
) -> axum::Json<ServerState> {
    let server_state = state.server_state.read().await;
    let mut response_state = (*server_state).clone();

    if let Some(last_update) = query.last_update {
        response_state.bedroom.temperature_history = server_state.bedroom.temperature_history
            .iter()
            .filter(|point| point.timestamp > last_update)
            .cloned()
            .collect();
        response_state.kids_bedroom.temperature_history = server_state.kids_bedroom.temperature_history
            .iter()
            .filter(|point| point.timestamp > last_update)
            .cloned()
            .collect();
    }
    axum::Json(response_state)
}

async fn control_relay(
    State(state): State<WebState>,
    Json(request): Json<RelayControlRequest>,
) -> axum::Json<serde_json::Value> {
    let relay_hostname = match request.room.as_str() {
        "bedroom" => "esp8266-relay0.local",
        "kids_bedroom" => "esp8266-relay2.local",
        _ => return axum::Json(serde_json::json!({ "success": false, "error": "Invalid room" }))
    };

    match set_relay(relay_hostname, request.state, 0) {
        Ok(_) => {
            let mut server_state = state.server_state.write().await;
            match request.room.as_str() {
                "bedroom" => server_state.bedroom.relay_state = request.state,
                "kids_bedroom" => server_state.kids_bedroom.relay_state = request.state,
                _ => {}
            }
            axum::Json(serde_json::json!({ "success": true }))
        }
        Err(e) => axum::Json(serde_json::json!({ "success": false, "error": e.to_string() }))
    }
}

async fn disable_heater(
    State(state): State<WebState>,
    Json(request): Json<DisableHeaterRequest>,
) -> axum::Json<serde_json::Value> {
    let mut server_state = state.server_state.write().await;
    let room_state_arc = match request.room.as_str() {
        "bedroom" => &mut server_state.bedroom,
        "kids_bedroom" => &mut server_state.kids_bedroom,
        _ => return axum::Json(serde_json::json!({ "success": false, "error": "Invalid room" }))
    };

    if request.disable {
        room_state_arc.disabled_until = Some(Local::now().timestamp() + 2 * 3600);
        if room_state_arc.relay_state { // if heater is on, turn it off
            let relay_hostname = match request.room.as_str() {
                "bedroom" => "esp8266-relay0.local",
                "kids_bedroom" => "esp8266-relay2.local",
                _ => unreachable!()
            };
            if let Err(e) = set_relay(relay_hostname, false, 0) {
                return axum::Json(serde_json::json!({ "success": false, "error": e.to_string() }));
            }
            room_state_arc.relay_state = false;
        }
    } else {
        room_state_arc.disabled_until = None;
    }
    axum::Json(serde_json::json!({ "success": true }))
}
