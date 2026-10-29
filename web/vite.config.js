import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

// The dashboard is served from /app/ by the same binary that serves the public
// pages, so every asset URL must be prefixed and the output must not be hashed
// into a directory the Rust embed does not know about.
export default defineConfig({
  base: '/app/',
  plugins: [svelte()],
  build: {
    outDir: 'dist',
    emptyOutDir: true,
    assetsDir: 'assets',
    target: 'es2022',
    rollupOptions: {
      output: {
        entryFileNames: 'app-[hash].js',
        chunkFileNames: 'chunk-[hash].js',
        assetFileNames: 'app-[hash][extname]'
      }
    }
  },
  server: {
    port: 5174,
    proxy: {
      '/api': 'http://127.0.0.1:8090',
      '/l': 'http://127.0.0.1:8090'
    }
  }
});
