// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark.ui.downloads

/**
 * The staged tracker list of the Trackers tab (plan torrent-trackers §2): pure, so the
 * add/edit/remove rules are tested without Compose. The engine checks again on save.
 */
object TrackerEdits {
    /** Schemes the engine accepts. */
    val schemes = listOf("http", "https", "udp", "ws", "wss")

    /** At most this many trackers per torrent. */
    const val MAX = 100

    private const val MAX_BYTES = 2048

    /** Whether [url] (already trimmed) looks like a tracker URL: an allowed scheme, a host part, no spaces. */
    fun isValid(url: String): Boolean {
        val sep = url.indexOf("://")
        if (sep <= 0 || url.length <= sep + 3) return false
        if (url.any { it.isWhitespace() } || url.toByteArray().size > MAX_BYTES) return false
        return url.substring(0, sep).lowercase() in schemes
    }

    /** The lines of a paste: trimmed, blank lines dropped, split into valid URLs and the rest. */
    fun parse(text: String): Parsed {
        val lines = text.lines().map { it.trim() }.filter { it.isNotEmpty() }
        val (valid, invalid) = lines.partition(::isValid)
        return Parsed(valid, invalid)
    }

    data class Parsed(val valid: List<String>, val invalid: List<String>)

    /** [staged] with [urls] appended; duplicates dropped, first one kept. */
    fun add(staged: List<String>, urls: List<String>): List<String> = (staged + urls).distinct()

    /** [staged] with the URL at [index] replaced by [url] (trimmed); a duplicate of another row is dropped. */
    fun edit(staged: List<String>, index: Int, url: String): List<String> {
        if (index !in staged.indices) return staged
        return staged.toMutableList().also { it[index] = url.trim() }.distinct()
    }

    /** [staged] without the URL at [index]. */
    fun remove(staged: List<String>, index: Int): List<String> =
        if (index in staged.indices) staged.filterIndexed { i, _ -> i != index } else staged

    /**
     * The message under the add field for the lines that aren't tracker URLs, or `null` when all are.
     * Blank lines are not reported: an empty field shows no hint.
     */
    fun problem(invalid: List<String>): String? {
        val bad = invalid.filter { it.isNotBlank() }
        return when {
            bad.isEmpty() -> null
            bad.size == 1 -> "Not a tracker URL (${schemes.joinToString(", ")}): ${bad.single()}"
            else -> "${bad.size} lines aren't tracker URLs (${schemes.joinToString(", ")}): ${bad.first()}, …"
        }
    }
}
