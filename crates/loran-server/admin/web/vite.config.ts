import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  plugins: [
    react(),
    tailwindcss(),
    {
      name: "loran-admin-no-runtime-picomatch",
      apply: "build",
      generateBundle(_options, bundle) {
        for (const output of Object.values(bundle)) {
          if (
            output.type === "chunk" &&
            Object.keys(output.modules).some(
              (id) => /[\\/]node_modules[\\/]picomatch[\\/]/.test(id),
            )
          ) {
            this.error("The Admin UI bundle includes build-only dependency picomatch");
          }
        }
      },
    },
  ],
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
