// SPDX-License-Identifier: GPL-3.0-or-later

//! Typed access to the Hashlark HTTP API.

import createClient, { type Middleware } from 'openapi-fetch';
import type { components, paths } from './schema';
import { SseParser } from './sse';
import { isTauri } from '$lib/platform';

export type Schemas = components['schemas'];
export type MergedResult = Schemas['MergedResult'];
export type SearchResult = Schemas['SearchResult'];
export type SearchEvent = Schemas['SearchEvent'];
export type SearchQuery = Schemas['SearchQuery'];
export type Category = Schemas['Category'];
export type SortOrder = Schemas['SortOrder'];
export type ProviderView = Schemas['ProviderView'];
export type Settings = Schemas['Settings'];
export type DownloadTarget = Schemas['DownloadTarget'];
export type HistoryEntry = Schemas['HistoryEntry'];
export type TestReport = Schemas['TestReport'];
export type ErrorKind = Schemas['ErrorKind'];
export type SettingView = Schemas['SettingView'];
export type DefinitionCheck = Schemas['DefinitionCheck'];
export type StoredDefinition = Schemas['StoredDefinition'];
export type RepoView = Schemas['RepoView'];
export type SyncReport = Schemas['SyncReport'];
export type Favorite = Schemas['Favorite'];

/** Where the API lives and how to authenticate. */
export interface Endpoint {
  baseUrl: string;
  token: string | null;
}

/** An error response from the API, or a network failure (`code: network`). */
export class ApiError extends Error {
  constructor(
    public readonly status: number,
    public readonly code: string,
    message: string,
    /** For `invalid_definition`: every problem found. */
    public readonly details: string[] = [],
  ) {
    super(message);
    this.name = 'ApiError';
  }
}

const TOKEN_KEY = 'hashlark.token';

/** Token for headless (browser) mode, remembered for this browser session. */
export function storedToken(): string | null {
  try {
    return sessionStorage.getItem(TOKEN_KEY);
  } catch {
    return null;
  }
}

export function storeToken(token: string | null): void {
  try {
    if (token) sessionStorage.setItem(TOKEN_KEY, token);
    else sessionStorage.removeItem(TOKEN_KEY);
  } catch {
    // Storage unavailable (private mode): the token lives in memory only.
  }
}

/** Finds the API: the desktop app's in-process server, a dev override, or
 * the server that served this page (headless mode). */
export async function discoverEndpoint(): Promise<Endpoint> {
  if (isTauri()) {
    const { invoke } = await import('@tauri-apps/api/core');
    const ep = await invoke<{ base_url: string; token: string }>('api_endpoint');
    return { baseUrl: ep.base_url, token: ep.token };
  }
  const devUrl = import.meta.env.VITE_API_URL as string | undefined;
  if (devUrl) {
    return { baseUrl: devUrl, token: (import.meta.env.VITE_API_TOKEN as string) ?? null };
  }
  return { baseUrl: `${location.origin}/api/v1`, token: storedToken() };
}

async function errorFrom(response: Response): Promise<ApiError> {
  try {
    const body = await response.json();
    return new ApiError(response.status, body?.error?.code ?? 'error', body?.error?.message ?? response.statusText);
  } catch {
    return new ApiError(response.status, 'error', response.statusText || `HTTP ${response.status}`);
  }
}

function unwrap<T>(result: { data?: T; error?: unknown; response: Response }): T {
  if (result.error !== undefined || !result.response.ok) {
    const body = result.error as
      | { error?: { code?: string; message?: string; details?: string[] | null } }
      | undefined;
    throw new ApiError(
      result.response.status,
      body?.error?.code ?? 'error',
      body?.error?.message ?? result.response.statusText,
      body?.error?.details ?? [],
    );
  }
  return result.data as T;
}

/** Search parameters for the streaming endpoint. */
export interface SearchParams {
  q: string;
  categories?: Category[];
  providers?: string[];
  page?: number;
  sort?: SortOrder;
}

export class Api {
  private client;

  constructor(public readonly endpoint: Endpoint) {
    this.client = createClient<paths>({ baseUrl: endpoint.baseUrl });
    const auth: Middleware = {
      onRequest: ({ request }) => {
        if (endpoint.token) request.headers.set('Authorization', `Bearer ${endpoint.token}`);
        return request;
      },
    };
    this.client.use(auth);
  }

  private headers(): HeadersInit {
    return this.endpoint.token ? { Authorization: `Bearer ${this.endpoint.token}` } : {};
  }

  async health() {
    return unwrap(await this.client.GET('/health'));
  }

  /** Streams a search. Abort `signal` to cancel it. */
  async *search(params: SearchParams, signal?: AbortSignal): AsyncGenerator<SearchEvent> {
    const qs = new URLSearchParams({ q: params.q });
    if (params.categories?.length) qs.set('cat', params.categories.join(','));
    if (params.providers?.length) qs.set('providers', params.providers.join(','));
    if (params.page && params.page > 1) qs.set('page', String(params.page));
    if (params.sort) qs.set('sort', params.sort);

    let response: Response;
    try {
      response = await fetch(`${this.endpoint.baseUrl}/search?${qs}`, {
        headers: { ...this.headers(), Accept: 'text/event-stream' },
        signal,
      });
    } catch (e) {
      if (signal?.aborted) return;
      throw new ApiError(0, 'network', 'Could not reach the Hashlark engine.');
    }
    if (!response.ok || !response.body) throw await errorFrom(response);

    const reader = response.body.pipeThrough(new TextDecoderStream()).getReader();
    const parser = new SseParser();
    try {
      for (;;) {
        const { value, done } = await reader.read();
        if (done) break;
        for (const message of parser.push(value)) {
          yield JSON.parse(message.data) as SearchEvent;
        }
      }
    } catch (e) {
      if (signal?.aborted) return;
      throw e;
    } finally {
      reader.releaseLock();
    }
  }

  async resolve(resultId: string, prefer?: 'magnet' | 'torrent_file'): Promise<DownloadTarget> {
    return unwrap(await this.client.POST('/resolve', { body: { result_id: resultId, prefer } }));
  }

  async providers(): Promise<ProviderView[]> {
    return unwrap(await this.client.GET('/providers'));
  }

  async updateProvider(id: string, patch: Schemas['ProviderPatch']): Promise<ProviderView> {
    return unwrap(
      await this.client.PATCH('/providers/{id}', { params: { path: { id } }, body: patch }),
    );
  }

  async deleteProvider(id: string): Promise<void> {
    unwrap(await this.client.DELETE('/providers/{id}', { params: { path: { id } } }));
  }

  async testProvider(id: string): Promise<TestReport> {
    return unwrap(await this.client.POST('/providers/{id}/test', { params: { path: { id } } }));
  }

  async addTorznab(name: string, url: string, apiKey: string | null): Promise<ProviderView> {
    return unwrap(
      await this.client.POST('/providers', {
        body: { kind: 'torznab', name, url, api_key: apiKey },
      }),
    );
  }

  async addDefinition(yaml: string): Promise<ProviderView> {
    return unwrap(await this.client.POST('/providers', { body: { kind: 'definition', yaml } }));
  }

  async definition(id: string): Promise<StoredDefinition> {
    return unwrap(await this.client.GET('/definitions/{id}', { params: { path: { id } } }));
  }

  async checkDefinition(yaml: string): Promise<DefinitionCheck> {
    return unwrap(await this.client.POST('/definitions/check', { body: { yaml } }));
  }

  async previewDefinition(
    yaml: string,
    query: string,
    settings: Record<string, string> = {},
  ): Promise<SearchResult[]> {
    return unwrap(
      await this.client.POST('/definitions/preview', { body: { yaml, query, settings } }),
    );
  }

  async repos(): Promise<RepoView[]> {
    return unwrap(await this.client.GET('/repos'));
  }

  async addRepo(url: string): Promise<{ repo: RepoView; sync: SyncReport }> {
    return unwrap(await this.client.POST('/repos', { body: { url } }));
  }

  async syncRepo(id: string): Promise<SyncReport> {
    return unwrap(await this.client.POST('/repos/{id}/sync', { params: { path: { id } } }));
  }

  async removeRepo(id: string): Promise<void> {
    unwrap(await this.client.DELETE('/repos/{id}', { params: { path: { id } } }));
  }

  async favorites(): Promise<Favorite[]> {
    return unwrap(await this.client.GET('/favorites'));
  }

  async addFavorite(resultId: string): Promise<Favorite> {
    return unwrap(await this.client.PUT('/favorites/{id}', { params: { path: { id: resultId } } }));
  }

  async removeFavorite(resultId: string): Promise<void> {
    unwrap(await this.client.DELETE('/favorites/{id}', { params: { path: { id: resultId } } }));
  }

  async trackers(): Promise<string[]> {
    return unwrap(await this.client.GET('/trackers'));
  }

  async refreshTrackers(): Promise<number> {
    return unwrap(await this.client.POST('/trackers/refresh')).fetched;
  }

  async apiKeys(): Promise<Schemas['ApiKeyInfo'][]> {
    return unwrap(await this.client.GET('/api-keys'));
  }

  async createApiKey(name: string): Promise<Schemas['NewApiKey']> {
    return unwrap(await this.client.POST('/api-keys', { body: { name } }));
  }

  async revokeApiKey(id: string): Promise<void> {
    unwrap(await this.client.DELETE('/api-keys/{id}', { params: { path: { id } } }));
  }

  async starterDefinition(kind: 'html' | 'json' | 'rss', id: string): Promise<string> {
    return unwrap(await this.client.GET('/definitions/starter', { params: { query: { kind, id } } })).yaml;
  }

  async convertCardigann(yaml: string): Promise<Schemas['Conversion']> {
    return unwrap(await this.client.POST('/definitions/convert', { body: { yaml } }));
  }

  async fetchPage(url: string): Promise<Schemas['FetchedPage']> {
    return unwrap(await this.client.POST('/tools/fetch-page', { body: { url } }));
  }

  async torStatus(): Promise<Schemas['TorStatus']> {
    return unwrap(await this.client.GET('/network/tor'));
  }

  async settings(): Promise<Settings> {
    return unwrap(await this.client.GET('/settings'));
  }

  async saveSettings(settings: Settings): Promise<Settings> {
    return unwrap(await this.client.PUT('/settings', { body: settings }));
  }

  async history(limit = 100): Promise<HistoryEntry[]> {
    return unwrap(await this.client.GET('/history', { params: { query: { limit } } }));
  }

  async clearHistory(): Promise<void> {
    unwrap(await this.client.DELETE('/history'));
  }
}
