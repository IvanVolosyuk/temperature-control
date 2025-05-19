use axum::{
    routing::{get, post},
    Router,
    response::Html,
    extract::{State, Json, Query},
};
use std::sync::Arc;
use tower_http::services::ServeDir;
use std::sync::RwLock;
use serde::{Serialize, Deserialize};
use temperature_protocol::relay::set_relay;
use chrono::Local;

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

    let app = Router::new()
        .route("/", get(serve_status_page))
        .route("/api/status", get(get_status))
        .route("/api/relay", post(control_relay))
        .route("/api/disable", post(disable_heater))
        .nest_service("/static", ServeDir::new("apps/server/static"))
        .with_state(app_state);

    println!("Starting web server on http://localhost:8080");
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn serve_status_page() -> Html<String> {
    // Read the HTML file from the filesystem
    match tokio::fs::read_to_string("apps/server/static/status.html").await {
        Ok(content) => Html(content),
        Err(e) => {
            eprintln!("Error reading status.html: {}", e);
            Html("Error loading page".to_string())
        }
    }
}

async fn get_status(
    State(state): State<WebState>,
    Query(query): Query<StatusQuery>,
) -> axum::Json<ServerState> {
    let server_state = state.server_state.read().unwrap();
    let mut response_state = (*server_state).clone();

    // If last_update timestamp is provided, filter temperature history
    if let Some(last_update) = query.last_update {
        // Filter bedroom temperature history
        response_state.bedroom.temperature_history = server_state.bedroom.temperature_history
            .iter()
            .filter(|point| point.timestamp > last_update)
            .cloned()
            .collect();

        // Filter kids bedroom temperature history
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
        _ => return axum::Json(serde_json::json!({
            "success": false,
            "error": "Invalid room"
        }))
    };

    // Use set_relay from relay.rs with 0 delay
    match set_relay(relay_hostname, request.state, 0) {
        Ok(_) => {
            // Update the web state to reflect the change
            let mut server_state = state.server_state.write().unwrap();
            match request.room.as_str() {
                "bedroom" => server_state.bedroom.relay_state = request.state,
                "kids_bedroom" => server_state.kids_bedroom.relay_state = request.state,
                _ => {}
            }
            axum::Json(serde_json::json!({
                "success": true
            }))
        }
        Err(e) => axum::Json(serde_json::json!({
            "success": false,
            "error": e.to_string()
        }))
    }
}

async fn disable_heater(
    State(state): State<WebState>,
    Json(request): Json<DisableHeaterRequest>,
) -> axum::Json<serde_json::Value> {
    let mut server_state = state.server_state.write().unwrap();
    let room_state = match request.room.as_str() {
        "bedroom" => &mut server_state.bedroom,
        "kids_bedroom" => &mut server_state.kids_bedroom,
        _ => return axum::Json(serde_json::json!({
            "success": false,
            "error": "Invalid room"
        }))
    };

    if request.disable {
        // Disable for 2 hours
        room_state.disabled_until = Some(Local::now().timestamp() + 2 * 3600);
        // Turn off heater if it's on
        if room_state.relay_state {
            let relay_hostname = match request.room.as_str() {
                "bedroom" => "esp8266-relay0.local",
                "kids_bedroom" => "esp8266-relay2.local",
                _ => unreachable!()
            };
            if let Err(e) = set_relay(relay_hostname, false, 0) {
                return axum::Json(serde_json::json!({
                    "success": false,
                    "error": e.to_string()
                }));
            }
            room_state.relay_state = false;
        }
    } else {
        room_state.disabled_until = None;
    }

    axum::Json(serde_json::json!({
        "success": true
    }))
}
