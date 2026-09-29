// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark

import android.content.Intent
import android.net.Uri
import io.github.ashxtrem.hashlark.core.SearchIntent
import io.github.ashxtrem.hashlark.core.SearchQuery
import io.github.ashxtrem.hashlark.ui.AppEvent
import io.github.ashxtrem.hashlark.ui.Destination

/**
 * Turns the intents the app accepts into what the UI should do:
 *
 * - share text or an IMDb link (`ACTION_SEND`) and the text-selection menu
 *   (`ACTION_PROCESS_TEXT`) start a search;
 * - `hashlark://search?q=...` starts a search, `hashlark://repo?url=...` asks
 *   to add a repository (the user still has to confirm its signing key);
 * - a `.yml` file offers to add a provider definition (after a check);
 * - the launcher shortcuts open a screen.
 */
object IntentRouter {
    const val ACTION_NEW_SEARCH = "io.github.ashxtrem.hashlark.action.NEW_SEARCH"
    const val ACTION_FAVORITES = "io.github.ashxtrem.hashlark.action.FAVORITES"
    const val ACTION_HISTORY = "io.github.ashxtrem.hashlark.action.HISTORY"

    /** Largest definition file that is read. */
    const val MAX_DEFINITION_BYTES = 200_000

    /**
     * @param readText reads the text behind a `content:` or `file:` Uri (`null` when it cannot be read).
     */
    fun route(intent: Intent, readText: (Uri) -> String?): AppEvent? = when (intent.action) {
        ACTION_NEW_SEARCH -> AppEvent.Navigate(Destination.Search)
        ACTION_FAVORITES -> AppEvent.Navigate(Destination.Favorites)
        ACTION_HISTORY -> AppEvent.Navigate(Destination.History)
        Intent.ACTION_SEND -> searchOf(intent.getStringExtra(Intent.EXTRA_TEXT))
        Intent.ACTION_PROCESS_TEXT -> searchOf(intent.getCharSequenceExtra(Intent.EXTRA_PROCESS_TEXT)?.toString())
        Intent.ACTION_VIEW -> intent.data?.let { fromUri(it, readText) }
        else -> null
    }

    private fun searchOf(text: String?): AppEvent? =
        SearchIntent.fromSharedText(text)?.let { AppEvent.RunSearch(it) }

    private fun fromUri(uri: Uri, readText: (Uri) -> String?): AppEvent? {
        if (uri.scheme == "hashlark") {
            return when (uri.host) {
                "repo" -> uri.getQueryParameter("url")?.takeIf { it.startsWith("http") }?.let { AppEvent.ImportRepo(it) }
                "search" -> uri.getQueryParameter("q")
                    ?.let { SearchIntent.fromSharedText(it) }
                    ?.let { AppEvent.RunSearch(it) }
                else -> null
            }
        }
        if (uri.scheme == "content" || uri.scheme == "file") {
            val yaml = readText(uri)?.takeIf { it.isNotBlank() } ?: return null
            return AppEvent.ImportDefinition(yaml, uri.lastPathSegment)
        }
        return null
    }

    /** A `hashlark://search` link for a query, used by launcher shortcuts. */
    fun searchUri(query: SearchQuery): Uri = Uri.Builder()
        .scheme("hashlark")
        .authority("search")
        .appendQueryParameter("q", query.text.ifBlank { query.imdbId.orEmpty() })
        .build()
}
