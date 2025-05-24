/** @type {import('tailwindcss').Config} */
export default {
  content: [
    "./index.html",
    "./src/**/*.{js,ts,jsx,tsx}",
  ],
  darkMode: 'class', // Enable dark mode based on class
  theme: {
    extend: {
      colors: {
        // Status colors
        'status-available': {
          light: '#16a34a', // green-600
          dark: '#4ade80', // green-400
        },
        'status-unavailable': {
          light: '#dc2626', // red-600
          dark: '#ef4444', // red-500
        },
        'status-disabled': {
          light: '#ca8a04', // yellow-600
          dark: '#facc15', // yellow-400
        },
        'status-timer': {
          light: '#ca8a04', // yellow-600
          dark: '#facc15', // yellow-400
        },
        // Button colors
        'button-default': {
          bg: {
            light: '#e5e7eb', // gray-200
            dark: '#6b7280', // gray-500
          },
          hover: {
            light: '#d1d5db', // gray-300
            dark: '#4b5563', // gray-600
          },
        },
        'button-on': {
          bg: {
            light: '#c81e1e', // red-700
            dark: '#dc2626', // red-600
          },
          hover: {
            light: '#b91c1c', // red-800
            dark: '#b91c1c', // red-700
          },
        },
        'button-off': {
          bg: {
            light: '#2563eb', // blue-600
            dark: '#2563eb', // blue-600
          },
          hover: {
            light: '#1d4ed8', // blue-700
            dark: '#1d4ed8', // blue-700
          },
        },
        'button-control-disable': {
          bg: {
            light: '#f59e0b', // yellow-500
            dark: '#a16207', // yellow-700
          },
          hover: {
            light: '#d97706', // yellow-600
            dark: '#854d0e', // yellow-800
          },
        },
        'button-control-restore': {
          bg: {
            light: '#10b981', // green-500
            dark: '#16a34a', // green-600
          },
          hover: {
            light: '#059669', // green-600
            dark: '#15803d', // green-700
          },
        },
      },
      fontFamily: {
        sans: ['Inter', 'sans-serif'],
      },
    },
  },
  plugins: [],
} 