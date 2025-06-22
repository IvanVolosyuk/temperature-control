pub mod pwm;
pub mod schedule;
pub mod web;

use crate::schedule::INTERPOLATE_INTERVALS;
use anyhow::Result;
use axum::extract::ws::Message as WsMessage;
use chrono::{DateTime, Local, Timelike};
use std::fs::{rename, File};
use std::io::{stdout, Write};
use std::net::SocketAddr;
use std::sync::Arc;
use std::collections::HashMap;
use tokio::{
    fs::{File as TokioFile, OpenOptions}, io::AsyncWriteExt, sync::{mpsc, RwLock}
};
// Make WsTx available to web.rs by defining it here and making it public
pub type WsTx = mpsc::Sender<WsMessage>;
use crate::web::create_web_server;
use history::{
    read_and_parse_binary_state_v2, serialize_history_point_v2, ServerState, TemperaturePoint,
};

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

#[derive(Debug, Clone, Default)]
struct HardwareState {
    last_sensor_message_timestamp: i64,
    last_relay_message_timestamp: i64,
    last_reported_temperature: f64,
    last_relay_on_status: bool,
    relay_unconfirmed: bool,
}

struct Server {
    hardware_states: HashMap<u32, HardwareState>,
    controls: Vec<Box<dyn Control>>,
    web_state: Arc<RwLock<ServerState>>,
    ws_connections: Arc<RwLock<Vec<WsTx>>>, // WsTx is now defined above
    binary_state_file: Option<TokioFile>,
    prev_timestamps: [u32; 4],
}

#[derive(PartialEq, Debug)]
enum PrintHeaderStatus {
    Failure,
    Ok,
    HasStatusUpdate,
}

impl Server {
    async fn new() -> Server {
        let controls: Vec<Box<dyn Control>> = vec![
            Box::new(PWMControl::new(-0.36)),
            Box::new(SimpleControl::new()),
            Box::new(PWMControl::new(-0.36)),
            Box::new(SimpleControl::new()),
        ];

        let server_state_bin_path = "server_state_v2.bin";
        let (final_web_state, final_prev_timestamps) =
            match read_and_parse_binary_state_v2(server_state_bin_path).await {
                Ok(Some((loaded_state, timestamps))) => {
                    println!(
                        "Successfully loaded V2 state from binary file: {}",
                        server_state_bin_path
                    );
                    (loaded_state, timestamps)
                }
                Ok(None) => {
                    println!(
                        "Binary state file {} not found or empty. Initializing with default state.",
                        server_state_bin_path
                    );
                    (ServerState::default(), [0; 4])
                }
                Err(e) => {
                    eprintln!(
                        "Error reading binary state file {}: {}. Initializing with default state.",
                        server_state_bin_path, e
                    );
                    (ServerState::default(), [0; 4])
                }
            };

        // Open the binary state file for normal append operations
        let binary_file_handle = match OpenOptions::new()
            .append(true)
            .create(true)
            .open(server_state_bin_path)
            .await
        {
            Ok(file) => Some(file),
            Err(e) => {
                eprintln!(
                    "Failed to open or create {} for appending: {}. Binary logging will be disabled.",
                    server_state_bin_path, e
                );
                None
            }
        };

        Server {
            hardware_states: HashMap::new(),
            controls,
            web_state: Arc::new(RwLock::new(final_web_state)),
            ws_connections: Arc::new(RwLock::new(Vec::new())),
            binary_state_file: binary_file_handle,
            prev_timestamps: final_prev_timestamps,
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
        &mut self, // Changed to &mut self
        device_id: u32,
        current_timestamp: i64,
        temp: f64,
        target_temp: f64,
        heater_on: bool,
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
            heater_on,
            is_disabled,
        });
        drop(web_state_lock);

        let new_point_for_binary = TemperaturePoint {
            timestamp: current_timestamp,
            temperature: temp,
            target: target_temp,
            heater_on,
            is_disabled,
        };

        if let Err(e) = self.append_to_binary_state(device_id, new_point_for_binary).await {
            eprintln!("Error appending to binary state for device {}: {}", device_id, e);
        }

        return Ok(());
    }

    async fn append_to_binary_state(&mut self, device_id: u32, point: TemperaturePoint) -> Result<()> {
        if device_id >= 4 {
            return Ok(()); // Only log for devices 0-3
        }

        let prev_timestamp = self.prev_timestamps[device_id as usize];

        let serialized_data =
            serialize_history_point_v2(device_id, prev_timestamp, point.clone());

        if let Some(file) = self.binary_state_file.as_mut() {
            for val in serialized_data {
                if let Err(e) = file.write_u32_le(val).await {
                    eprintln!(
                        "Failed to write to server_state.bin for device {}: {}. Further binary logging might be affected.",
                        device_id, e
                    );
                    return Err(e.into());
                }
            }
            self.prev_timestamps[device_id as usize] = point.timestamp as u32;
        }
        Ok(())
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
        } else {
            // No specific room updated for history, clear all
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
            // Sensor availability and current temperature
            if let Some(hw_state) = self.hardware_states.get(&room.id) {
                room.sensor_available =
                    now.timestamp() - hw_state.last_sensor_message_timestamp < 180;
                room.current_temp = hw_state.last_reported_temperature;
                room.relay_available =
                    now.timestamp() - hw_state.last_relay_message_timestamp < 180;
                room.relay_state = hw_state.last_relay_on_status;
            } else {
                // Default values if no hardware state exists for this room.id
                room.sensor_available = false;
                room.current_temp = 0.0;
                room.relay_available = false;
                room.relay_state = false;
            }

            // Target temperature from schedule and override
            let mut target_temp_val =
                interpolate_fn_rust(INTERPOLATE_INTERVALS[room.id as usize], now);
            if let (Some(override_until_ts), Some(override_temp_val)) =
                (room.override_until, room.override_temperature)
            {
                if override_until_ts > now.timestamp() {
                    target_temp_val = override_temp_val;
                }
            }
            room.target_temp = target_temp_val;
        }

        drop(state_write_guard); // Release write lock before broadcasting

        self.broadcast_updates(updated_device_id_for_history).await;
    }

    async fn new_relay_report(&mut self, src: SocketAddr, report: &RelayReport) -> Result<()> {
        let device_id: Option<u32> = report.info.as_ref().and_then(|i| i.id);

        let header_status = self.print_header(
            &src.ip().to_string(),
            report.info.as_ref().unwrap_or(&DeviceInfo::default()),
        );

        let relay_is_on = report.relay_status();
        if let Some(id) = device_id {
            if id as usize <= RELAYS.len() {
                // Check if ID is within bounds of RELAYS array
                let state = self.hardware_states.entry(id).or_default();
                state.last_relay_message_timestamp = Local::now().timestamp();

                if header_status == PrintHeaderStatus::Failure {
                    return Ok(());
                }

                state.last_relay_on_status = relay_is_on;
                state.relay_unconfirmed = false;
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
        let header_status = self.print_header(
            &src.ip().to_string(),
            report.info.as_ref().unwrap_or(&DeviceInfo::default()),
        );

        let device_id = report.info.as_ref().and_then(|i| i.id).unwrap_or(u32::MAX); // Use a sentinel if no ID

        if header_status == PrintHeaderStatus::Failure {
            // Still update last_message_timestamp even if header fails but message has ID
            if report.info.as_ref().and_then(|i| i.id).is_some() {
                let state = self.hardware_states.entry(device_id).or_default();
                state.last_sensor_message_timestamp = Local::now().timestamp();
            }
            return Ok(());
        }

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

        // Update sensor message timestamp for the device
        let state_for_sensor_ts = self.hardware_states.entry(device_id).or_default();
        state_for_sensor_ts.last_sensor_message_timestamp = Local::now().timestamp();

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

            let state_for_temp = self.hardware_states.entry(device_id).or_default();
            state_for_temp.last_reported_temperature = temp;

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
                        let state_for_relay_confirm =
                            self.hardware_states.entry(device_id).or_default();

                        // C++ Relay::set_relay logging part 2: Print status based on confirmation
                        if state_for_relay_confirm.relay_unconfirmed {
                            print!(" [UNCONFIRMED]");
                        } else if delay_ms != 0 {
                            // Print current *confirmed* state before new command with delay
                            print!(
                                " {}",
                                if state_for_relay_confirm.last_relay_on_status {
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
                        state_for_relay_confirm.relay_unconfirmed = true;
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
    let mut server = Server::new().await;
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
