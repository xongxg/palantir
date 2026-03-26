import { defineConfig, loadEnv } from 'vite'
import react from '@vitejs/plugin-react'
import path from 'path'

export default defineConfig(({ mode }) => {
  const env     = loadEnv(mode, __dirname, '')
  const backend = env.VITE_API_BASE || 'http://localhost:8080'
  const port    = parseInt(env.VITE_PORT || '3000', 10)

  return {
    plugins: [react()],
    resolve: {
      alias: {
        '@': path.resolve(__dirname, './src'),
      },
    },
    server: {
      port,
      proxy: {
        '/api':    { target: backend, changeOrigin: true },
        '/static': { target: backend, changeOrigin: true },
      },
    },
    build: {
      outDir: '../../dist',
      emptyOutDir: true,
    },
  }
})
