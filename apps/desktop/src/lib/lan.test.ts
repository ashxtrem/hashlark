// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from 'vitest';
import { formatCountdown, formatPin, receivedToast } from './lan';
import type { LanReceived } from './platform';

const base: Omit<LanReceived, 'outcome'> = {
  from: 'Pixel 9',
  title: 'Big Buck Bunny',
  url: 'magnet:?xt=urn:btih:dd8255ecdc7ca55fb0bbf81323d87062db1f6d1c',
};

describe('formatCountdown', () => {
  it('shows minutes and padded seconds, rounding up', () => {
    expect(formatCountdown(120_000)).toBe('2:00');
    expect(formatCountdown(65_000)).toBe('1:05');
    expect(formatCountdown(500)).toBe('0:01');
  });

  it('never goes negative', () => {
    expect(formatCountdown(-3000)).toBe('0:00');
  });
});

describe('formatPin', () => {
  it('splits a 6-digit PIN in two', () => {
    expect(formatPin('123456')).toBe('123 456');
    expect(formatPin('12')).toBe('12');
  });
});

describe('receivedToast', () => {
  it('confirms an opened link with its title and sender', () => {
    expect(receivedToast({ ...base, outcome: { status: 'opened' } })).toEqual({
      kind: 'success',
      message: 'Opened “Big Buck Bunny” from Pixel 9',
    });
  });

  it('leaves a missing torrent client to the dialog', () => {
    expect(receivedToast({ ...base, outcome: { status: 'no_handler' } })).toBeNull();
  });

  it('reports failures, and copes without a title', () => {
    const toast = receivedToast({
      ...base,
      title: '',
      outcome: { status: 'failed', message: 'access denied' },
    });
    expect(toast).toEqual({
      kind: 'error',
      message: "Couldn't open a link from Pixel 9: access denied",
    });
  });
});
