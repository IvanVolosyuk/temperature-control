import React, { useEffect, useRef, useState } from 'react';
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
  TimeScale, // For time series data
  ChartOptions,
  ChartData
} from 'chart.js';
import zoomPlugin from 'chartjs-plugin-zoom';
// While chartjs-adapter-date-fns is installed, Chart.js v3+ can often handle timestamps directly with TimeScale
// import 'chartjs-adapter-date-fns'; // Uncomment if explicit adapter needed for formatting or parsing

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
  roomName: string;
  roomData: RoomState | null;
}

const TemperatureChart: React.FC<TemperatureChartProps> = ({ roomName, roomData }) => {
  const chartRef = useRef<ChartJS<'line'> | null>(null);
  const [isDarkMode, setIsDarkMode] = useState(window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches);

  useEffect(() => {
    const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)');
    const handleChange = () => setIsDarkMode(mediaQuery.matches);
    mediaQuery.addEventListener('change', handleChange);
    return () => mediaQuery.removeEventListener('change', handleChange);
  }, []);

  const makeChartOptions = (): ChartOptions<'line'> => {
    const gridColor = isDarkMode ? 'rgba(255, 255, 255, 0.1)' : 'rgba(0, 0, 0, 0.1)';
    const textColor = isDarkMode ? 'rgba(255, 255, 255, 0.85)' : 'rgba(0, 0, 0, 0.85)';

    return {
      responsive: true,
      maintainAspectRatio: false,
      scales: {
        x: {
          type: 'time',
          time: {
            unit: 'minute', // Adjust based on data density
            tooltipFormat: 'HH:mm:ss', // e.g., 14:30:00
            displayFormats: {
              minute: 'HH:mm', // e.g., 14:30
              hour: 'HH:00', // e.g., 14:00
            },
          },
          ticks: {
            color: textColor,
            maxTicksLimit: 8,
            autoSkip: true,
          },
          title: {
            display: true,
            text: 'Time',
            color: textColor,
            font: { size: 14 },
          },
          grid: { color: gridColor },
        },
        y: {
          title: {
            display: true,
            text: 'Temperature (°C)',
            color: textColor,
            font: { size: 14 },
          },
          ticks: {
            color: textColor,
            stepSize: 1, // Adjust as needed
            callback: (value) => `${Number(value).toFixed(1)}°C`,
          },
          grid: { color: gridColor },
        },
      },
      plugins: {
        legend: {
          display: true,
          position: 'top',
          labels: { color: textColor, font: { size: 14 } },
        },
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
          pan: { enabled: true, mode: 'x' },
          zoom: { mode: 'x', wheel: { enabled: true }, pinch: { enabled: true } },
        },
      },
    };
  };

  const chartData: ChartData<'line'> = {
    datasets: [
      {
        label: 'Current Temperature',
        data: roomData?.temperature_history.map(p => ({ x: p.timestamp * 1000, y: p.temperature })) || [],
        borderColor: isDarkMode ? 'rgba(100, 180, 243, 0.7)' : 'rgba(59, 130, 246, 0.7)',
        backgroundColor: isDarkMode ? 'rgba(100, 180, 243, 0.4)' : 'rgba(59, 130, 246, 0.4)',
        pointRadius: 3,
        pointBackgroundColor: (context) => {
          const index = context.dataIndex;
          const pointData = roomData?.temperature_history[index];
          if (!pointData) return isDarkMode ? 'rgba(100, 180, 243, 0.7)' : 'rgba(59, 130, 246, 0.7)';
          
          const pointHeaterOnColor = isDarkMode ? 'rgba(255, 80, 80, 1)' : 'rgb(239, 68, 68)';
          const pointHeaterOffColor = isDarkMode ? 'rgba(80, 80, 255, 1)' : 'rgb(59, 130, 246)';
          const disabledOnColor = isDarkMode ? 'rgba(180, 140, 140, 0.6)' : 'rgba(130, 100, 100, 0.6)';
          const disabledOffColor = isDarkMode ? 'rgba(140, 140, 180, 0.6)' : 'rgba(100, 100, 130, 0.6)';

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
  
  // Placeholder for zoom functions - these would be called by buttons in RoomCard
  // For now, they are not connected.
  // const zoomIn = () => chartRef.current?.zoom(1.1);
  // const zoomOut = () => chartRef.current?.zoom(0.9);
  // const resetZoom = () => chartRef.current?.resetZoom();

  if (!roomData || roomData.temperature_history.length === 0) {
    return (
      <div className="chart-container relative h-64 md:h-72 lg:h-80 flex items-center justify-center text-gray-500 dark:text-gray-400">
        No temperature data available.
      </div>
    );
  }

  return (
    <div className="chart-container relative h-64 md:h-72 lg:h-80">
      <Line ref={chartRef} options={makeChartOptions()} data={chartData} />
    </div>
  );
};

export default TemperatureChart;
