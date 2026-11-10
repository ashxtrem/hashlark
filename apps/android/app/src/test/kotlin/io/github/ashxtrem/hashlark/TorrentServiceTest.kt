// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark

import android.content.ComponentName
import android.content.Intent
import android.content.IntentFilter
import io.github.ashxtrem.hashlark.core.KeepAwake
import io.github.ashxtrem.hashlark.core.OnBattery
import io.github.ashxtrem.hashlark.core.RunMode
import io.github.ashxtrem.hashlark.core.SessionStats
import io.github.ashxtrem.hashlark.core.TorrentOrigin
import io.github.ashxtrem.hashlark.core.TorrentState
import io.github.ashxtrem.hashlark.core.TorrentSummary
import io.github.ashxtrem.hashlark.system.MagnetHandler
import io.github.ashxtrem.hashlark.system.MagnetLauncher
import io.github.ashxtrem.hashlark.system.NetworkGate
import io.github.ashxtrem.hashlark.system.PowerGate
import io.github.ashxtrem.hashlark.system.TorrentServiceLogic
import io.github.ashxtrem.hashlark.system.TorrentStorage
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment
import org.robolectric.Shadows.shadowOf
import org.robolectric.annotation.Config

// A plain Application: the real one schedules background work, which these tests do not need.
@Config(application = android.app.Application::class)
@RunWith(RobolectricTestRunner::class)
class TorrentServiceTest {
    @get:Rule
    val temp = TemporaryFolder()

    private fun torrent(
        state: TorrentState,
        mode: RunMode = RunMode.Auto,
        done: Boolean = false,
        size: Long = 100,
        progress: Double = 0.0,
        completed: Boolean = false,
    ) = TorrentSummary(
        id = state.name + mode.name,
        name = "t",
        state = state,
        runMode = mode,
        source = TorrentOrigin.Magnet,
        size = size,
        progress = progress,
        done = done,
        completedAt = if (completed) 1L else null,
    )

    private val running = SessionStats(running = true)

    // ----- Wi-Fi gate ----------------------------------------------------------------

    @Test
    fun `the network gate only allows metered networks when the user does`() {
        assertTrue(NetworkGate.allowed(unmetered = true, unmeteredOnly = true))
        assertFalse(NetworkGate.allowed(unmetered = false, unmeteredOnly = true))
        assertTrue(NetworkGate.allowed(unmetered = false, unmeteredOnly = false))
        assertTrue(NetworkGate.allowed(unmetered = true, unmeteredOnly = false))
    }

    // ----- battery gate --------------------------------------------------------------

    @Test
    fun `power saver counts as on battery unless charging`() {
        assertTrue(PowerGate.onBattery(powerSaver = true, levelPercent = 80, charging = false))
        assertFalse(PowerGate.onBattery(powerSaver = true, levelPercent = 80, charging = true))
    }

    @Test
    fun `a level under 15 percent counts as on battery unless charging`() {
        assertTrue(PowerGate.onBattery(powerSaver = false, levelPercent = 14, charging = false))
        assertTrue(PowerGate.onBattery(powerSaver = false, levelPercent = 10, charging = false))
        assertFalse(PowerGate.onBattery(powerSaver = false, levelPercent = 15, charging = false))
        assertFalse(PowerGate.onBattery(powerSaver = false, levelPercent = 10, charging = true))
        assertFalse(PowerGate.onBattery(powerSaver = false, levelPercent = null, charging = false))
    }

    @Test
    fun `battery level and charging are read from the battery intent`() {
        assertEquals(10, PowerGate.levelPercent(level = 10, scale = 100))
        assertEquals(50, PowerGate.levelPercent(level = 128, scale = 256))
        assertNull(PowerGate.levelPercent(level = -1, scale = 100))
        assertNull(PowerGate.levelPercent(level = 5, scale = 0))
        assertTrue(PowerGate.isCharging(plugged = android.os.BatteryManager.BATTERY_PLUGGED_USB))
        assertFalse(PowerGate.isCharging(plugged = 0))
    }

    // ----- service decisions ---------------------------------------------------------

    @Test
    fun `the service runs while something downloads, seeds or waits`() {
        for (state in listOf(
            TorrentState.Metadata, TorrentState.Checking, TorrentState.Downloading, TorrentState.Seeding,
            TorrentState.WaitingForNetwork, TorrentState.OnBattery, TorrentState.ScheduledOff,
        )) {
            assertTrue(state.name, TorrentServiceLogic.keepsRunning(running, listOf(torrent(state))))
        }
    }

    @Test
    fun `paused, stopped, queued and finished torrents don't keep the service`() {
        val idle = listOf(
            torrent(TorrentState.Paused, RunMode.Paused),
            torrent(TorrentState.Stopped, RunMode.Stopped),
            torrent(TorrentState.Queued),
            torrent(TorrentState.Done, done = true),
            torrent(TorrentState.Error),
            // The core marks every torrent as waiting for Wi-Fi; the user's Stop and finished ones don't count.
            torrent(TorrentState.WaitingForNetwork, RunMode.Stopped),
            torrent(TorrentState.WaitingForNetwork, RunMode.Paused),
            torrent(TorrentState.WaitingForNetwork, done = true),
        )
        assertFalse(TorrentServiceLogic.keepsRunning(running, idle))
        assertFalse(TorrentServiceLogic.keepsRunning(running, emptyList()))
        assertFalse(TorrentServiceLogic.keepsRunning(SessionStats(running = false), listOf(torrent(TorrentState.Downloading))))
    }

    @Test
    fun `opening the app starts the service for unfinished torrents without a state yet`() {
        assertTrue(TorrentServiceLogic.startsOnOpen(running, listOf(torrent(TorrentState.Queued))))
        assertFalse(TorrentServiceLogic.startsOnOpen(running, listOf(torrent(TorrentState.Paused, RunMode.Paused))))
        assertFalse(TorrentServiceLogic.startsOnOpen(running, listOf(torrent(TorrentState.Done, completed = true, done = true))))
    }

    @Test
    fun `keep awake follows the setting`() {
        val downloading = listOf(torrent(TorrentState.Downloading))
        val seeding = listOf(torrent(TorrentState.Seeding))
        val waiting = listOf(torrent(TorrentState.WaitingForNetwork))
        assertFalse(TorrentServiceLogic.keepAwake(KeepAwake.Off, downloading))
        assertTrue(TorrentServiceLogic.keepAwake(KeepAwake.Downloading, downloading))
        assertFalse(TorrentServiceLogic.keepAwake(KeepAwake.Downloading, seeding))
        assertTrue(TorrentServiceLogic.keepAwake(KeepAwake.Active, seeding))
        // Nothing transfers while waiting, so the locks are released.
        assertFalse(TorrentServiceLogic.keepAwake(KeepAwake.Active, waiting))
    }

    @Test
    fun `the notification says why transfers wait`() {
        assertEquals("Waiting for Wi-Fi", TorrentServiceLogic.reason(SessionStats(networkAllowed = false), OnBattery.AltSpeed))
        assertEquals("Paused: battery", TorrentServiceLogic.reason(SessionStats(onBattery = true), OnBattery.Pause))
        assertEquals(
            "On battery: alternative speed",
            TorrentServiceLogic.reason(SessionStats(onBattery = true), OnBattery.AltSpeed),
        )
        assertNull(TorrentServiceLogic.reason(SessionStats(onBattery = true), OnBattery.Nothing))
        assertNull(TorrentServiceLogic.reason(SessionStats(), OnBattery.Pause))
    }

    @Test
    fun `the notification shows overall progress and speeds`() {
        val list = listOf(
            torrent(TorrentState.Downloading, size = 300, progress = 1.0 / 3),
            torrent(TorrentState.WaitingForNetwork, size = 100, progress = 1.0).copy(id = "b"),
            torrent(TorrentState.Seeding, size = 1000, progress = 1.0, completed = true),
        )
        assertEquals(500, TorrentServiceLogic.progress(list))
        val text = TorrentServiceLogic.text(list, running.copy(downBps = 2048, upBps = 0), OnBattery.AltSpeed)
        assertEquals("Downloading 2 torrents", text.title)
        assertEquals("50% · ↓ 2.0 KiB/s  ↑ 0 B/s", text.text)
        assertNull(TorrentServiceLogic.progress(listOf(torrent(TorrentState.Seeding, completed = true))))
    }

    // ----- storage and hand-off ------------------------------------------------------

    @Test
    fun `the download folder is checked with a write probe`() {
        val dir = temp.root.resolve("Download/Hashlark")
        assertTrue(TorrentStorage.canWrite(dir))
        assertTrue(dir.isDirectory)
        assertTrue(dir.listFiles().isNullOrEmpty())
        val file = temp.newFile("not-a-folder")
        assertFalse(TorrentStorage.canWrite(file))
    }

    @Test
    fun `hand-off never offers Hashlark itself`() {
        val found = listOf(
            MagnetHandler("org.client.b", "Beta"),
            MagnetHandler("io.github.ashxtrem.hashlark", "Hashlark"),
            MagnetHandler("org.client.a", "alpha"),
            MagnetHandler("org.client.a", "alpha"),
        )
        assertEquals(
            listOf("org.client.a", "org.client.b"),
            MagnetLauncher.withoutSelf(found, "io.github.ashxtrem.hashlark").map { it.packageName },
        )
    }

    @Test
    fun `handlers on the device leave out our package`() {
        val context = RuntimeEnvironment.getApplication()
        val pm = shadowOf(context.packageManager)
        for (pkg in listOf(context.packageName, "org.client.a")) {
            pm.addActivityIfNotPresent(ComponentName(pkg, "$pkg.Main"))
            pm.addIntentFilterForActivity(
                ComponentName(pkg, "$pkg.Main"),
                IntentFilter(Intent.ACTION_VIEW).apply {
                    addCategory(Intent.CATEGORY_BROWSABLE)
                    addCategory(Intent.CATEGORY_DEFAULT)
                    addDataScheme("magnet")
                },
            )
        }
        assertEquals(listOf("org.client.a"), MagnetLauncher.handlers(context).map { it.packageName })
    }
}
