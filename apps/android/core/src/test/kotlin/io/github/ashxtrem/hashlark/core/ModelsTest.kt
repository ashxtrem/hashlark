// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark.core

import kotlinx.serialization.decodeFromString
import kotlinx.serialization.encodeToString
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** Parses documents shaped like the core's real output (see the OpenAPI schema). */
class ModelsTest {
    private val json = HashlarkJson

    private val mergedResult = """
        {"id":"r_1dc689","primary":{"title":"Ubuntu 24.04 (amd64)","size_bytes":6114656256,"seeders":812,
         "leechers":14,"info_hash":"1dc689206d6f6ef6021d558d95350ed09004b40c","magnet":null,
         "torrent_url":"https://example.org/ubuntu.torrent","details_url":null,
         "published":"2026-08-01T10:00:00Z","category":"software","provider_id":"linuxtracker","needs_resolve":false},
         "sources":["linuxtracker","foss-torrents"],"seeders":900,"leechers":20,"score":0.93,"future_field":1}
    """.trimIndent()

    @Test
    fun `merged result parses and ignores unknown fields`() {
        val result = json.decodeFromString<MergedResult>(mergedResult)
        assertEquals("r_1dc689", result.id)
        assertEquals(6_114_656_256L, result.primary.sizeBytes)
        assertEquals(Category.Software, result.primary.category)
        assertEquals(listOf("linuxtracker", "foss-torrents"), result.sources)
        assertTrue(result.hasMagnet)
        assertTrue(result.hasTorrentFile)
    }

    @Test
    fun `search events parse by their event tag`() {
        val started = json.decodeFromString<SearchEvent>("""{"event":"provider_started","provider":"a"}""")
        assertEquals(SearchEvent.ProviderStarted("a"), started)

        val results = json.decodeFromString<SearchEvent>("""{"event":"results","provider":"a","items":[$mergedResult]}""")
        assertEquals(1, (results as SearchEvent.Results).items.size)

        val failed = json.decodeFromString<SearchEvent>(
            """{"event":"provider_failed","provider":"a","error_kind":"challenge_required","message":"check","latency_ms":42}""",
        )
        assertEquals(ErrorKind.ChallengeRequired, (failed as SearchEvent.ProviderFailed).errorKind)

        val done = json.decodeFromString<SearchEvent>("""{"event":"done","total":3,"duration_ms":1200}""")
        assertEquals(SearchEvent.Done(3, 1200), done)
    }

    @Test
    fun `download target is tagged by type`() {
        val magnet = json.decodeFromString<DownloadTarget>("""{"type":"magnet","url":"magnet:?xt=urn:btih:abc"}""")
        assertEquals(DownloadTarget.Magnet("magnet:?xt=urn:btih:abc"), magnet)
        val file = json.decodeFromString<DownloadTarget>("""{"type":"torrent_file","url":"https://x/y.torrent"}""")
        assertTrue(file is DownloadTarget.TorrentFile)
    }

    @Test
    fun `provider view parses each source kind`() {
        val definition = json.decodeFromString<ProviderView>(
            """{"id":"linuxtracker","name":"LinuxTracker","kind":"definition","description":"d","enabled":true,
              "builtin":true,"categories":["software"],"capabilities":{"imdb_search":false,"paging":true,"text_search":true},
              "health":{"state":"degraded","success_rate":0.8,"p50_ms":300,"p95_ms":900,"last_error_kind":"timeout",
              "last_checked_at":1,"consecutive_failures":2,"disabled_until":null},
              "error":null,"network_policy":{"route":"tor","proxy":null},
              "source":{"type":"definition","version":3,"repo_id":"builtin","site_type":"public","links":["https://linuxtracker.org/"]},
              "settings":[{"name":"password","label":"Password","kind":"password","required":true,"options":{},"value":null,"is_set":true}]}""",
        )
        assertEquals(HealthState.Degraded, definition.health.state)
        assertEquals(Route.Tor, definition.networkPolicy.route)
        assertEquals("https://linuxtracker.org/", definition.siteUrl)
        assertTrue(definition.settings.single().isSet)

        val native = json.decodeFromString<ProviderView>(
            """{"id":"internet-archive","name":"Internet Archive","kind":"native","enabled":true,"source":{"type":"native"}}""",
        )
        assertEquals(ProviderSource.Native, native.source)
        assertNull(native.siteUrl)

        val torznab = json.decodeFromString<ProviderView>(
            """{"id":"jackett","name":"Jackett","kind":"torznab","enabled":false,"source":{"type":"torznab","url":"http://x/api"}}""",
        )
        assertEquals(ProviderSource.Torznab("http://x/api"), torznab.source)
    }

    @Test
    fun `search query omits nulls and keeps defaults the core expects`() {
        val encoded = json.encodeToString(SearchQuery(text = "ubuntu", categories = listOf(Category.Software)))
        assertTrue(encoded, """"text":"ubuntu"""" in encoded)
        assertTrue(encoded, """"categories":["software"]""" in encoded)
        assertTrue(encoded, """"page":1""" in encoded)
        assertTrue(encoded, """"sort":"relevance"""" in encoded)
        assertFalse(encoded, "imdb_id" in encoded)
        assertFalse(encoded, "providers" in encoded)
    }

    @Test
    fun `provider patch encodes only what changed and lets a setting be cleared`() {
        val encoded = json.encodeToString(ProviderPatch(enabled = false, settings = mapOf("username" to null, "x" to "1")))
        assertEquals("""{"enabled":false,"settings":{"username":null,"x":"1"}}""", encoded)
    }

    @Test
    fun `settings round trip keeps every field`() {
        val settings = Settings(
            network = NetworkSettings(tor = TorSettings(enabled = true, mode = TorMode.External)),
            ui = UiSettings(theme = Theme.Dark, firstRunDone = true),
        )
        val decoded = json.decodeFromString<Settings>(json.encodeToString(settings))
        assertEquals(settings, decoded)
    }

    @Test
    fun `settings from the core parse`() {
        val settings = json.decodeFromString<Settings>(
            """{"search":{"provider_timeout_secs":10,"max_concurrency":16,"cache_ttl_secs":600,"save_history":true},
              "network":{"doh":{"enabled":true,"resolver":"quad9","custom_url":null,"fallback_to_system":true},
              "proxy":null,"tor":{"enabled":false,"mode":"embedded","socks_url":"socks5h://127.0.0.1:9050"},"per_host_rate":1},
              "magnets":{"append_default_trackers":false,"trackers_url":null},"downloads":{"torrent_dir":null},
              "updates":{"check_for_updates":true},"ui":{"theme":"system","first_run_done":false}}""",
        )
        assertEquals(DohResolver.Quad9, settings.network.doh.resolver)
        assertEquals(TorMode.Embedded, settings.network.tor.mode)
    }

    @Test
    fun `query emptiness matches the core`() {
        assertTrue(SearchQuery(text = "  ").isEmpty)
        assertFalse(SearchQuery(text = "ubuntu").isEmpty)
        assertFalse(SearchQuery(text = "", imdbId = "tt0063350").isEmpty)
    }
}
