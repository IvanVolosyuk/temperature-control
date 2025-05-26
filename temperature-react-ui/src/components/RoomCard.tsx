import React from 'react';
import TemperatureChart from './TemperatureChart';
import { RoomStateWithId } from '../types'; // Updated to RoomStateWithId
import StatusIcon from './StatusIcon';

type RoomCardProps = {
  room: RoomStateWithId; // Changed from roomName, roomApiName, roomData
  onControlRelay: (roomId: number, state: boolean) => Promise<void>;
  onDisableHeater: (roomId: number, disable: boolean) => Promise<void>;
  onOverrideTemperature: (roomId: number, temperature: number | null) => Promise<void>;
  isLoading: boolean;
  isDarkMode: boolean;
};

const RoomCard: React.FC<RoomCardProps> = ({
  room, // Changed from roomName, roomApiName, roomData
  onControlRelay,
  onDisableHeater,
  onOverrideTemperature,
  isLoading,
  isDarkMode,
}) => {
  const isHeaterDisabled = Boolean(room.disabled_until && Date.now() < room.disabled_until * 1000);
  const isOverrideActive = Boolean(room.override_until && Date.now() < room.override_until * 1000 && room.override_temperature !== null && room.override_temperature !== undefined);
  const currentOverrideTemp = room.override_temperature;

  const handleRelayToggle = () => {
    if (!room.relay_available) return;
    onControlRelay(room.id, !room.relay_state);
  };

  const handleHeaterControl = (disable: boolean) => {
    if (!room.relay_available) return;
    onDisableHeater(room.id, disable);
  };

  // isLoading prop is handled by App.tsx, if room is not available, this component won't be rendered.
  // However, App.tsx passes a general isLoading flag. We can use it for individual card loading state if needed,
  // or rely on App.tsx's global loading. For now, let's assume if !room, it's loading or an issue.
  // The `isLoading && !roomData` check from before becomes just `isLoading` if `room` is guaranteed.
  // App.tsx's current logic for RoomCard: `isLoading={isLoading && !roomsData}`. This means this individual
  // card doesn't need to check for !room for loading, just the `isLoading` prop passed from App.
  // If `room` itself could be null while not loading, then `if (isLoading || !room)` would be needed.
  // Given App.tsx maps over `roomsData`, `room` should always be defined here.
  
  if (isLoading) { // Simplified loading check as room object should be present
    return (
      <div className="bg-white dark:bg-gray-800 shadow-md rounded-lg p-6 animate-pulse">
        <h2 className="text-2xl font-semibold mb-4 text-gray-700 dark:text-gray-300">{room.name || 'Loading...'}</h2>
        <div className="h-8 bg-gray-200 dark:bg-gray-700 rounded w-3/4 mb-4"></div>
        <div className="h-6 bg-gray-200 dark:bg-gray-700 rounded w-1/2 mb-2"></div>
        <div className="h-6 bg-gray-200 dark:bg-gray-700 rounded w-1/2 mb-4"></div>
        <div className="h-8 bg-gray-200 dark:bg-gray-700 rounded w-full mb-2"></div>
        <div className="h-8 bg-gray-200 dark:bg-gray-700 rounded w-full mb-4"></div>
        <div className="h-64 bg-gray-200 dark:bg-gray-700 rounded"></div>
      </div>
    );
  }


  return (
    <div className="bg-white dark:bg-gray-800 rounded-lg shadow-md p-6 border border-gray-200 dark:border-gray-700">
      <h2 className="text-2xl font-semibold text-gray-700 dark:text-gray-300 mb-4">{room.name}</h2>

      {/* Temperature Sensor Section */}
      <div className="mb-4">
        <h3 className="text-xl font-medium text-gray-500 dark:text-gray-400 mb-1">Temperature Sensor</h3>
        <div className="flex items-center">
          <StatusIcon type="thermometer" className="text-gray-400 dark:text-gray-500" />
          <span className={`text-lg ${
            room.sensor_available
              ? 'text-green-500 dark:text-green-400'
              : 'text-red-500 dark:text-red-400'
          }`}>
            {room.sensor_available ? 'Available' : 'Unavailable'}
          </span>
        </div>
        {room.sensor_available && (
          <div className="mt-2 text-lg text-gray-600 dark:text-gray-300">
            <p>Current: <span className="font-medium">{room.current_temp?.toFixed(1) ?? 'N/A'}</span>°C</p>
            <p>Target: <span className="font-medium">{room.target_temp?.toFixed(1) ?? 'N/A'}</span>°C</p>
          </div>
        )}
      </div>

      {/* Heater Relay Section */}
      <div className="mb-4">
        <h3 className="text-xl font-medium text-gray-500 dark:text-gray-400 mb-1">Heater Relay</h3>
        <div className="flex items-center mb-2">
          <StatusIcon type="power" className="text-gray-400 dark:text-gray-500" />
          <span className={`text-lg ${
            !room.relay_available
              ? 'text-red-500 dark:text-red-400'
              : isHeaterDisabled
                ? 'text-yellow-500 dark:text-yellow-400'
                : 'text-green-500 dark:text-green-400'
          }`}>
            {!room.relay_available
              ? 'Unavailable'
              : isHeaterDisabled
                ? 'Current: DISABLED'
                : `Current: ${room.relay_state ? 'ON' : 'OFF'}`}
          </span>
        </div>

        <div className="flex flex-col gap-2 mt-2"> {/* Main button container */}
          <div className="flex flex-col sm:flex-row gap-2"> {/* Row 1: Existing Buttons */}
            <button
              onClick={handleRelayToggle}
              disabled={!room.relay_available || isHeaterDisabled}
              className={`px-6 py-3 text-lg rounded transition-colors duration-200 ${
                !room.relay_available || isHeaterDisabled
                  ? 'bg-gray-400 dark:bg-gray-600 text-gray-200 cursor-not-allowed'
                  : room.relay_state
                    ? 'bg-red-600 hover:bg-red-700 text-white'
                    : 'bg-blue-600 hover:bg-blue-700 text-white'
              }`}
            >
              {!room.relay_available
                ? 'Toggle (Unavailable)'
                : isHeaterDisabled
                  ? 'Heater Disabled'
                  : room.relay_state
                    ? 'Turn Off'
                    : 'Turn On'}
            </button>

            <button
              onClick={() => handleHeaterControl(!isHeaterDisabled)}
              disabled={!room.relay_available || isOverrideActive}
              className={`px-6 py-3 text-lg rounded transition-colors duration-200 ${
                !room.relay_available || isOverrideActive
                  ? 'bg-gray-400 dark:bg-gray-600 text-gray-200 cursor-not-allowed'
                  : isHeaterDisabled
                    ? 'bg-green-600 hover:bg-green-700 text-white'
                    : 'bg-yellow-600 hover:bg-yellow-700 text-white'
              }`}
            >
              {!room.relay_available
                ? 'Disable (Unavailable)'
                : isOverrideActive
                  ? 'Override Active'
                  : isHeaterDisabled
                    ? 'Restore Heating'
                    : 'Disable Heater for 2 Hours'}
            </button>
          </div>

          {/* NEW: Temperature Override Controls */}
          {room.relay_available && (
            <div className="mt-2 border-t border-gray-200 dark:border-gray-700 pt-2">
              <h4 className="text-sm font-medium text-gray-500 dark:text-gray-400 mb-1">
                Set Target Override (2 Hours):
              </h4>
              <div className="flex flex-wrap gap-1 items-center">
                {[18, 19, 20, 21, 22, 23].map((temp) => (
                  <button
                    key={temp}
                    onClick={() => onOverrideTemperature(room.id, temp)}
                    disabled={!room.relay_available || isHeaterDisabled || (isOverrideActive && currentOverrideTemp === temp)}
                    className={`px-2 py-1 text-xs rounded font-medium transition-colors duration-150 ${
                      (isOverrideActive && currentOverrideTemp === temp)
                        ? 'bg-green-600 text-white ring-2 ring-green-300'
                        : (!room.relay_available || isHeaterDisabled)
                          ? 'bg-gray-300 dark:bg-gray-500 text-gray-500 dark:text-gray-400 cursor-not-allowed'
                          : 'bg-sky-500 hover:bg-sky-600 text-white dark:bg-sky-600 dark:hover:bg-sky-700'
                    }`}
                    title={`Set target to ${temp}°C for 2 hours`}
                  >
                    {temp}°C
                  </button>
                ))}
                <button
                  onClick={() => onOverrideTemperature(room.id, null)}
                  disabled={!room.relay_available || !isOverrideActive || isHeaterDisabled}
                  className={`px-2 py-1 text-xs rounded font-medium transition-colors duration-150 ${
                    (!room.relay_available || !isOverrideActive || isHeaterDisabled)
                      ? 'bg-gray-300 dark:bg-gray-500 text-gray-500 dark:text-gray-400 cursor-not-allowed'
                      : 'bg-orange-500 hover:bg-orange-600 text-white dark:bg-orange-600 dark:hover:bg-orange-700'
                  }`}
                  title="Clear temperature override"
                >
                  Clear
                </button>
              </div>
            </div>
          )}
          
          {/* Display active override status */}
          {isOverrideActive && room.override_until && typeof room.override_temperature === 'number' && (
            <div className="mt-2 text-sm text-sky-600 dark:text-sky-400">
              Override active: Target {room.override_temperature.toFixed(1)}°C. Ends in {Math.max(0, Math.round((room.override_until * 1000 - Date.now()) / 60000))} min.
            </div>
          )}

          {/* Display heater disabled status (existing) */}
          {isHeaterDisabled && room.disabled_until && (
            <div className="mt-2 text-sm text-yellow-600 dark:text-yellow-400"> {/* Adjusted text size to sm */}
              Automatic restore in: {Math.max(0, Math.round((room.disabled_until * 1000 - Date.now()) / 60000))} minutes
            </div>
          )}
        </div>
      </div>

      {/* Temperature History Section */}
      <div className="mt-6 pt-4 pb-2 border-t border-gray-200 dark:border-gray-700">
        <div className="h-[300px] w-full">
          <TemperatureChart
            roomName={room.name}
            roomData={room} 
            isDarkMode={isDarkMode}
          />
        </div>
      </div>
    </div>
  );
};

export default RoomCard;
