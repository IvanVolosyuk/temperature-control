import { useState, useEffect, useRef, useCallback } from 'react';
import RoomCard from './components/RoomCard';
import { getStatus, controlRelay, disableHeater } from './services/api';
import { RoomState } from './types';
import './index.css';

const POLLING_INTERVAL = 5000; // 5 seconds for polling
const ROOM_ID_BEDROOM = 'bedroom';
const ROOM_ID_KIDS = 'kids_bedroom';


function App() {
  const [bedroomData, setBedroomData] = useState<RoomState | null>(null);
  const [kidsRoomData, setKidsRoomData] = useState<RoomState | null>(null);
  const [isLoading, setIsLoading] = useState<boolean>(true);
  const [error, setError] = useState<string | null>(null);

  const lastUpdateTimestampRef = useRef<{ bedroom: number | null; kids_bedroom: number | null }>({
    bedroom: null,
    kids_bedroom: null,
  });

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
    const intervalId = setInterval(() => fetchStatus(false), POLLING_INTERVAL);
    return () => clearInterval(intervalId); // Cleanup on unmount
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
    return <div className="min-h-screen flex items-center justify-center bg-gray-100 dark:bg-gray-900 text-gray-800 dark:text-gray-200">Loading initial data...</div>;
  }

  return (
    <div className="bg-gray-100 dark:bg-gray-900 min-h-screen p-4 font-sans">
      <header className="mb-6">
        <h1 className="text-3xl font-bold text-center text-gray-800 dark:text-gray-200">
          Temperature Control
        </h1>
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
          isLoading={isLoading && !bedroomData} // Pass loading specific to this card if data is absent
        />
        <RoomCard
          roomName="Kids Bedroom"
          roomApiName={ROOM_ID_KIDS}
          roomData={kidsRoomData}
          onControlRelay={handleControlRelay}
          onDisableHeater={handleDisableHeater}
          isLoading={isLoading && !kidsRoomData}
        />
      </main>
    </div>
  );
}

export default App;
