import React from 'react';
import TemperatureChart from './TemperatureChart';
import { RoomState } from '../types'; // Assuming types.ts is in src

interface RoomCardProps {
  roomName: string;
  roomApiName: string; // "bedroom" or "kids_bedroom"
  roomData: RoomState | null;
  onControlRelay: (roomApiName: string, state: boolean) => Promise<void>;
  onDisableHeater: (roomApiName: string, disable: boolean) => Promise<void>;
  isLoading: boolean; // True if initial data for this card is loading or global action pending
}

const RoomCard: React.FC<RoomCardProps> = ({
  roomName,
  roomApiName,
  roomData,
  onControlRelay,
  onDisableHeater,
  isLoading,
}) => {
  const handleRelayToggle = () => {
    if (!roomData || !roomData.relay_available) return;
    // New state is the opposite of current roomData.relay_state
    onControlRelay(roomApiName, !roomData.relay_state);
  };

  const handleHeaterControl = (disable: boolean) => {
    if (!roomData || !roomData.relay_available) return; // Or sensor_available?
    onDisableHeater(roomApiName, disable);
  };

  const currentTimestamp = Date.now() / 1000; // Current time in seconds for disabled_until check

  const isHeaterGloballyDisabled = roomData?.disabled_until !== null && roomData?.disabled_until !== undefined && roomData.disabled_until > currentTimestamp;
  const sensorText = roomData?.sensor_available ? 'Available' : 'Unavailable';
  const relayText = roomData?.relay_available ? (isHeaterGloballyDisabled ? 'DISABLED' : (roomData.relay_state ? 'ON' : 'OFF')) : 'Unavailable';

  const sensorStatusColor = roomData?.sensor_available ? 'text-green-500 dark:text-green-400' : 'text-red-500 dark:text-red-400';
  let relayStatusColor = 'text-gray-500 dark:text-gray-400';
  if (roomData?.relay_available) {
    if (isHeaterGloballyDisabled) {
        relayStatusColor = 'text-yellow-500 dark:text-yellow-400';
    } else {
        relayStatusColor = roomData.relay_state ? 'text-green-500 dark:text-green-400' : 'text-red-500 dark:text-red-400';
    }
  } else {
    relayStatusColor = 'text-red-500 dark:text-red-400';
  }


  const relayToggleButtonText = isHeaterGloballyDisabled ? 'Heater Disabled' : (roomData?.relay_state ? 'Turn Off' : 'Turn On');
  const controlButtonText = isHeaterGloballyDisabled ? 'Restore Heating' : 'Disable Heater (2h)';

  const relayToggleButtonClasses = `button w-full sm:w-auto ${
    isHeaterGloballyDisabled || !roomData?.relay_available
      ? 'button-disabled'
      : roomData?.relay_state
      ? 'button-relay-on' // You'll need to define this style
      : 'button-relay-off' // You'll need to define this style
  }`;

  const controlButtonClasses = `button w-full sm:w-auto ${
    !roomData?.relay_available // Assuming sensor availability implies relay might be controllable
      ? 'button-disabled'
      : isHeaterGloballyDisabled
      ? 'button-control-restore' // Define this
      : 'button-control-disable' // Define this
  }`;
  
  const getTimerStatusText = () => {
    if (!isHeaterGloballyDisabled || !roomData?.disabled_until) return '';
    const timeLeftSeconds = Math.max(0, Math.round(roomData.disabled_until - currentTimestamp));
    const minutes = Math.floor(timeLeftSeconds / 60);
    return `Automatic restore in: ${minutes} minutes`;
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
    <div className="bg-white dark:bg-gray-800 shadow-md rounded-lg p-6">
      <h2 className="text-2xl font-semibold mb-4 text-gray-700 dark:text-gray-300">{roomName}</h2>

      {/* Temperature Display Section */}
      <div className="mb-4">
        <h3 className="text-xl font-medium text-gray-600 dark:text-gray-400 mb-1">Temperature Sensor</h3>
        <div className={`text-lg ${sensorStatusColor}`}>
          <p>Status: <span id={`${roomApiName}-sensor-text`}>{sensorText}</span></p>
        </div>
        {roomData?.sensor_available && (
          <div id={`${roomApiName}-sensor-data`} className="mt-1 text-lg">
            <p>Current: <span id={`${roomApiName}-current-temp`} className="font-medium text-gray-700 dark:text-gray-300">{roomData.current_temp.toFixed(1)}</span>°C</p>
            <p>Target: <span id={`${roomApiName}-target-temp`} className="font-medium text-gray-700 dark:text-gray-300">{roomData.target_temp.toFixed(1)}</span>°C</p>
          </div>
        )}
      </div>

      {/* Heater Relay Section */}
      <div className="mb-4">
        <h3 className="text-xl font-medium text-gray-600 dark:text-gray-400 mb-1">Heater Relay</h3>
        <div className={`text-lg ${relayStatusColor}`}>
          <p>Status: <span id={`${roomApiName}-relay-text`}>{relayText}</span></p>
        </div>
        <div className="mt-2 flex flex-col sm:flex-row gap-2">
          <button
            id={`${roomApiName}-relay-toggle`}
            className={relayToggleButtonClasses}
            onClick={handleRelayToggle}
            disabled={isHeaterGloballyDisabled || !roomData?.relay_available || isLoading}
          >
            {relayToggleButtonText}
          </button>
          <button
            id={`${roomApiName}-control-btn`}
            className={controlButtonClasses}
            onClick={() => handleHeaterControl(!isHeaterGloballyDisabled)}
            disabled={!roomData?.relay_available || isLoading}
          >
            {controlButtonText}
          </button>
        </div>
        {isHeaterGloballyDisabled && (
             <div id={`${roomApiName}-timer-status`} className="mt-2 text-sm text-yellow-600 dark:text-yellow-400">
                {getTimerStatusText()}
             </div>
        )}
      </div>

      {/* Temperature History Section */}
      <div>
        {/* Title will be moved to TemperatureChart component */}
        <TemperatureChart roomName={roomName} roomData={roomData} />
      </div>
    </div>
  );
};

export default RoomCard;
