import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

export default defineConfig({
  plugins: [svelte()],
  server: {
    port: 5173,
    proxy: {
      '/api': 'http://localhost:30080',
    },
  },
  build: {
    outDir: 'dist',
    emptyOutDir: true,
  },
});
