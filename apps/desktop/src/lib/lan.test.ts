// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from 'vitest';
import { formatCountdown, formatPin, isPin, lanErrorMessage, pickerSections, pinHint, receivedToast } from './lan';
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

  it('confirms a link added to the built-in client', () => {
    expect(receivedToast({ ...base, outcome: { status: 'added' } })).toEqual({
      kind: 'success',
      message: 'Downloading “Big Buck Bunny” from Pixel 9',
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

describe('picker helpers', () => {
  const device = (name: string, paired: boolean, online: boolean, platform: 'desktop' | 'android' = 'android') => ({
    id: name.toLowerCase(),
    name,
    platform,
    paired,
    online,
    addr: null,
  });

  it('lists paired devices first, those on the network before the rest', () => {
    const { paired, others } = pickerSections([
      device('zeta', false, true),
      device('Office', true, false),
      device('pixel', true, true),
      device('Attic', false, true),
    ]);
    expect(paired.map((d) => d.name)).toEqual(['pixel', 'Office']);
    expect(others.map((d) => d.name)).toEqual(['Attic', 'zeta']);
  });

  it('accepts only six-digit PINs', () => {
    expect(isPin('012345')).toBe(true);
    expect(isPin('12345')).toBe(false);
    expect(isPin('12a456')).toBe(false);
  });

  it('points at the right screen on each platform', () => {
    expect(pinHint(device('Pixel', false, true))).toContain('Show pairing PIN');
    expect(pinHint(device('Desk', false, true, 'desktop'))).toContain('Pair a phone');
  });

  it('turns coded failures into advice', () => {
    expect(lanErrorMessage({ code: 'unreachable', message: 'x' }, 'Pixel')).toMatch(/^Couldn't reach Pixel\./);
    expect(lanErrorMessage({ code: 'rejected', message: 'x' }, 'Pixel')).toContain('Pair again');
    expect(lanErrorMessage({ code: 'protocol', message: 'raw' }, 'Pixel')).toBe('raw');
    expect(lanErrorMessage(new Error('boom'), 'Pixel')).toBe('boom');
  });
});
