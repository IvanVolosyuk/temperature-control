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
  const [isDarkMode, setIsDarkMode] = useState(window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches);

  useEffect(() => {
    const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)');
    const handleChange = () => setIsDarkMode(mediaQuery.matches);
    mediaQuery.addEventListener('change', handleChange);
    return () => mediaQuery.removeEventListener('change', handleChange);
  }, []);

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
  return (
    <div className="chart-container relative h-64 md:h-72 lg:h-80">
      <Line
        key={roomName}
        ref={chartRef}
        options={memoizedChartOptions()}
        data={memoizedChartData}
      />
    </div>
  );
};

export default TemperatureChart;
