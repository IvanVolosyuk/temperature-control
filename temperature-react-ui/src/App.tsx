import { useState, useEffect, useRef, useCallback } from 'react';
import RoomCard from './components/RoomCard';
import { getStatus, controlRelay, disableHeater, overrideTemperature } from './services/api';
import { RoomState } from './types';
import './index.css';

// const POLLING_INTERVAL = 1000; // Polling is being removed
const ROOM_ID_BEDROOM = 'bedroom';
const ROOM_ID_KIDS = 'kids_bedroom';

function App() {
  const [bedroomData, setBedroomData] = useState<RoomState | null>(null);
  const [kidsRoomData, setKidsRoomData] = useState<RoomState | null>(null);
  const [isLoading, setIsLoading] = useState<boolean>(true);
  const [error, setError] = useState<string | null>(null);
  const [isDarkMode, setIsDarkMode] = useState(() => {
    // Check if user has a saved preference
    const saved = localStorage.getItem('darkMode');
    if (saved !== null) {
      return saved === 'true';
    }
    // If no saved preference, use system preference
    return window.matchMedia('(prefers-color-scheme: dark)').matches;
  });

  const lastUpdateTimestampRef = useRef<{ bedroom: number | null; kids_bedroom: number | null }>({
    bedroom: null,
    kids_bedroom: null,
  });
  // const intervalIdRef = useRef<number | null>(null); // For polling, will be removed or repurposed
  const ws = useRef<WebSocket | null>(null);
  const [isConnected, setIsConnected] = useState(false);

  // Update dark mode class on HTML element
  useEffect(() => {
    if (isDarkMode) {
      document.documentElement.classList.add('dark');
    } else {
      document.documentElement.classList.remove('dark');
    }
    // Save preference
    localStorage.setItem('darkMode', isDarkMode.toString());
  }, [isDarkMode]);

  const fetchStatus = useCallback(async (isInitialLoad = false) => {
    if (!isInitialLoad) {
      // For subsequent polls, don't set global isLoading unless necessary
      // Individual components can show stale data or specific loading indicators
    } else {
      setIsLoading(true);
    }
    setError(null);

    try {
      // Use the latest timestamp from either room for the last_update query parameter
      const latestTimestampForQuery = Math.max(
        lastUpdateTimestampRef.current.bedroom || 0,
        lastUpdateTimestampRef.current.kids_bedroom || 0
      );
      const queryTimestamp = latestTimestampForQuery > 0 ? latestTimestampForQuery : undefined;

      const data = await getStatus(queryTimestamp);

      // Merge new history data with existing, avoid full replacement if not needed
      setBedroomData(prev => ({
        ...(prev || data.bedroom), // use new data for static fields or if no previous data
        ...data.bedroom, // override with latest static fields
        temperature_history: mergeTemperatureHistory(prev?.temperature_history, data.bedroom.temperature_history)
      }));

      setKidsRoomData(prev => ({
        ...(prev || data.kids_bedroom),
        ...data.kids_bedroom,
        temperature_history: mergeTemperatureHistory(prev?.temperature_history, data.kids_bedroom.temperature_history)
      }));

      // Update last update timestamps from the new data
      if (data.bedroom.temperature_history.length > 0) {
        lastUpdateTimestampRef.current.bedroom = data.bedroom.temperature_history[data.bedroom.temperature_history.length - 1].timestamp;
      }
      if (data.kids_bedroom.temperature_history.length > 0) {
        lastUpdateTimestampRef.current.kids_bedroom = data.kids_bedroom.temperature_history[data.kids_bedroom.temperature_history.length - 1].timestamp;
      }

    } catch (err) {
      setError(err instanceof Error ? err.message : 'An unknown error occurred.');
      // Keep stale data on error for polling, clear for initial load?
      // if (isInitialLoad) {
      //   setBedroomData(null);
      //   setKidsRoomData(null);
      // }
    } finally {
      if (isInitialLoad) {
        setIsLoading(false);
      }
    }
  }, []);

  // Helper to merge temperature history arrays
  const mergeTemperatureHistory = (existing: RoomState['temperature_history'] = [], incoming: RoomState['temperature_history'] = []) => {
    if (!incoming || incoming.length === 0) return existing;
    if (!existing || existing.length === 0) return incoming;

    const combined = [...existing];
    const lastExistingTimestamp = existing[existing.length - 1]?.timestamp || 0;

    for (const point of incoming) {
      if (point.timestamp > lastExistingTimestamp) {
        combined.push(point);
      }
    }
    // Optional: Limit history size if needed
    // const MAX_HISTORY_POINTS = 1000; // Example
    // return combined.slice(-MAX_HISTORY_POINTS);
    return combined;
  };


  const connectAttemptTimeoutRef = useRef<number | null>(null);
  const reconnectAttemptsRef = useRef(0);
  const MAX_RECONNECT_ATTEMPTS = 5;
  const RECONNECT_DELAY_MS = 3000; // 3 seconds

  const lastTimestampsBeforeHiddenRef = useRef<{ bedroom: number | null; kids_bedroom: number | null } | null>(null);


  useEffect(() => {
    // Initial fetch is handled by WebSocket connection sending full state
    // fetchStatus(true); 

    const connectWebSocket = (isReconnect = false) => {
      if (ws.current && ws.current.readyState === WebSocket.OPEN) {
        console.log("WebSocket already open.");
        return;
      }
      if (ws.current && ws.current.readyState === WebSocket.CONNECTING) {
        console.log("WebSocket already connecting.");
        return;
      }

      // Determine WebSocket protocol based on current window protocol
      const wsProtocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
      const wsUrl = `${wsProtocol}//${window.location.host}/ws`;
      ws.current = new WebSocket(wsUrl);
      console.log(`Attempting to connect WebSocket... (Reconnect: ${isReconnect})`);

      ws.current.onopen = () => {
        console.log('WebSocket connected');
        setIsConnected(true);
        setError(null); 
        reconnectAttemptsRef.current = 0; // Reset reconnect attempts on successful connection
        if (connectAttemptTimeoutRef.current) {
          clearTimeout(connectAttemptTimeoutRef.current);
          connectAttemptTimeoutRef.current = null;
        }
        // Initial state is sent by backend on connect.
        // If this was a reconnect after being hidden, fetchStatus would have already run.
      };

      ws.current.onmessage = (event) => {
        try {
          const data = JSON.parse(event.data);
          // console.log('WebSocket message received:', data);

          setBedroomData(prev => ({
            ...(prev || data.bedroom),
            ...data.bedroom,
            temperature_history: mergeTemperatureHistory(prev?.temperature_history, data.bedroom.temperature_history, true) // Assume full state for now
          }));
    
          setKidsRoomData(prev => ({
            ...(prev || data.kids_bedroom),
            ...data.kids_bedroom,
            temperature_history: mergeTemperatureHistory(prev?.temperature_history, data.kids_bedroom.temperature_history, true) // Assume full state for now
          }));
    
          // Update last update timestamps from the new data
          if (data.bedroom.temperature_history.length > 0) {
            lastUpdateTimestampRef.current.bedroom = data.bedroom.temperature_history[data.bedroom.temperature_history.length - 1].timestamp;
          } else if (prevData => prevData?.bedroom?.temperature_history?.length > 0 && data.bedroom.temperature_history.length === 0) {
             // If history was cleared by an update, reset timestamp
            lastUpdateTimestampRef.current.bedroom = 0;
          }

          if (data.kids_bedroom.temperature_history.length > 0) {
            lastUpdateTimestampRef.current.kids_bedroom = data.kids_bedroom.temperature_history[data.kids_bedroom.temperature_history.length - 1].timestamp;
          } else if (prevData => prevData?.kids_bedroom?.temperature_history?.length > 0 && data.kids_bedroom.temperature_history.length === 0) {
            // If history was cleared by an update, reset timestamp
            lastUpdateTimestampRef.current.kids_bedroom = 0;
          }
          setIsLoading(false); // Data received, no longer initial loading

        } catch (e) {
          console.error('Error processing WebSocket message:', e);
          // setError('Error processing data from server.'); // Avoid setting global error for every minor processing issue
        }
      };

      ws.current.onerror = (event) => {
        console.error('WebSocket error:', event);
        setError('WebSocket connection error.'); // General error
        // onclose will handle reconnection attempts.
      };

      ws.current.onclose = (event) => {
        console.log('WebSocket disconnected:', event.code, event.reason);
        setIsConnected(false);
        
        if (connectAttemptTimeoutRef.current) {
          clearTimeout(connectAttemptTimeoutRef.current);
          connectAttemptTimeoutRef.current = null;
        }

        // Check if the disconnection was intentional (e.g., tab hidden, component unmount)
        // ws.current might be null if unmounted and cleanup ran.
        const intentionalClose = ws.current?.readyState === WebSocket.CLOSING || document.hidden;

        if (!intentionalClose && reconnectAttemptsRef.current < MAX_RECONNECT_ATTEMPTS) {
          reconnectAttemptsRef.current++;
          setError(`WebSocket disconnected. Attempting reconnect ${reconnectAttemptsRef.current}/${MAX_RECONNECT_ATTEMPTS}...`);
          console.log(`Attempting reconnect ${reconnectAttemptsRef.current}/${MAX_RECONNECT_ATTEMPTS} in ${RECONNECT_DELAY_MS}ms`);
          
          connectAttemptTimeoutRef.current = window.setTimeout(async () => {
            setIsLoading(true);
            const latestTimestampForQuery = Math.max(
              lastUpdateTimestampRef.current.bedroom || 0,
              lastUpdateTimestampRef.current.kids_bedroom || 0
            );
            console.log(`Reconnecting: Fetching missed updates since ${latestTimestampForQuery}`);
            await fetchStatus(false, latestTimestampForQuery > 0 ? latestTimestampForQuery : undefined);
            setIsLoading(false);
            connectWebSocket(true); // Attempt to reconnect
          }, RECONNECT_DELAY_MS);

        } else if (!intentionalClose) {
          setError('WebSocket disconnected. Max reconnect attempts reached. Please refresh the page or check your connection.');
          console.error('Max reconnect attempts reached.');
        }
      };
    };

    connectWebSocket(false); // Initial connection

    const handleVisibilityChange = async () => {
      if (document.hidden) {
        console.log('Tab hidden, closing WebSocket if open.');
        // Store timestamps before closing
        lastTimestampsBeforeHiddenRef.current = { ...lastUpdateTimestampRef.current };
        if (ws.current && ws.current.readyState === WebSocket.OPEN) {
          ws.current.close(1000, "Tab hidden"); // Normal closure
        }
        if (connectAttemptTimeoutRef.current) { // Clear any pending reconnect timeout
            clearTimeout(connectAttemptTimeoutRef.current);
            connectAttemptTimeoutRef.current = null;
            reconnectAttemptsRef.current = 0; // Reset attempts as this is an intentional close
        }
      } else {
        console.log('Tab visible.');
        if (!ws.current || ws.current.readyState === WebSocket.CLOSED) {
          console.log('Attempting to reconnect WebSocket due to tab visibility.');
          setIsLoading(true);
          setError('Re-establishing connection...');
          
          const queryTimestamp = Math.max(
            lastTimestampsBeforeHiddenRef.current?.bedroom || 0,
            lastTimestampsBeforeHiddenRef.current?.kids_bedroom || 0,
            lastUpdateTimestampRef.current.bedroom || 0, // Also consider current if somehow updated
            lastUpdateTimestampRef.current.kids_bedroom || 0
          );

          console.log(`Tab visible: Fetching missed updates since ${queryTimestamp}`);
          await fetchStatus(false, queryTimestamp > 0 ? queryTimestamp : undefined);
          // fetchStatus updates lastUpdateTimestampRef internally
          
          setIsLoading(false);
          connectWebSocket(true); // Reconnect WebSocket
        } else if (ws.current && ws.current.readyState === WebSocket.OPEN) {
          console.log('WebSocket already open on tab visible.');
          setError(null); // Clear any "reconnecting" messages
        }
      }
    };

    document.addEventListener('visibilitychange', handleVisibilityChange);

    return () => {
      console.log('Cleaning up App component...');
      document.removeEventListener('visibilitychange', handleVisibilityChange);
      if (connectAttemptTimeoutRef.current) {
        clearTimeout(connectAttemptTimeoutRef.current);
      }
      if (ws.current) {
        console.log('Closing WebSocket connection (component unmount)');
        ws.current.onclose = null; // Prevent onclose handler from trying to reconnect
        ws.current.onerror = null;
        ws.current.close(1000, "Component unmounting");
        ws.current = null;
      }
      reconnectAttemptsRef.current = 0; 
    };
  }, [fetchStatus]); // fetchStatus is stable due to useCallback

  // Helper to merge temperature history arrays
  // Added isFullStateUpdate flag: if true, incoming replaces existing if timestamps are older or same.
  // This is because WebSocket sends full state, so we don't want to just append.
  const mergeTemperatureHistory = (
    existing: RoomState['temperature_history'] = [],
    incoming: RoomState['temperature_history'] = [],
    isFullStateUpdate = false
  ) => {
    if (!incoming || incoming.length === 0) {
      return isFullStateUpdate ? [] : existing; // If full update and incoming is empty, history is cleared
    }
    if (!existing || existing.length === 0 || isFullStateUpdate) {
      // If it's a full state update, or no existing history, incoming is the new history
      // Sort incoming just in case, though backend should send sorted
      return [...incoming].sort((a, b) => a.timestamp - b.timestamp);
    }

    // This part is for merging partial updates (e.g. from HTTP catch-up)
    const combined = [...existing];
    const lastExistingTimestamp = existing[existing.length - 1]?.timestamp || 0;

    for (const point of incoming) {
      if (point.timestamp > lastExistingTimestamp) {
        combined.push(point);
      }
      // If not a full state update, we don't modify existing points
      // If it IS a full state update, the logic above (isFullStateUpdate=true) handles it by replacing.
    }
    // Sort and limit history size if needed
    combined.sort((a, b) => a.timestamp - b.timestamp);
    // const MAX_HISTORY_POINTS = 2000; // Example: Keep last 2000 points
    // return combined.slice(-MAX_HISTORY_POINTS);
    return combined;
  };

  const handleApiAction = async (action: () => Promise<any>) => {
    // No longer calling fetchStatus here, WebSocket should provide updates.
    // setIsLoading(true); // Consider if this global loading is still desired for API actions
    try {
      await action();
      // await fetchStatus(false); // Refresh data after action - REMOVED
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to perform action.');
    } finally {
      // setIsLoading(false);
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

  if (isLoading && !bedroomData && !kidsRoomData && !isConnected) {
    // Show loading only if not connected and no data yet
    return (
      <div className="min-h-screen flex items-center justify-center bg-gray-100 dark:bg-gray-900 text-gray-800 dark:text-gray-200">
        Loading initial data and connecting to server...
      </div>
    );
  }

  return (
    <div className="min-h-screen bg-gray-100 dark:bg-gray-900 p-4 font-sans">
      <header className="mb-6 flex justify-between items-center">
        <h1 className="text-3xl font-bold text-gray-800 dark:text-gray-200">
          Temperature Control <span className={`text-sm ${isConnected ? 'text-green-500' : 'text-red-500'}`}>{isConnected ? '● Connected' : '● Disconnected'}</span>
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
    </div>
  );
}

export default App;
