import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// https://vite.dev/config/
export default defineConfig({
  plugins: [react()],
  server: {
    proxy: {
      '/api': {
        target: 'http://localhost:8080',
        changeOrigin: true,
      },
      '/ws': { // New rule for WebSocket
        target: 'http://localhost:8080', // Target the HTTP address, Vite handles WS upgrade
        changeOrigin: true,
        ws: true, // Enable WebSocket proxying
      }
    }
  }
})
