// rill/packages/sandbox-web/vite.config.ts
import { defineConfig } from 'vite';

export default defineConfig({
  server: {
    port: 5173,
    host: '127.0.0.1', // Ensure Vite binds to the correct host
  },
});
