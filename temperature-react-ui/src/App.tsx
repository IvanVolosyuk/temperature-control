/* eslint-disable @typescript-eslint/no-use-before-define */
/* eslint-disable no-console */
import { useState, useEffect, useRef, useCallback } from 'react';
import RoomCard from './components/RoomCard';
import { getStatus, controlRelay, disableHeater, overrideTemperature } from './services/api';
import { connectWebSocket, WebSocketCallbacks } from './services/websocket';
import { ServerStatusResponse, RoomState, TemperaturePoint } from './types';
import './index.css';

// Constants
const ROOM_ID_BEDROOM = 'bedroom';
const ROOM_ID_KIDS = 'kids_bedroom';
const RECONNECT_DELAY_MS = 5000; // 5 seconds
const MAX_HISTORY_POINTS = 2 * 60 * 60; // Approx 2 hours of data at 1s interval, adjust as needed

function App() {
  const [bedroomData, setBedroomData] = useState<RoomState | null>(null);
  const [kidsRoomData, setKidsRoomData] = useState<RoomState | null>(null);
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
    setBedroomData(prev => ({
      ...(prev || data.bedroom),
      ...data.bedroom,
      temperature_history: mergeTemperatureHistory(prev?.temperature_history, data.bedroom.temperature_history),
    }));
    setKidsRoomData(prev => ({
      ...(prev || data.kids_bedroom),
      ...data.kids_bedroom,
      temperature_history: mergeTemperatureHistory(prev?.temperature_history, data.kids_bedroom.temperature_history),
    }));

    const bedroomLatest = data.bedroom.temperature_history.slice(-1)[0]?.timestamp;
    const kidsLatest = data.kids_bedroom.temperature_history.slice(-1)[0]?.timestamp;
    const latestTimestamp = Math.max(bedroomLatest || 0, kidsLatest || 0);

    if (latestTimestamp > (lastKnownServerTimestampRef.current || 0)) {
      lastKnownServerTimestampRef.current = latestTimestamp;
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
        if (!isManuallyDisconnectedRef.current && event.code !== 1000) { // 1000 is normal closure
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

  const performReconnect = useCallback(async () => {
    if (wsRef.current || isManuallyDisconnectedRef.current) {
      console.log('Reconnect skipped (already connected/connecting or manually disconnected).');
      return;
    }
    console.log('Performing reconnect...');
    setIsLoading(true);
    setError('Connection lost. Attempting to reconnect...');
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
        performReconnect();
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

  const handleControlRelay = (roomApiName: string, state: boolean) => {
    return handleApiAction(() => controlRelay(roomApiName, state));
  };

  const handleDisableHeater = (roomApiName: string, disable: boolean) => {
    return handleApiAction(() => disableHeater(roomApiName, disable));
  };

  const handleOverrideTemperature = (roomApiName: string, temperature: number | null) => {
    return handleApiAction(() => overrideTemperature(roomApiName, temperature));
  };


  if (isLoading && !bedroomData && !kidsRoomData && !error) {
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
        <RoomCard
          roomName="Bedroom"
          roomApiName={ROOM_ID_BEDROOM}
          roomData={bedroomData}
          onControlRelay={handleControlRelay}
          onDisableHeater={handleDisableHeater}
          onOverrideTemperature={handleOverrideTemperature}
          isLoading={isLoading && !bedroomData}
          isDarkMode={isDarkMode}
        />
        <RoomCard
          roomName="Kids Bedroom"
          roomApiName={ROOM_ID_KIDS}
          roomData={kidsRoomData}
          onControlRelay={handleControlRelay}
          onDisableHeater={handleDisableHeater}
          onOverrideTemperature={handleOverrideTemperature}
          isLoading={isLoading && !kidsRoomData}
          isDarkMode={isDarkMode}
        />
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
