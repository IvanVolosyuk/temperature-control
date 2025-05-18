pub mod pwm;
pub mod schedule;
pub mod web;

use anyhow::{Context, Result};
use chrono::{DateTime, Local, Timelike};
use std::collections::HashMap;
use std::fs::{File, rename};
use std::io::{Write, stdout};
use std::net::{SocketAddr, UdpSocket};
use std::sync::Arc;
use std::sync::RwLock;
use tokio::sync::RwLock as AsyncRwLock;
use crate::schedule::INTERPOLATE_INTERVALS;
use crate::web::{ServerState, create_web_server, TemperaturePoint};

// These are from the temperature_protocol crate
use temperature_protocol::fragment_combiner::{FragmentCombiner, MessageHandler};
use temperature_protocol::protos::generated::dev::{
    DeviceMessage, DeviceInfo, SensorReport, RelayReport, SensorError,
};
use temperature_protocol::relay::set_relay;

use crate::pwm::{Control, SimpleControl, PWMControl};

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
const KIDS_RELAY_EXPECTED_IP: &str = "192.168.0.212";   // This is esp8266-relay2.local

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

    let hour_target = t.hour() as f64
        + (t.minute() as f64 / 60.0)
        + (t.second() as f64 / 3600.0);

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
            unconfirmed: false, // Initially, assume confirmed (or no operation pending)
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

    controls: Arc<RwLock<Vec<Control>>>,
    web_state: Arc<AsyncRwLock<ServerState>>,
}

#[derive(PartialEq, Debug)]
enum PrintHeaderStatus {
    Failure,
    Ok,
    HasStatusUpdate,
}

impl Server {
    fn new() -> Server {
        let controls: Vec<Control> = vec![
            Control::PWM(PWMControl::new(-0.36)),
            Control::Simple(SimpleControl::new()),
            Control::PWM(PWMControl::new(-0.36)),
        ];

        Server {
            last_message_timestamp: HashMap::new(),
            last_temp_deci: HashMap::new(),
            last_relay_on_status: HashMap::new(),
            relay_confirmations: HashMap::new(),
            controls: Arc::new(RwLock::new(controls)),
            web_state: Arc::new(AsyncRwLock::new(ServerState::default())),
        }
    }

    fn print_header(&self, client_address_str: &str, info: &DeviceInfo) -> PrintHeaderStatus {
        let device_id = match info.id {
            Some(id) => id,
            None => {
                print!(
                    "Message without id from {}\n",
                    client_address_str
                );
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

    async fn update_history(&self, device_id : u32, current_timestamp: i64, temp: f64, target_temp: f64, header_on: bool) -> Result<()> {
        //Update temperature history in web state
        let mut web_state = self.web_state.write().await;
        let room_state = if device_id == 0 {
            &mut web_state.bedroom
        } else if device_id == 2 {
            &mut web_state.kids_bedroom
        } else {
            return Ok(());
        };

        // Add new temperature point
        room_state.temperature_history.push(TemperaturePoint {
            timestamp: current_timestamp,
            temperature: temp,
            target: target_temp,
            heater_on : header_on,
        });

        // Keep only last 6 hours of data (60 points)
        let one_hour_ago = current_timestamp - 3600 * 6;
        room_state.temperature_history.retain(|point| point.timestamp >= one_hour_ago);
        return Ok(());
    }

    async fn update_web_state(&self) {
        let mut state = self.web_state.write().await;

        // Update bedroom state
        state.bedroom.sensor_available = self.last_message_timestamp.get(BEDROOM_SENSOR_EXPECTED_IP)
            .map_or(false, |&ts| Local::now().timestamp() - ts < 180);
        state.bedroom.current_temp = self.last_temp_deci.get(&0).copied().unwrap_or(0.0);
        state.bedroom.target_temp = interpolate_fn_rust(INTERPOLATE_INTERVALS[0], Local::now());
        state.bedroom.relay_available = self.last_message_timestamp.get(BEDROOM_RELAY_EXPECTED_IP)
            .map_or(false, |&ts| Local::now().timestamp() - ts < 180);
        state.bedroom.relay_state = self.last_relay_on_status.get(BEDROOM_RELAY_EXPECTED_IP)
            .copied()
            .unwrap_or(false);

        // Update kids bedroom state
        state.kids_bedroom.sensor_available = self.last_message_timestamp.get(KIDS_SENSOR_EXPECTED_IP)
            .map_or(false, |&ts| Local::now().timestamp() - ts < 180);
        state.kids_bedroom.current_temp = self.last_temp_deci.get(&2).copied().unwrap_or(0.0);
        state.kids_bedroom.target_temp = interpolate_fn_rust(INTERPOLATE_INTERVALS[2], Local::now());
        state.kids_bedroom.relay_available = self.last_message_timestamp.get(KIDS_RELAY_EXPECTED_IP)
            .map_or(false, |&ts| Local::now().timestamp() - ts < 180);
        state.kids_bedroom.relay_state = self.last_relay_on_status.get(KIDS_RELAY_EXPECTED_IP)
            .copied()
            .unwrap_or(false);
    }

    fn new_relay_report(&mut self, src: SocketAddr, report: &RelayReport) -> Result<()> {
        let client_ip_str = src.ip().to_string();
        self.last_message_timestamp.insert(client_ip_str.clone(), Local::now().timestamp());

        let device_id = report.info.as_ref().and_then(|i| i.id);

        let header_status = self.print_header(&client_ip_str, report.info.as_ref().unwrap_or(&DeviceInfo::default()));
        if header_status == PrintHeaderStatus::Failure {
            return Ok(());
        }

        let relay_is_on = report.relay_status();
        self.last_relay_on_status.insert(client_ip_str.clone(), relay_is_on);

        // Update confirmation state
        if let Some(id_val) = device_id {
            if let Some(relay_hostname) = RELAYS.get(id_val as usize) {
                let confirmation_entry = self.relay_confirmations
                    .entry(relay_hostname.to_string())
                    .or_default();
                confirmation_entry.unconfirmed = false;
                confirmation_entry.confirmed_on_state = relay_is_on;
            }
        }

        print!("Relay: {}{}",
            if relay_is_on { "ON" } else { "OFF" },
            if header_status == PrintHeaderStatus::HasStatusUpdate { "\n" } else { "\r" }
        );
        stdout().flush()?;

        // Update web state after processing the report
        tokio::spawn({
            let server = self.clone();
            async move {
                server.update_web_state().await;
            }
        });

        Ok(())
    }

    fn new_sensor_report(&mut self, src: SocketAddr, report: &SensorReport) -> Result<()> {
        let client_ip_str = src.ip().to_string();

        let header_status = self.print_header(&client_ip_str, report.info.as_ref().unwrap_or(&DeviceInfo::default()));
        if header_status == PrintHeaderStatus::Failure {
            // Still update last_message_timestamp even if header fails but message has ID
            if report.info.as_ref().and_then(|i| i.id).is_some() {
                 self.last_message_timestamp.insert(client_ip_str.clone(), Local::now().timestamp());
            }
            return Ok(());
        }

        let device_id = report.info.as_ref().and_then(|i| i.id).unwrap_or(u32::MAX); // Use a sentinel if no ID

        if report.has_sensor_error() {
            let error_name = match SensorError::try_from(report.sensor_error()).unwrap_or(SensorError::S_CHECKSUM) {
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

        self.last_message_timestamp.insert(client_ip_str.clone(), Local::now().timestamp());

        let mut temp = report.temperature_deci() as f64 * 0.1;
        let humidity = report.humidity_deci() as f64 * 0.1; // For Netdata

        let current_time = Local::now();
        let current_timestamp = current_time.timestamp();

        let mut target_temp = temp; // Default target to current temp if not controlled
        let mut heater_on = false;

        if (device_id as usize) < INTERPOLATE_INTERVALS.len() { // Check if ID is within manageable range
            target_temp = interpolate_fn_rust(INTERPOLATE_INTERVALS[device_id as usize], current_time);

            temp += CORRECTION[device_id as usize];
            print!("{:.1} (target {:.1}) ", temp, target_temp);
            self.last_temp_deci.insert(device_id, temp);

            // FIXME: add 10 minutes to future time
            let future_target_temp = interpolate_fn_rust(INTERPOLATE_INTERVALS[device_id as usize], current_time + chrono::Duration::minutes(10));

            if let Some(control_strategy) = self.controls.write().unwrap().get_mut(device_id as usize) {
                let (mode_on, delay_ms) = control_strategy.get_mode(
                    temp,
                    target_temp,
                    future_target_temp,
                    current_time
                );
                // Call set_output on the control strategy object itself (for its internal state)
                control_strategy.set_output(mode_on, delay_ms, current_time);

                // If delay is not zero, than mode_on is still opposite for now
                heater_on = mode_on ^ (delay_ms != 0);

                // Now, command the actual relay and log according to C++ logic
                let relay_hostname = RELAYS[device_id as usize];

                // C++ Relay::set_relay logging part 1: Print ON/OFF if delay is 0
                if delay_ms == 0 {
                    print!("{}", if mode_on { "ON" } else { "OFF" });
                }

                // Send the command
                match set_relay(relay_hostname, mode_on, delay_ms) {
                    Ok(_) => {
                        let confirmation_state = self.relay_confirmations
                            .entry(relay_hostname.to_string())
                            .or_default();

                        // C++ Relay::set_relay logging part 2: Print status based on confirmation
                        if confirmation_state.unconfirmed {
                            print!(" [UNCONFIRMED]");
                        } else if delay_ms != 0 {
                            // Print current *confirmed* state before new command with delay
                            print!(" {}", if confirmation_state.confirmed_on_state { "*ON" } else { "*OFF" });
                        }

                        if delay_ms != 0 {
                            print!(" ({:.1}m->{})",
                                delay_ms as f64 / 60_000.0,
                                if mode_on { "ON" } else { "OFF" });
                        }

                        // Mark as unconfirmed after sending command
                        confirmation_state.unconfirmed = true;
                    }
                    Err(_e) => {
                        print!(" [NRELAY]");
                    }
                }
            } else {
                print!("[NO_CONTROL_FOR_ID:{}] ", device_id);
            }
        } else {
            // Device ID out of range for configured controls/relays
            print!("{:.1} (unmanaged) ", temp);
            self.last_temp_deci.insert(device_id, temp); // Still store its temp if needed elsewhere
        }

        // Update web state after processing the report
        tokio::spawn({
            let server = self.clone();
            async move {
                let _ = server.update_history(device_id, current_timestamp, temp, target_temp, heater_on).await;
            }
        });

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
                    eprintln!("Error renaming {} to {}: {}", tmp_file_path_str, current_file_path_str, e);
                }
            }
            Err(e) => eprintln!("Error creating {}: {}", tmp_file_path_str, e),
        }

        // Write humidity
        match File::create(&tmp_file_path_str) { // Reuse tmp_file_path_str for humidity
            Ok(mut file) => {
                 if let Err(e) = writeln!(file, "SET humidity = {:.0}", humidity * 10.0) {
                     eprintln!("Error writing humidity to {}: {}", tmp_file_path_str, e);
                 }
                 drop(file);
                 if let Err(e) = rename(&tmp_file_path_str, &humidity_file_path_str) {
                    eprintln!("Error renaming {} to {}: {}", tmp_file_path_str, humidity_file_path_str, e);
                 }
            }
            Err(e) => eprintln!("Error creating {}: {}", tmp_file_path_str, e),
        }

        println!(); // End the line for sensor report
        stdout().flush()?;

        // Update web state after processing the report
        tokio::spawn({
            let server = self.clone();
            async move {
                server.update_web_state().await;
            }
        });

        Ok(())
    }

    fn format_diag(&self, src: SocketAddr) -> Result<()> {
        let current_time = Local::now();
        println!(
            "{} Diag request from {}",
            current_time.format("%Y-%m-%d %H:%M:%S"),
            src
        );

        let temp0_str = self.last_temp_deci.get(&0).map_or_else(|| "N/A".to_string(), |t| format!("{:.1}", t));
        let relay0_on_str = self.last_relay_on_status.get(BEDROOM_RELAY_EXPECTED_IP).map_or_else(|| "", |&on| if on { " [ON]" } else { "" });

        let temp2_str = self.last_temp_deci.get(&2).map_or_else(|| "N/A".to_string(), |t| format!("{:.1}", t));
        let relay2_on_str = self.last_relay_on_status.get(KIDS_RELAY_EXPECTED_IP).map_or_else(|| "", |&on| if on { " [ON]" } else { "" });

        let mut diag_message = format!("Temp0: {}{}, Temp2: {}{}", temp0_str, relay0_on_str, temp2_str, relay2_on_str);

        let now_ts = current_time.timestamp();
        let staleness_threshold = 180; // 3 minutes

        if now_ts - self.last_message_timestamp.get(BEDROOM_SENSOR_EXPECTED_IP).cloned().unwrap_or(0) > staleness_threshold {
            diag_message += "\nFAIL: Bedroom sensor";
        } else if now_ts - self.last_message_timestamp.get(BEDROOM_RELAY_EXPECTED_IP).cloned().unwrap_or(0) > staleness_threshold {
            diag_message += "\nFAIL: Bedroom relay";
        }

        if now_ts - self.last_message_timestamp.get(KIDS_SENSOR_EXPECTED_IP).cloned().unwrap_or(0) > staleness_threshold {
            diag_message += "\nFAIL: Kids sensor";
        } else if now_ts - self.last_message_timestamp.get(KIDS_RELAY_EXPECTED_IP).cloned().unwrap_or(0) > staleness_threshold {
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

// Make Server cloneable for async tasks
impl Clone for Server {
    fn clone(&self) -> Self {
        Server {
            last_message_timestamp: self.last_message_timestamp.clone(),
            last_temp_deci: self.last_temp_deci.clone(),
            last_relay_on_status: self.last_relay_on_status.clone(),
            relay_confirmations: self.relay_confirmations.clone(),
            controls: self.controls.clone(),
            web_state: self.web_state.clone(),
        }
    }
}

impl MessageHandler<DeviceMessage> for Server {
    fn on_message(
        &mut self,
        src: std::net::SocketAddr,
        msg: DeviceMessage,
    ) -> anyhow::Result<()> {
        let mut known_message_component_found = false;

        if let Some(sensor_report) = msg.sensor.as_ref() {
            self.new_sensor_report(src, sensor_report)?;
            known_message_component_found = true;
        } else if let Some(relay_report) = msg.relay.as_ref() {
            self.new_relay_report(src, relay_report)?;
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

    // Start the web server in a separate task
    tokio::spawn(async move {
        create_web_server(web_state).await;
    });

    // Start the main loop using FragmentCombiner
    println!("Starting temperature server on 0.0.0.0:4000...");
    FragmentCombiner::new(&mut server).main_loop("0.0.0.0:4000")
}
