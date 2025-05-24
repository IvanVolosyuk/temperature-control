import { ServerStatusResponse, RelayControlRequest, DisableHeaterRequest, ApiResponse } from '../types';

const API_BASE_URL = '/api'; // Assuming the React app is served from the same domain as the API

export async function getStatus(lastUpdate?: number): Promise<ServerStatusResponse> {
  let url = `${API_BASE_URL}/status`;
  if (lastUpdate) {
    url += `?last_update=${lastUpdate}`;
  }
  const response = await fetch(url);
  if (!response.ok) {
    throw new Error(`Failed to fetch status: ${response.statusText}`);
  }
  return response.json();
}

export async function controlRelay(roomName: string, state: boolean): Promise<ApiResponse> {
  const payload: RelayControlRequest = { room: roomName, state };
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

export async function disableHeater(roomName: string, disable: boolean): Promise<ApiResponse> {
  const payload: DisableHeaterRequest = { room: roomName, disable };
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
