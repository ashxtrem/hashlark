// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark

import android.content.Context
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import io.github.ashxtrem.hashlark.core.EngineApi
import io.github.ashxtrem.hashlark.core.HashlarkFailure
import io.github.ashxtrem.hashlark.core.KeystoreSecretStore
import io.github.ashxtrem.hashlark.core.ProviderPatch
import io.github.ashxtrem.hashlark.core.SearchEvent
import io.github.ashxtrem.hashlark.core.SearchQuery
import io.github.ashxtrem.hashlark.core.Settings
import io.github.ashxtrem.hashlark.core.Route
import io.github.ashxtrem.hashlark.core.NetworkPolicy
import io.github.ashxtrem.hashlark.core.TorMode
import io.github.ashxtrem.hashlark.core.TorSettings
import io.github.ashxtrem.hashlark.core.NetworkSettings
import io.github.ashxtrem.hashlark.ffi.openEngine
import kotlinx.coroutines.async
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import kotlinx.coroutines.withTimeoutOrNull
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File
import java.util.UUID

/**
 * Runs the real native engine on a device or emulator: a streaming search
 * against a local Torznab fixture, cancellation, and the Keystore-backed secret store.
 */
@RunWith(AndroidJUnit4::class)
class EngineInstrumentedTest {
    private val context: Context = ApplicationProvider.getApplicationContext()
    private val dirs = mutableListOf<File>()
    private val fixtures = mutableListOf<TorznabFixture>()

    @Before
    fun setUp() = Unit

    @After
    fun tearDown() {
        fixtures.forEach(TorznabFixture::close)
        dirs.forEach { it.deleteRecursively() }
    }

    private fun fixture(count: Int = 5, delayMs: Long = 0) = TorznabFixture(count, delayMs).also(fixtures::add)

    /** An engine with its own data directory, so tests never touch the app's real data. */
    private fun engine(): EngineApi {
        val dir = File(context.cacheDir, "engine-${UUID.randomUUID()}").also { dirs += it }
        return EngineApi(runBlocking { openEngine(dir.absolutePath, KeystoreSecretStore(context)) })
    }

    @Test
    fun a_search_streams_results_from_a_provider() = runBlocking {
        val api = engine()
        val provider = api.addTorznab("Fixture", fixture().url, null)
        // Only the fixture: the built-in providers need the internet.
        api.providers().filter { it.id != provider.id }.forEach { api.updateProvider(it.id, ProviderPatch(enabled = false)) }

        val events = withTimeout(30_000) { api.search(SearchQuery(text = "anything", providers = listOf(provider.id))).toList() }

        assertTrue(events.first() is SearchEvent.ProviderStarted)
        assertTrue(events.last() is SearchEvent.Done)
        val titles = events.filterIsInstance<SearchEvent.Results>().flatMap { it.items }.map { it.primary.title }.toSet()
        assertEquals((1..5).map { "Fixture Result $it" }.toSet(), titles)
        val finished = events.filterIsInstance<SearchEvent.ProviderFinished>().single()
        assertEquals(5, finished.count)
    }

    @Test
    fun a_result_resolves_to_a_magnet_built_from_its_infohash() = runBlocking {
        val api = engine()
        val provider = api.addTorznab("Fixture", fixture().url, null)
        val results = withTimeout(30_000) {
            api.search(SearchQuery(text = "x", providers = listOf(provider.id))).toList()
        }.filterIsInstance<SearchEvent.Results>().flatMap { it.items }

        val target = api.resolve(results.first().id, io.github.ashxtrem.hashlark.core.TargetPreference.Magnet)
        assertTrue(target.url, target.url.startsWith("magnet:?xt=urn:btih:"))
    }

    @Test
    fun stopping_the_collection_cancels_the_search_without_a_crash() = runBlocking {
        val api = engine()
        val provider = api.addTorznab("Slow", fixture(delayMs = 20_000).url, null)
        val started = withTimeout(30_000) {
            api.search(SearchQuery(text = "x", providers = listOf(provider.id))).first()
        }
        // `first()` stopped collecting: the flow must have cancelled the native search.
        assertTrue(started is SearchEvent.ProviderStarted)
        // The engine is still usable afterwards.
        delay(200)
        assertTrue(api.providers().isNotEmpty())
    }

    @Test
    fun an_empty_query_is_refused_with_a_readable_error() = runBlocking {
        val api = engine()
        val failure = runCatching { api.search(SearchQuery(text = "  ")).toList() }.exceptionOrNull()
        assertTrue(failure.toString(), failure is HashlarkFailure)
        assertEquals("invalid_request", (failure as HashlarkFailure).code)
    }

    @Test
    fun settings_are_validated_by_the_core() = runBlocking {
        val api = engine()
        val bad = api.settings().let { it.copy(search = it.search.copy(providerTimeoutSecs = 0)) }
        val failure = runCatching { api.setSettings(bad) }.exceptionOrNull()
        assertTrue(failure is HashlarkFailure)
        assertEquals(10L, api.settings().search.providerTimeoutSecs)
    }

    @Test
    fun the_keystore_secret_store_round_trips_and_persists() {
        val key = "provider/test/password-${UUID.randomUUID()}"
        val store = KeystoreSecretStore(context)
        assertNull(store.get(key))
        store.set(key, "hunter2")
        assertEquals("hunter2", store.get(key))
        // A second instance (a new process, in real life) reads it back with the same Keystore key.
        assertEquals("hunter2", KeystoreSecretStore(context).get(key))
        store.delete(key)
        assertNull(store.get(key))
    }

    /** Opt-in (needs a network that allows Tor): `-Pandroid.testInstrumentationRunnerArguments.tor=true`. */
    @Test
    fun built_in_tor_can_carry_a_search() {
        val enabled = androidx.test.platform.app.InstrumentationRegistry.getArguments().getString("tor") == "true"
        assumeTrue("Tor test is opt-in", enabled)
        runBlocking { torSearch() }
    }

    private suspend fun torSearch() {
        val api = engine()
        api.setSettings(
            Settings(network = NetworkSettings(tor = TorSettings(enabled = true, mode = TorMode.Embedded))),
        )
        // Wait for Tor to connect (it bootstraps in the background).
        val ready = withTimeoutOrNull(180_000) {
            while (!api.torStatus().ready) delay(2_000)
            true
        }
        assertEquals("Tor did not connect within 3 minutes", true, ready)
        val ia = api.providers().first { it.id == "internet-archive" }
        api.updateProvider(ia.id, ProviderPatch(networkPolicy = NetworkPolicy(Route.Tor)))
        val results = withTimeout(120_000) {
            api.search(SearchQuery(text = "ubuntu", providers = listOf(ia.id))).toList()
        }
        assertTrue(results.any { it is SearchEvent.Results })
    }
}
