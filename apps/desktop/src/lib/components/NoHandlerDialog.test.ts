// SPDX-License-Identifier: GPL-3.0-or-later
import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Api } from '$lib/api/client';
import { defaultSettings } from '$lib/testFixtures';

vi.mock('$app/navigation', () => ({ goto: vi.fn() }));

import { app } from '$lib/state/app.svelte';
import { torrents } from '$lib/state/torrents.svelte';
import { ui } from '$lib/state/ui.svelte';
import NoHandlerDialog from './NoHandlerDialog.svelte';

const MAGNET = 'magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567';

beforeAll(() => {
  const proto = HTMLDialogElement.prototype;
  proto.showModal ??= function (this: HTMLDialogElement) {
    this.setAttribute('open', '');
  };
  proto.close ??= function (this: HTMLDialogElement) {
    this.removeAttribute('open');
  };
});

function setEnabled(enabled: boolean) {
  const s = defaultSettings();
  s.torrents.enabled = enabled;
  app.settings = s;
}

beforeEach(() => {
  ui.noHandlerMagnet = MAGNET;
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  ui.noHandlerMagnet = null;
});

describe('NoHandlerDialog', () => {
  it('offers to turn on the built-in client when it is off', async () => {
    setEnabled(false);
    const saveSettings = vi.fn(async (s) => s);
    app.api = { saveSettings } as unknown as Api;
    const add = vi
      .spyOn(torrents, 'add')
      .mockResolvedValue({ already: false } as unknown as Awaited<ReturnType<typeof torrents.add>>);

    render(NoHandlerDialog);
    const button = screen.getByRole('button', { name: /Turn on the built-in client/, hidden: true });
    await fireEvent.click(button);

    expect(saveSettings).toHaveBeenCalledTimes(1);
    expect(saveSettings.mock.calls[0]![0].torrents.enabled).toBe(true);
    // The saved setting is what the add flow reads, so the retry goes through.
    expect(torrents.enabled).toBe(true);
    expect(add).toHaveBeenCalledWith({ source: { magnet: MAGNET } });
    await vi.waitFor(() => expect(ui.noHandlerMagnet).toBeNull());
  });

  it('keeps the link when the add fails, so it can be retried', async () => {
    setEnabled(false);
    app.api = { saveSettings: vi.fn(async (s) => s) } as unknown as Api;
    vi.spyOn(torrents, 'add').mockResolvedValue(null);

    render(NoHandlerDialog);
    const button = screen.getByRole('button', { name: /Turn on the built-in client/, hidden: true });
    await fireEvent.click(button);

    expect(ui.noHandlerMagnet).toBe(MAGNET);
  });

  it('keeps the dialog and does not add when saving fails', async () => {
    setEnabled(false);
    app.api = {
      saveSettings: vi.fn(async () => {
        throw new Error('nope');
      }),
    } as unknown as Api;
    const add = vi.spyOn(torrents, 'add').mockResolvedValue(null);

    render(NoHandlerDialog);
    await fireEvent.click(screen.getByRole('button', { name: /Turn on the built-in client/, hidden: true }));
    expect(add).not.toHaveBeenCalled();
    expect(ui.noHandlerMagnet).toBe(MAGNET);
  });

  it('has no such button when the built-in client is already on', () => {
    setEnabled(true);
    render(NoHandlerDialog);
    expect(screen.queryByRole('button', { name: /Turn on the built-in client/, hidden: true })).toBeNull();
    expect(screen.getByRole('button', { name: /Copy magnet link/, hidden: true })).toBeTruthy();
  });
});
