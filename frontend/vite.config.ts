import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// https://vite.dev/config/
export default defineConfig({
  plugins: [react()],
  build: {
    // The `gl` vendor chunk (deck.gl + maplibre-gl, ~1.7 MB) is the WebGL map
    // stack. It powers the default Runs-tab map, so it must load on first paint
    // and cannot be split below 500 kB — raise the limit above it so the
    // (cosmetic) size warning only fires on genuinely unexpected growth.
    chunkSizeWarningLimit: 1800,
    rollupOptions: {
      output: {
        // Split the heavy vendors into their own long-cached chunks so the
        // app-code chunk stays small and each library is fetched/cached once.
        // (rolldown-vite honours rollup's `manualChunks`.)
        manualChunks(id: string) {
          if (!id.includes('node_modules')) return
          if (/[\\/]node_modules[\\/](deck\.gl|@deck\.gl|@luma\.gl|@math\.gl|@loaders\.gl|maplibre-gl|react-map-gl|@mapbox)[\\/]/.test(id)) {
            return 'gl'
          }
          if (/[\\/]node_modules[\\/](recharts|d3-[^\\/]+|victory-vendor|internmap)[\\/]/.test(id)) {
            return 'charts'
          }
          if (/[\\/]node_modules[\\/](react|react-dom|scheduler)[\\/]/.test(id)) {
            return 'react'
          }
        },
      },
    },
  },
})
