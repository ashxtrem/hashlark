// SPDX-License-Identifier: GPL-3.0-or-later

/** One Server-Sent Event. */
export interface SseMessage {
  event: string;
  data: string;
}

/**
 * Incremental Server-Sent Events parser. Feed it text chunks as they arrive;
 * it returns every complete event. Used instead of `EventSource` because
 * that can't send an `Authorization` header.
 */
export class SseParser {
  private buffer = '';
  private event = '';
  private data: string[] = [];

  push(chunk: string): SseMessage[] {
    this.buffer += chunk;
    const lines = this.buffer.split(/\r\n|\r|\n/);
    // The last element is an incomplete line (or empty after a newline).
    this.buffer = lines.pop() ?? '';
    const out: SseMessage[] = [];
    for (const line of lines) {
      if (line === '') {
        if (this.data.length > 0) {
          out.push({ event: this.event || 'message', data: this.data.join('\n') });
        }
        this.event = '';
        this.data = [];
        continue;
      }
      if (line.startsWith(':')) continue; // comment / keep-alive
      const colon = line.indexOf(':');
      const field = colon === -1 ? line : line.slice(0, colon);
      let value = colon === -1 ? '' : line.slice(colon + 1);
      if (value.startsWith(' ')) value = value.slice(1);
      if (field === 'event') this.event = value;
      else if (field === 'data') this.data.push(value);
    }
    return out;
  }
}
