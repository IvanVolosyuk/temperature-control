use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct ServerState {
    pub rooms: Vec<RoomStateWithId>,
}

impl Default for ServerState {
    fn default() -> Self {
        ServerState {
            rooms: vec![
                RoomStateWithId {
                    id: 0,
                    name: "Bedroom".to_string(),
                    ..Default::default()
                },
                RoomStateWithId {
                    id: 2,
                    name: "Kids Bedroom".to_string(),
                    ..Default::default()
                },
            ],
        }
    }
}

#[derive(Default, Clone, Serialize, Deserialize)]
pub struct RoomStateWithId {
    pub id: u32,
    pub name: String,
    pub sensor_available: bool,
    pub current_temp: f64,
    pub target_temp: f64,
    pub relay_available: bool,
    pub relay_state: bool,
    pub temperature_history: Vec<TemperaturePoint>,
    pub disabled_until: Option<i64>,
    pub override_temperature: Option<f64>,
    pub override_until: Option<i64>,
}

#[derive(Default, Clone, Serialize, Deserialize)]
pub struct RoomState {
    pub sensor_available: bool,
    pub current_temp: f64,
    pub target_temp: f64,
    pub relay_available: bool,
    pub relay_state: bool,
    pub temperature_history: Vec<TemperaturePoint>,
    pub disabled_until: Option<i64>,
    pub override_temperature: Option<f64>,
    pub override_until: Option<i64>,
}

#[derive(Default, Clone, Serialize, Deserialize)]
pub struct TemperaturePoint {
    pub timestamp: i64,
    pub temperature: f64,
    pub target: f64,
    pub heater_on: bool,
    pub is_disabled: bool,
}

pub fn serialize_history_point(device_id: u32, prev_timestamp: u32, point: TemperaturePoint) -> Vec<u32> {
    // Bit layout:
    // [sec:2][min:2][target_temp:15][temp:9][disabled][on][dev][extra_timestamp_flag]
    // [optional timestamp:32]
    // 9 bits
    let temp : u32 = ((point.temperature * 10. + 0.5) as u32).clamp(0, 511);
    // 15 bits
    let target_bits : u32 = ((point.target * 1024.) as u32).clamp(0, 32767);
    let dt : u32 = 30 + (point.timestamp as u32) - prev_timestamp;
    let min : u32 = dt / 60;
    let sec : u32 = dt - min * 60;
    let dev_bit : u32 = if device_id != 0 { 2 } else { 0 };
    let on_bit : u32 = if point.heater_on { 4 } else { 0 };
    let disabled_bit : u32 = if point.is_disabled { 8 } else { 0 };
    let partial = dev_bit | on_bit | disabled_bit | (temp << 4 | (target_bits << 13));
    if min < 5 && sec >= 29 && sec <= 32 {
        let min_bits = min - 1;
        let sec_bits = sec - 29;
        vec!(partial | (min_bits << 28) | (sec_bits) << 30)
    } else {
        vec!(1 | partial, point.timestamp as u32 )
    }
}

use anyhow::Result;
use tokio::{
    fs::File as TokioFile,
    io::AsyncReadExt,
};

pub async fn read_and_parse_binary_state(path: &str) -> Result<Option<(ServerState, u32, u32)>> {
    let mut file = match TokioFile::open(path).await {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };

    let metadata = match file.metadata().await {
        Ok(md) => md,
        Err(e) => return Err(e.into()),
    };

    if metadata.len() == 0 {
        return Ok(None); // Empty file, treat as no state
    }
    if metadata.len() % 4 != 0 { // Each entry is one or two u32s, so total size must be multiple of 4.
        eprintln!(
            "Binary state file {} has unexpected size {}. It might be corrupted.",
            path,
            metadata.len()
        );
        return Err(anyhow::anyhow!("Corrupted binary state file: invalid size"));
    }

    let mut server_state = ServerState::default(); // Initializes rooms 0, 1, 2
    let mut current_prev_ts0: u32 = 0;
    let mut current_prev_ts2: u32 = 0;

    // Buffer to hold points before adding to rooms, to ensure they are sorted later if needed.
    // Though they should be in order if written correctly.
    let mut parsed_points: Vec<(u32, TemperaturePoint)> = Vec::new();

    loop {
        match file.read_u32_le().await {
            Ok(val1) => {
                let extra_timestamp_flag = (val1 & 1) != 0;
                let dev_bit_set = (val1 & 2) != 0; // true for device 2, false for device 0
                let on_bit_set = (val1 & 4) != 0;
                let disabled_bit_set = (val1 & 8) != 0;

                let temp_raw = (val1 >> 4) & 0x1FF;    // 9 bits
                let target_raw = (val1 >> 13) & 0x7FFF; // 15 bits

                let device_id = if dev_bit_set { 2 } else { 0 };

                let timestamp_u32: u32;
                let prev_ts_for_this_point = if device_id == 0 {
                    current_prev_ts0
                } else {
                    current_prev_ts2
                };

                if extra_timestamp_flag {
                    match file.read_u32_le().await {
                        Ok(full_ts) => timestamp_u32 = full_ts,
                        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                            eprintln!(
                                "Binary state parsing error in {}: Unexpected EOF after reading first part of a two-word record for device {}.",
                                path, device_id
                            );
                            return Err(anyhow::anyhow!(
                                "Corrupted binary state file: missing full timestamp"
                            ));
                        }
                        Err(e) => return Err(e.into()),
                    }
                } else {
                    let min_bits = (val1 >> 28) & 0x3; // 2 bits for min - 1
                    let sec_bits = (val1 >> 30) & 0x3; // 2 bits for sec - 29

                    let min_val = min_bits + 1;
                    let sec_val = sec_bits + 29;
                    let dt = min_val * 60 + sec_val;
                    timestamp_u32 = prev_ts_for_this_point.wrapping_add(dt).wrapping_sub(30);
                }

                let temperature = temp_raw as f64 / 10.0;
                let target = target_raw as f64 / 1024.0;
                let heater_on = on_bit_set;
                let is_disabled = disabled_bit_set;

                let point = TemperaturePoint {
                    timestamp: timestamp_u32 as i64, // TemperaturePoint uses i64
                    temperature,
                    target,
                    heater_on,
                    is_disabled,
                };

                parsed_points.push((device_id, point));

                // Update the prev_timestamp for the *next* iteration for this device
                if device_id == 0 {
                    current_prev_ts0 = timestamp_u32;
                } else {
                    current_prev_ts2 = timestamp_u32;
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                // Expected way to finish reading a well-formed file.
                break;
            }
            Err(e) => {
                eprintln!("Binary state parsing I/O error in {}: {}", path, e);
                return Err(e.into());
            }
        }
    }

    // Add parsed points to the correct room state and sort history
    for (device_id, point) in parsed_points {
        if let Some(room) = server_state.rooms.iter_mut().find(|r| r.id == device_id) {
            room.temperature_history.push(point);
        } else {
            // Should not happen with ServerState::default() creating rooms 0,1,2
            eprintln!(
                "Binary state parsing warning: Device ID {} found in {} but no corresponding room in ServerState.",
                device_id, path
            );
        }
    }

    Ok(Some((server_state, current_prev_ts0, current_prev_ts2)))
}
