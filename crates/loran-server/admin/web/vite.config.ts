import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  base: "/admin/",
  server: {
    host: "::",
    allowedHosts: ["loran.example.com"],
    proxy: {
      "/admin/api": "http://localhost:80",
      "/admin/health": "http://localhost:80",
    },
  },
  build: {
    outDir: "dist",
    emptyOutDir: true,
  },
});
