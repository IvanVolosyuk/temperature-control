import React from 'react';
import TemperatureChart from './TemperatureChart';
import { RoomState } from '../types'; // Assuming types.ts is in src
import StatusIcon from './StatusIcon';

type RoomCardProps = {
  roomName: string;
  roomApiName: string;
  roomData: RoomState | null;
  onControlRelay: (room: string, state: boolean) => Promise<void>;
  onDisableHeater: (room: string, disable: boolean) => Promise<void>;
  isLoading: boolean;
  isDarkMode: boolean;
};

const RoomCard: React.FC<RoomCardProps> = ({
  roomName,
  roomApiName,
  roomData,
  onControlRelay,
  onDisableHeater,
  isLoading,
  isDarkMode,
}) => {
  const isHeaterDisabled = Boolean(roomData?.disabled_until && Date.now() < roomData.disabled_until * 1000);

  const handleRelayToggle = () => {
    if (!roomData || !roomData.relay_available) return;
    onControlRelay(roomApiName, !roomData.relay_state);
  };

  const handleHeaterControl = (disable: boolean) => {
    if (!roomData || !roomData.relay_available) return;
    onDisableHeater(roomApiName, disable);
  };

  if (isLoading && !roomData) {
    return (
      <div className="bg-white dark:bg-gray-800 shadow-md rounded-lg p-6 animate-pulse">
        <h2 className="text-2xl font-semibold mb-4 text-gray-700 dark:text-gray-300">{roomName}</h2>
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
      <h2 className="text-2xl font-semibold text-gray-700 dark:text-gray-300 mb-4">{roomName}</h2>

      {/* Temperature Sensor Section */}
      <div className="mb-4">
        <h3 className="text-xl font-medium text-gray-500 dark:text-gray-400 mb-1">Temperature Sensor</h3>
        <div className="flex items-center">
          <StatusIcon type="thermometer" className="text-gray-400 dark:text-gray-500" />
          <span className={`text-lg ${
            roomData?.sensor_available
              ? 'text-green-500 dark:text-green-400'
              : 'text-red-500 dark:text-red-400'
          }`}>
            {roomData?.sensor_available ? 'Available' : 'Unavailable'}
          </span>
        </div>
        {roomData?.sensor_available && (
          <div className="mt-2 text-lg text-gray-600 dark:text-gray-300">
            <p>Current: <span className="font-medium">{roomData.current_temp?.toFixed(1) ?? 'N/A'}</span>°C</p>
            <p>Target: <span className="font-medium">{roomData.target_temp?.toFixed(1) ?? 'N/A'}</span>°C</p>
          </div>
        )}
      </div>

      {/* Heater Relay Section */}
      <div className="mb-4">
        <h3 className="text-xl font-medium text-gray-500 dark:text-gray-400 mb-1">Heater Relay</h3>
        <div className="flex items-center mb-2">
          <StatusIcon type="power" className="text-gray-400 dark:text-gray-500" />
          <span className={`text-lg ${
            !roomData?.relay_available
              ? 'text-red-500 dark:text-red-400'
              : isHeaterDisabled
                ? 'text-yellow-500 dark:text-yellow-400'
                : 'text-green-500 dark:text-green-400'
          }`}>
            {!roomData?.relay_available
              ? 'Unavailable'
              : isHeaterDisabled
                ? 'Current: DISABLED'
                : `Current: ${roomData.relay_state ? 'ON' : 'OFF'}`}
          </span>
        </div>

        <div className="flex flex-col sm:flex-row gap-2 mt-2">
          <button
            onClick={handleRelayToggle}
            disabled={!roomData?.relay_available || isHeaterDisabled}
            className={`px-6 py-3 text-lg rounded transition-colors duration-200 ${
              !roomData?.relay_available || isHeaterDisabled
                ? 'bg-gray-400 dark:bg-gray-600 text-gray-200 cursor-not-allowed'
                : roomData?.relay_state
                  ? 'bg-red-600 hover:bg-red-700 text-white'
                  : 'bg-blue-600 hover:bg-blue-700 text-white'
            }`}
          >
            {!roomData?.relay_available
              ? 'Toggle (Unavailable)'
              : isHeaterDisabled
                ? 'Heater Disabled'
                : roomData?.relay_state
                  ? 'Turn Off'
                  : 'Turn On'}
          </button>

          <button
            onClick={() => handleHeaterControl(!isHeaterDisabled)}
            disabled={!roomData?.relay_available}
            className={`px-6 py-3 text-lg rounded transition-colors duration-200 ${
              !roomData?.relay_available
                ? 'bg-gray-400 dark:bg-gray-600 text-gray-200 cursor-not-allowed'
                : isHeaterDisabled
                  ? 'bg-green-600 hover:bg-green-700 text-white'
                  : 'bg-yellow-600 hover:bg-yellow-700 text-white'
            }`}
          >
            {!roomData?.relay_available
              ? 'Disable (Unavailable)'
              : isHeaterDisabled
                ? 'Restore Heating'
                : 'Disable Heater for 2 Hours'}
          </button>
        </div>

        {isHeaterDisabled && roomData?.disabled_until && (
          <div className="mt-2 text-lg text-yellow-500 dark:text-yellow-400">
            Automatic restore in: {Math.max(0, Math.round((roomData.disabled_until * 1000 - Date.now()) / 60000))} minutes
          </div>
        )}
      </div>

      {/* Temperature History Section */}
      <div className="mt-6 pt-4 pb-2 border-t border-gray-200 dark:border-gray-700">
        <div className="h-[300px] w-full">
          <TemperatureChart
            roomName={roomName}
            roomData={roomData}
            isDarkMode={isDarkMode}
          />
        </div>
      </div>
    </div>
  );
};

export default RoomCard;
