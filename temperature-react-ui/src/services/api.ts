import { ServerStatusResponse, RelayControlRequest, DisableHeaterRequest, ApiResponse } from '../types';

const API_BASE_URL = '/api'; // Assuming the React app is served from the same domain as the API

export async function getStatus(lastUpdate?: number): Promise<ServerStatusResponse> {
  let url = `${API_BASE_URL}/status`;
  if (lastUpdate) {
    url += `?last_update=${lastUpdate}`;
  } else {
    let cutoff = Math.floor(Date.now() / 1000 - 2 * 24 * 3600);
    url += `?last_update=${cutoff}`;
  }
  const response = await fetch(url);
  if (!response.ok) {
    throw new Error(`Failed to fetch status: ${response.statusText}`);
  }
  return response.json();
}

// Interface for the override temperature request payload
interface OverrideTemperaturePayload {
  room_id: number;
  temperature: number | null; // null to clear override
}

export async function overrideTemperature(roomId: number, temperature: number | null): Promise<ApiResponse> {
  const payload: OverrideTemperaturePayload = { room_id: roomId, temperature };
  const response = await fetch(`${API_BASE_URL}/override_temperature`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(payload),
  });
  if (!response.ok) {
    try {
      const errData = await response.json();
      throw new Error(errData.error || `Failed to set override: ${response.statusText}`);
    } catch (e) {
      // If parsing JSON fails or it's not a JSON error from backend
      if (e instanceof Error) {
        throw e; // rethrow if it's already an Error
      }
      throw new Error(`Failed to set override: ${response.statusText}`);
    }
  }
  return response.json();
}

export async function controlRelay(roomId: number, state: boolean): Promise<ApiResponse> {
  const payload: RelayControlRequest = { room_id: roomId, state };
  const response = await fetch(`${API_BASE_URL}/relay`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(payload),
  });
  if (!response.ok) {
    // Try to parse error from backend if available
    try {
        const errData = await response.json();
        throw new Error(errData.error || `Failed to control relay: ${response.statusText}`);
    } catch (e) {
        throw new Error(`Failed to control relay: ${response.statusText}`);
    }
  }
  return response.json();
}

export async function disableHeater(roomId: number, disable: boolean): Promise<ApiResponse> {
  const payload: DisableHeaterRequest = { room_id: roomId, disable };
  const response = await fetch(`${API_BASE_URL}/disable`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(payload),
  });
   if (!response.ok) {
    try {
        const errData = await response.json();
        throw new Error(errData.error || `Failed to disable/enable heater: ${response.statusText}`);
    } catch (e) {
        throw new Error(`Failed to disable/enable heater: ${response.statusText}`);
    }
  }
  return response.json();
}
