// SPDX-License-Identifier: GPL-3.0-or-later

//! Differences between the desktop app (Tauri) and the browser UI served by
//! the headless server.

export function isTauri(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const core = await import('@tauri-apps/api/core');
  return core.invoke<T>(cmd, args);
}

export type OpenOutcome = 'opened' | 'no_handler';

/** Hands a magnet link to the registered torrent client. */
export async function openMagnet(url: string): Promise<OpenOutcome> {
  if (isTauri()) {
    const outcome = await invoke<{ status: OpenOutcome }>('open_magnet', { url });
    return outcome.status;
  }
  // The browser passes magnet: links to the OS of the device it runs on.
  window.location.href = url;
  return 'opened';
}

/**
 * Opens a `.torrent`. On desktop it's saved to the downloads folder and
 * opened with the default app (returns the saved path); in a browser it's
 * downloaded by the browser.
 */
export async function openTorrent(url: string, title: string): Promise<string | null> {
  if (isTauri()) {
    return invoke<string>('save_torrent', { url, title, open: true });
  }
  window.open(url, '_blank', 'noopener');
  return null;
}

/** Opens a web page in the default browser. */
export async function openExternal(url: string): Promise<void> {
  if (isTauri()) {
    await invoke('open_url', { url });
  } else {
    window.open(url, '_blank', 'noopener,noreferrer');
  }
}

/** Shows a saved file in the file manager (desktop only). */
export async function revealPath(path: string): Promise<void> {
  if (isTauri()) await invoke('reveal_path', { path });
}

/** Whether a torrent client is registered for magnet links (`null` if unknown). */
export async function magnetHandlerStatus(): Promise<boolean | null> {
  return isTauri() ? invoke<boolean | null>('magnet_handler_status') : null;
}

/** Lets the user pick a folder (desktop only). */
export async function pickFolder(): Promise<string | null> {
  if (!isTauri()) return null;
  const { open } = await import('@tauri-apps/plugin-dialog');
  const picked = await open({ directory: true, multiple: false });
  return typeof picked === 'string' ? picked : null;
}

/** Opens a site in a separate window for its browser check (desktop only). */
export async function openChallenge(url: string): Promise<void> {
  await invoke('open_challenge', { url });
}

/** Hands the check window's cookies to the provider. Returns the number stored. */
export async function finishChallenge(providerId: string, url: string): Promise<number> {
  return invoke<number>('finish_challenge', {
    providerId,
    url,
    userAgent: navigator.userAgent,
  });
}

/** Subscribes to an event from the Rust side; returns the unsubscribe function. */
export async function listenTauri<T>(event: string, handler: (payload: T) => void): Promise<() => void> {
  const { listen } = await import('@tauri-apps/api/event');
  return listen<T>(event, (e) => handler(e.payload));
}

// ----- LAN sharing (desktop only) -------------------------------------------

export interface LanDevice {
  id: string;
  name: string;
  platform: 'desktop' | 'android';
  /** This computer holds a key for it. */
  paired: boolean;
  /** Seen on the network right now. */
  online: boolean;
  addr: string | null;
}

export interface PairingCode {
  pin: string;
  /** Unix milliseconds. */
  expires_at: number;
}

export interface LanStatus {
  enabled: boolean;
  running: boolean;
  name: string | null;
  /** `ip:port` to type on a phone when it can't find this computer. */
  addresses: string[];
  discovery_error: string | null;
  pairing: PairingCode | null;
  error: string | null;
}

/** A link a paired device sent, after it was handed to the torrent client. */
export interface LanReceived {
  from: string;
  title: string;
  url: string;
  outcome: { status: 'opened' } | { status: 'no_handler' } | { status: 'failed'; message: string };
}

export const lanApi = {
  status: () => invoke<LanStatus>('lan_status'),
  setEnabled: (enabled: boolean) => invoke<LanStatus>('lan_set_enabled', { enabled }),
  devices: () => invoke<LanDevice[]>('lan_devices'),
  openPairing: () => invoke<PairingCode>('lan_open_pairing'),
  closePairing: () => invoke<void>('lan_close_pairing'),
  forget: (deviceId: string) => invoke<void>('lan_forget', { deviceId }),
  addAddress: (address: string) => invoke<LanDevice>('lan_add_address', { address }),
  pair: (deviceId: string, pin: string) => invoke<LanDevice>('lan_pair', { deviceId, pin }),
  send: (deviceId: string, url: string, title: string) => invoke<void>('lan_send', { deviceId, url, title }),
};

/** How a LAN command fails: a stable code and a sentence. */
export interface LanError {
  code: string;
  message: string;
}

export function isLanError(e: unknown): e is LanError {
  return typeof e === 'object' && e !== null && 'code' in e && 'message' in e;
}

export async function copyText(text: string): Promise<void> {
  await navigator.clipboard.writeText(text);
}
