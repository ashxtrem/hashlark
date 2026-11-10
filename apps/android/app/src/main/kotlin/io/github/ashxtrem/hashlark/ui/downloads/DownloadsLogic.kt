// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark.ui.downloads

import io.github.ashxtrem.hashlark.core.Format
import io.github.ashxtrem.hashlark.core.QueueStep
import io.github.ashxtrem.hashlark.core.RunMode
import io.github.ashxtrem.hashlark.core.TorrentAction
import io.github.ashxtrem.hashlark.core.TorrentState
import io.github.ashxtrem.hashlark.core.TorrentSummary

/** The filter chips of the Downloads screen, in the order they are shown (as on the desktop). */
enum class DownloadFilter(val label: String) {
    All("All"),
    Downloading("Downloading"),
    Seeding("Seeding"),
    Queued("Queued"),
    Paused("Paused"),
    Stopped("Stopped"),
    Finished("Finished"),
    Error("Error"),
}

/**
 * Pure helpers of the Downloads screen (plan §7.2 and §9.3), mirroring
 * `apps/desktop/src/lib/torrents.ts` so both clients agree on wording and filters.
 */
object DownloadsLogic {
    private val fetching = setOf(TorrentState.Metadata, TorrentState.Checking, TorrentState.Downloading)

    /** States in which the Wi-Fi gate, the schedule or the battery hold a torrent back; they count as queued. */
    private val held = setOf(
        TorrentState.Queued,
        TorrentState.ScheduledOff,
        TorrentState.ScheduledSeedOnly,
        TorrentState.OnBattery,
        TorrentState.WaitingForNetwork,
    )

    /** 100% of the selected files, whether seeding, done (seed limit) or stopped. */
    fun isComplete(t: TorrentSummary): Boolean = when (t.state) {
        TorrentState.Seeding, TorrentState.Finished, TorrentState.Done -> true
        TorrentState.Metadata, TorrentState.Error -> false
        else -> t.progress >= 1.0
    }

    /** Whether [t] shows under [filter]. The chips overlap: Finished also holds complete seeding and stopped torrents. */
    fun matches(t: TorrentSummary, filter: DownloadFilter): Boolean = when (filter) {
        DownloadFilter.All -> true
        DownloadFilter.Downloading -> t.state in fetching
        DownloadFilter.Seeding -> t.state == TorrentState.Seeding
        DownloadFilter.Queued -> t.state in held
        DownloadFilter.Paused -> t.state == TorrentState.Paused
        DownloadFilter.Stopped -> t.state == TorrentState.Stopped || t.state == TorrentState.Finished ||
            (t.runMode == RunMode.Stopped && t.state != TorrentState.Error)
        DownloadFilter.Finished -> isComplete(t)
        DownloadFilter.Error -> t.state == TorrentState.Error
    }

    /** How many torrents each chip holds. */
    fun counts(list: List<TorrentSummary>): Map<DownloadFilter, Int> =
        DownloadFilter.entries.associateWith { f -> list.count { matches(it, f) } }

    /** Queue order: `queue_pos`, then the time added (as the core sorts). */
    fun byQueue(list: List<TorrentSummary>): List<TorrentSummary> =
        list.sortedWith(compareBy<TorrentSummary> { it.queuePos }.thenBy { it.addedAt })

    /** The rows a chip shows, in queue order. */
    fun visible(list: List<TorrentSummary>, filter: DownloadFilter): List<TorrentSummary> =
        byQueue(list).filter { matches(it, filter) }

    /** Why a torrent is (not) moving. */
    fun statusLabel(t: TorrentSummary): String {
        val label = when (t.state) {
            TorrentState.Metadata -> "Fetching metadata"
            TorrentState.Checking -> "Checking"
            TorrentState.Downloading -> "Downloading"
            TorrentState.Seeding -> "Seeding"
            TorrentState.Queued -> "Queued"
            TorrentState.Paused -> "Paused"
            TorrentState.Stopped -> "Stopped"
            TorrentState.Finished -> "Finished"
            TorrentState.Done -> "Done (seed limit)"
            TorrentState.ScheduledOff -> "Scheduled: off"
            TorrentState.ScheduledSeedOnly -> "Scheduled: seed only"
            TorrentState.OnBattery -> "On battery"
            TorrentState.WaitingForNetwork -> "Waiting for Wi-Fi"
            TorrentState.Error -> "Error"
        }
        val running = t.state == TorrentState.Downloading || t.state == TorrentState.Seeding || t.state == TorrentState.Metadata
        return if (t.runMode == RunMode.Forced && running) "$label (forced)" else label
    }

    /** Resume when every selected torrent is paused, else pause. */
    fun pauseToggle(list: List<TorrentSummary>): TorrentAction =
        if (list.isNotEmpty() && list.all { it.runMode == RunMode.Paused }) TorrentAction.Resume else TorrentAction.Pause

    /** Start when every selected torrent is stopped, else stop. */
    fun stopToggle(list: List<TorrentSummary>): TorrentAction =
        if (list.isNotEmpty() && list.all { it.runMode == RunMode.Stopped }) TorrentAction.Start else TorrentAction.Stop

    /** The whole-queue index that puts a dragged torrent in [targetId]'s place. */
    fun dropIndex(all: List<TorrentSummary>, targetId: String): Int? =
        byQueue(all).indexOfFirst { it.id == targetId }.takeIf { it >= 0 }

    /**
     * Order to send one queue step per torrent so a multi-selection keeps its own order:
     * top and down go from the last, up and bottom from the first.
     */
    fun moveOrder(all: List<TorrentSummary>, ids: Collection<String>, step: QueueStep): List<String> {
        val ordered = byQueue(all).filter { it.id in ids }.map { it.id }
        return if (step == QueueStep.Top || step == QueueStep.Down) ordered.reversed() else ordered
    }

    /** `1.5 MB/s`; nothing moving is `–`. */
    fun speed(bps: Long): String = if (bps <= 0) "–" else "${Format.bytes(bps)}/s"

    /** Seconds left: `45s`, `3m 20s`, `2h 5m`, `3d 4h`; unknown is `–`, over 100 days `∞`. */
    fun eta(secs: Long?): String {
        if (secs == null || secs < 0) return "–"
        return when {
            secs < 60 -> "${secs}s"
            secs < 3600 -> "${secs / 60}m ${secs % 60}s"
            secs < 86_400 -> "${secs / 3600}h ${(secs % 3600) / 60}m"
            secs < 100 * 86_400L -> "${secs / 86_400}d ${(secs % 86_400) / 3600}h"
            else -> "∞"
        }
    }

    /** `0.4567` → `45.6%`, rounded down so only a complete torrent shows 100%. */
    fun percent(progress: Double): String {
        val tenths = (progress.coerceIn(0.0, 1.0) * 1000).toInt()
        return "${tenths / 10}.${tenths % 10}%"
    }

    /** Share ratio with two decimals, rounded down; `∞` when it can't be computed. */
    fun ratio(r: Double): String {
        if (r.isNaN()) return "–"
        if (r.isInfinite()) return "∞"
        val hundredths = (r.coerceAtLeast(0.0) * 100).toLong()
        return "${hundredths / 100}.${(hundredths % 100).toString().padStart(2, '0')}"
    }

    /** Whether a result's info hash is in the client (ids are lowercase hex). */
    fun inClient(infoHash: String?, ids: Set<String>): Boolean = infoHash != null && infoHash.lowercase() in ids
}
