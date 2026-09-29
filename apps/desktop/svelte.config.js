// SPDX-License-Identifier: GPL-3.0-or-later
import adapter from '@sveltejs/adapter-static';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

/** @type {import('@sveltejs/kit').Config} */
const config = {
  preprocess: vitePreprocess(),
  kit: {
    // Single-page app: the same static build runs in the Tauri webview and
    // is served by the headless server.
    adapter: adapter({ fallback: 'index.html' }),
    alias: { $components: 'src/lib/components' },
  },
};

export default config;
