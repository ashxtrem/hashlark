// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark

import android.content.Context
import androidx.activity.ComponentActivity
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.test.junit4.AndroidComposeTestRule
import androidx.compose.ui.test.onAllNodesWithContentDescription
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.dp
import androidx.core.view.drawToBitmap
import androidx.test.core.app.ApplicationProvider
import io.github.ashxtrem.hashlark.core.HashlarkRuntime
import io.github.ashxtrem.hashlark.core.MergedResult
import io.github.ashxtrem.hashlark.core.SearchEvent
import io.github.ashxtrem.hashlark.core.Settings
import io.github.ashxtrem.hashlark.core.UiSettings
import io.github.ashxtrem.hashlark.core.UpdateSettings
import io.github.ashxtrem.hashlark.system.AppPrefs
import io.github.ashxtrem.hashlark.ui.AppViewModel
import io.github.ashxtrem.hashlark.ui.HashlarkRoot
import io.github.ashxtrem.hashlark.ui.LibraryViewModel
import io.github.ashxtrem.hashlark.ui.Screens
import io.github.ashxtrem.hashlark.ui.WindowShape
import io.github.ashxtrem.hashlark.ui.providers.ProvidersViewModel
import io.github.ashxtrem.hashlark.ui.providers.ReposViewModel
import io.github.ashxtrem.hashlark.ui.rememberResultActions
import io.github.ashxtrem.hashlark.ui.search.RESULTS_TAG
import io.github.ashxtrem.hashlark.ui.search.SearchScreen
import io.github.ashxtrem.hashlark.ui.search.SearchViewModel
import io.github.ashxtrem.hashlark.ui.settings.SettingsViewModel
import io.github.ashxtrem.hashlark.ui.theme.HashlarkTheme
import java.io.File
import java.io.FileOutputStream

/** Results that cover what the rows must cope with: a long filename, unknown and zero seeders, several sources. */
fun sampleResults(count: Int = 12): List<MergedResult> = (1..count).map { i ->
    when (i) {
        1 -> mergedResult(
            "r1",
            title = "Ubuntu.26.04.LTS.Desktop.amd64.ISO.[Official.Release].Multi-Language.Checksums.Included.Torrent-UbuntuGroup",
            seeders = 812,
        ).copy(sources = listOf("alpha", "beta", "gamma"))
        2 -> mergedResult("r2", title = "Ubuntu 26.04 build 2 (unknown seeders)", seeders = null)
        3 -> mergedResult("r3", title = "Ubuntu 26.04 build 3 (zero seeders)", seeders = 0)
        else -> mergedResult("r$i", title = "Ubuntu 26.04 build $i", seeders = 100 - i)
    }
}

/**
 * The search screen or the whole app, fed by a scripted engine, in a Robolectric window. The window
 * shape comes from the test's `@Config(qualifiers = ...)`; font scale is injected through the density.
 */
class SearchHarness(
    private val compose: AndroidComposeTestRule<*, ComponentActivity>,
    results: List<MergedResult> = sampleResults(),
    events: ((List<MergedResult>) -> List<SearchEvent>)? = null,
    providers: List<io.github.ashxtrem.hashlark.core.ProviderView>? = null,
) {
    private val context = ApplicationProvider.getApplicationContext<Context>()
    val api = FakeApi(
        settings = Settings(ui = UiSettings(firstRunDone = true), updates = UpdateSettings(checkForUpdates = false)),
        providers = providers ?: listOf(providerView("alpha"), providerView("beta"), providerView("gamma")),
        script = {
            events?.invoke(results) ?: listOf(
                SearchEvent.ProviderStarted("alpha"),
                SearchEvent.Results("alpha", results),
                SearchEvent.ProviderFinished("alpha", results.size, 300),
                SearchEvent.Done(results.size, 320),
            )
        },
    )
    private val runtime = HashlarkRuntime { api }
    val app = AppViewModel(runtime, AppPrefs(context)) { null }
    val search = SearchViewModel(runtime)
    val library = LibraryViewModel(app)

    /** The whole app frame (navigation included) at the window the test's qualifiers describe. */
    fun showApp(fontScale: Float = 1f) {
        val screens = Screens(app, search, library, ProvidersViewModel(app), ReposViewModel(app), SettingsViewModel(app))
        compose.setContent {
            val density = LocalDensity.current
            CompositionLocalProvider(LocalDensity provides Density(density.density, fontScale)) {
                HashlarkTheme(dynamicColor = false) { HashlarkRoot(screens, null) {} }
            }
        }
        compose.waitForIdle()
    }

    /** Only the search screen, at a shape the test can change while it is shown. */
    var shape by mutableStateOf(WindowShape(360.dp, 800.dp))

    fun showScreen(initial: WindowShape) {
        shape = initial
        compose.setContent {
            HashlarkTheme(dynamicColor = false) {
                val actions = rememberResultActions(app) {}
                SearchScreen(shape, app, search, library, actions, remember { FocusRequester() })
            }
        }
        compose.waitForIdle()
    }

    fun search(text: String = "ubuntu") {
        compose.runOnIdle {
            search.setText(text)
            search.run()
        }
        compose.waitForIdle()
    }

    fun select(id: String) {
        compose.runOnIdle { search.select(id) }
        compose.waitForIdle()
    }

    fun back() {
        compose.runOnUiThread { compose.activity.onBackPressedDispatcher.onBackPressed() }
        compose.waitForIdle()
    }

    private fun count(matches: () -> Int) = matches() > 0

    /** Two panes: the list next to a details pane with its Close button. */
    fun sideBySide() = count { compose.onAllNodesWithContentDescription("Close details").fetchSemanticsNodes().size } && listShown()

    /** One pane: details replaced the list and offer Back. */
    fun fullScreenDetails() = count { compose.onAllNodesWithContentDescription("Back to results").fetchSemanticsNodes().size } && !listShown()

    fun listShown() = compose.onAllNodesWithTag(RESULTS_TAG).fetchSemanticsNodes().isNotEmpty()

    fun detailsShown() = compose.onAllNodesWithContentDescription("Close details").fetchSemanticsNodes().isNotEmpty() ||
        compose.onAllNodesWithContentDescription("Back to results").fetchSemanticsNodes().isNotEmpty()

    /** The navigation is showing: its labels are on screen. */
    fun navigationShown() = compose.onAllNodesWithText("Favourites").fetchSemanticsNodes().isNotEmpty() ||
        compose.onAllNodesWithContentDescription("Favourites").fetchSemanticsNodes().isNotEmpty()

    fun text(text: String, substring: Boolean = false) = compose.onAllNodesWithText(text, substring = substring).fetchSemanticsNodes().isNotEmpty()

    /** A picture for review (CI uploads `app/build/screenshots`). Best effort: it must not hide a failed assertion. */
    fun picture(name: String) {
        val dir = File("build/screenshots").also { it.mkdirs() }
        runCatching {
            val bitmap = compose.activity.window.decorView.drawToBitmap()
            FileOutputStream(File(dir, "$name.png")).use { bitmap.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, it) }
        }
    }
}

