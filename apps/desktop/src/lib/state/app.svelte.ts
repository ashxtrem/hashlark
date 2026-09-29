// SPDX-License-Identifier: GPL-3.0-or-later

import { Api, ApiError, discoverEndpoint, type Settings } from '$lib/api/client';

type Connection = 'connecting' | 'ready' | 'needs_login' | 'error';

/** Connection to the engine plus app-wide data (settings). */
class AppState {
  connection = $state<Connection>('connecting');
  error = $state<string | null>(null);
  api = $state<Api | null>(null);
  settings = $state<Settings | null>(null);
  version = $state<string | null>(null);

  async connect(): Promise<void> {
    this.connection = 'connecting';
    this.error = null;
    try {
      const api = new Api(await discoverEndpoint());
      const health = await api.health();
      this.version = health.version;
      if (!api.endpoint.token) {
        this.connection = 'needs_login';
        return;
      }
      this.settings = await api.settings();
      this.api = api;
      this.connection = 'ready';
      applyTheme(this.settings.ui.theme);
    } catch (e) {
      if (e instanceof ApiError && e.status === 401) {
        this.connection = 'needs_login';
        return;
      }
      this.connection = 'error';
      this.error = e instanceof Error ? e.message : String(e);
    }
  }

  /** The API, for code that only runs once connected. */
  get client(): Api {
    if (!this.api) throw new Error('not connected');
    return this.api;
  }

  async saveSettings(next: Settings): Promise<Settings> {
    const saved = await this.client.saveSettings(next);
    this.settings = saved;
    applyTheme(saved.ui.theme);
    return saved;
  }
}

export function applyTheme(theme: Settings['ui']['theme']): void {
  const root = document.documentElement;
  if (theme === 'system') root.removeAttribute('data-theme');
  else root.setAttribute('data-theme', theme);
}

export const app = new AppState();
