// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark

import androidx.activity.ComponentActivity
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.longClick
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.onRoot
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import io.github.ashxtrem.hashlark.core.AddSource
import io.github.ashxtrem.hashlark.core.BulkAction
import io.github.ashxtrem.hashlark.core.LanDevice
import io.github.ashxtrem.hashlark.core.LanPlatform
import io.github.ashxtrem.hashlark.core.LanReceived
import io.github.ashxtrem.hashlark.core.QueueMove
import io.github.ashxtrem.hashlark.core.QueueStep
import io.github.ashxtrem.hashlark.core.RunMode
import io.github.ashxtrem.hashlark.core.TorrentState
import io.github.ashxtrem.hashlark.ui.AddStage
import io.github.ashxtrem.hashlark.ui.AppEvent
import io.github.ashxtrem.hashlark.ui.Destination
import io.github.ashxtrem.hashlark.ui.ResultActions
import io.github.ashxtrem.hashlark.ui.downloads.AddTorrentFlow
import io.github.ashxtrem.hashlark.ui.rememberResultActions
import io.github.ashxtrem.hashlark.ui.settings.ScheduleCell
import io.github.ashxtrem.hashlark.ui.settings.ScheduleGrid
import io.github.ashxtrem.hashlark.ui.settings.SettingsViewModel
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/** The Downloads screen, the add flow and the navigation with six destinations, against a scripted engine. */
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(application = android.app.Application::class)
@RunWith(RobolectricTestRunner::class)
class DownloadsUiTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    private fun harness(noticeAccepted: Boolean = true, enabled: Boolean = true, grant: Boolean = true) = SearchHarness(compose) { api ->
        // Granted up front: Robolectric never answers a permission prompt (tests that answer one pass grant = false).
        if (grant) {
            org.robolectric.Shadows.shadowOf(androidx.test.core.app.ApplicationProvider.getApplicationContext<android.app.Application>())
                .grantPermissions("android.permission.POST_NOTIFICATIONS", "android.permission.WRITE_EXTERNAL_STORAGE")
        }
        api.settings = api.settings.copy(torrents = api.settings.torrents.copy(noticeAccepted = noticeAccepted, enabled = enabled))
        api.torrentList = listOf(
            torrent("a", TorrentState.Downloading, queuePos = 0, name = "Ubuntu desktop"),
            torrent("b", TorrentState.Queued, queuePos = 1, name = "Debian netinst"),
            torrent("c", TorrentState.Queued, queuePos = 2, name = "Fedora workstation"),
            torrent("d", TorrentState.Seeding, progress = 1.0, queuePos = 3, name = "Arch linux"),
        )
    }

    private fun SearchHarness.go(destination: Destination) {
        compose.runOnIdle { app.post(AppEvent.Navigate(destination)) }
        compose.waitForIdle()
    }

    private fun count(text: String) = compose.onAllNodesWithText(text).fetchSemanticsNodes().size

    @Test
    @Config(qualifiers = "w360dp-h800dp-xhdpi")
    fun bottom_bar_has_five_items_and_history_is_a_tab_of_favourites() {
        val h = harness()
        h.showApp()
        assertTrue(h.navigationShown())
        assertEquals("Downloads in the bar", 1, count("Downloads"))
        assertEquals("no History item in the bar", 0, count("History"))

        // The History shortcut still works: it opens the History tab of Favourites.
        h.go(Destination.History)
        assertTrue(h.text("No searches yet"))
        assertEquals("the tab, not a bar item", 1, count("History"))
        h.picture("bar-history-tab")
        assertEquals("the tab and the bar item", 2, count("Favourites"))
    }

    @Test
    @Config(qualifiers = "w750dp-h832dp-xhdpi")
    fun rail_has_all_six_destinations() {
        val h = harness()
        h.showApp()
        for (label in listOf("Search", "Downloads", "Favourites", "History", "Providers", "Settings")) {
            assertTrue(label, h.text(label))
        }
        h.go(Destination.History)
        // On a rail History is its own screen, with its own title.
        assertTrue(h.text("No searches yet"))
    }

    @Test
    @Config(qualifiers = "w360dp-h800dp-xhdpi")
    fun downloads_show_filter_counts_and_act_on_a_selection() {
        val h = harness()
        h.showApp()
        h.go(Destination.Downloads)
        compose.waitUntil(5_000) { h.text("Ubuntu desktop") }
        assertTrue(h.text("All 4"))
        assertTrue(h.text("Queued 2"))
        assertTrue(h.text("Finished 1"))
        assertTrue(h.text("Downloading", substring = true))
        h.picture("downloads-360")

        // Long-press selects; the bar acts on the selection.
        compose.onNode(hasContentDescription("Ubuntu desktop", substring = true)).performTouchInput { longClick() }
        compose.waitForIdle()
        assertTrue(h.text("1 selected"))
        compose.onNodeWithText("Pause").performClick()
        compose.waitUntil(5_000) { h.api.bulks.isNotEmpty() }
        assertEquals(listOf("a"), h.api.bulks.last().ids)
        assertEquals(BulkAction.Pause, h.api.bulks.last().action)

        compose.onNodeWithText("Stop").performClick()
        compose.waitUntil(5_000) { h.api.bulks.size >= 2 }
        assertEquals(BulkAction.Stop, h.api.bulks.last().action)

        compose.onNodeWithText("Force start").performClick()
        compose.waitUntil(5_000) { h.api.bulks.size >= 3 }
        assertEquals(BulkAction.ForceStart, h.api.bulks.last().action)

        // Delete keeps the files unless the box is ticked.
        compose.onNodeWithText("Delete").performClick()
        compose.onNodeWithText("Delete this torrent?").assertExists()
        compose.onNodeWithText("Also delete downloaded files").performClick()
        compose.onNodeWithText("Delete and remove files").performClick()
        compose.waitUntil(5_000) { h.api.bulks.size >= 4 }
        assertEquals(BulkAction.Remove(deleteFiles = true), h.api.bulks.last().action)
    }

    @Test
    @Config(qualifiers = "w841dp-h701dp-xhdpi")
    fun downloads_on_the_unfolded_fold_use_the_rail() {
        val h = harness()
        h.showApp()
        h.go(Destination.Downloads)
        compose.waitUntil(5_000) { h.text("Ubuntu desktop") }
        assertTrue(h.text("History"))
        assertTrue(h.text("All 4"))
        h.picture("downloads-fold-841")
    }

    @Test
    @Config(qualifiers = "w360dp-h800dp-xhdpi")
    fun delete_without_files_and_queue_reorder() {
        val h = harness()
        h.showApp()
        h.go(Destination.Downloads)
        compose.waitUntil(5_000) { h.text("Queued 2") }
        compose.onNodeWithText("Queued 2").performClick()
        compose.waitForIdle()
        // In queue order, with a drag handle each.
        assertFalse(h.text("Ubuntu desktop"))
        assertEquals(2, compose.onAllNodes(hasContentDescription("Reorder")).fetchSemanticsNodes().size)

        compose.onNode(hasContentDescription("Fedora workstation", substring = true)).performTouchInput { longClick() }
        compose.onNodeWithText("Move up").performClick()
        compose.waitUntil(5_000) { h.api.patches.isNotEmpty() }
        assertEquals("c" to QueueMove.Step(QueueStep.Up), h.api.patches.last().let { it.first to it.second.queuePos })

        compose.onNodeWithText("Delete").performClick()
        compose.onNodeWithText("Delete this torrent?").assertExists()
        // The dialog's confirm button is the last "Delete" on screen.
        compose.onAllNodes(hasText("Delete") and androidx.compose.ui.test.hasClickAction()).let { nodes ->
            val n = nodes.fetchSemanticsNodes().size
            nodes[n - 1].performClick()
        }
        compose.waitUntil(5_000) { h.api.bulks.any { it.action is BulkAction.Remove } }
        assertEquals(BulkAction.Remove(deleteFiles = false), h.api.bulks.last { it.action is BulkAction.Remove }.action)
    }

    @Test
    @Config(qualifiers = "w360dp-h800dp-xhdpi")
    fun long_press_on_download_opens_the_add_sheet() {
        val h = harness()
        h.showApp()
        h.search()
        h.select("r1")
        compose.onNodeWithText("Download").performTouchInput { longClick() }
        compose.waitForIdle()
        assertEquals(AddStage.Options, h.app.pendingAdd.value?.stage)
        assertTrue(h.text("Add torrent"))
        assertTrue(h.text("Start paused"))
        compose.onNodeWithText("Start paused").performClick()
        // The sheet's button (the details' Download is behind it).
        compose.onAllNodes(hasText("Download") and androidx.compose.ui.test.hasClickAction()).let { nodes ->
            nodes[nodes.fetchSemanticsNodes().size - 1].performClick()
        }
        compose.waitUntil(5_000) { h.api.adds.isNotEmpty() }
        val add = h.api.adds.single()
        assertEquals(AddSource.ResultId("r1"), add.source)
        assertTrue(add.paused)
        assertNull(h.app.pendingAdd.value)
    }

    @Test
    @Config(qualifiers = "w360dp-h800dp-xhdpi")
    fun the_first_add_shows_the_notice_once() {
        val h = harness(noticeAccepted = false)
        h.showApp()
        compose.runOnIdle { h.app.requestAdd(AddSource.Magnet("magnet:?xt=urn:btih:00&dn=Ubuntu")) }
        compose.waitForIdle()
        assertTrue(h.text("shows your IP address", substring = true))
        compose.onNodeWithText("I understand").performClick()
        compose.waitUntil(5_000) { h.api.adds.isNotEmpty() }
        assertTrue(h.api.settings.torrents.noticeAccepted)
        assertTrue(h.app.torrentIds.value.isNotEmpty())

        compose.runOnIdle { h.app.requestAdd(AddSource.Magnet("magnet:?xt=urn:btih:11")) }
        compose.waitUntil(5_000) { h.api.adds.size == 2 }
        assertFalse(h.text("shows your IP address", substring = true))
    }

    @Test
    @Config(qualifiers = "w360dp-h800dp-xhdpi")
    fun with_the_client_off_results_are_handed_off_and_the_screen_offers_to_turn_it_on() {
        val h = harness(enabled = false)
        h.showApp()
        h.go(Destination.Downloads)
        assertTrue(h.text("The built-in client is off"))
        compose.onNodeWithText("Turn on the built-in client").performClick()
        compose.waitUntil(5_000) { h.api.settings.torrents.enabled }
    }

    @Test
    @Config(qualifiers = "w360dp-h800dp-xhdpi")
    fun the_schedule_editor_saves_the_grid() {
        val h = harness()
        val settings = SettingsViewModel(h.app)
        compose.setContent {
            io.github.ashxtrem.hashlark.ui.theme.HashlarkTheme(dynamicColor = false) {
                val draft by settings.draft.collectAsState()
                draft?.torrents?.let { t ->
                    androidx.compose.foundation.layout.Column {
                        io.github.ashxtrem.hashlark.ui.settings.ScheduleGridEditor(t.schedule ?: ScheduleGrid.ALL_NORMAL) { s ->
                            settings.edit { it.copy(torrents = it.torrents.copy(schedule = s)) }
                        }
                    }
                }
            }
        }
        compose.waitUntil(5_000) { h.text("Off") }
        compose.onNodeWithText("Off").performClick()
        compose.onNodeWithText("Mon").performClick()
        compose.onNodeWithContentDescription("Monday 03:00, Normal").performClick()
        compose.waitForIdle()
        compose.runOnIdle { settings.save() }
        compose.waitUntil(5_000) { h.api.settings.torrents.schedule != null }
        val saved = h.api.settings.torrents.schedule
        assertNotNull(saved)
        assertEquals(ScheduleCell.Off, ScheduleGrid.dayStrip(saved, 0)[3])
        assertEquals(ScheduleCell.Normal, ScheduleGrid.dayStrip(saved, 0)[4])
        h.picture("schedule-strip-360")
    }

    @Test
    @Config(qualifiers = "w360dp-h800dp-xhdpi")
    fun a_drag_over_three_hours_paints_all_three() {
        val h = harness()
        val settings = SettingsViewModel(h.app)
        compose.setContent {
            io.github.ashxtrem.hashlark.ui.theme.HashlarkTheme(dynamicColor = false) {
                val draft by settings.draft.collectAsState()
                draft?.torrents?.let { t ->
                    androidx.compose.foundation.layout.Column {
                        io.github.ashxtrem.hashlark.ui.settings.ScheduleGridEditor(t.schedule ?: ScheduleGrid.ALL_NORMAL) { s ->
                            settings.edit { it.copy(torrents = it.torrents.copy(schedule = s)) }
                        }
                    }
                }
            }
        }
        compose.waitUntil(5_000) { h.text("Off") }
        compose.onNodeWithText("Off").performClick()
        compose.onNodeWithText("Mon").performClick()
        compose.waitForIdle()
        val bounds = (3..5).map { hour ->
            compose.onNodeWithContentDescription("Monday 0$hour:00, Normal", substring = true).fetchSemanticsNode().boundsInRoot
        }
        val centers = bounds.map { it.center }
        compose.onRoot().performTouchInput {
            // The drag starts where it passes the touch slop: a finger moves gradually, so that is still in the first cell.
            down(androidx.compose.ui.geometry.Offset(bounds[0].left + 2f, centers[0].y))
            moveTo(androidx.compose.ui.geometry.Offset(bounds[0].right - 2f, centers[0].y))
            moveTo(centers[1])
            moveTo(centers[2])
            up()
        }
        compose.waitForIdle()
        val strip = ScheduleGrid.dayStrip(settings.draft.value?.torrents?.schedule, 0)
        assertEquals(listOf(ScheduleCell.Off, ScheduleCell.Off, ScheduleCell.Off), strip.subList(3, 6))
        assertEquals(ScheduleCell.Normal, strip[2])
        assertEquals(ScheduleCell.Normal, strip[6])
    }

    @Test
    @Config(qualifiers = "w360dp-h800dp-xhdpi")
    fun a_refused_permission_still_adds_the_torrent() {
        val h = harness(grant = false)
        h.showApp()
        compose.runOnIdle { h.app.requestAdd(AddSource.Magnet("magnet:?xt=urn:btih:22")) }
        compose.waitForIdle()
        val request = org.robolectric.Shadows.shadowOf(compose.activity).lastRequestedPermission
        assertNotNull("the notification permission is asked", request)
        assertTrue(h.app.awaitingPermissions)
        assertTrue("nothing is added before the answer", h.api.adds.isEmpty())

        val denied = IntArray(request.requestedPermissions.size) { android.content.pm.PackageManager.PERMISSION_DENIED }
        compose.runOnUiThread {
            @Suppress("DEPRECATION")
            compose.activity.onRequestPermissionsResult(request.requestCode, request.requestedPermissions, denied)
        }
        compose.waitUntil(5_000) { h.api.adds.isNotEmpty() }
        assertEquals(AddSource.Magnet("magnet:?xt=urn:btih:22"), h.api.adds.single().source)
        assertFalse(h.app.awaitingPermissions)
        assertNull(h.app.pendingAdd.value)
    }

    @Test
    @Config(qualifiers = "w360dp-h800dp-xhdpi")
    fun the_add_waits_for_the_permission_answer_when_the_flow_is_composed_again() {
        val h = harness(grant = false)
        var shown by mutableStateOf(true)
        compose.setContent {
            io.github.ashxtrem.hashlark.ui.theme.HashlarkTheme(dynamicColor = false) {
                if (shown) AddTorrentFlow(h.app)
            }
        }
        compose.waitUntil(5_000) { h.app.settings.value != null }
        compose.runOnIdle { h.app.requestAdd(AddSource.Magnet("magnet:?xt=urn:btih:33")) }
        compose.waitForIdle()
        assertTrue(h.app.awaitingPermissions)
        // As after a recreation under the dialog: the flow's effect runs again.
        compose.runOnIdle { shown = false }
        compose.waitForIdle()
        compose.runOnIdle { shown = true }
        compose.waitForIdle()
        assertTrue("not added before the answer", h.api.adds.isEmpty())
        assertEquals(AddStage.Permissions, h.app.pendingAdd.value?.stage)
    }

    @Test
    @Config(qualifiers = "w360dp-h800dp-xhdpi")
    fun no_torrent_client_turns_on_the_built_in_one_and_retries() {
        val h = harness(enabled = false)
        h.showApp()
        h.search()
        h.select("r1")
        compose.onNodeWithText("Open in client").performClick()
        // Robolectric has no app for magnet links.
        compose.waitUntil(5_000) { h.text("No torrent client found") }
        compose.onNodeWithText("Turn on the built-in client").performClick()
        compose.waitUntil(5_000) { h.api.adds.isNotEmpty() }
        assertTrue(h.api.settings.torrents.enabled)
        assertEquals(AddSource.Magnet("magnet:?xt=urn:btih:r1"), h.api.adds.single().source)
        assertFalse(h.text("No torrent client found"))
    }

    @Test
    @Config(qualifiers = "w360dp-h800dp-xhdpi")
    fun a_magnet_link_with_the_client_off_offers_to_turn_it_on_without_the_notice() {
        val h = harness(noticeAccepted = false, enabled = false)
        h.showApp()
        compose.waitUntil(5_000) { h.app.settings.value != null }
        compose.runOnIdle { h.app.post(AppEvent.AddTorrent(AddSource.Magnet("magnet:?xt=urn:btih:66"))) }
        compose.waitUntil(5_000) { h.text("No torrent client found") }
        assertTrue(h.text("Turn on the built-in client", substring = true))
        assertFalse(h.text("shows your IP address", substring = true))
        assertNull(h.app.pendingAdd.value)
        assertTrue(h.api.adds.isEmpty())

        // Turning it on retries the add, which then goes through the notice.
        compose.onNodeWithText("Turn on the built-in client").performClick()
        compose.waitUntil(5_000) { h.text("shows your IP address", substring = true) }
        assertTrue(h.api.settings.torrents.enabled)
        assertFalse(h.text("No torrent client found"))
        compose.onNodeWithText("I understand").performClick()
        compose.waitUntil(5_000) { h.api.adds.isNotEmpty() }
        assertEquals(AddSource.Magnet("magnet:?xt=urn:btih:66"), h.api.adds.single().source)
    }

    @Test
    @Config(qualifiers = "w360dp-h800dp-xhdpi")
    fun a_torrent_file_with_the_client_off_asks_to_turn_it_on_and_retries() {
        val h = harness(enabled = false)
        h.showApp()
        compose.waitUntil(5_000) { h.app.settings.value != null }
        compose.runOnIdle { h.app.post(AppEvent.AddTorrent(AddSource.Torrent("ZGF0YQ=="))) }
        compose.waitUntil(5_000) { h.text("The built-in client is off") }
        assertNull(h.app.pendingAdd.value)
        assertTrue(h.api.adds.isEmpty())

        compose.onNodeWithText("Turn on the built-in client").performClick()
        compose.waitUntil(5_000) { h.api.adds.isNotEmpty() }
        assertTrue(h.api.settings.torrents.enabled)
        assertEquals(AddSource.Torrent("ZGF0YQ=="), h.api.adds.single().source)
        assertFalse(h.text("The built-in client is off"))
    }

    private fun showActions(h: SearchHarness): () -> ResultActions {
        var actions: ResultActions? = null
        compose.setContent {
            io.github.ashxtrem.hashlark.ui.theme.HashlarkTheme(dynamicColor = false) {
                actions = rememberResultActions(h.app) {}
                AddTorrentFlow(h.app)
            }
        }
        compose.waitUntil(5_000) { h.app.settings.value != null }
        return { actions!! }
    }

    private val desk = LanDevice("desk", "Desk", LanPlatform.Desktop, paired = true, online = true)

    @Test
    @Config(qualifiers = "w360dp-h800dp-xhdpi")
    fun a_lan_link_goes_to_the_built_in_client_when_it_is_on() {
        val h = harness()
        val actions = showActions(h)
        compose.runOnIdle { actions().openReceived(LanReceived(desk, "magnet:?xt=urn:btih:44", "Debian")) }
        compose.waitUntil(5_000) { h.api.adds.isNotEmpty() }
        assertEquals(AddSource.Magnet("magnet:?xt=urn:btih:44"), h.api.adds.single().source)
        assertNull(actions().noHandlerMagnet)
    }

    @Test
    @Config(qualifiers = "w360dp-h800dp-xhdpi")
    fun a_lan_link_goes_to_the_torrent_app_when_the_client_is_off() {
        val h = harness(enabled = false)
        val actions = showActions(h)
        compose.runOnIdle { actions().openReceived(LanReceived(desk, "magnet:?xt=urn:btih:55", "Debian")) }
        // Handed to openMagnet: with no torrent app installed, it ends in the No torrent client dialog's state.
        compose.waitUntil(5_000) { actions().noHandlerMagnet != null }
        assertEquals("magnet:?xt=urn:btih:55", actions().noHandlerMagnet)
        assertTrue(h.api.adds.isEmpty())
        assertNull(h.app.pendingAdd.value)
    }
}
