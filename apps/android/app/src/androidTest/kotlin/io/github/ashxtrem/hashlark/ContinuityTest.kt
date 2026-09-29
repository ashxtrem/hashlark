// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark

import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.hasSetTextAction
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performImeAction
import androidx.compose.ui.test.performScrollToNode
import androidx.compose.ui.test.performTextInput
import androidx.compose.ui.test.hasText
import androidx.test.core.app.ActivityScenario
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import io.github.ashxtrem.hashlark.core.ProviderPatch
import io.github.ashxtrem.hashlark.ui.search.RESULTS_TAG
import kotlinx.coroutines.runBlocking
import org.junit.After
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/**
 * The activity is destroyed and recreated in the middle of a session (what a
 * fold, a display-size change or the system reclaiming it does): the results
 * and the scroll position must still be there, because the engine lives in
 * `Application` scope and the state in ViewModels.
 */
@RunWith(AndroidJUnit4::class)
class ContinuityTest {
    @get:Rule
    val compose = createEmptyComposeRule()

    private val fixture = TorznabFixture(count = 60)
    private lateinit var scenario: ActivityScenario<MainActivity>

    @Before
    fun seed() {
        val app = ApplicationProvider.getApplicationContext<HashlarkApplication>()
        runBlocking {
            val api = app.runtime.api()
            api.setSettings(api.settings().let { it.copy(ui = it.ui.copy(firstRunDone = true), updates = it.updates.copy(checkForUpdates = false)) })
            val provider = api.addTorznab("Continuity", fixture.url, null)
            // Only the fixture answers: the built-in providers need the internet.
            api.providers().filter { it.id != provider.id }.forEach { api.updateProvider(it.id, ProviderPatch(enabled = false)) }
        }
        scenario = ActivityScenario.launch(MainActivity::class.java)
    }

    @After
    fun tearDown() {
        scenario.close()
        fixture.close()
    }

    private fun waitForText(text: String, timeoutMs: Long = 20_000) {
        compose.waitUntil(timeoutMs) { compose.onAllNodesWithText(text, substring = true).fetchSemanticsNodes().isNotEmpty() }
    }

    @Test
    fun results_and_scroll_position_survive_recreating_the_activity() {
        compose.onNode(hasSetTextAction()).performTextInput("continuity")
        compose.onNode(hasSetTextAction()).performImeAction()
        waitForText("60 results", timeoutMs = 30_000)

        // Scroll far down, then recreate the activity.
        compose.onNodeWithTag(RESULTS_TAG).performScrollToNode(hasText("Fixture Result 45"))
        compose.onNodeWithText("Fixture Result 45").assertIsDisplayed()
        scenario.recreate()

        waitForText("Fixture Result 45", timeoutMs = 10_000)
        compose.onNodeWithText("Fixture Result 45").assertIsDisplayed()
    }
}
