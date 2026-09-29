// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark

import androidx.activity.ComponentActivity
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.hasSetTextAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isSelected
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollToNode
import androidx.compose.ui.unit.dp
import io.github.ashxtrem.hashlark.ui.Posture
import io.github.ashxtrem.hashlark.ui.WindowShape
import io.github.ashxtrem.hashlark.ui.search.RESULTS_TAG
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode

/**
 * Renders the app at the window sizes of the refactor (narrow phones, the Galaxy Z Fold7 cover and inner
 * displays as computed from its specifications, the 840 dp boundary, tablets, short and split-screen
 * windows, large text), checks which layout each one gets, and saves a picture of each to
 * `app/build/screenshots/` (CI uploads them, so a layout change can be looked at in a review).
 *
 * Robolectric renders on the JVM: this proves layout decisions and clipping-free composition at those sizes,
 * not what a physical device does with its hinge, keyboard or gestures. System bars are not simulated
 * (insets are zero here), so real windows are a little smaller than these.
 */
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(application = android.app.Application::class)
@RunWith(RobolectricTestRunner::class)
class LayoutMatrixTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    private fun harness() = SearchHarness(compose)

    private fun SearchHarness.open(name: String, fontScale: Float = 1f): SearchHarness {
        showApp(fontScale)
        search()
        text("12 results", substring = true)
        picture("$name-list")
        return this
    }

    // ----- narrow windows: bottom navigation, results only, full-screen details -----------------------

    @Test
    @Config(qualifiers = "w320dp-h640dp-xhdpi")
    fun narrow_phone_320() {
        val h = harness().open("phone-320")
        assertTrue(h.navigationShown())
        assertFalse(h.detailsShown())
        compose.onNode(hasSetTextAction()).assertIsDisplayed()
        h.select("r1")
        assertTrue(h.fullScreenDetails())
        h.picture("phone-320-details")
    }

    @Test
    @Config(qualifiers = "w360dp-h800dp-xhdpi")
    fun narrow_phone_360_details_replace_the_list_and_hide_navigation() {
        val h = harness().open("phone-360")
        assertTrue("bottom navigation with the list", h.navigationShown())
        h.select("r2")
        assertTrue(h.fullScreenDetails())
        assertFalse("navigation is hidden in full-screen details", h.navigationShown())
        h.picture("phone-360-details")

        h.back()
        assertTrue(h.listShown())
        assertTrue(h.navigationShown())
        assertFalse(h.detailsShown())
        // The row that was open stays marked as selected.
        assertTrue(compose.onAllNodes(isSelected()).fetchSemanticsNodes().isNotEmpty())
    }

    @Test
    @Config(qualifiers = "w411dp-h960dp-xhdpi")
    fun fold7_cover_screen_411() {
        val h = harness().open("fold7-cover-411")
        assertTrue(h.navigationShown())
        assertFalse(h.detailsShown())
        h.select("r3")
        assertTrue(h.fullScreenDetails())
        h.picture("fold7-cover-411-details")
    }

    // ----- the Fold7's inner display: no forced split ---------------------------------------------------

    @Test
    @Config(qualifiers = "w750dp-h832dp-xhdpi")
    fun fold7_inner_portrait_uses_the_list_then_full_screen_flow() {
        val h = harness().open("fold7-inner-750x832")
        assertTrue("results use the whole width before anything is selected", h.listShown())
        assertFalse("no empty details pane", h.text("Select a result"))
        assertFalse(h.detailsShown())
        h.select("r1")
        assertTrue("750 dp is too narrow for two comfortable panes", h.fullScreenDetails())
        h.picture("fold7-inner-750x832-details")
        h.back()
        assertTrue(h.listShown())
    }

    @Test
    @Config(qualifiers = "w832dp-h750dp-xhdpi")
    fun fold7_inner_landscape_uses_the_list_then_full_screen_flow() {
        val h = harness().open("fold7-inner-832x750")
        assertFalse(h.detailsShown())
        h.select("r1")
        assertTrue(h.fullScreenDetails())
        h.picture("fold7-inner-832x750-details")
    }

    // ----- the 840 dp boundary ---------------------------------------------------------------------------

    @Test
    @Config(qualifiers = "w839dp-h800dp-xhdpi")
    fun just_below_840_stays_single_pane() {
        val h = harness().open("width-839")
        h.select("r1")
        assertTrue(h.fullScreenDetails())
    }

    @Test
    @Config(qualifiers = "w840dp-h800dp-xhdpi")
    fun at_840_details_appear_beside_the_results() {
        val h = harness().open("width-840")
        assertFalse("no details pane before selection", h.detailsShown())
        h.select("r1")
        assertTrue(h.sideBySide())
        assertTrue("navigation stays with two panes", h.navigationShown())
        assertTrue("the selected row is marked", compose.onAllNodes(isSelected()).fetchSemanticsNodes().isNotEmpty())
        h.picture("width-840-split")
    }

    // ----- tablets ----------------------------------------------------------------------------------------

    @Test
    @Config(qualifiers = "w1024dp-h768dp-xhdpi")
    fun tablet_1024_shows_results_and_details() {
        val h = harness().open("tablet-1024")
        h.select("r1")
        assertTrue(h.sideBySide())
        h.picture("tablet-1024-split")
        // Closing the details gives the width back to the results.
        compose.onNodeWithTag(RESULTS_TAG).assertIsDisplayed()
        h.back()
        assertFalse(h.detailsShown())
        assertTrue(h.listShown())
    }

    @Test
    @Config(qualifiers = "w1280dp-h800dp-xhdpi")
    fun wide_1280_has_the_expanded_sidebar() {
        val h = harness().open("wide-1280")
        assertTrue("the sidebar carries the app name", h.text("Hashlark"))
        assertFalse(h.text("Select a result"))
        h.select("r1")
        assertTrue(h.sideBySide())
        h.picture("wide-1280-split")
    }

    // ----- short and split-screen windows --------------------------------------------------------------

    @Test
    @Config(qualifiers = "w800dp-h360dp-xhdpi")
    fun short_landscape_is_single_pane() {
        val h = harness().open("short-landscape")
        h.select("r1")
        assertTrue(h.fullScreenDetails())
        h.picture("short-landscape-details")
    }

    @Test
    @Config(qualifiers = "w375dp-h832dp-xhdpi")
    fun half_of_a_split_screen_is_a_single_pane() {
        val h = harness().open("split-half-375")
        h.select("r1")
        assertTrue(h.fullScreenDetails())
    }

    @Test
    @Config(qualifiers = "w250dp-h832dp-xhdpi")
    fun a_third_of_a_split_screen_still_works() {
        val h = harness().open("split-third-250")
        assertFalse(h.detailsShown())
        h.select("r1")
        assertTrue(h.fullScreenDetails())
    }

    // ----- text scale --------------------------------------------------------------------------------------

    @Test
    @Config(qualifiers = "w360dp-h800dp-xhdpi")
    fun text_scaled_to_1_3_on_a_phone() {
        val h = harness().open("phone-360-font-1.3", fontScale = 1.3f)
        compose.onNode(hasSetTextAction()).assertIsDisplayed()
        h.select("r1")
        assertTrue(h.fullScreenDetails())
        h.picture("phone-360-font-1.3-details")
    }

    @Test
    @Config(qualifiers = "w360dp-h800dp-xhdpi")
    fun text_scaled_to_2_0_on_a_phone() {
        val h = harness().open("phone-360-font-2.0", fontScale = 2.0f)
        compose.onNode(hasSetTextAction()).assertIsDisplayed()
        h.select("r1")
        assertTrue(h.fullScreenDetails())
        h.picture("phone-360-font-2.0-details")
    }

    @Test
    @Config(qualifiers = "w1024dp-h768dp-xhdpi")
    fun text_scaling_can_turn_two_panes_into_one() {
        val h = harness().open("tablet-1024-font-1.3", fontScale = 1.3f)
        h.select("r1")
        assertTrue("1.3x still fits on 1024 dp", h.sideBySide())
    }

    @Test
    @Config(qualifiers = "w1280dp-h900dp-xhdpi")
    fun double_size_text_never_forces_two_panes() {
        val h = harness().open("wide-1280-font-2.0", fontScale = 2.0f)
        h.select("r1")
        assertTrue("2.0x needs more room than 1280 dp offers, so the details go full-screen", h.fullScreenDetails())
        h.picture("wide-1280-font-2.0-details")
    }

    // ----- state continuity while the layout changes ------------------------------------------------------

    @Test
    @Config(qualifiers = "w1200dp-h800dp-xhdpi")
    fun folding_keeps_the_open_result_and_never_reopens_a_closed_one() {
        val h = harness()
        h.showScreen(WindowShape(1024.dp, 800.dp))
        h.search()
        h.select("r1")
        assertTrue("wide: side by side", h.sideBySide())

        // Fold to the cover screen's width: the same result is now shown full-screen.
        h.shape = WindowShape(411.dp, 960.dp)
        compose.waitForIdle()
        assertTrue(h.fullScreenDetails())
        assertEquals("r1", h.search.state.value.selectedId)

        // Close it, unfold: the list is back at full width and nothing reopens by itself.
        h.back()
        assertTrue(h.listShown())
        h.shape = WindowShape(1024.dp, 800.dp)
        compose.waitForIdle()
        assertFalse(h.detailsShown())
        assertTrue(h.listShown())
        assertEquals("r1", h.search.state.value.selectedId)

        // Opening it again on the wide window shows it beside the results.
        h.select("r1")
        assertTrue(h.sideBySide())
    }

    @Test
    @Config(qualifiers = "w1200dp-h800dp-xhdpi")
    fun the_fold7_inner_shape_and_a_wider_one_switch_layout_at_the_threshold() {
        val h = harness()
        h.showScreen(WindowShape(832.dp, 750.dp))
        h.search()
        h.select("r2")
        assertTrue(h.fullScreenDetails())
        h.shape = WindowShape(840.dp, 750.dp)
        compose.waitForIdle()
        assertTrue("at 840 dp both panes fit", h.sideBySide())
        h.shape = WindowShape(839.dp, 750.dp)
        compose.waitForIdle()
        assertTrue("one dp less and they fall back to one pane", h.fullScreenDetails())
    }

    @Test
    @Config(qualifiers = "w750dp-h832dp-xhdpi")
    fun back_returns_to_the_same_place_in_a_long_list() {
        val h = SearchHarness(compose, results = sampleResults(60))
        h.showApp()
        h.search()
        text60(h)
        compose.onNodeWithTag(RESULTS_TAG).performScrollToNode(hasText("Ubuntu 26.04 build 45"))
        compose.onNodeWithText("Ubuntu 26.04 build 45").assertIsDisplayed()
        compose.onNodeWithText("Ubuntu 26.04 build 45").performClick()
        compose.waitForIdle()
        assertTrue(h.fullScreenDetails())

        h.back()
        // Same place, and the row that was open is still on screen and marked.
        compose.onNodeWithText("Ubuntu 26.04 build 45").assertIsDisplayed()
        assertTrue(compose.onAllNodes(isSelected()).fetchSemanticsNodes().isNotEmpty())
    }

    private fun text60(h: SearchHarness) = assertTrue(h.text("60 results", substring = true))

    // ----- long names and seed counts --------------------------------------------------------------------

    @Test
    @Config(qualifiers = "w360dp-h800dp-xhdpi")
    fun rows_label_seeds_and_tell_unknown_from_zero() {
        val h = harness().open("rows-360")
        assertTrue(h.text("3 sources", substring = true))
        compose.onNodeWithTag(RESULTS_TAG).performScrollToNode(hasText("Ubuntu 26.04 build 2 (unknown seeders)"))
        assertTrue(h.text("Seeds unknown", substring = true))
        compose.onNodeWithTag(RESULTS_TAG).performScrollToNode(hasText("Ubuntu 26.04 build 3 (zero seeders)"))
        assertTrue(h.text("Seeds 0", substring = true))
        // The complete filename is in the row (clamped to two lines visually) and in the details.
        h.select("r1")
        assertTrue(compose.onAllNodesWithText("Ubuntu.26.04.LTS.Desktop.amd64.ISO.[Official.Release].Multi-Language.Checksums.Included.Torrent-UbuntuGroup").fetchSemanticsNodes().isNotEmpty())
    }

    @Test
    @Config(qualifiers = "w360dp-h800dp-xhdpi")
    fun details_say_what_is_not_reported() {
        val h = harness().open("details-unknown")
        h.select("r2")
        assertTrue(h.text("Not reported", substring = true))
    }
}

