import { defineConfig } from 'vite'
import { svelte } from '@sveltejs/vite-plugin-svelte'
import { svelteTesting } from '@testing-library/svelte/vite'

// https://vite.dev/config/
export default defineConfig({
  plugins: [svelte(), svelteTesting()],
  build: {
    chunkSizeWarningLimit: 2500, // omakase-player is 2.2MB, mediabunny 492KB — both are large but necessary
    rollupOptions: {
      output: {
        manualChunks: {
          'omakase-player': ['@byomakase/omakase-player'],
          'mediabunny': ['mediabunny'],
        },
      },
    },
  },
  test: {
    environment: 'jsdom',
    globals: true,
    setupFiles: ['src/__tests__/setup.ts'],
    exclude: ['research/**', 'node_modules/**'],
  },
})
