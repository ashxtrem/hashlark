// SPDX-License-Identifier: GPL-3.0-or-later
import { cleanup, fireEvent, render, screen } from '@testing-library/svelte';
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest';
import DeleteTorrentDialog from './DeleteTorrentDialog.svelte';

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

const targets = [
  { id: 'a', name: 'Ubuntu ISO' },
  { id: 'b', name: 'Debian ISO' },
];

describe('DeleteTorrentDialog', () => {
  it('lists the names and keeps files unless ticked', async () => {
    const onconfirm = vi.fn();
    render(DeleteTorrentDialog, { targets, onclose: () => {}, onconfirm });

    expect(screen.getByText('Ubuntu ISO')).toBeTruthy();
    expect(screen.getByText('Debian ISO')).toBeTruthy();
    const box = screen.getByLabelText('Also delete downloaded files') as HTMLInputElement;
    expect(box.checked).toBe(false);

    await fireEvent.click(screen.getByRole('button', { name: 'Delete', hidden: true }));
    expect(onconfirm).toHaveBeenCalledWith(['a', 'b'], false);
  });

  it('renames the button and deletes files when ticked', async () => {
    const onconfirm = vi.fn();
    render(DeleteTorrentDialog, { targets: [targets[0]!], onclose: () => {}, onconfirm });

    await fireEvent.click(screen.getByLabelText('Also delete downloaded files'));
    const button = screen.getByRole('button', { name: 'Delete and remove files', hidden: true });
    await fireEvent.click(button);
    expect(onconfirm).toHaveBeenCalledWith(['a'], true);
  });

  it('keeps the tick when the targets are refreshed while open', async () => {
    const onconfirm = vi.fn();
    const { rerender } = render(DeleteTorrentDialog, { targets, onclose: () => {}, onconfirm });

    const box = screen.getByLabelText('Also delete downloaded files') as HTMLInputElement;
    await fireEvent.click(box);
    expect(box.checked).toBe(true);

    // An equal but new array, as a live list refresh would give.
    await rerender({ targets: targets.map((t) => ({ ...t })) });
    expect(box.checked).toBe(true);

    // Closing and opening again unticks it.
    await rerender({ targets: [] });
    await rerender({ targets: [...targets] });
    expect((screen.getByLabelText('Also delete downloaded files') as HTMLInputElement).checked).toBe(false);
  });
});
