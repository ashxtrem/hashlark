// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark

import io.github.ashxtrem.hashlark.core.QueueStep
import io.github.ashxtrem.hashlark.core.RunMode
import io.github.ashxtrem.hashlark.core.TorrentAction
import io.github.ashxtrem.hashlark.core.TorrentOrigin
import io.github.ashxtrem.hashlark.core.TorrentState
import io.github.ashxtrem.hashlark.core.TorrentSummary
import io.github.ashxtrem.hashlark.ui.downloads.DownloadFilter
import io.github.ashxtrem.hashlark.ui.downloads.DownloadsLogic
import io.github.ashxtrem.hashlark.ui.settings.ScheduleCell
import io.github.ashxtrem.hashlark.ui.settings.ScheduleGrid
import io.github.ashxtrem.hashlark.ui.settings.TorrentLimits
import io.github.ashxtrem.hashlark.core.TorrentSettings
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

fun torrent(
    id: String,
    state: TorrentState,
    runMode: RunMode = RunMode.Auto,
    progress: Double = 0.5,
    queuePos: Long = 0,
    name: String = "Torrent $id",
) = TorrentSummary(
    id = id,
    name = name,
    state = state,
    runMode = runMode,
    source = TorrentOrigin.Magnet,
    progress = progress,
    size = 1_000_000,
    have = (progress * 1_000_000).toLong(),
    queuePos = queuePos,
)

/** The Downloads screen's filters, wording and queue moves, and the schedule editor's grid mapping. */
class DownloadsLogicTest {
    private val list = listOf(
        torrent("a", TorrentState.Downloading, queuePos = 0),
        torrent("b", TorrentState.Metadata, progress = 0.0, queuePos = 1),
        torrent("c", TorrentState.Seeding, progress = 1.0, queuePos = 2),
        torrent("d", TorrentState.Queued, queuePos = 3),
        torrent("e", TorrentState.WaitingForNetwork, queuePos = 4),
        torrent("f", TorrentState.Paused, RunMode.Paused, queuePos = 5),
        torrent("g", TorrentState.Stopped, RunMode.Stopped, progress = 1.0, queuePos = 6),
        torrent("h", TorrentState.Stopped, RunMode.Stopped, progress = 0.2, queuePos = 7),
        torrent("i", TorrentState.Done, progress = 1.0, queuePos = 8),
        torrent("j", TorrentState.Error, queuePos = 9),
    )

    @Test
    fun `filter counts follow the desktop tabs`() {
        val counts = DownloadsLogic.counts(list)
        assertEquals(10, counts[DownloadFilter.All])
        assertEquals("metadata counts as downloading", 2, counts[DownloadFilter.Downloading])
        assertEquals(1, counts[DownloadFilter.Seeding])
        assertEquals("held back by Wi-Fi counts as queued", 2, counts[DownloadFilter.Queued])
        assertEquals(1, counts[DownloadFilter.Paused])
        assertEquals(2, counts[DownloadFilter.Stopped])
        assertEquals("seeding, done and stopped complete torrents are finished", 3, counts[DownloadFilter.Finished])
        assertEquals(1, counts[DownloadFilter.Error])
        assertEquals(listOf("c", "g", "i"), DownloadsLogic.visible(list, DownloadFilter.Finished).map { it.id })
    }

    @Test
    fun `the Queued filter is in queue order`() {
        val shuffled = listOf(torrent("x", TorrentState.Queued, queuePos = 5), torrent("y", TorrentState.Queued, queuePos = 2))
        assertEquals(listOf("y", "x"), DownloadsLogic.visible(shuffled, DownloadFilter.Queued).map { it.id })
    }

    @Test
    fun `status says why a torrent isn't moving`() {
        assertEquals("Scheduled: off", DownloadsLogic.statusLabel(torrent("a", TorrentState.ScheduledOff)))
        assertEquals("Scheduled: seed only", DownloadsLogic.statusLabel(torrent("a", TorrentState.ScheduledSeedOnly)))
        assertEquals("On battery", DownloadsLogic.statusLabel(torrent("a", TorrentState.OnBattery)))
        assertEquals("Waiting for Wi-Fi", DownloadsLogic.statusLabel(torrent("a", TorrentState.WaitingForNetwork)))
        assertEquals("Downloading (forced)", DownloadsLogic.statusLabel(torrent("a", TorrentState.Downloading, RunMode.Forced)))
    }

    @Test
    fun `pause and stop toggle on the whole selection`() {
        val paused = torrent("f", TorrentState.Paused, RunMode.Paused)
        val running = torrent("a", TorrentState.Downloading)
        assertEquals(TorrentAction.Resume, DownloadsLogic.pauseToggle(listOf(paused)))
        assertEquals(TorrentAction.Pause, DownloadsLogic.pauseToggle(listOf(paused, running)))
        assertEquals(TorrentAction.Start, DownloadsLogic.stopToggle(listOf(torrent("g", TorrentState.Stopped, RunMode.Stopped))))
        assertEquals(TorrentAction.Stop, DownloadsLogic.stopToggle(listOf(running)))
    }

    @Test
    fun `queue moves keep the selection's own order`() {
        assertEquals(listOf("a", "d"), DownloadsLogic.moveOrder(list, setOf("d", "a"), QueueStep.Up))
        assertEquals(listOf("d", "a"), DownloadsLogic.moveOrder(list, setOf("a", "d"), QueueStep.Down))
        assertEquals(4, DownloadsLogic.dropIndex(list, "e"))
    }

    @Test
    fun `formatters`() {
        assertEquals("45.6%", DownloadsLogic.percent(0.4567))
        assertEquals("99.9%", DownloadsLogic.percent(0.9999))
        assertEquals("3m 20s", DownloadsLogic.eta(200))
        assertEquals("–", DownloadsLogic.eta(null))
        assertEquals("–", DownloadsLogic.speed(0))
        assertTrue(DownloadsLogic.inClient("0123ABCD", setOf("0123abcd")))
        assertFalse(DownloadsLogic.inClient(null, setOf("0123abcd")))
    }

    // ----- schedule -----------------------------------------------------------------------

    @Test
    fun `a day strip maps to and from its 24 cells of the grid`() {
        val monday = List(24) { if (it in 9..17) ScheduleCell.Alt else ScheduleCell.Normal }
        val withMonday = ScheduleGrid.withDay(null, 0, monday)
        assertEquals(168, withMonday.length)
        assertEquals("nnnnnnnnnaaaaaaaaannnnnn", withMonday.substring(0, 24))
        assertEquals(monday, ScheduleGrid.dayStrip(withMonday, 0))

        // Sunday 23:00 is the last letter; the other days are untouched.
        val sunday = ScheduleGrid.paint(withMonday, 6, 23, ScheduleCell.Off)
        assertEquals('o', sunday.last())
        assertEquals(ScheduleCell.Off, ScheduleGrid.dayStrip(sunday, 6)[23])
        assertEquals(monday, ScheduleGrid.dayStrip(sunday, 0))
        assertEquals(ScheduleGrid.ALL_NORMAL.substring(24, 144), sunday.substring(24, 144))

        // Tuesday 05:00 is cell 24 + 5.
        assertEquals('s', ScheduleGrid.paint(null, 1, 5, ScheduleCell.SeedOnly)[29])
        assertEquals(List(7) { monday }.flatten(), ScheduleGrid.cells(ScheduleGrid.copyDayToAll(withMonday, 0)))
    }

    @Test
    fun `a missing or malformed schedule reads as all normal`() {
        assertEquals(List(168) { ScheduleCell.Normal }, ScheduleGrid.cells(null))
        assertEquals(List(168) { ScheduleCell.Normal }, ScheduleGrid.cells("nax"))
        assertFalse(ScheduleGrid.isValid("n".repeat(167) + "z"))
        assertTrue(ScheduleGrid.isValid(ScheduleGrid.ALL_NORMAL))
    }

    @Test
    fun `settings checks match the core's validation`() {
        assertTrue(TorrentLimits.proxyValid(null))
        assertTrue(TorrentLimits.proxyValid("socks5h://127.0.0.1:9050"))
        assertTrue(TorrentLimits.proxyValid("socks5://proxy:1080"))
        assertFalse(TorrentLimits.proxyValid("http://proxy:8080"))
        assertFalse(TorrentLimits.proxyValid("socks5://"))
        assertTrue(TorrentLimits.peersValid(TorrentSettings()))
        assertFalse(TorrentLimits.peersValid(TorrentSettings(peerLimitGlobal = 50, peerLimitTorrent = 100)))
    }
}
