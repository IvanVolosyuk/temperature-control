import React, { useEffect, useRef, useState, useMemo, useCallback } from 'react'; // Added useMemo, useCallback
import { Line } from 'react-chartjs-2';
import {
  Chart as ChartJS,
  CategoryScale,
  LinearScale,
  PointElement,
  LineElement,
  Title,
  Tooltip,
  Legend,
  TimeScale,
  ChartOptions,
  ChartData
} from 'chart.js';
import 'chartjs-adapter-date-fns';
import zoomPlugin from 'chartjs-plugin-zoom';
import { RoomState } from '../types';

const OFFSET_FRACTION = 0.03; // Ported from TC.jsx

ChartJS.register(
  CategoryScale,
  LinearScale,
  PointElement,
  LineElement,
  Title,
  Tooltip,
  Legend,
  TimeScale,
  zoomPlugin
);

interface TemperatureChartProps {
  roomName: string; // Keep roomName for potential unique chart IDs if ever needed, or logging
  roomData: RoomState | null;
}

const TemperatureChart: React.FC<TemperatureChartProps> = ({ roomName, roomData }) => {
  const chartRef = useRef<ChartJS<'line', ChartData<'line'>['datasets'][0]['data'], string> | null>(null);
  const prevLastTimestampRef = useRef<number | null>(null); // For auto-scroll logic from TC.jsx
  const [isDarkMode, setIsDarkMode] = useState(window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches);

  useEffect(() => {
    const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)');
    const handleChange = () => setIsDarkMode(mediaQuery.matches);
    mediaQuery.addEventListener('change', handleChange);
    return () => mediaQuery.removeEventListener('change', handleChange);
  }, []);

  // Effect for initial zoom AND auto-scrolling, adapted from TC.jsx
  useEffect(() => {
    if (chartRef.current && roomData?.temperature_history && roomData.temperature_history.length > 0) {
      const chart = chartRef.current;
      const historyData = roomData.temperature_history;
      const currentXMin = chart.options?.scales?.x?.min as number | undefined;
      const currentXMax = chart.options?.scales?.x?.max as number | undefined;

      const lastDataPoint = historyData[historyData.length - 1];
      const newLastTimestamp = lastDataPoint.timestamp * 1000;

      if (prevLastTimestampRef.current === null) {
        // Initial load: set default zoom (e.g., 1 hour)
        const now = Date.now();
        if (chart.options && chart.options.scales && chart.options.scales.x) {
          chart.options.scales.x.min = now - 1 * 60 * 60 * 1000; // Default to 1 hour
          chart.options.scales.x.max = now + (1 * 60 * 60 * 1000) * OFFSET_FRACTION * 2; // Add buffer
          chart.update('none');
        }
      } else if (prevLastTimestampRef.current !== null && currentXMin !== undefined && currentXMax !== undefined) {
        // Auto-scroll logic based on TC.jsx
        const prevTimestamp = prevLastTimestampRef.current;
        const prevLastVisible = prevTimestamp >= currentXMin && prevTimestamp <= currentXMax;
        const newLastVisible = newLastTimestamp >= currentXMin && newLastTimestamp <= currentXMax;

        if (prevLastVisible && !newLastVisible && newLastTimestamp > prevTimestamp) {
          // The new point is to the right and out of view, prev point was in view, and it's a newer point.
          // This implies newLastTimestamp > currentXMax.
          const viewWidth = currentXMax - currentXMin;
          const offset = viewWidth * OFFSET_FRACTION;
          
          // Calculate move to bring the newLastTimestamp into view with an offset, maintaining viewWidth (TC.jsx style)
          const move = newLastTimestamp + offset - currentXMax;

          if (chart.options && chart.options.scales && chart.options.scales.x) {
            chart.options.scales.x.min = currentXMin + move;
            chart.options.scales.x.max = currentXMax + move;
            chart.update('none'); 
          }
        }
      }
      prevLastTimestampRef.current = newLastTimestamp;
    } else if (!roomData?.temperature_history || roomData.temperature_history.length === 0) {
      prevLastTimestampRef.current = null; // Reset if data is cleared
      // Optionally, reset zoom to a default if chart exists
      if (chartRef.current) {
        const chart = chartRef.current;
        const now = Date.now();
        if (chart.options && chart.options.scales && chart.options.scales.x) {
          chart.options.scales.x.min = now - 1 * 60 * 60 * 1000; 
          chart.options.scales.x.max = now + (1 * 60 * 60 * 1000) * OFFSET_FRACTION * 2;
          // chart.update('none'); // Avoid update if no data, could be jarring
        }
      }
    }
  }, [roomData, OFFSET_FRACTION]); // Dependencies: roomData and OFFSET_FRACTION (though constant, good practice if it could change)

  // Memoize chart options
  const memoizedChartOptions = useCallback((): ChartOptions<'line'> => {
    const gridColor = isDarkMode ? 'rgba(255, 255, 255, 0.1)' : 'rgba(0, 0, 0, 0.1)';
    const textColor = isDarkMode ? 'rgba(255, 255, 255, 0.85)' : 'rgba(0, 0, 0, 0.85)';

    return {
      responsive: true,
      maintainAspectRatio: false,
      scales: {
        x: {
          type: 'time',
          time: {
            unit: 'minute',
            tooltipFormat: 'HH:mm:ss',
            displayFormats: { minute: 'HH:mm', hour: 'HH:00' },
          },
          ticks: { color: textColor, maxTicksLimit: 8, autoSkip: true },
          title: { display: true, text: 'Time', color: textColor, font: { size: 14 } },
          grid: { color: gridColor },
        },
        y: {
          title: { display: true, text: 'Temperature (°C)', color: textColor, font: { size: 14 } },
          ticks: { color: textColor, stepSize: 1, callback: (value) => `${Number(value).toFixed(1)}°C` },
          grid: { color: gridColor },
        },
      },
      plugins: {
        legend: { display: true, position: 'top', labels: { color: textColor, font: { size: 14 } } },
        tooltip: {
          callbacks: {
            title: (context) => {
              const timestamp = context[0].parsed.x;
              return new Date(timestamp).toLocaleTimeString('default', { hour: '2-digit', minute: '2-digit', second: '2-digit' });
            },
            label: (context) => `${context.dataset.label}: ${Number(context.parsed.y).toFixed(1)}°C`,
          },
        },
        zoom: {
          pan: { enabled: true, mode: 'x', threshold: 5 }, // Added threshold
          zoom: { mode: 'x', wheel: { enabled: true, speed: 0.1 }, pinch: { enabled: true }, drag: { enabled: false } }, // Adjusted speed, disabled drag zoom
        },
      },
      animation: false, // Potentially disable animation to prevent issues during rapid updates
      // If using react-chartjs-2, direct manipulation of chart instance for destroy might not be needed
      // as the component should handle it. The key is stable props.
    };
  }, [isDarkMode]); // Dependency: isDarkMode

  // Memoize chart data
  const memoizedChartData = useMemo((): ChartData<'line'> => {
    const pointHeaterOnColor = isDarkMode ? 'rgba(255, 80, 80, 1)' : 'rgb(239, 68, 68)';
    const pointHeaterOffColor = isDarkMode ? 'rgba(80, 80, 255, 1)' : 'rgb(59, 130, 246)';
    const disabledOnColor = isDarkMode ? 'rgba(180, 140, 140, 0.6)' : 'rgba(130, 100, 100, 0.6)';
    const disabledOffColor = isDarkMode ? 'rgba(140, 140, 180, 0.6)' : 'rgba(100, 100, 130, 0.6)';

    return {
      datasets: [
        {
          label: 'Current Temperature',
          data: roomData?.temperature_history.map(p => ({ x: p.timestamp * 1000, y: p.temperature })) || [],
          borderColor: isDarkMode ? 'rgba(100, 180, 243, 0.7)' : 'rgba(59, 130, 246, 0.7)',
          backgroundColor: isDarkMode ? 'rgba(100, 180, 243, 0.4)' : 'rgba(59, 130, 246, 0.4)',
          pointRadius: 3,
          pointBackgroundColor: (context: any) => { // Added any type for context for now
            const index = context.dataIndex;
            const pointData = roomData?.temperature_history[index];
            if (!pointData) return isDarkMode ? 'rgba(100, 180, 243, 0.7)' : 'rgba(59, 130, 246, 0.7)';
            
            const color = pointData.heater_on ? pointHeaterOnColor : pointHeaterOffColor;
            const disabledColor = pointData.heater_on ? disabledOnColor : disabledOffColor;
            return pointData.is_disabled ? disabledColor : color;
          },
          tension: 0.1,
        },
        {
          label: 'Target Temperature',
          data: roomData?.temperature_history.map(p => ({ x: p.timestamp * 1000, y: p.target })) || [],
          borderColor: isDarkMode ? 'rgba(200, 200, 150, 0.7)' : 'rgb(150, 150, 100, 0.7)',
          backgroundColor: isDarkMode ? 'rgba(90, 100, 90, 0.4)' : 'rgba(50, 50, 50, 0.4)',
          pointRadius: 0,
          tension: 0.1,
        },
      ],
    };
  }, [roomData, isDarkMode]); // Dependencies: roomData, isDarkMode

  // The useEffect for chart instance destruction is generally handled by react-chartjs-2 <Line /> component itself
  // when its key changes or it unmounts. Explicitly destroying might conflict.
  // However, if issues persist, manual destruction in a useEffect cleanup is a fallback.
  // For now, rely on react-chartjs-2's handling with memoized props.

  // useEffect(() => {
  //   const chart = chartRef.current;
  //   return () => {
  //     if (chart) {
  //       console.log("Destroying chart instance for room:", roomName);
  //       chart.destroy();
  //     }
  //   };
  // }, [roomName]); // Keying by roomName if charts are truly independent and replaced

  if (!roomData || roomData.temperature_history.length === 0) {
    return (
      <div className="chart-container relative h-64 md:h-72 lg:h-80 flex items-center justify-center text-gray-500 dark:text-gray-400">
        No temperature data available for {roomName}.
      </div>
    );
  }
  
  // Adding a key to the Line component can help React differentiate chart instances
  // if multiple charts could be rendered and swapped. For a stable chart per room, might not be strictly needed
  // if options/data props are stable.

  const handleZoom = (hours: number | 'all') => {
    if (chartRef.current && roomData?.temperature_history && roomData.temperature_history.length > 0) {
      const chart = chartRef.current;
      const now = Date.now();
      let minTime;

      if (hours === 'all') {
        // Find the earliest timestamp in the data
        minTime = roomData.temperature_history[0].timestamp * 1000;
      } else {
        minTime = now - hours * 60 * 60 * 1000;
      }
      const offset = (now - minTime) * OFFSET_FRACTION;

      // Ensure options and scales are defined before trying to set min/max
      if (chart.options && chart.options.scales && chart.options.scales.x) {
        chart.options.scales.x.min = minTime - offset;
        chart.options.scales.x.max = now + offset;
        chart.update('none'); // 'none' for no animation, as in TC.jsx
      } else {
        console.error('Chart options or scales are not defined for zoom.');
      }
    } else if (chartRef.current && hours !== 'all') {
      // Handle case with no history data but a specific time range (e.g., 1h)
      // This will show an empty chart zoomed to the last 'hours' period.
      const chart = chartRef.current;
      const now = Date.now();
      const minTime = now - hours * 60 * 60 * 1000;
      const offset = (now - minTime) * OFFSET_FRACTION;
      if (chart.options && chart.options.scales && chart.options.scales.x) {
        chart.options.scales.x.min = minTime - offset;
        chart.options.scales.x.max = now + offset;
        chart.update('none');
      }
    }
  };

  return (
    <>
      <div className="flex justify-between items-center mb-2">
        <h3 className="text-xl font-medium text-gray-600 dark:text-gray-400">Temperature History</h3>
        <div className="chart-zoom-buttons flex gap-1">
          <button type="button" className="button-chart-zoom px-3 py-1 text-sm" onClick={() => handleZoom(1)}>1h</button>
          <button type="button" className="button-chart-zoom px-3 py-1 text-sm" onClick={() => handleZoom('all')}>All</button>
        </div>
      </div>
      <div className="chart-container relative h-64 md:h-72 lg:h-80">
        <Line
          key={roomName}
          ref={chartRef}
          options={memoizedChartOptions()}
          data={memoizedChartData}
        />
      </div>
    </>
  );
};

export default TemperatureChart;
