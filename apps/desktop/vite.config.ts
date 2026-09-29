// SPDX-License-Identifier: GPL-3.0-or-later
import { sveltekit } from '@sveltejs/kit/vite';
import tailwindcss from '@tailwindcss/vite';
import { defineConfig } from 'vitest/config';

export default defineConfig({
  plugins: [tailwindcss(), sveltekit()],
  // Tauri expects a fixed port and must see Rust errors in the terminal.
  clearScreen: false,
  server: { port: 5173, strictPort: true },
  envPrefix: ['VITE_', 'TAURI_ENV_'],
  test: {
    include: ['src/**/*.test.ts'],
    environment: 'jsdom',
  },
  resolve: process.env.VITEST ? { conditions: ['browser'] } : undefined,
});
