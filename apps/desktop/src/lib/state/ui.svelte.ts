// SPDX-License-Identifier: GPL-3.0-or-later

import type { MergedResult } from '$lib/api/client';

/** Dialogs and drawers shared across pages. */
class UiState {
  /** Result shown in the details drawer. */
  details = $state<MergedResult | null>(null);
  /** Magnet that couldn't be opened because no torrent client is registered. */
  noHandlerMagnet = $state<string | null>(null);
  /** Provider whose browser check the user is completing. */
  challenge = $state<{ providerId: string; name: string; url: string } | null>(null);
}

export const ui = new UiState();
