// temperature-react-ui/src/services/websocket.ts
/* eslint-disable no-console */

import { ServerStatusResponse } from '../types';

export interface WebSocketCallbacks {
  onOpen: () => void;
  onMessage: (data: ServerStatusResponse) => void;
  onClose: (event: CloseEvent) => void;
  onError: (event: Event) => void;
}

export function connectWebSocket(
  url: string,
  callbacks: WebSocketCallbacks,
): WebSocket {
  const ws = new WebSocket(url);

  ws.onopen = () => {
    console.log('WebSocket connection established');
    callbacks.onOpen();
  };

  ws.onmessage = (event) => {
    try {
      const data = JSON.parse(event.data as string) as ServerStatusResponse;
      // console.log('WebSocket message received:', data);
      callbacks.onMessage(data);
    } catch (error) {
      console.error('Error parsing WebSocket message:', error);
    }
  };

  ws.onclose = (event) => {
    console.log('WebSocket connection closed:', event);
    callbacks.onClose(event);
  };

  ws.onerror = (event) => {
    console.error('WebSocket error:', event);
    callbacks.onError(event);
  };

  return ws;
}
