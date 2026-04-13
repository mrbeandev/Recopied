import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// https://vite.dev/config/
export default defineConfig({
	plugins: [react(), tailwindcss()],
	clearScreen: false,
	server: {
		strictPort: true,
		watch: {
			ignored: ["**/flatpak-build-dir/**", "**/flatpak-repo/**", "**/.flatpak-builder/**"],
		},
	},
});
