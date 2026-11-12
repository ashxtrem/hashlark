// SPDX-License-Identifier: GPL-3.0-or-later
import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import RecheckTorrentDialog from './RecheckTorrentDialog.svelte';

beforeAll(() => {
  // jsdom may lack the modal dialog API.
  const proto = HTMLDialogElement.prototype;
  proto.showModal ??= function (this: HTMLDialogElement) {
    this.setAttribute('open', '');
  };
  proto.close ??= function (this: HTMLDialogElement) {
    this.removeAttribute('open');
  };
});

afterEach(cleanup);

describe('RecheckTorrentDialog', () => {
  it('asks before rechecking and sends the ids', async () => {
    const onconfirm = vi.fn();
    render(RecheckTorrentDialog, { ids: ['a', 'b'], onclose: () => {}, onconfirm });

    expect(screen.getByText(/Re-verify all downloaded data of 2 torrents\?/)).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Recheck', hidden: true }));
    expect(onconfirm).toHaveBeenCalledWith(['a', 'b']);
  });

  it('cancels without rechecking', async () => {
    const onconfirm = vi.fn();
    const onclose = vi.fn();
    render(RecheckTorrentDialog, { ids: ['a'], onclose, onconfirm });

    expect(screen.getByText(/of 1 torrent\?/)).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Cancel', hidden: true }));
    expect(onclose).toHaveBeenCalled();
    expect(onconfirm).not.toHaveBeenCalled();
  });
});
