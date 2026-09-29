// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark

import android.content.Context
import androidx.activity.ComponentActivity
import androidx.compose.ui.focus.FocusRequester
import androidx.core.view.drawToBitmap
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.hasSetTextAction
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.runtime.remember
import androidx.compose.ui.unit.dp
import androidx.test.core.app.ApplicationProvider
import io.github.ashxtrem.hashlark.core.HashlarkRuntime
import io.github.ashxtrem.hashlark.core.SearchEvent
import io.github.ashxtrem.hashlark.system.AppPrefs
import io.github.ashxtrem.hashlark.ui.AppViewModel
import io.github.ashxtrem.hashlark.ui.LibraryViewModel
import io.github.ashxtrem.hashlark.ui.Posture
import io.github.ashxtrem.hashlark.ui.WindowShape
import io.github.ashxtrem.hashlark.ui.rememberResultActions
import io.github.ashxtrem.hashlark.ui.search.SearchScreen
import io.github.ashxtrem.hashlark.ui.search.SearchViewModel
import io.github.ashxtrem.hashlark.ui.theme.HashlarkTheme
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config
import org.robolectric.annotation.GraphicsMode
import java.io.File
import java.io.FileOutputStream

/**
 * Renders the search screen at the window shapes of a Galaxy Z Fold7 (and its
 * split-screen sizes), checks which layout each one gets, and saves a picture of
 * each to `app/build/screenshots/` (CI uploads them, so a layout change can be
 * looked at in a review).
 */
@GraphicsMode(GraphicsMode.Mode.NATIVE)
@Config(application = android.app.Application::class)
@RunWith(RobolectricTestRunner::class)
class LayoutMatrixTest {
    @get:Rule
    val compose = createAndroidComposeRule<ComponentActivity>()

    private val hint = "Select a result"

    private fun render(name: String, width: Int, height: Int, posture: Posture = Posture.Flat) {
        val context = ApplicationProvider.getApplicationContext<Context>()
        val api = FakeApi(script = {
            listOf(
                SearchEvent.ProviderStarted("alpha"),
                SearchEvent.Results("alpha", (1..12).map { mergedResult("r$it", title = "Ubuntu 26.04 build $it", seeders = 100 - it) }),
                SearchEvent.ProviderFinished("alpha", 12, 300),
                SearchEvent.Done(12, 320),
            )
        })
        val runtime = HashlarkRuntime { api }
        val app = AppViewModel(runtime, AppPrefs(context)) { null }
        val search = SearchViewModel(runtime)
        val library = LibraryViewModel(app)
        val shape = WindowShape(width.dp, height.dp, posture)
        compose.setContent {
            HashlarkTheme(dynamicColor = false) {
                val actions = rememberResultActions(app) {}
                SearchScreen(shape, app, search, library, actions, remember { FocusRequester() })
            }
        }
        search.setText("ubuntu")
        search.run()
        compose.waitForIdle()
        compose.onNodeWithText("12 results", substring = true).assertIsDisplayed()

        savePicture(name)
    }

    /** Best effort: a picture is a review aid, so a failed capture must not hide a failed assertion. */
    private fun savePicture(name: String) {
        val dir = File("build/screenshots").also { it.mkdirs() }
        runCatching {
            val bitmap = compose.activity.window.decorView.drawToBitmap()
            FileOutputStream(File(dir, "$name.png")).use { bitmap.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, it) }
        }
    }

    private fun twoPanes() = compose.onAllNodesWithText(hint).fetchSemanticsNodes().isNotEmpty()

    private fun filterPanel() = compose.onAllNodesWithText("Sort by").fetchSemanticsNodes().isNotEmpty()

    @Test
    @Config(qualifiers = "w411dp-h960dp-xhdpi")
    fun cover_portrait_shows_one_pane() {
        render("cover-portrait", 411, 960)
        assertEquals(false, twoPanes())
        compose.onNode(hasSetTextAction()).assertIsDisplayed()
    }

    @Test
    @Config(qualifiers = "w960dp-h411dp-xhdpi")
    fun cover_landscape_shows_list_and_details() {
        render("cover-landscape", 960, 411)
        assertEquals(true, twoPanes())
    }

    @Test
    @Config(qualifiers = "w750dp-h832dp-xhdpi")
    fun inner_portrait_shows_list_and_details() {
        render("inner-portrait", 750, 832)
        assertEquals(true, twoPanes())
        assertEquals(false, filterPanel())
    }

    @Test
    @Config(qualifiers = "w1000dp-h800dp-xhdpi")
    fun inner_landscape_shows_list_and_details() {
        render("inner-landscape", 1000, 800)
        assertEquals(true, twoPanes())
    }

    @Test
    @Config(qualifiers = "w1400dp-h900dp-xhdpi")
    fun a_large_window_adds_the_filter_panel() {
        render("large", 1400, 900)
        assertEquals(true, twoPanes())
        assertEquals(true, filterPanel())
    }

    @Test
    @Config(qualifiers = "w750dp-h832dp-xhdpi")
    fun tabletop_puts_the_search_field_below_the_results() {
        render("tabletop", 750, 832, Posture.Tabletop)
        assertEquals(false, twoPanes())
        compose.onNode(hasSetTextAction()).assertIsDisplayed()
    }

    @Test
    @Config(qualifiers = "w832dp-h750dp-xhdpi")
    fun book_shows_list_and_details_either_side_of_the_hinge() {
        render("book", 832, 750, Posture.Book)
        assertEquals(true, twoPanes())
    }

    @Test
    @Config(qualifiers = "w375dp-h832dp-xhdpi")
    fun half_of_a_split_screen_is_a_single_pane() {
        render("split-half", 375, 832)
        assertEquals(false, twoPanes())
    }

    @Test
    @Config(qualifiers = "w250dp-h832dp-xhdpi")
    fun a_third_of_a_split_screen_still_works() {
        render("split-third", 250, 832)
        assertEquals(false, twoPanes())
    }
}
