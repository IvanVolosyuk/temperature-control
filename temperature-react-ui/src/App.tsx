/* eslint-disable @typescript-eslint/no-use-before-define */
/* eslint-disable no-console */
import { useState, useEffect, useRef, useCallback } from 'react';
import RoomCard from './components/RoomCard';
import { getStatus, controlRelay, disableHeater, overrideTemperature } from './services/api';
import { connectWebSocket, WebSocketCallbacks } from './services/websocket';
import { ServerStatusResponse, RoomStateWithId, TemperaturePoint } from './types'; // Updated RoomState to RoomStateWithId
import './index.css';

// Constants
const RECONNECT_DELAY_MS = 5000; // 5 seconds
const MAX_HISTORY_POINTS = 48 * 60 * 60; // Approx 48 hours of data at 1s interval, adjust as needed

function App() {
  const [roomsData, setRoomsData] = useState<RoomStateWithId[] | null>(null); // New state for rooms
  const [isLoading, setIsLoading] = useState<boolean>(true);
  const [error, setError] = useState<string | null>(null);
  const [isDarkMode, setIsDarkMode] = useState(() => {
    const saved = localStorage.getItem('darkMode');
    if (saved !== null) return saved === 'true';
    return window.matchMedia('(prefers-color-scheme: dark)').matches;
  });
  const [isConnected, setIsConnected] = useState<boolean>(false);

  const wsRef = useRef<WebSocket | null>(null);
  const lastKnownServerTimestampRef = useRef<number | null>(null);
  const isManuallyDisconnectedRef = useRef<boolean>(false);
  const reconnectTimeoutRef = useRef<number | null>(null);


  useEffect(() => {
    if (isDarkMode) {
      document.documentElement.classList.add('dark');
    } else {
      document.documentElement.classList.remove('dark');
    }
    localStorage.setItem('darkMode', isDarkMode.toString());
  }, [isDarkMode]);

  const mergeTemperatureHistory = useCallback((
    existing: TemperaturePoint[] = [],
    incoming: TemperaturePoint[] = []
  ): TemperaturePoint[] => {
    if (!incoming || incoming.length === 0) return existing;
    if (!existing || existing.length === 0) return incoming.slice(-MAX_HISTORY_POINTS);

    const combined = [...existing];
    const existingTimestamps = new Set(existing.map(p => p.timestamp));

    for (const point of incoming) {
      if (!existingTimestamps.has(point.timestamp)) {
        combined.push(point);
      }
    }
    // Sort by timestamp just in case there are out-of-order points from merging
    combined.sort((a, b) => a.timestamp - b.timestamp);
    return combined.slice(-MAX_HISTORY_POINTS);
  }, []);


  const updateLocalStateWithFullServerResponse = useCallback((data: ServerStatusResponse) => {
    setRoomsData(prevRoomsData => {
      const newRoomsDataMap = new Map<number, RoomStateWithId>();

      // Add existing rooms to map
      if (prevRoomsData) {
        for (const room of prevRoomsData) {
          newRoomsDataMap.set(room.id, room);
        }
      }

      // Update with rooms from server
      for (const roomFromServer of data.rooms) {
        const existingRoom = newRoomsDataMap.get(roomFromServer.id);
        const mergedHistory = mergeTemperatureHistory(
          existingRoom?.temperature_history,
          roomFromServer.temperature_history
        );
        newRoomsDataMap.set(roomFromServer.id, {
          ...(existingRoom || {} as RoomStateWithId), // Spread existing or empty object, ensure type
          ...roomFromServer,
          temperature_history: mergedHistory,
        });
      }
      return Array.from(newRoomsDataMap.values()).sort((a, b) => a.id - b.id); // Sort by ID for consistent order
    });

    let maxTimestamp = 0;
    for (const room of data.rooms) {
      if (room.temperature_history && room.temperature_history.length > 0) {
        const roomMaxTimestamp = room.temperature_history.reduce((max, p) => Math.max(max, p.timestamp), 0);
        if (roomMaxTimestamp > maxTimestamp) {
          maxTimestamp = roomMaxTimestamp;
        }
      }
    }

    if (maxTimestamp > (lastKnownServerTimestampRef.current || 0)) {
      lastKnownServerTimestampRef.current = maxTimestamp;
    }
    setIsLoading(false); // Ensure loading is false after updates
  }, [mergeTemperatureHistory]);


  const connectWs = useCallback(() => {
    if (wsRef.current || isManuallyDisconnectedRef.current) {
      console.log('WebSocket connection attempt skipped (already connected/connecting or manually disconnected).');
      return;
    }
    console.log('Attempting WebSocket connection...');
    setIsLoading(true); // Indicate connection attempt

    const wsUrl = `ws://${window.location.host}/ws`;
    const callbacks: WebSocketCallbacks = {
      onOpen: () => {
        setIsConnected(true);
        setError(null);
        setIsLoading(false);
        if (reconnectTimeoutRef.current) {
          clearTimeout(reconnectTimeoutRef.current);
          reconnectTimeoutRef.current = null;
        }
        console.log('WebSocket connection established.');
      },
      onMessage: (data) => {
        // console.log('WebSocket message received:', data);
        updateLocalStateWithFullServerResponse(data);
      },
      onClose: (event) => {
        setIsConnected(false);
        wsRef.current = null;
        console.log('WebSocket connection closed:', event.code, event.reason);
        // 1000 is normal closure
        // 1006 is manual closure initiated by us on focus lost
        if (!isManuallyDisconnectedRef.current && event.code !== 1000 && event.code !== 1006) {
          setError(`WebSocket disconnected unexpectedly (code: ${event.code}). Attempting to reconnect...`);
          if (!reconnectTimeoutRef.current) {
            // eslint-disable-next-line @typescript-eslint/no-use-before-define
            reconnectTimeoutRef.current = window.setTimeout(performReconnect, RECONNECT_DELAY_MS);
          }
        } else if (isManuallyDisconnectedRef.current) {
          console.log('WebSocket closed manually.');
        }
      },
      onError: (event) => {
        setError('WebSocket error. See console for details.');
        setIsConnected(false);
        setIsLoading(false); // Stop loading on error
        console.error('WebSocket error event:', event);
        // Consider attempting reconnect here as well, depending on desired behavior
        if (!isManuallyDisconnectedRef.current && !reconnectTimeoutRef.current) {
            // eslint-disable-next-line @typescript-eslint/no-use-before-define
            reconnectTimeoutRef.current = window.setTimeout(performReconnect, RECONNECT_DELAY_MS);
        }
      },
    };
    wsRef.current = connectWebSocket(wsUrl, callbacks);
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [updateLocalStateWithFullServerResponse]); // performReconnect is defined later, ESLint might warn


  const disconnectWs = useCallback(() => {
    if (reconnectTimeoutRef.current) {
      clearTimeout(reconnectTimeoutRef.current);
      reconnectTimeoutRef.current = null;
    }
    isManuallyDisconnectedRef.current = true;
    if (wsRef.current) {
      console.log('Manually disconnecting WebSocket...');
      wsRef.current.close(1000, 'Manual disconnection'); // 1000 is a normal closure
    }
    setIsConnected(false); // Assume disconnection immediately
  }, []);

  const performReconnect = useCallback(async (options?: { isTabReconnection?: boolean }) => {
    if (wsRef.current || isManuallyDisconnectedRef.current) {
      console.log('Reconnect skipped (already connected/connecting or manually disconnected).');
      return;
    }
    console.log('Performing reconnect...');
    setIsLoading(true);
    if (!options?.isTabReconnection) {
      setError('Connection lost. Attempting to reconnect...');
    }
    isManuallyDisconnectedRef.current = false; // Reset manual flag for reconnection attempts

    try {
      console.log('Fetching catch-up data before reconnecting WebSocket...');
      const catchUpData = await getStatus(lastKnownServerTimestampRef.current || undefined);
      updateLocalStateWithFullServerResponse(catchUpData);
      console.log('Catch-up data processed.');
    } catch (err) {
      console.error('Failed to fetch catch-up data:', err);
      setError(`Failed to fetch catch-up data: ${err instanceof Error ? err.message : 'Unknown error'}. Still attempting WebSocket reconnect.`);
      // Don't necessarily stop the WebSocket connection attempt here
    }

    connectWs(); // Attempt to reconnect WebSocket
  }, [connectWs, updateLocalStateWithFullServerResponse]);


  // Initial load and WebSocket connection
  useEffect(() => {
    const initialize = async () => {
      console.log('Initializing application...');
      setIsLoading(true);
      setError(null);
      isManuallyDisconnectedRef.current = false; // Ensure it's false on initial load

      try {
        console.log('Fetching initial full status...');
        const initialData = await getStatus(); // Get all data initially
        updateLocalStateWithFullServerResponse(initialData);
        console.log('Initial data processed.');
        if (!isManuallyDisconnectedRef.current) { // Check if disconnect was called during init
          connectWs();
        }
      } catch (err) {
        console.error('Initialization failed:', err);
        setError(`Initialization failed: ${err instanceof Error ? err.message : 'Unknown error'}. Retrying connection...`);
        // Still try to connect WebSocket even if initial HTTP fetch fails,
        // as the server might become available.
        if (!isManuallyDisconnectedRef.current) {
            // Schedule a reconnect attempt, which includes connectWs
            if (reconnectTimeoutRef.current) clearTimeout(reconnectTimeoutRef.current);
            reconnectTimeoutRef.current = window.setTimeout(performReconnect, RECONNECT_DELAY_MS);
        }
      }
    };

    initialize();

    return () => {
      console.log('Cleaning up App component (unmount or re-render).');
      disconnectWs();
      if (reconnectTimeoutRef.current) {
        clearTimeout(reconnectTimeoutRef.current);
        reconnectTimeoutRef.current = null;
      }
    };
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [connectWs, disconnectWs, performReconnect, updateLocalStateWithFullServerResponse]); // Dependencies for initial setup and cleanup


  // Handle document visibility changes
  useEffect(() => {
    const handleVisibilityChange = () => {
      if (document.hidden) {
        console.log('Document hidden, disconnecting WebSocket temporarily.');
        disconnectWs(); // Disconnect when tab is not visible
      } else {
        console.log('Document visible, attempting to reconnect WebSocket.');
        // Reset manual flag if user is actively bringing tab to foreground
        isManuallyDisconnectedRef.current = false;
        performReconnect({ isTabReconnection: true });
      }
    };

    document.addEventListener('visibilitychange', handleVisibilityChange);
    return () => {
      document.removeEventListener('visibilitychange', handleVisibilityChange);
    };
  }, [disconnectWs, performReconnect]);


  const handleApiAction = async (action: () => Promise<any>) => {
    setIsLoading(true);
    try {
      await action();
      // After a successful action, fetch the latest state to ensure UI consistency.
      // This is important if the WebSocket connection is down or if the action
      // itself doesn't trigger an immediate broadcast of the specific change.
      console.log('API action successful, fetching updated status...');
      const updatedData = await getStatus(lastKnownServerTimestampRef.current || undefined);
      updateLocalStateWithFullServerResponse(updatedData);
      console.log('Status updated after API action.');
    } catch (err) {
      console.error('API action failed:', err);
      setError(err instanceof Error ? err.message : 'Failed to perform action.');
      // Optionally, trigger a reconnect or full status refresh here if appropriate
    } finally {
      setIsLoading(false);
    }
  };

  const handleControlRelay = (roomId: number, state: boolean) => {
    return handleApiAction(() => controlRelay(roomId, state));
  };

  const handleDisableHeater = (roomId: number, disable: boolean) => {
    return handleApiAction(() => disableHeater(roomId, disable));
  };

  const handleOverrideTemperature = (roomId: number, temperature: number | null) => {
    return handleApiAction(() => overrideTemperature(roomId, temperature));
  };


  if (isLoading && !roomsData && !error) {
    return (
      <div className="min-h-screen flex items-center justify-center bg-gray-100 dark:bg-gray-900 text-gray-800 dark:text-gray-200">
        Loading initial data...
      </div>
    );
  }

  return (
    <div className="min-h-screen bg-gray-100 dark:bg-gray-900 p-4 font-sans">
      <header className="mb-6 flex justify-between items-center">
        <h1 className="text-3xl font-bold text-gray-800 dark:text-gray-200">
          Temperature Control
        </h1>
        <button
          onClick={() => setIsDarkMode(!isDarkMode)}
          className="p-2 rounded-lg bg-gray-200 dark:bg-gray-700 hover:bg-gray-300 dark:hover:bg-gray-600 transition-colors duration-200"
          aria-label="Toggle dark mode"
        >
          {isDarkMode ? (
            <svg className="w-6 h-6 text-yellow-500" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 3v1m0 16v1m9-9h-1M4 12H3m15.364 6.364l-.707-.707M6.343 6.343l-.707-.707m12.728 0l-.707.707M6.343 17.657l-.707.707M16 12a4 4 0 11-8 0 4 4 0 018 0z" />
            </svg>
          ) : (
            <svg className="w-6 h-6 text-gray-700" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M20.354 15.354A9 9 0 018.646 3.646 9.003 9.003 0 0012 21a9.003 9.003 0 008.354-5.646z" />
            </svg>
          )}
        </button>
      </header>
      {error && (
        <div className="mb-4 p-3 bg-red-100 dark:bg-red-800 border border-red-400 dark:border-red-600 text-red-700 dark:text-red-200 rounded text-center">
          Error: {error}
        </div>
      )}
      <main className="grid grid-cols-1 md:grid-cols-2 gap-6">
        {roomsData && roomsData.map(room => (
          <RoomCard
            key={room.id} // Important: add a key for list rendering
            room={room} // Pass the whole room object
            onControlRelay={handleControlRelay} // Assumes RoomCard calls with (roomId, state)
            onDisableHeater={handleDisableHeater} // Assumes RoomCard calls with (roomId, disable)
            onOverrideTemperature={handleOverrideTemperature} // Assumes RoomCard calls with (roomId, temp)
            isLoading={isLoading && !roomsData} // Simplified isLoading for now
            isDarkMode={isDarkMode}
          />
        ))}
      </main>
      <footer className="mt-8 text-center text-sm text-gray-600 dark:text-gray-400">
        WebSocket: {isConnected ? (
          <span className="text-green-500 dark:text-green-400">Connected</span>
        ) : (
          <span className="text-red-500 dark:text-red-400">Disconnected</span>
        )}
        {isLoading && !isConnected && <span className="ml-2">Attempting to connect...</span>}
      </footer>
    </div>
  );
}

export default App;
