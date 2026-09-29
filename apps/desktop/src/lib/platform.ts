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

export async function copyText(text: string): Promise<void> {
  await navigator.clipboard.writeText(text);
}
