import { defineConfig } from "vite-plus";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { resolve } from "path";

const asahiPort = process.env.ASAHI_PORT;
const asahiApiUrl =
  process.env.ASAHI_API_URL ??
  process.env.VITE_ASAHI_API_URL ??
  (asahiPort ? `http://127.0.0.1:${asahiPort}` : "http://localhost:8000");

export default defineConfig({
  plugins: [react(), tailwindcss()],
  server: {
    proxy: {
      "/api": asahiApiUrl,
    },
  },
  resolve: {
    alias: {
      "@": resolve(import.meta.dirname, "src"),
    },
  },
  fmt: {},
  lint: { options: { typeAware: true, typeCheck: true } },
});
