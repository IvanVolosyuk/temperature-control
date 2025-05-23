// Based on apps/server/src/web.rs and status.html

export interface TemperaturePoint {
  timestamp: number; // Unix timestamp in seconds
  temperature: number;
  target: number;
  heater_on: boolean;
  is_disabled: boolean;
}

export interface RoomState {
  sensor_available: boolean;
  current_temp: number;
  target_temp: number;
  relay_available: boolean;
  relay_state: boolean; // true if ON, false if OFF
  temperature_history: TemperaturePoint[];
  disabled_until: number | null; // Unix timestamp in seconds, or null
}

export interface ServerStatusResponse {
  bedroom: RoomState;
  kids_bedroom: RoomState;
}

// For POST request bodies
export interface RelayControlRequest {
  room: string; // "bedroom" or "kids_bedroom"
  state: boolean; // true for ON, false for OFF
}

export interface DisableHeaterRequest {
  room: string; // "bedroom" or "kids_bedroom"
  disable: boolean; // true to disable, false to restore
}

// Generic API response for POSTs
export interface ApiResponse {
  success: boolean;
  error?: string;
}
