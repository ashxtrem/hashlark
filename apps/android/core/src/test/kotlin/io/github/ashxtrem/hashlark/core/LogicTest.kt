// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark.core

import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.time.Instant
import javax.crypto.KeyGenerator

private fun result(
    id: String,
    title: String = id,
    seeders: Int? = null,
    size: Long? = null,
    published: String? = null,
    score: Float = 0f,
) = MergedResult(
    id = id,
    primary = SearchResult(title = title, sizeBytes = size, published = published, providerId = "p"),
    seeders = seeders,
    score = score,
)

class FormatTest {
    @Test
    fun `bytes use binary units`() {
        assertEquals("–", Format.bytes(null))
        assertEquals("512 B", Format.bytes(512))
        assertEquals("1.5 KiB", Format.bytes(1536))
        assertEquals("5.7 GiB", Format.bytes(6_114_656_256))
    }

    @Test
    fun `age picks the largest fitting unit`() {
        val now = Instant.parse("2026-09-29T12:00:00Z")
        assertEquals("–", Format.age(null, now))
        assertEquals("–", Format.age("not a date", now))
        assertEquals("5h", Format.age("2026-09-29T07:00:00Z", now))
        assertEquals("3d", Format.age("2026-09-26T12:00:00Z", now))
        assertEquals("4mo", Format.age("2026-05-29T12:00:00Z", now))
        assertEquals("2y", Format.age("2024-08-01T12:00:00Z", now))
        assertEquals("0h", Format.age("2027-01-01T00:00:00Z", now))
    }

    @Test
    fun `durations switch to seconds at one second`() {
        assertEquals("250 ms", Format.duration(250))
        assertEquals("1.2 s", Format.duration(1234))
        assertEquals("–", Format.duration(null))
    }
}

class ResultSorterTest {
    private val items = listOf(
        result("a", title = "Alpha", seeders = 5, size = 10, score = 0.2f),
        result("b", title = "beta", seeders = null, size = 30, score = 0.9f),
        result("c", title = "Gamma", seeders = 50, size = null, score = 0.5f),
    )

    private fun ids(order: SortOrder, dir: SortDirection = SortDirection.Descending) =
        ResultSorter.sort(items, order, dir).map { it.id }

    @Test
    fun `relevance orders by score`() = assertEquals(listOf("b", "c", "a"), ids(SortOrder.Relevance))

    @Test
    fun `unknown values sort last in both directions`() {
        assertEquals(listOf("c", "a", "b"), ids(SortOrder.Seeders))
        assertEquals(listOf("a", "c", "b"), ids(SortOrder.Seeders, SortDirection.Ascending))
        assertEquals(listOf("b", "a", "c"), ids(SortOrder.Size))
        assertEquals(listOf("a", "b", "c"), ids(SortOrder.Size, SortDirection.Ascending))
    }

    @Test
    fun `title sort ignores case`() {
        assertEquals(listOf("a", "b", "c"), ids(SortOrder.Title, SortDirection.Ascending))
        assertEquals(listOf("c", "b", "a"), ids(SortOrder.Title))
    }

    @Test
    fun `date sort uses the published time`() {
        val dated = listOf(
            result("old", published = "2020-01-01T00:00:00Z"),
            result("new", published = "2026-01-01T00:00:00Z"),
            result("none"),
        )
        assertEquals(listOf("new", "old", "none"), ResultSorter.sort(dated, SortOrder.Date).map { it.id })
    }
}

class SearchProgressTest {
    @Test
    fun `events build up the state and later results replace earlier ones`() {
        var state = SearchProgress()
        state = state.reduce(SearchEvent.ProviderStarted("a")).reduce(SearchEvent.ProviderStarted("b"))
        assertEquals(2, state.pending)

        state = state.reduce(SearchEvent.Results("a", listOf(result("1", seeders = 1), result("2"))))
        state = state.reduce(SearchEvent.Results("b", listOf(result("1", seeders = 9))))
        assertEquals(2, state.results.size)
        assertEquals(9, state.results.getValue("1").seeders)

        state = state.reduce(SearchEvent.ProviderFinished("a", 2, 100))
        state = state.reduce(SearchEvent.ProviderFailed("b", ErrorKind.Timeout, "slow", 10_000))
        assertEquals(0, state.pending)
        assertEquals(setOf("b"), state.failed.keys)
        assertFalse(state.done)

        state = state.reduce(SearchEvent.Done(2, 10_100))
        assertTrue(state.done)
        assertEquals(10_100L, state.durationMs)
    }
}

class SearchCancelTest {
    @Test
    fun `cancelling marks running providers and keeps finished ones`() {
        val state = SearchProgress()
            .reduce(SearchEvent.ProviderStarted("a"))
            .reduce(SearchEvent.ProviderStarted("b"))
            .reduce(SearchEvent.ProviderFinished("a", 1, 5))
            .cancelled()
        assertTrue(state.done)
        assertTrue(state.providers.getValue("a") is ProviderProgress.Finished)
        assertEquals("Cancelled", (state.providers.getValue("b") as ProviderProgress.Failed).message)
        assertEquals(0, state.pending)
    }
}

class SearchIntentTest {
    @Test
    fun `imdb links and ids become exact searches`() {
        val fromUrl = SearchIntent.fromSharedText("https://www.imdb.com/title/tt0063350/?ref_=fn_al_tt_1")
        assertEquals("tt0063350", fromUrl?.imdbId)
        assertEquals("", fromUrl?.text)
        assertEquals("tt0063350", SearchIntent.fromSharedText("  TT0063350 ")?.imdbId)
        assertEquals("tt0063350", SearchIntent.fromSharedText("Night of the Living Dead https://m.imdb.com/title/tt0063350/")?.imdbId)
    }

    @Test
    fun `an id inside another site's link is just text`() {
        val query = SearchIntent.fromSharedText("https://example.com/tt0063350")
        assertNull(query?.imdbId)
    }

    @Test
    fun `plain text is trimmed and collapsed`() {
        assertEquals("ubuntu 24.04", SearchIntent.fromSharedText("  ubuntu \n  24.04  ")?.text)
        assertNull(SearchIntent.fromSharedText("   "))
        assertNull(SearchIntent.fromSharedText(null))
    }

    @Test
    fun `long text is cut`() {
        assertEquals(200, SearchIntent.fromSharedText("x".repeat(500))?.text?.length)
    }

    @Test
    fun `magnets are recognised`() {
        assertTrue(SearchIntent.isMagnet(" MAGNET:?xt=urn:btih:abc"))
        assertFalse(SearchIntent.isMagnet("ubuntu"))
    }
}

class UpdateCheckerTest {
    @Test
    fun `version comparison`() {
        assertTrue(UpdateChecker.isNewer("v1.0.1", "1.0.0"))
        assertTrue(UpdateChecker.isNewer("1.10.0", "1.9.9"))
        assertTrue(UpdateChecker.isNewer("v2.0", "1.9.9"))
        assertFalse(UpdateChecker.isNewer("v1.0.0", "1.0.0"))
        assertFalse(UpdateChecker.isNewer("v0.9.0", "1.0.0"))
        assertFalse(UpdateChecker.isNewer("garbage", "1.0.0"))
    }

    private fun checker(body: String, current: String = "0.1.0") =
        UpdateChecker(current, fetch = { body })

    @Test
    fun `a newer published release is offered`() = runTest {
        val update = checker("""{"tag_name":"v0.2.0","html_url":"https://github.com/x/releases/v0.2.0","body":"Notes"}""").check()
        assertNotNull(update)
        assertEquals("0.2.0", update?.version)
        assertEquals("https://github.com/x/releases/v0.2.0", update?.pageUrl)
    }

    @Test
    fun `same version, drafts, prereleases and errors offer nothing`() = runTest {
        assertNull(checker("""{"tag_name":"v0.1.0","html_url":"u"}""").check())
        assertNull(checker("""{"tag_name":"v9.0.0","html_url":"u","draft":true}""").check())
        assertNull(checker("""{"tag_name":"v9.0.0","html_url":"u","prerelease":true}""").check())
        assertNull(checker("not json").check())
        assertNull(UpdateChecker("0.1.0", fetch = { error("offline") }).check())
    }
}

class AesGcmBoxTest {
    private fun box() = AesGcmBox(KeyGenerator.getInstance("AES").apply { init(256) }.generateKey())

    @Test
    fun `sealed values open again and differ each time`() {
        val box = box()
        val first = box.seal("hunter2")
        val second = box.seal("hunter2")
        assertTrue(first != second)
        assertEquals("hunter2", box.open(first))
        assertEquals("ünïcödé ✓", box.open(box.seal("ünïcödé ✓")))
    }

    @Test
    fun `tampered or foreign values do not open`() {
        val sealed = box().seal("secret")
        assertNull(box().open(sealed))
        assertNull(box().open("not base64 !"))
        assertNull(box().open("AAAA"))
    }
}
