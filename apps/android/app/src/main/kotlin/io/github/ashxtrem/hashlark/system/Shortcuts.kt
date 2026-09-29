// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark.system

import android.content.Context
import android.content.Intent
import androidx.core.content.pm.ShortcutInfoCompat
import androidx.core.content.pm.ShortcutManagerCompat
import androidx.core.graphics.drawable.IconCompat
import io.github.ashxtrem.hashlark.IntentRouter
import io.github.ashxtrem.hashlark.MainActivity
import io.github.ashxtrem.hashlark.R
import io.github.ashxtrem.hashlark.core.HistoryEntry

/**
 * The launcher's long-press menu: *New search*, *Favourites* and *History* are
 * static (see `res/xml/shortcuts.xml`); the last three searches are added here
 * as dynamic shortcuts. Turning history off in Settings stops this too, because
 * shortcuts are built from saved history only.
 */
object Shortcuts {
    private const val MAX_DYNAMIC = 3

    fun publishRecent(context: Context, history: List<HistoryEntry>) {
        val shortcuts = history
            .distinctBy { it.query.text.lowercase() to it.query.imdbId }
            .take(MAX_DYNAMIC)
            .mapNotNull { entry ->
                val label = entry.query.text.ifBlank { entry.query.imdbId.orEmpty() }.takeIf { it.isNotBlank() } ?: return@mapNotNull null
                ShortcutInfoCompat.Builder(context, "recent-${entry.id}")
                    .setShortLabel(label.take(25))
                    .setLongLabel(label.take(60))
                    .setIcon(IconCompat.createWithResource(context, R.drawable.ic_shortcut_history))
                    .setIntent(
                        Intent(Intent.ACTION_VIEW, IntentRouter.searchUri(entry.query), context, MainActivity::class.java),
                    )
                    .build()
            }
        runCatching { ShortcutManagerCompat.setDynamicShortcuts(context, shortcuts) }
    }
}
