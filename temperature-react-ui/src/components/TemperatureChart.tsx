import React, { useEffect, useRef, useState, useMemo} from 'react'; // Added useMemo, useCallback
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
  ChartData,
  ScaleOptionsByType
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
  const [isDarkMode, setIsDarkMode] = useState(window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches);

  // Update chartOptions when isDarkMode changes
  useEffect(() => {
    setChartOptions(generateChartOptions(isDarkMode));
  }, [isDarkMode]);

  // Effect to listen to OS theme changes for isDarkMode state
  useEffect(() => {
    const mediaQuery = window.matchMedia('(prefers-color-scheme: dark)');
    const handleChange = () => setIsDarkMode(mediaQuery.matches);
    mediaQuery.addEventListener('change', handleChange);
    return () => mediaQuery.removeEventListener('change', handleChange);
  }, []);

  // Effect for auto-scrolling
  useEffect(() => {
    const chart = chartRef.current;
    if (!chart || !chart.options?.scales?.x) {
      return;
    }

    const xScale = chart.options.scales.x as ScaleOptionsByType<'time'>;
    const currentXMin = xScale.min as number;
    const currentXMax = xScale.max as number;

    let prevLastVisible = true;
    const dataset = chart.data.datasets[0];
    if (dataset.data.length > 0) {
      const firstPoint = dataset.data[0] as { x: number };
      if (firstPoint && typeof firstPoint.x === 'number') {
        prevLastVisible = currentXMin <= firstPoint.x && firstPoint.x <= currentXMax;
      }
    }

    const history = roomData?.temperature_history;
    if (history && history.length > 0) {
      const lastDataPoint = history[history.length - 1];
      const newLastTimestamp = lastDataPoint.timestamp * 1000;
      const newLastVisible = newLastTimestamp >= currentXMin && newLastTimestamp <= currentXMax;

      if (prevLastVisible && !newLastVisible) {
        const viewWidth = currentXMax - currentXMin;
        const offset = viewWidth * OFFSET_FRACTION;
        const move = newLastTimestamp + offset - currentXMax;
        
        if (chart.options.scales?.x) {
          const xScale = chart.options.scales.x as ScaleOptionsByType<'time'>;
          xScale.min = currentXMin + move;
          xScale.max = currentXMax + move;
          chart.update('none');
        }
      }
    }
  }, [roomData]);

  // Function to generate chart options dynamically based on dark mode
  // This will be used for initial state and when dark mode changes.
  const generateChartOptions = (currentIsDarkMode: boolean): ChartOptions<'line'> => {
    const gridColor = currentIsDarkMode ? 'rgba(255, 255, 255, 0.1)' : 'rgba(0, 0, 0, 0.1)';
    const textColor = currentIsDarkMode ? 'rgba(255, 255, 255, 0.85)' : 'rgba(0, 0, 0, 0.85)';

    const xScalesConfig: any = {
      min: Date.now() - 3600 * 1000,
      max: Date.now() + OFFSET_FRACTION * 3600 * 1000,
      type: 'time',
      time: {
        unit: 'minute',
        tooltipFormat: 'HH:mm:ss',
        displayFormats: { minute: 'HH:mm', hour: 'HH:00' },
      },
      ticks: { color: textColor, maxTicksLimit: 8, autoSkip: true },
      title: { display: true, text: 'Time', color: textColor, font: { size: 14 } },
      grid: { color: gridColor },
    };

    return {
      responsive: true,
      maintainAspectRatio: false,
      scales: {
        x: xScalesConfig,
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
          pan: { enabled: true, mode: 'x', threshold: 5 },
          zoom: { mode: 'x', wheel: { enabled: true, speed: 0.1 }, pinch: { enabled: true }, drag: { enabled: false } },
        },
      },
    };
  };

  // Initialize chartOptions state using the isDarkMode state
  const [chartOptions, setChartOptions] = useState<ChartOptions<'line'>>(() => generateChartOptions(isDarkMode));

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
  }, [roomData, isDarkMode]);

  const handleZoom = (hours: number | 'all') => {
    const history = roomData?.temperature_history;
    const now = Date.now();
    let newMinTime: number;
    let newMaxTime: number;

    const chart = chartRef.current;
    if (!chart?.options?.scales?.x) return;
    
    const xScale = chart.options.scales.x as ScaleOptionsByType<'time'>;

    if (hours === 'all') {
      if (history && history.length > 0) {
        newMinTime = history[0].timestamp * 1000;
        const dataRange = now - newMinTime;
        const offset = dataRange * OFFSET_FRACTION;
        newMinTime -= offset;
        newMaxTime = now + offset;
      } else {
        // No data, but 'All' selected - default to 1 hour
        newMinTime = now - 1 * 60 * 60 * 1000;
        const offset = (1 * 60 * 60 * 1000) * OFFSET_FRACTION;
        newMinTime -= offset;
        newMaxTime = now + offset;
      }
    } else {
      // Specific hour range (e.g., 1h)
      newMinTime = now - hours * 60 * 60 * 1000;
      const dataRange = now - newMinTime;
      const offset = dataRange * OFFSET_FRACTION;
      newMinTime -= offset;
      newMaxTime = now + offset;
    }

    xScale.min = newMinTime;
    xScale.max = newMaxTime;
    chart.update();
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
          options={chartOptions}
          data={memoizedChartData}
        />
      </div>
    </>
  );
};

export default TemperatureChart;
