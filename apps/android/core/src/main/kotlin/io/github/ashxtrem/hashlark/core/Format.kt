// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark.core

import java.time.Instant
import java.time.OffsetDateTime
import java.time.format.DateTimeParseException
import java.util.Locale

/** Number and date formatting shared by every screen (the same rules as the desktop UI). */
object Format {
    private val units = listOf("B", "KiB", "MiB", "GiB", "TiB", "PiB")
    const val UNKNOWN = "–"

    /** `1536` becomes `1.5 KiB`. */
    fun bytes(n: Long?): String {
        if (n == null) return UNKNOWN
        var value = n.toDouble()
        var unit = 0
        while (value >= 1024 && unit < units.lastIndex) {
            value /= 1024
            unit += 1
        }
        return if (unit == 0) "$n B" else String.format(Locale.US, "%.1f %s", value, units[unit])
    }

    /** Seeder and leecher counts; unknown shows as a dash. */
    fun count(n: Int?): String = if (n == null) UNKNOWN else String.format(Locale.getDefault(), "%,d", n)

    /** Age since an RFC 3339 date: `5h`, `3d`, `4mo`, `2y`. */
    fun age(iso: String?, now: Instant = Instant.now()): String {
        val then = parseInstant(iso) ?: return UNKNOWN
        val secs = maxOf(0L, now.epochSecond - then.epochSecond)
        return when {
            secs < DAY -> "${secs / HOUR}h"
            secs < 30 * DAY -> "${secs / DAY}d"
            secs < 365 * DAY -> "${secs / (30 * DAY)}mo"
            else -> "${secs / (365 * DAY)}y"
        }
    }

    /** `1234` becomes `1.2 s`, `250` becomes `250 ms`. */
    fun duration(ms: Long?): String = when {
        ms == null -> UNKNOWN
        ms < 1000 -> "$ms ms"
        else -> String.format(Locale.US, "%.1f s", ms / 1000.0)
    }

    fun parseInstant(iso: String?): Instant? {
        if (iso.isNullOrBlank()) return null
        return try {
            Instant.parse(iso)
        } catch (_: DateTimeParseException) {
            try {
                OffsetDateTime.parse(iso).toInstant()
            } catch (_: DateTimeParseException) {
                null
            }
        }
    }

    private const val HOUR = 3600L
    private const val DAY = 86_400L
}
