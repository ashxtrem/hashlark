// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark.core

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import java.net.HttpURLConnection
import java.net.URL

/** A published release newer than the running app. */
data class AvailableUpdate(val version: String, val pageUrl: String, val notes: String?)

/**
 * Looks for a newer **published** GitHub release. The app only shows a
 * notice and opens the release page; it never installs anything itself, so it
 * needs no install permission. Turned off with `updates.check_for_updates`.
 */
class UpdateChecker(
    private val currentVersion: String,
    private val fetch: suspend (String) -> String = ::httpGet,
    private val endpoint: String = LATEST_RELEASE,
) {
    @Serializable
    private data class Release(
        @SerialName("tag_name") val tagName: String,
        @SerialName("html_url") val htmlUrl: String,
        val body: String? = null,
        val draft: Boolean = false,
        val prerelease: Boolean = false,
    )

    /** The newer release, or `null` when up to date or when it could not be checked. */
    suspend fun check(): AvailableUpdate? {
        val release = runCatching { HashlarkJson.decodeFromString<Release>(fetch(endpoint)) }.getOrNull()
            ?: return null
        if (release.draft || release.prerelease) return null
        return if (isNewer(release.tagName, currentVersion)) {
            AvailableUpdate(release.tagName.removePrefix("v"), release.htmlUrl, release.body)
        } else {
            null
        }
    }

    companion object {
        const val LATEST_RELEASE = "https://api.github.com/repos/ashxtrem/hashlark/releases/latest"

        /** Whether [candidate] (`v1.2.0`) is a higher version than [current] (`1.1.9`). */
        fun isNewer(candidate: String, current: String): Boolean {
            val a = parse(candidate) ?: return false
            val b = parse(current) ?: return false
            for (i in 0 until maxOf(a.size, b.size)) {
                val x = a.getOrElse(i) { 0 }
                val y = b.getOrElse(i) { 0 }
                if (x != y) return x > y
            }
            return false
        }

        private fun parse(version: String): List<Int>? {
            val core = version.trim().removePrefix("v").substringBefore('-').substringBefore('+')
            val parts = core.split('.').map { it.toIntOrNull() ?: return null }
            return parts.takeIf { it.isNotEmpty() }
        }

        private suspend fun httpGet(url: String): String = withContext(Dispatchers.IO) {
            val connection = URL(url).openConnection() as HttpURLConnection
            try {
                connection.connectTimeout = 10_000
                connection.readTimeout = 10_000
                connection.setRequestProperty("Accept", "application/vnd.github+json")
                connection.setRequestProperty("User-Agent", "Hashlark-Android")
                check(connection.responseCode == 200) { "HTTP ${connection.responseCode}" }
                connection.inputStream.bufferedReader().use { it.readText() }
            } finally {
                connection.disconnect()
            }
        }
    }
}
