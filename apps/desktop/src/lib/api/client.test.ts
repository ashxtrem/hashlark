// SPDX-License-Identifier: GPL-3.0-or-later
import { afterEach, describe, expect, it, vi } from 'vitest';
import { Api, ApiError, type TorrentEvent } from './client';

const endpoint = { baseUrl: 'http://127.0.0.1:1/api/v1', token: 'tok' };

function sse(chunks: string[]): Response {
  const encoder = new TextEncoder();
  const body = new ReadableStream<Uint8Array>({
    start(controller) {
      for (const chunk of chunks) controller.enqueue(encoder.encode(chunk));
      controller.close();
    },
  });
  return new Response(body, { headers: { 'content-type': 'text/event-stream' } });
}

afterEach(() => vi.unstubAllGlobals());

describe('torrentEvents', () => {
  it('parses events split across chunks and sends the token', async () => {
    const snapshot = { type: 'snapshot', session: { running: true }, torrents: [] };
    const completed = { type: 'completed', id: 'abc', name: 'ISO' };
    const text = `event: snapshot\ndata: ${JSON.stringify(snapshot)}\n\n: keep-alive\n\nevent: completed\ndata: ${JSON.stringify(completed)}\n\n`;
    const fetchMock = vi.fn(async () => sse([text.slice(0, 25), text.slice(25)]));
    vi.stubGlobal('fetch', fetchMock);

    const events: TorrentEvent[] = [];
    for await (const event of new Api(endpoint).torrentEvents()) events.push(event);

    expect(events.map((e) => e.type)).toEqual(['snapshot', 'completed']);
    const [url, init] = fetchMock.mock.calls[0] as unknown as [string, RequestInit];
    expect(url).toBe('http://127.0.0.1:1/api/v1/torrents/events');
    expect((init.headers as Record<string, string>).Authorization).toBe('Bearer tok');
  });

  it('closes the stream when the consumer stops early', async () => {
    let cancelled = false;
    const encoder = new TextEncoder();
    const body = new ReadableStream<Uint8Array>({
      start(controller) {
        controller.enqueue(encoder.encode('event: completed\ndata: {"type":"completed","id":"a","name":"x"}\n\n'));
      },
      cancel() {
        cancelled = true;
      },
    });
    vi.stubGlobal('fetch', vi.fn(async () => new Response(body)));
    for await (const _ of new Api(endpoint).torrentEvents()) break;
    expect(cancelled).toBe(true);
  });

  it('turns an error response into an ApiError', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () =>
        Response.json({ error: { code: 'unauthorized', message: 'no' } }, { status: 401 }),
      ),
    );
    const run = async () => {
      for await (const _ of new Api(endpoint).torrentEvents()) void _;
    };
    await expect(run()).rejects.toBeInstanceOf(ApiError);
  });
});

describe('torrentFileUrl', () => {
  it('points at the file route', () => {
    expect(new Api(endpoint).torrentFileUrl('abc')).toBe(
      'http://127.0.0.1:1/api/v1/torrents/abc/file',
    );
  });
});
