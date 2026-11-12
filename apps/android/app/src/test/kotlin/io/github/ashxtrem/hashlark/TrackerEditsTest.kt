// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark

import io.github.ashxtrem.hashlark.core.TorrentState
import io.github.ashxtrem.hashlark.ui.downloads.DownloadsLogic
import io.github.ashxtrem.hashlark.ui.downloads.TrackerEdits
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class TrackerEditsTest {
    private val a = "udp://a.example:1337/announce"
    private val b = "https://b.example/announce"
    private val c = "wss://c.example/announce"

    @Test
    fun `only the engine's schemes are tracker URLs`() {
        for (url in listOf(a, b, c, "http://x/announce", "ws://x", "UDP://x:1")) assertTrue(url, TrackerEdits.isValid(url))
        for (url in listOf("ftp://x", "udp://", "a.example/announce", "://x", "http://a b", "http://" + "x".repeat(2050))) {
            assertFalse(url, TrackerEdits.isValid(url))
        }
    }

    @Test
    fun `a paste is trimmed, blank lines are ignored and bad lines are kept apart`() {
        val parsed = TrackerEdits.parse("  $a  \n\n\r\n$b\nftp://nope\n")
        assertEquals(listOf(a, b), parsed.valid)
        assertEquals(listOf("ftp://nope"), parsed.invalid)
        assertNull(TrackerEdits.problem(TrackerEdits.parse("$a\n").invalid))
        assertTrue(TrackerEdits.problem(parsed.invalid)!!.endsWith("ftp://nope"))
    }

    @Test
    fun `add, edit and remove keep the order and drop duplicates`() {
        assertEquals(listOf(a, b, c), TrackerEdits.add(listOf(a, b), listOf(b, c, a)))
        assertEquals(listOf(a, c), TrackerEdits.edit(listOf(a, b), 1, " $c "))
        // Editing a row into a copy of another drops the later one.
        assertEquals(listOf(a), TrackerEdits.edit(listOf(a, b), 1, a))
        assertEquals(listOf(a, c), TrackerEdits.remove(listOf(a, b, c), 1))
        assertEquals(listOf(a), TrackerEdits.remove(listOf(a), 5))
    }

    @Test
    fun `reannounce needs a running torrent and recheck the metadata`() {
        val metadata = torrent("m", TorrentState.Metadata)
        val paused = torrent("p", TorrentState.Paused)
        val seeding = torrent("s", TorrentState.Seeding)
        assertFalse(DownloadsLogic.canRecheck(listOf(metadata)))
        assertFalse(DownloadsLogic.canReannounce(listOf(metadata)))
        assertTrue(DownloadsLogic.canRecheck(listOf(metadata, paused)))
        assertFalse(DownloadsLogic.canReannounce(listOf(metadata, paused)))
        assertTrue(DownloadsLogic.canReannounce(listOf(paused, seeding)))
        assertTrue(DownloadsLogic.canReannounce(listOf(torrent("d", TorrentState.Downloading))))
    }
}
