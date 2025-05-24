import { useState, useEffect, useRef, useCallback } from 'react';
import RoomCard from './components/RoomCard';
import { getStatus, controlRelay, disableHeater } from './services/api';
import { RoomState } from './types';
import './index.css';

const POLLING_INTERVAL = 1000; // 1 seconds for polling
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
  const intervalIdRef = useRef<number | null>(null);

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


  useEffect(() => {
    fetchStatus(true); // Initial fetch

    const startPolling = () => {
      if (intervalIdRef.current === null) {
        intervalIdRef.current = window.setInterval(() => fetchStatus(false), POLLING_INTERVAL);
      }
    };

    const stopPolling = () => {
      if (intervalIdRef.current !== null) {
        clearInterval(intervalIdRef.current);
        intervalIdRef.current = null;
      }
    };

    const handleVisibilityChange = () => {
      if (document.hidden) {
        stopPolling();
      } else {
        fetchStatus(false); // Fetch immediately when tab becomes visible
        startPolling();
      }
    };

    startPolling();
    document.addEventListener('visibilitychange', handleVisibilityChange);

    return () => {
      stopPolling();
      document.removeEventListener('visibilitychange', handleVisibilityChange); // Cleanup on unmount
    };
  }, [fetchStatus]);

  const handleApiAction = async (action: () => Promise<any>) => {
    setIsLoading(true); // Indicate loading for the action
    try {
      await action();
      await fetchStatus(false); // Refresh data after action
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to perform action.');
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

  if (isLoading && !bedroomData && !kidsRoomData) {
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
          isLoading={isLoading && !bedroomData}
          isDarkMode={isDarkMode}
        />
        <RoomCard
          roomName="Kids Bedroom"
          roomApiName={ROOM_ID_KIDS}
          roomData={kidsRoomData}
          onControlRelay={handleControlRelay}
          onDisableHeater={handleDisableHeater}
          isLoading={isLoading && !kidsRoomData}
          isDarkMode={isDarkMode}
        />
      </main>
    </div>
  );
}

export default App;
