// SPDX-License-Identifier: GPL-3.0-or-later
import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { defaultSettings, result } from '$lib/testFixtures';

const { download, openInClient } = vi.hoisted(() => ({ download: vi.fn(), openInClient: vi.fn() }));

vi.mock('$lib/actions', async (importOriginal) => ({
  ...(await importOriginal<typeof import('$lib/actions')>()),
  download,
  openInClient,
}));
vi.mock('$app/navigation', () => ({ goto: vi.fn() }));

import { app } from '$lib/state/app.svelte';
import { torrents } from '$lib/state/torrents.svelte';
import ResultsList from './ResultsList.svelte';

const HASH = '0123456789abcdef0123456789abcdef01234567';
const results = [result('r1', 'Ubuntu ISO', HASH), result('r2', 'Debian ISO')];
const names = { prov: 'Provider' };

function setEnabled(enabled: boolean) {
  const s = defaultSettings();
  s.torrents.enabled = enabled;
  app.settings = s;
}

beforeEach(() => setEnabled(true));

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  torrents.list = [];
});

describe('ResultsList primary action', () => {
  it('is Download when the built-in client is on, with Open in client beside it', async () => {
    render(ResultsList, { results, names });
    const main = screen.getByRole('button', { name: 'Download Ubuntu ISO' });
    expect(screen.getByRole('button', { name: 'Open Ubuntu ISO in torrent client' })).toBeTruthy();

    await fireEvent.click(main);
    expect(download).toHaveBeenLastCalledWith(results[0], { dialog: false });
    // Shift+click opens the Add dialog.
    await fireEvent.click(main, { shiftKey: true });
    expect(download).toHaveBeenLastCalledWith(results[0], { dialog: true });

    await fireEvent.click(screen.getByRole('button', { name: 'Open Ubuntu ISO in torrent client' }));
    expect(openInClient).toHaveBeenCalledWith(results[0]);
  });

  it('is Open in client when the built-in client is off', async () => {
    setEnabled(false);
    render(ResultsList, { results, names });
    expect(screen.queryByRole('button', { name: 'Download Ubuntu ISO' })).toBeNull();
    await fireEvent.click(screen.getByRole('button', { name: 'Open Ubuntu ISO in torrent client' }));
    expect(download).toHaveBeenCalledWith(results[0]);
  });

  it('badges results that are already in downloads', () => {
    render(ResultsList, { results, names });
    expect(screen.queryByText('In downloads')).toBeNull();
    torrents.list = [{ id: HASH }] as never;
    return vi.waitFor(() => expect(screen.getAllByText('In downloads')).toHaveLength(1));
  });
});
