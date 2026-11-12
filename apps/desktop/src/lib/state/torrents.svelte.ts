// SPDX-License-Identifier: GPL-3.0-or-later

//! The built-in torrent client: the live list, adding torrents and actions
//! (plan `docs/torrent-client-plan.md` §7.1).

import {
  ApiError,
  type AddRequest,
  type AddResponse,
  type SessionStats,
  type TorrentDetails,
  type TorrentSummary,
} from '$lib/api/client';
import {
  isTauri,
  onClosedToTray,
  onTorrentOpen,
  readTorrentFile,
  whenDoneApi,
  type TorrentOpen,
  type WhenDoneAction,
  type WhenDoneStatus,
} from '$lib/platform';
import { app } from '$lib/state/app.svelte';
import { errorMessage, toasts } from '$lib/state/toasts.svelte';
import { ui } from '$lib/state/ui.svelte';
import {
  backoff,
  downloadingCount,
  isMagnet,
  maintainRequest,
  moveOrder,
  patch,
  readTorrent,
  reduce,
  removeRequest,
  toBase64,
  torrentFileError,
  type FileEntry,
  type QueueStep,
} from '$lib/torrents';

export type RowAction = 'pause' | 'resume' | 'stop' | 'start' | 'force_start' | 'reannounce' | 'recheck';

/** The Add dialog, waiting for the user's choice. */
export interface AddPrompt {
  req: AddRequest;
  /** Torrent name, when known. */
  title: string | null;
  /** Files, when the metadata is known (a `.torrent`). */
  files: FileEntry[] | null;
  resolve: (req: AddRequest | null) => void;
}

export interface AddOptions {
  /** Show the Add dialog even when `ask_on_add` is off (Shift held). */
  dialog?: boolean;
  title?: string | null;
  files?: FileEntry[] | null;
  /** The caller shows its own "added" toast. */
  quiet?: boolean;
}

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

class TorrentsState {
  list = $state.raw<TorrentSummary[]>([]);
  session = $state.raw<SessionStats | null>(null);
  /** The live-update stream is open. */
  connected = $state(false);
  /** Info hashes in the client (for "In downloads" badges). */
  byHash = $derived(new Set(this.list.map((t) => t.id)));
  downloading = $derived(downloadingCount(this.list));

  /** The one-time notice is shown; answer with {@link answerNotice}. */
  noticeOpen = $state(false);
  addPrompt = $state<AddPrompt | null>(null);
  createOpen = $state(false);

  /** "When all done" (desktop only). */
  whenDone = $state<WhenDoneStatus | null>(null);
  /** The when-done countdown runs; ends at this time (epoch ms). */
  countdown = $state<{ action: WhenDoneAction; endsAt: number } | null>(null);

  private started = false;
  /**
   * Bumped by {@link start} and {@link stop}; a stream loop from an older
   * run ends at its next check (also during its backoff sleep).
   */
  private run = 0;
  /** Desktop listeners are added once for the app's life. */
  private desktopListening = false;
  private abort: AbortController | null = null;
  private noticeWaiters: ((ok: boolean) => void)[] = [];

  get enabled(): boolean {
    return app.settings?.torrents.enabled ?? false;
  }

  /** Subscribes to live updates (again after {@link stop} or a 401) and, once, to desktop events. */
  start(): void {
    if (this.started) return;
    this.started = true;
    void this.stream(++this.run);
    if (!this.desktopListening) {
      this.desktopListening = true;
      void this.listenDesktop();
    }
  }

  /** Closes the live-update stream; {@link start} opens it again. */
  stop(): void {
    this.run += 1;
    this.started = false;
    this.connected = false;
    this.abort?.abort();
  }

  private async stream(run: number): Promise<void> {
    let attempt = 0;
    const current = () => run === this.run;
    while (current()) {
      const api = app.api;
      if (!api) break;
      const abort = new AbortController();
      this.abort = abort;
      try {
        for await (const event of api.torrentEvents(abort.signal)) {
          if (!current()) break;
          attempt = 0;
          this.connected = true;
          const { data, notice } = reduce({ list: this.list, session: this.session }, event);
          this.list = data.list;
          this.session = data.session;
          // The desktop shell already notifies about finished downloads.
          if (notice && (notice.kind === 'error' || !isTauri())) {
            toasts.show(notice.message, notice.kind, notice.kind === 'error' ? 7000 : 5000);
          }
        }
      } catch (e) {
        if (e instanceof ApiError && e.status === 401) break;
      }
      if (!current()) return;
      this.connected = false;
      await sleep(backoff(attempt));
      attempt += 1;
    }
    // A 401 or a lost API ends the loop; start() may open it again.
    if (current()) {
      this.connected = false;
      this.started = false;
    }
  }

  private async listenDesktop(): Promise<void> {
    if (!isTauri()) return;
    try {
      this.whenDone = await whenDoneApi.status();
      await Promise.all([
        whenDoneApi.onChanged((s) => {
          this.whenDone = s;
          if (!s.counting) this.countdown = null;
        }),
        whenDoneApi.onCountdown((c) => {
          this.countdown = { action: c.action, endsAt: Date.now() + c.seconds * 1000 };
        }),
        onClosedToTray(({ first }) => {
          if (first) {
            toasts.show('Hashlark keeps running in the tray. Quit it from the tray menu.', 'info', 8000);
          }
        }),
        onTorrentOpen((open) => void this.opened(open)),
      ]);
    } catch (e) {
      toasts.error(errorMessage(e));
    }
  }

  /** A magnet or `.torrent` the OS opened with Hashlark. */
  private async opened(open: TorrentOpen): Promise<void> {
    if ('magnet' in open) {
      if (open.client_on) await this.add({ source: { magnet: open.magnet } });
      else ui.noHandlerMagnet = open.magnet;
      return;
    }
    // A `.torrent` from the command line or a file association.
    const name = open.path.split(/[\\/]/).pop() ?? open.path;
    if (!open.client_on) {
      toasts.error(`Turn on the built-in client in Settings to open ${name}.`);
      return;
    }
    try {
      await this.addBytes(await readTorrentFile(open.path), name);
    } catch (e) {
      toasts.error(`Could not open ${name}: ${errorMessage(e)}`);
    }
  }

  private async addBytes(data: Uint8Array, fileName: string, dialog = false): Promise<void> {
    const info = readTorrent(data);
    await this.add(
      { source: { torrent: toBase64(data) } },
      { dialog, title: info?.name ?? fileName.replace(/\.torrent$/i, ''), files: info?.files ?? null },
    );
  }

  /** Reloads the list now instead of waiting for the next snapshot. */
  async refresh(): Promise<void> {
    try {
      this.list = await app.client.torrents();
    } catch {
      // The next snapshot brings it.
    }
  }

  // ----- Adding --------------------------------------------------------------

  /** Resolves once the user has accepted the one-time notice (or declined). */
  private acceptNotice(): Promise<boolean> {
    if (app.settings?.torrents.notice_accepted) return Promise.resolve(true);
    this.noticeOpen = true;
    return new Promise((resolve) => this.noticeWaiters.push(resolve));
  }

  async answerNotice(ok: boolean): Promise<void> {
    if (ok && app.settings) {
      try {
        await app.saveSettings({
          ...app.settings,
          torrents: { ...app.settings.torrents, notice_accepted: true },
        });
      } catch (e) {
        toasts.error(`Could not save your answer: ${errorMessage(e)}`);
      }
    }
    this.noticeOpen = false;
    const waiters = this.noticeWaiters;
    this.noticeWaiters = [];
    for (const resolve of waiters) resolve(ok);
  }

  /**
   * The add flow: the one-time notice, then the Add dialog when
   * `ask_on_add` is on or `dialog` is set (Shift held), then the API.
   */
  async add(req: AddRequest, opts: AddOptions = {}): Promise<AddResponse | null> {
    if (!this.enabled) {
      toasts.error('The built-in torrent client is off. Turn it on in Settings.');
      return null;
    }
    if (!(await this.acceptNotice())) return null;
    let final: AddRequest | null = req;
    if (opts.dialog || app.settings?.torrents.ask_on_add) {
      // `addPrompt` is a deep proxy; its `resolve` tells whose dialog shows.
      let mine: AddPrompt['resolve'] | null = null;
      final = await new Promise<AddRequest | null>((resolve) => {
        // A newer add replaces an open dialog; the older one is cancelled.
        this.addPrompt?.resolve(null);
        mine = resolve;
        this.addPrompt = { req, title: opts.title ?? null, files: opts.files ?? null, resolve };
      });
      if (this.addPrompt?.resolve === mine) this.addPrompt = null;
      if (!final) return null;
    }
    try {
      const res = await app.client.addTorrent(final);
      if (!res.already && !this.byHash.has(res.torrent.id)) this.list = [...this.list, res.torrent];
      if (res.already) toasts.show(`Already in Downloads: ${res.torrent.name}`);
      else if (!opts.quiet) toasts.success(`Added ${res.torrent.name}`);
      return res;
    } catch (e) {
      toasts.error(`Could not add the torrent: ${errorMessage(e)}`);
      return null;
    }
  }

  /** Adds a magnet link typed or pasted. Returns false when it isn't one. */
  async addText(text: string, dialog = false): Promise<boolean> {
    if (!isMagnet(text)) return false;
    await this.add({ source: { magnet: text.trim() } }, { dialog });
    return true;
  }

  /** Adds `.torrent` files (picked or dropped). */
  async addFiles(files: Iterable<File>, dialog = false): Promise<void> {
    for (const file of files) {
      // Checked before reading, so a huge file is never loaded.
      const problem = torrentFileError(file);
      if (problem) {
        toasts.error(problem);
        continue;
      }
      await this.addBytes(new Uint8Array(await file.arrayBuffer()), file.name, dialog);
    }
  }

  // ----- Actions -------------------------------------------------------------

  private report(failed: { id: string; message: string }[]): void {
    if (failed.length === 0) return;
    const name = (id: string) => this.list.find((t) => t.id === id)?.name ?? id;
    toasts.error(failed.map((f) => `${name(f.id)}: ${f.message}`).join('\n'));
  }

  /**
   * Pause, resume, stop, start, force start, reannounce or recheck; every
   * torrent when `ids` is null. Reannounce and recheck always name the
   * torrents (a missing `ids` would reach every one on the engine side).
   */
  async act(ids: string[] | null, action: RowAction): Promise<void> {
    try {
      const res = await app.client.bulkTorrents(
        action === 'reannounce' || action === 'recheck'
          ? maintainRequest(ids ?? this.list.map((t) => t.id), action)
          : { ids, action },
      );
      this.report(res.failed);
    } catch (e) {
      toasts.error(errorMessage(e));
    }
    await this.refresh();
  }

  async remove(ids: string[], deleteFiles: boolean): Promise<void> {
    try {
      const res = await app.client.bulkTorrents(removeRequest(ids, deleteFiles));
      this.report(res.failed);
    } catch (e) {
      toasts.error(errorMessage(e));
    }
    await this.refresh();
  }

  /**
   * Replaces a torrent's tracker list and returns the fresh details. Throws
   * the engine's error so the Trackers tab can show it next to the list.
   */
  async setTrackers(id: string, list: string[]): Promise<TorrentDetails> {
    const details = await app.client.updateTorrent(id, patch({ trackers: list }));
    await this.refresh();
    return details;
  }

  /** Moves torrents in the queue; a single one moves past its shown neighbour. */
  async move(ids: string[], step: QueueStep, index: number | null = null): Promise<void> {
    try {
      const [only] = ids;
      if (ids.length === 1 && only && index !== null) {
        await app.client.updateTorrent(only, patch({ queue_pos: index }));
      } else {
        for (const id of moveOrder(this.list, ids, step)) {
          await app.client.updateTorrent(id, patch({ queue_pos: step }));
        }
      }
    } catch (e) {
      toasts.error(errorMessage(e));
    }
    await this.refresh();
  }

  /** Drag and drop in the Queued tab: `index` in the whole queue. */
  async reorder(id: string, index: number): Promise<void> {
    await this.move([id], 'up', index);
  }

  async setTurtle(on: boolean): Promise<void> {
    try {
      await app.client.bulkTorrents({ action: { turtle: on } });
      if (this.session) this.session = { ...this.session, turtle: on };
    } catch (e) {
      toasts.error(errorMessage(e));
    }
  }

  async setWhenDone(action: WhenDoneAction): Promise<void> {
    try {
      this.whenDone = await whenDoneApi.set(action);
    } catch (e) {
      toasts.error(errorMessage(e));
    }
  }

  async cancelWhenDone(): Promise<void> {
    this.countdown = null;
    try {
      this.whenDone = await whenDoneApi.cancel();
    } catch (e) {
      toasts.error(errorMessage(e));
    }
  }
}

export const torrents = new TorrentsState();
