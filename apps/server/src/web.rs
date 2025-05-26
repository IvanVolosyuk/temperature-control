use crate::WsTx; // Corrected import path again
use axum::{
    extract::{
        ws::{Message as WsMessage, WebSocket, WebSocketUpgrade},
        Json, Query, State,
    },
    http::{StatusCode, Uri},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Router,
};
use chrono::Local;
use futures_util::{
    sink::SinkExt,
    stream::{SplitSink, SplitStream, StreamExt},
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf; // Added PathBuf
use std::sync::Arc;
use temperature_protocol::relay::set_relay;
use tokio::fs;
use tokio::sync::{mpsc, RwLock};
use tower_http::compression::CompressionLayer;
use tower_http::services::ServeDir; // Added tokio::fs for reading index.html

// Shared state between temperature server and web server
#[derive(Clone)]
pub struct WebState {
    pub server_state: Arc<RwLock<ServerState>>,
    pub ws_connections: Arc<RwLock<Vec<WsTx>>>,
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
    pub override_temperature: Option<f64>,
    pub override_until: Option<i64>,
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

#[derive(Deserialize)]
pub struct OverrideTemperatureRequest {
    room: String,
    temperature: Option<f64>,
}

pub async fn create_web_server(
    server_state: Arc<RwLock<ServerState>>,
    ws_connections: Arc<RwLock<Vec<WsTx>>>,
) {
    let app_state = WebState {
        server_state,
        ws_connections,
    };

    // Path to the React app's dist directory - adjust if server runs from different location
    let react_dist_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap() // Navigate from apps/server/Cargo.toml to repo root
        .join("temperature-react-ui/dist");

    let assets_path = react_dist_path.join("assets");

    let spa_router = Router::new()
        .nest_service("/assets", ServeDir::new(assets_path.clone())) // Serve static assets (JS, CSS)
        .nest_service(
            "/favicon.ico",
            tower_http::services::ServeFile::new(react_dist_path.join("favicon.ico")),
        )
        .nest_service(
            "/manifest.json",
            tower_http::services::ServeFile::new(react_dist_path.join("manifest.json")),
        )
        // Add other specific public files if needed (e.g., favicons, svgs)
        // These were moved to temperature-react-ui/public and should be in dist after build
        .nest_service(
            "/favicon.png",
            tower_http::services::ServeFile::new(react_dist_path.join("favicon.png")),
        )
        .nest_service(
            "/favicon-192.png",
            tower_http::services::ServeFile::new(react_dist_path.join("favicon-192.png")),
        )
        .nest_service(
            "/favicon-512.png",
            tower_http::services::ServeFile::new(react_dist_path.join("favicon-512.png")),
        )
        .nest_service(
            "/power.svg",
            tower_http::services::ServeFile::new(react_dist_path.join("power.svg")),
        )
        .nest_service(
            "/thermometer.svg",
            tower_http::services::ServeFile::new(react_dist_path.join("thermometer.svg")),
        )
        .nest_service(
            "/vite.svg",
            tower_http::services::ServeFile::new(react_dist_path.join("vite.svg")),
        ) // Vite's default icon
        .fallback(get(serve_react_app_index)); // Fallback to serving index.html for SPA routing

    let app = Router::new()
        // API routes (ensure they are matched before SPA fallback)
        .route("/api/status", get(get_status))
        .route("/api/relay", post(control_relay))
        .route("/api/disable", post(disable_heater))
        .route(
            "/api/override_temperature",
            post(override_temperature_handler),
        )
        .route("/ws", get(ws_handler))
        // Mount the SPA router (serving static files and index.html)
        // IMPORTANT: This should generally be the last thing if it has a broad fallback
        .merge(spa_router)
        .layer(CompressionLayer::new())
        .with_state(app_state);

    println!("Starting web server on http://localhost:8080");
    println!(
        "React app should be served from: {}",
        react_dist_path.display()
    );
    println!("Assets should be served from: {}", assets_path.display());
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<WebState>) -> Response {
    println!("WebSocket connection upgrade requested");
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: WebState) {
    println!("WebSocket connection established");
    let (ws_sender, ws_receiver) = socket.split();
    let (tx, rx) = mpsc::channel(100); // Channel for this specific connection

    // Add this connection's sender to the shared list
    state.ws_connections.write().await.push(tx.clone());
    println!(
        "WebSocket TX channel added to shared list. Total connections: {}",
        state.ws_connections.read().await.len()
    );

    let mut rx_task = tokio::spawn(send_state_updates(rx, ws_sender));
    let mut tx_task = tokio::spawn(receive_ws_messages(ws_receiver, tx.clone(), state.clone())); // Pass a clone of tx for removal later

    // Keep the connection alive until one of the tasks finishes
    tokio::select! {
        _ = (&mut rx_task) => {
            println!("RX task finished.");
            tx_task.abort(); // Abort the other task
        },
        _ = (&mut tx_task) => {
            println!("TX task finished.");
            rx_task.abort(); // Abort the other task
        },
    }

    println!("WebSocket connection closing. Removing TX channel from shared list.");
    // Remove the sender from the list
    let mut conns = state.ws_connections.write().await;
    if let Some(pos) = conns.iter().position(|x| x.same_channel(&tx)) {
        conns.remove(pos);
        println!(
            "WebSocket TX channel removed. Total connections: {}",
            conns.len()
        );
    } else {
        println!("WebSocket TX channel not found in shared list for removal.");
    }
}

async fn send_state_updates(
    mut rx: mpsc::Receiver<WsMessage>,
    mut ws_sender: SplitSink<WebSocket, WsMessage>,
) -> Result<(), axum::Error> {
    while let Some(message) = rx.recv().await {
        if ws_sender.send(message).await.is_err() {
            println!("Failed to send message to WebSocket client, client disconnected?");
            break; // Client disconnected
        }
    }
    Ok(())
}

async fn receive_ws_messages(
    mut ws_receiver: SplitStream<WebSocket>,
    _tx: WsTx, // Keep for potential future use (e.g., client sending commands) or removal logic
    _state: WebState, // Keep for potential future use
) -> Result<(), axum::Error> {
    while let Some(msg_result) = ws_receiver.next().await {
        match msg_result {
            Ok(msg) => {
                if let WsMessage::Close(_) = msg {
                    println!("Client sent close message.");
                    break; // Exit loop on close message
                }
                // Process other messages if needed
                println!(
                    "Received message from client (currently ignored): {:?}",
                    msg
                );
            }
            Err(e) => {
                println!("Error receiving message from WebSocket client: {}", e);
                break; // Error receiving message
            }
        }
    }
    Ok(())
}

// Serves the index.html for the React SPA
async fn serve_react_app_index(uri: Uri) -> impl IntoResponse {
    println!("Fallback route hit for URI: {}", uri); // Log which URI is hitting fallback
    let react_dist_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("temperature-react-ui/dist");
    let index_html_path = react_dist_path.join("index.html");

    match fs::read_to_string(index_html_path).await {
        Ok(content) => Html(content).into_response(),
        Err(e) => {
            eprintln!("Error reading React index.html: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to load React app: {}", e),
            )
                .into_response()
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
        response_state.bedroom.temperature_history = server_state
            .bedroom
            .temperature_history
            .iter()
            .filter(|point| point.timestamp > last_update)
            .cloned()
            .collect();
        response_state.kids_bedroom.temperature_history = server_state
            .kids_bedroom
            .temperature_history
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
        _ => return axum::Json(serde_json::json!({ "success": false, "error": "Invalid room" })),
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
        Err(e) => axum::Json(serde_json::json!({ "success": false, "error": e.to_string() })),
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
        _ => return axum::Json(serde_json::json!({ "success": false, "error": "Invalid room" })),
    };

    if request.disable {
        room_state_arc.disabled_until = Some(Local::now().timestamp() + 2 * 3600);
        room_state_arc.override_temperature = None; // Clear override
        room_state_arc.override_until = None; // Clear override
        if room_state_arc.relay_state {
            // if heater is on, turn it off
            let relay_hostname = match request.room.as_str() {
                "bedroom" => "esp8266-relay0.local",
                "kids_bedroom" => "esp8266-relay2.local",
                _ => unreachable!(),
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

async fn override_temperature_handler(
    State(state): State<WebState>,
    Json(request): Json<OverrideTemperatureRequest>,
) -> axum::Json<serde_json::Value> {
    let mut server_state = state.server_state.write().await;
    let room_state_arc = match request.room.as_str() {
        "bedroom" => &mut server_state.bedroom,
        "kids_bedroom" => &mut server_state.kids_bedroom,
        _ => return axum::Json(serde_json::json!({ "success": false, "error": "Invalid room" })),
    };

    if let Some(temp_val) = request.temperature {
        room_state_arc.override_temperature = Some(temp_val);
        room_state_arc.override_until = Some(Local::now().timestamp() + 2 * 3600);
        room_state_arc.disabled_until = None; // Clear disabled state
    } else {
        room_state_arc.override_temperature = None;
        room_state_arc.override_until = None;
    }

    axum::Json(serde_json::json!({ "success": true }))
}
