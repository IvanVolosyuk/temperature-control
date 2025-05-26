pub mod pwm;
pub mod schedule;
pub mod web;

use crate::schedule::INTERPOLATE_INTERVALS;
use anyhow::{Context, Result};
use axum::extract::ws::Message as WsMessage;
use chrono::{DateTime, Local, Timelike};
use std::collections::HashMap;
use std::fs::{rename, File};
use std::io::{stdout, Write};
use std::net::{SocketAddr, UdpSocket};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
// Make WsTx available to web.rs by defining it here and making it public
pub type WsTx = mpsc::Sender<WsMessage>;
use crate::web::{create_web_server, ServerState, TemperaturePoint};

// These are from the temperature_protocol crate
use temperature_protocol::fragment_combiner::{FragmentCombiner, MessageHandler};
use temperature_protocol::protos::generated::dev::{
    DeviceInfo, DeviceMessage, RelayReport, SensorError, SensorReport,
};
use temperature_protocol::relay::set_relay;

use crate::pwm::{Control, PWMControl, SimpleControl};

// --- Constants mimicking C++ globals ---
const RELAYS: [&str; 3] = [
    "esp8266-relay0.local", // ID 0: bedroom
    "esp8266-relay1.local", // ID 1: irina
    "esp8266-relay2.local", // ID 2: kids room
];

const CORRECTION: [f64; 3] = [
    -0.0, // ID 0
    -0.9, // ID 1
    -0.6, // ID 2
];

// For diagnostic staleness checks
const BEDROOM_SENSOR_EXPECTED_IP: &str = "192.168.0.200";
const BEDROOM_RELAY_EXPECTED_IP: &str = "192.168.0.210"; // This is esp8266-relay0.local
const KIDS_SENSOR_EXPECTED_IP: &str = "192.168.0.202";
const KIDS_RELAY_EXPECTED_IP: &str = "192.168.0.212"; // This is esp8266-relay2.local

// Path for Netdata files
const NETDATA_PATH_PREFIX: &str = "/var/lib/temperature";

fn linear_rust(val_start: f64, val_end: f64, x_start: f64, x_end: f64, x_target: f64) -> f64 {
    if x_end == x_start {
        // If the interval is zero-length, return the starting value.
        // (or val_end, C++ used val_start, could also be an average or specific logic)
        return val_start;
    }

    let mut progress = (x_target - x_start) / (x_end - x_start);

    // Clamp progress to [0.0, 1.0]
    if progress < 0.0 {
        progress = 0.0;
    } else if progress > 1.0 {
        progress = 1.0;
    }

    val_end * progress + val_start * (1.0 - progress)
}

// --- Generic Interpolation Function ---
// Equivalent to C++ interpolate_fn, taking DateTime<Local> as requested
fn interpolate_fn_rust(intervals: &[(f64, f64)], t: DateTime<Local>) -> f64 {
    if intervals.is_empty() {
        // Or return a Result, or a default temperature. Panicking for now as intervals are const.
        panic!("Intervals slice cannot be empty.");
    }

    let hour_target = t.hour() as f64 + (t.minute() as f64 / 60.0) + (t.second() as f64 / 3600.0);

    // If target hour is before or at the first point's hour, return the first point's temperature
    if hour_target <= intervals[0].0 {
        return intervals[0].1;
    }

    // Iterate through intervals to find the segment for interpolation
    for i in 1..intervals.len() {
        let prev_point = intervals[i - 1];
        let curr_point = intervals[i];

        if hour_target < curr_point.0 {
            // Target hour is between prev_point.0 and curr_point.0
            return linear_rust(
                prev_point.1,
                curr_point.1,
                prev_point.0,
                curr_point.0,
                hour_target,
            );
        }
    }

    // If target hour is after or at the last point's hour, return the last point's temperature
    intervals.last().unwrap().1 // .unwrap() is safe due to prior .is_empty() check
}

// --- Server Structures ---
#[derive(Debug, Clone, Copy)]
struct RelayConfirmationState {
    unconfirmed: bool,
    confirmed_on_state: bool, // Last known actual state from relay report
}

impl Default for RelayConfirmationState {
    fn default() -> Self {
        RelayConfirmationState {
            unconfirmed: false,        // Initially, assume confirmed (or no operation pending)
            confirmed_on_state: false, // Default to OFF
        }
    }
}

struct Server {
    // Key: Source IP string (e.g., "192.168.0.100")
    last_message_timestamp: HashMap<String, i64>,
    // Key: Device ID (u32)
    last_temp_deci: HashMap<u32, f64>, // Storing as corrected temp
    // Key: Relay's source IP string (e.g. "192.168.0.210")
    last_relay_on_status: HashMap<String, bool>,
    // Key: Relay hostname (e.g. "esp8266-relay0.local")
    relay_confirmations: HashMap<String, RelayConfirmationState>,

    controls: Vec<Box<dyn Control>>,
    web_state: Arc<RwLock<ServerState>>,
    ws_connections: Arc<RwLock<Vec<WsTx>>>, // WsTx is now defined above
}

#[derive(PartialEq, Debug)]
enum PrintHeaderStatus {
    Failure,
    Ok,
    HasStatusUpdate,
}

impl Server {
    fn new() -> Server {
        let controls: Vec<Box<dyn Control>> = vec![
            Box::new(PWMControl::new(-0.36)),
            Box::new(SimpleControl::new()),
            Box::new(PWMControl::new(-0.36)),
        ];

        Server {
            last_message_timestamp: HashMap::new(),
            last_temp_deci: HashMap::new(),
            last_relay_on_status: HashMap::new(),
            relay_confirmations: HashMap::new(),
            controls,
            web_state: Arc::new(RwLock::new(ServerState::default())),
            ws_connections: Arc::new(RwLock::new(Vec::new())),
        }
    }

    fn print_header(&self, client_address_str: &str, info: &DeviceInfo) -> PrintHeaderStatus {
        let device_id = match info.id {
            Some(id) => id,
            None => {
                print!("Message without id from {}\n", client_address_str);
                let _ = stdout().flush();
                return PrintHeaderStatus::Failure;
            }
        };

        let current_time = Local::now();
        // C++ ctime format: "Wed Jun 30 21:49:08 2021"
        // Rust: "%a %b %e %H:%M:%S %Y"
        // Note: %e pads with space for single digit day, %d pads with 0. ctime uses space.
        let formatted_time = current_time.format("%a %b %_d %H:%M:%S %Y").to_string(); // %_d for space padding

        print!("{} [{}]: ", formatted_time, device_id);

        let mut status = PrintHeaderStatus::Ok;
        if info.started() {
            print!("(STARTED) ");
            status = PrintHeaderStatus::HasStatusUpdate;
        }
        if let Some(offline_sec) = info.offline_sec {
            print!("(OFFLINE {:.2}m) ", offline_sec as f64 / 60.0);
            status = PrintHeaderStatus::HasStatusUpdate;
        }
        status
    }

    async fn update_history(
        &self,
        device_id: u32,
        current_timestamp: i64,
        temp: f64,
        target_temp: f64,
        header_on: bool,
        is_disabled: bool,
    ) -> Result<()> {
        //Update temperature history in web state
        let mut web_state_lock = self.web_state.write().await;
        let room_state = match web_state_lock.rooms.iter_mut().find(|r| r.id == device_id) {
            Some(room) => room,
            None => return Ok(()), // Or handle error if a room with this ID is expected
        };

        // Add new temperature point
        room_state.temperature_history.push(TemperaturePoint {
            timestamp: current_timestamp,
            temperature: temp,
            target: target_temp,
            heater_on: header_on,
            is_disabled,
        });

        // Keep only last 48 hours of data
        let cutout = current_timestamp - 3600 * 48;
        room_state
            .temperature_history
            .retain(|point| point.timestamp >= cutout);
        return Ok(());
    }

    async fn broadcast_updates(&self, updated_room_id_for_history: Option<u32>) {
        let state_guard = self.web_state.read().await;
        let mut state_for_broadcast = (*state_guard).clone(); // Clone the state to modify it

        // Prune temperature history based on updated_room_id_for_history
        if let Some(updated_id) = updated_room_id_for_history {
            for room_state in state_for_broadcast.rooms.iter_mut() {
                if room_state.id == updated_id {
                    if let Some(last_point) = room_state.temperature_history.last().cloned() {
                        room_state.temperature_history = vec![last_point];
                    } else {
                        room_state.temperature_history = Vec::new();
                    }
                } else {
                    room_state.temperature_history = Vec::new();
                }
            }
        } else { // No specific room updated for history, clear all
            for room_state in state_for_broadcast.rooms.iter_mut() {
                room_state.temperature_history = Vec::new();
            }
        }
        // Ensure other room fields (current_temp, target_temp, etc.) are still from the original state_guard (they are, due to clone)

        drop(state_guard); // Release read lock on web_state

        let current_server_state_json = match serde_json::to_string(&state_for_broadcast) {
            Ok(json) => json,
            Err(e) => {
                eprintln!("Failed to serialize server state for WebSocket: {}", e);
                return;
            }
        };

        let ws_msg = WsMessage::Text(current_server_state_json.into()); // .into() was already here
        let mut conns = self.ws_connections.write().await;

        if conns.is_empty() {
            return;
        }

        let mut dead_indices = Vec::new();
        for (i, tx) in conns.iter().enumerate() {
            if tx.try_send(ws_msg.clone()).is_err() {
                // Use try_send for non-blocking behavior
                dead_indices.push(i);
            }
        }

        for &i in dead_indices.iter().rev() {
            conns.remove(i);
        }
    }

    async fn update_and_broadcast_web_state(&self, updated_device_id_for_history: Option<u32>) {
        let mut state_write_guard = self.web_state.write().await; // Acquire write lock to update state
        let now = Local::now(); // Define now here for reuse

        for room in state_write_guard.rooms.iter_mut() {
            let (sensor_ip_str, relay_ip_str) = match room.id {
                0 => (Some(BEDROOM_SENSOR_EXPECTED_IP), Some(BEDROOM_RELAY_EXPECTED_IP)),
                2 => (Some(KIDS_SENSOR_EXPECTED_IP), Some(KIDS_RELAY_EXPECTED_IP)),
                _ => (None, None), // No specific IP checks for other rooms
            };

            // Sensor availability and current temperature
            if let Some(ip_s) = sensor_ip_str {
                room.sensor_available = self
                    .last_message_timestamp
                    .get(ip_s)
                    .map_or(false, |&ts| now.timestamp() - ts < 180);
            } else {
                // For rooms without a specific sensor IP, check if we have any recent temperature data for this room.id
                // and if any sensor reported recently (general sensor activity).
                room.sensor_available = self.last_temp_deci.contains_key(&room.id) &&
                                      self.last_message_timestamp.values().any(|&ts| now.timestamp() - ts < 180);
            }
            room.current_temp = self.last_temp_deci.get(&room.id).copied().unwrap_or(0.0);

            // Target temperature from schedule and override
            if (room.id as usize) < INTERPOLATE_INTERVALS.len() {
                let mut target_temp_val = interpolate_fn_rust(INTERPOLATE_INTERVALS[room.id as usize], now);
                if let (Some(override_until_ts), Some(override_temp_val)) = (
                    room.override_until,
                    room.override_temperature,
                ) {
                    if override_until_ts > now.timestamp() {
                        target_temp_val = override_temp_val;
                    }
                }
                room.target_temp = target_temp_val;
            } else {
                // Default target temp if no schedule for this room ID
                room.target_temp = room.current_temp; // Or a sensible default like 20.0
            }

            // Relay availability and state
            if let Some(ip_r) = relay_ip_str {
                room.relay_available = self
                    .last_message_timestamp
                    .get(ip_r)
                    .map_or(false, |&ts| now.timestamp() - ts < 180);
                room.relay_state = self
                    .last_relay_on_status
                    .get(ip_r) // last_relay_on_status is keyed by relay's source IP
                    .copied()
                    .unwrap_or(false);
            } else {
                // For rooms without a specific relay IP, try to infer from RELAYS and relay_confirmations
                if let Some(hostname_str) = RELAYS.get(room.id as usize) {
                    // Check if any relay reported recently (general check)
                    // Renamed ts to ts_val to emphasize it's a value and removed dereferencing.
                    let any_relay_active = self.last_message_timestamp.values().any(|&ts_val| { 
                        self.last_relay_on_status.keys().any(|k| self.last_message_timestamp.get(k).map_or(false, |&lts| lts == ts_val)) && now.timestamp() - ts_val < 180
                    });
                    
                    if let Some(confirmation_state) = self.relay_confirmations.get(*hostname_str) {
                        room.relay_available = any_relay_active; // Simplified: if its hostname is in confirmations & any relay active
                        room.relay_state = confirmation_state.confirmed_on_state;
                    } else {
                        room.relay_available = false;
                        room.relay_state = false;
                    }
                } else {
                    room.relay_available = false;
                    room.relay_state = false;
                }
            }
        }

        drop(state_write_guard); // Release write lock before broadcasting

        self.broadcast_updates(updated_device_id_for_history).await;
    }

    async fn new_relay_report(&mut self, src: SocketAddr, report: &RelayReport) -> Result<()> {
        let client_ip_str = src.ip().to_string();
        self.last_message_timestamp
            .insert(client_ip_str.clone(), Local::now().timestamp());

        let device_id = report.info.as_ref().and_then(|i| i.id);

        let header_status = self.print_header(
            &client_ip_str,
            report.info.as_ref().unwrap_or(&DeviceInfo::default()),
        );
        if header_status == PrintHeaderStatus::Failure {
            return Ok(());
        }

        let relay_is_on = report.relay_status();
        self.last_relay_on_status
            .insert(client_ip_str.clone(), relay_is_on);

        // Update confirmation state
        if let Some(id_val) = device_id {
            if let Some(relay_hostname) = RELAYS.get(id_val as usize) {
                let confirmation_entry = self
                    .relay_confirmations
                    .entry(relay_hostname.to_string())
                    .or_default();
                confirmation_entry.unconfirmed = false;
                confirmation_entry.confirmed_on_state = relay_is_on;
            }
        }

        print!(
            "Relay: {}{}",
            if relay_is_on { "ON" } else { "OFF" },
            if header_status == PrintHeaderStatus::HasStatusUpdate {
                "\n"
            } else {
                "\r"
            }
        );
        stdout().flush()?;
        self.update_and_broadcast_web_state(None).await; // Relay reports don't generate new temp points directly
        Ok(())
    }

    async fn is_heater_disabled(&self, device_id: u32, current_timestamp: i64) -> bool {
        // Check if heater is disabled
        let web_state_lock = self.web_state.read().await;
        let room_state = match web_state_lock.rooms.iter().find(|r| r.id == device_id) {
            Some(room) => room,
            None => return false, // If room doesn't exist, it's not disabled
        };

        return room_state
            .disabled_until
            .map(|until| current_timestamp < until)
            .unwrap_or(false);
    }

    async fn new_sensor_report(&mut self, src: SocketAddr, report: &SensorReport) -> Result<()> {
        let client_ip_str = src.ip().to_string();

        let header_status = self.print_header(
            &client_ip_str,
            report.info.as_ref().unwrap_or(&DeviceInfo::default()),
        );
        if header_status == PrintHeaderStatus::Failure {
            // Still update last_message_timestamp even if header fails but message has ID
            if report.info.as_ref().and_then(|i| i.id).is_some() {
                self.last_message_timestamp
                    .insert(client_ip_str.clone(), Local::now().timestamp());
            }
            return Ok(());
        }

        let device_id = report.info.as_ref().and_then(|i| i.id).unwrap_or(u32::MAX); // Use a sentinel if no ID

        if report.has_sensor_error() {
            let error_name = match SensorError::try_from(report.sensor_error())
                .unwrap_or(SensorError::S_CHECKSUM)
            {
                SensorError::S_TIMEOUT_LOW_PULSE => "S_TIMEOUT_LOW_PULSE",
                SensorError::S_TIMEOUT_HIGH_PULSE => "S_TIMEOUT_HIGH_PULSE",
                SensorError::S_TIME_PULSE => "S_TIME_PULSE",
                SensorError::S_CHECKSUM => "S_CHECKSUM",
                SensorError::S_BUTTON_EVENT => "S_BUTTON_EVENT",
            };
            print!("({}) ", error_name);
        } else if report.has_temperature_deci() {
            let temp = report.temperature_deci() as f64 * 0.1;
            let humidity = report.humidity_deci() as f64 * 0.1;
            print!("t={:.1} h={:.1} ", temp, humidity);
        }

        if !report.has_temperature_deci() {
            println!(); // End line if no temperature data
            return Ok(());
        }

        self.last_message_timestamp
            .insert(client_ip_str.clone(), Local::now().timestamp());

        let mut temp = report.temperature_deci() as f64 * 0.1;
        let humidity = report.humidity_deci() as f64 * 0.1; // For Netdata

        let current_time = Local::now();
        let current_timestamp = current_time.timestamp();

        let mut target_temp = temp;

        // Check for temperature override
        // Read override status early and release the lock
        let mut room_override_temp = None;

        // Check for temperature override using the new structure
        let web_state_lock = self.web_state.read().await;
        if let Some(room_for_override) = web_state_lock.rooms.iter().find(|r| r.id == device_id) {
            if let (Some(override_until_ts), Some(override_val)) = (
                room_for_override.override_until,
                room_for_override.override_temperature,
            ) {
                if override_until_ts > current_timestamp {
                    room_override_temp = Some(override_val);
                }
            }
        }
        // Drop the read lock as soon as possible
        drop(web_state_lock);


        if (device_id as usize) < INTERPOLATE_INTERVALS.len() {
            // Check if ID is within manageable range

            target_temp = interpolate_fn_rust(
                INTERPOLATE_INTERVALS[device_id as usize],
                current_time + chrono::Duration::minutes(10),
            );
            temp += CORRECTION[device_id as usize];
            print!("{:.1} (target {:.1}) ", temp, target_temp);
            self.last_temp_deci.insert(device_id, temp);

            if let Some(override_temp_val) = room_override_temp {
                target_temp = override_temp_val;
                print!("[SET {:.1}C] ", target_temp);
            } else {
            }

            let future_target_temp = if room_override_temp.is_some() {
                target_temp
            } else {
                interpolate_fn_rust(
                    INTERPOLATE_INTERVALS[device_id as usize],
                    current_time + chrono::Duration::minutes(15),
                )
            };

            let is_disabled = self.is_heater_disabled(device_id, current_timestamp).await;

            if let Some(control_strategy) = self.controls.get_mut(device_id as usize) {
                let (mode_on, delay_ms) =
                    control_strategy.get_mode(temp, target_temp, future_target_temp, current_time);
                // Call set_output on the control strategy object itself (for its internal state)
                control_strategy.set_output(mode_on, delay_ms, current_time);

                // Now, command the actual relay and log according to C++ logic
                let relay_hostname = RELAYS[device_id as usize];

                // C++ Relay::set_relay logging part 1: Print ON/OFF if delay is 0
                if delay_ms == 0 {
                    print!("{}", if mode_on { "ON" } else { "OFF" });
                }

                if is_disabled {
                    print!(" [DISABLED]");
                }

                // Send the command
                match set_relay(relay_hostname, mode_on & !is_disabled, delay_ms) {
                    Ok(_) => {
                        let confirmation_state = self
                            .relay_confirmations
                            .entry(relay_hostname.to_string())
                            .or_default();

                        // C++ Relay::set_relay logging part 2: Print status based on confirmation
                        if confirmation_state.unconfirmed {
                            print!(" [UNCONFIRMED]");
                        } else if delay_ms != 0 {
                            // Print current *confirmed* state before new command with delay
                            print!(
                                " {}",
                                if confirmation_state.confirmed_on_state {
                                    "*ON"
                                } else {
                                    "*OFF"
                                }
                            );
                        }

                        if delay_ms != 0 {
                            print!(
                                " ({:.1}m->{})",
                                delay_ms as f64 / 60_000.0,
                                if mode_on { "ON" } else { "OFF" }
                            );
                        }

                        // Mark as unconfirmed after sending command
                        confirmation_state.unconfirmed = true;
                    }
                    Err(_e) => {
                        print!(" [NRELAY]");
                    }
                }

                // If delay is not zero, than mode_on is still opposite for now
                let heater_on = mode_on ^ (delay_ms != 0);

                // Update web state after processing the report
                self.update_history(
                    device_id,
                    current_timestamp,
                    temp,
                    target_temp,
                    heater_on,
                    is_disabled,
                )
                .await?;
            } else {
                print!("[NO_CONTROL_FOR_ID:{}] ", device_id);
            }
        } else {
            // Device ID out of range for configured controls/relays
            print!("{:.1} (unmanaged) ", temp);
            self.last_temp_deci.insert(device_id, temp); // Still store its temp if needed elsewhere
        }

        // Reporting for Netdata collector
        let tmp_file_path_str = format!("{}/new{}", NETDATA_PATH_PREFIX, device_id);
        let current_file_path_str = format!("{}/current{}", NETDATA_PATH_PREFIX, device_id);
        let humidity_file_path_str = format!("{}/humidity{}", NETDATA_PATH_PREFIX, device_id);

        // Write temperature and target
        match File::create(&tmp_file_path_str) {
            Ok(mut file) => {
                if let Err(e) = writeln!(file, "SET temperature = {:.0}", temp * 10.0) {
                    eprintln!("Error writing temperature to {}: {}", tmp_file_path_str, e);
                }
                if let Err(e) = writeln!(file, "SET target = {:.0}", target_temp * 10.0) {
                    eprintln!("Error writing target to {}: {}", tmp_file_path_str, e);
                }
                // C++ dprintf, then close, then rename. Rust write, then rename.
                drop(file); // Ensure file is closed before rename
                if let Err(e) = rename(&tmp_file_path_str, &current_file_path_str) {
                    eprintln!(
                        "Error renaming {} to {}: {}",
                        tmp_file_path_str, current_file_path_str, e
                    );
                }
            }
            Err(e) => eprintln!("Error creating {}: {}", tmp_file_path_str, e),
        }

        // Write humidity
        match File::create(&tmp_file_path_str) {
            // Reuse tmp_file_path_str for humidity
            Ok(mut file) => {
                if let Err(e) = writeln!(file, "SET humidity = {:.0}", humidity * 10.0) {
                    eprintln!("Error writing humidity to {}: {}", tmp_file_path_str, e);
                }
                drop(file);
                if let Err(e) = rename(&tmp_file_path_str, &humidity_file_path_str) {
                    eprintln!(
                        "Error renaming {} to {}: {}",
                        tmp_file_path_str, humidity_file_path_str, e
                    );
                }
            }
            Err(e) => eprintln!("Error creating {}: {}", tmp_file_path_str, e),
        }

        println!(); // End the line for sensor report
                    // self.update_history is called before this, so web_state has latest point
        stdout().flush()?;
        self.update_and_broadcast_web_state(Some(device_id)).await; // Pass the device_id of the sensor
        Ok(())
    }

    fn format_diag(&self, src: SocketAddr) -> Result<()> {
        let current_time = Local::now();
        println!(
            "{} Diag request from {}",
            current_time.format("%Y-%m-%d %H:%M:%S"),
            src
        );

        let temp0_str = self
            .last_temp_deci
            .get(&0)
            .map_or_else(|| "N/A".to_string(), |t| format!("{:.1}", t));
        let relay0_on_str = self
            .last_relay_on_status
            .get(BEDROOM_RELAY_EXPECTED_IP)
            .map_or_else(|| "", |&on| if on { " [ON]" } else { "" });

        let temp2_str = self
            .last_temp_deci
            .get(&2)
            .map_or_else(|| "N/A".to_string(), |t| format!("{:.1}", t));
        let relay2_on_str = self
            .last_relay_on_status
            .get(KIDS_RELAY_EXPECTED_IP)
            .map_or_else(|| "", |&on| if on { " [ON]" } else { "" });

        let mut diag_message = format!(
            "Temp0: {}{}, Temp2: {}{}",
            temp0_str, relay0_on_str, temp2_str, relay2_on_str
        );

        let now_ts = current_time.timestamp();
        let staleness_threshold = 180; // 3 minutes

        if now_ts
            - self
                .last_message_timestamp
                .get(BEDROOM_SENSOR_EXPECTED_IP)
                .cloned()
                .unwrap_or(0)
            > staleness_threshold
        {
            diag_message += "\nFAIL: Bedroom sensor";
        } else if now_ts
            - self
                .last_message_timestamp
                .get(BEDROOM_RELAY_EXPECTED_IP)
                .cloned()
                .unwrap_or(0)
            > staleness_threshold
        {
            diag_message += "\nFAIL: Bedroom relay";
        }

        if now_ts
            - self
                .last_message_timestamp
                .get(KIDS_SENSOR_EXPECTED_IP)
                .cloned()
                .unwrap_or(0)
            > staleness_threshold
        {
            diag_message += "\nFAIL: Kids sensor";
        } else if now_ts
            - self
                .last_message_timestamp
                .get(KIDS_RELAY_EXPECTED_IP)
                .cloned()
                .unwrap_or(0)
            > staleness_threshold
        {
            diag_message += "\nFAIL: Kids relay";
        }

        // Send the diagnostic message back to src
        // The C++ Relay::send_message is more complex (hostname resolution).
        // Here, src is already a SocketAddr.
        let udp_socket = UdpSocket::bind("0.0.0.0:0") // Bind to any available local port
            .context("Failed to bind UDP socket for diagnostics")?;

        match udp_socket.send_to(diag_message.as_bytes(), src) {
            Ok(_) => { /* Successfully sent */ }
            Err(e) => {
                print!(" [NDIAG_SEND_ERR: {}] ", e); // C++ prints "[NDIAG]"
                stdout().flush()?;
            }
        }
        Ok(())
    }
}

impl MessageHandler<DeviceMessage> for Server {
    async fn on_message(
        &mut self,
        src: std::net::SocketAddr,
        msg: DeviceMessage,
    ) -> anyhow::Result<()> {
        let mut known_message_component_found = false;

        if let Some(sensor_report) = msg.sensor.as_ref() {
            self.new_sensor_report(src, sensor_report).await?;
            known_message_component_found = true;
        } else if let Some(relay_report) = msg.relay.as_ref() {
            self.new_relay_report(src, relay_report).await?;
            known_message_component_found = true;
        } else if msg.format_diag() {
            self.format_diag(src)?;
            known_message_component_found = true;
        }

        if !known_message_component_found {
            println!(
                "{} Unknown message type from {} (or empty message components). Message: {:?}",
                Local::now().format("%Y-%m-%d %H:%M:%S"),
                src,
                msg
            );
        }
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize the server state
    let mut server = Server::new();
    let web_state = server.web_state.clone();
    let ws_connections = server.ws_connections.clone();

    // Start the web server in a separate task
    tokio::spawn(async move {
        create_web_server(web_state, ws_connections).await;
    });

    // Start the main loop using FragmentCombiner
    println!("Starting temperature server on 0.0.0.0:4000...");
    FragmentCombiner::new(&mut server)
        .main_loop("0.0.0.0:4000")
        .await
}
