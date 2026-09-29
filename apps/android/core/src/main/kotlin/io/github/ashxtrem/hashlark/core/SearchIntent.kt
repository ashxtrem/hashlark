// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark.core

import java.net.URI

/**
 * Turns text that arrives from outside (a share sheet, the text-selection
 * menu, a deep link) into a search.
 */
object SearchIntent {
    private val imdbId = Regex("""\btt\d{6,10}\b""", RegexOption.IGNORE_CASE)
    private val imdbHost = Regex("""(^|\.)imdb\.com$""", RegexOption.IGNORE_CASE)
    private val whitespace = Regex("""\s+""")

    /**
     * An IMDb link or id becomes an exact search; anything else is a text
     * search with extra whitespace trimmed. Returns `null` when there is
     * nothing to search for.
     */
    fun fromSharedText(raw: String?): SearchQuery? {
        val text = raw?.trim().orEmpty()
        if (text.isEmpty()) return null
        imdbIdIn(text)?.let { return SearchQuery(text = "", imdbId = it) }
        val cleaned = text.replace(whitespace, " ").take(MAX_QUERY_CHARS).trim()
        return SearchQuery(text = cleaned).takeUnless { it.isEmpty }
    }

    /** The IMDb id (`tt0063350`) in an imdb.com URL or a bare id, lowercase. */
    fun imdbIdIn(text: String): String? {
        val trimmed = text.trim()
        val match = imdbId.find(trimmed) ?: return null
        if (trimmed.equals(match.value, ignoreCase = true)) return match.value.lowercase()
        val host = trimmed.split(whitespace).firstNotNullOfOrNull { token ->
            runCatching { URI(token).host }.getOrNull()
        }
        return if (host != null && imdbHost.containsMatchIn(host)) match.value.lowercase() else null
    }

    /** Magnet links are handed to a torrent client, never searched. */
    fun isMagnet(text: String): Boolean = text.trim().startsWith("magnet:", ignoreCase = true)

    private const val MAX_QUERY_CHARS = 200
}
