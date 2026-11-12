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
        // Settings saved before the torrent client existed have no `torrents` section.
        assertEquals(TorrentSettings(), settings.torrents)
    }

    private val customTorrents = TorrentSettings(
        enabled = false,
        downloadDir = "/storage/emulated/0/Download/Hashlark",
        askOnAdd = true,
        noticeAccepted = true,
        listenPort = 51413,
        upnp = false,
        dht = false,
        lsd = false,
        utp = false,
        limits = SpeedLimits(downKib = 2048, upKib = 512),
        altLimits = SpeedLimits(downKib = 100),
        turtle = true,
        schedule = "n".repeat(167) + "o",
        maxActiveDownloads = 7,
        maxActiveSeeds = 9,
        peerLimitGlobal = 250,
        peerLimitTorrent = 60,
        keepAwake = KeepAwake.Active,
        onBattery = OnBattery.Pause,
        seedRatio = 2.5f,
        seedMinutes = 90,
        proxy = "socks5h://127.0.0.1:9050",
        blocklistUrl = "https://example.org/list.gz",
        watchDir = "/sdcard/watch",
        unmeteredOnly = false,
        closeToTray = false,
    )

    @Test
    fun `settings round trip keeps a non default torrents section`() {
        val settings = Settings(torrents = customTorrents)
        val encoded = json.encodeToString(settings)
        assertTrue(encoded, """"peer_limit_global":250""" in encoded)
        assertTrue(encoded, """"keep_awake":"active"""" in encoded)
        assertTrue(encoded, """"on_battery":"pause"""" in encoded)
        assertEquals(settings, json.decodeFromString<Settings>(encoded))
        assertEquals(settings, json.decodeFromString<Settings>(encodeSettings(json, settings)))
    }

    @Test
    fun `torrent settings from the core parse and have the android defaults`() {
        val settings = json.decodeFromString<Settings>(
            """{"torrents":{"enabled":true,"download_dir":null,"ask_on_add":false,"notice_accepted":false,
              "listen_port":49393,"upnp":false,"dht":false,"lsd":false,"utp":false,
              "limits":{"down_kib":null,"up_kib":null},"alt_limits":{"down_kib":1024,"up_kib":256},
              "turtle":false,"schedule":null,"max_active_downloads":3,"max_active_seeds":5,
              "peer_limit_global":400,"peer_limit_torrent":100,"keep_awake":"downloading","on_battery":"alt_speed",
              "seed_ratio":0.0,"seed_minutes":null,"proxy":null,"blocklist_url":null,"watch_dir":null,
              "unmetered_only":true,"close_to_tray":true}}""",
        )
        assertEquals(49393, settings.torrents.listenPort)
        assertFalse(settings.torrents.dht)
        assertEquals(SpeedLimits(1024, 256), settings.torrents.altLimits)
        assertEquals(TorrentSettings().copy(listenPort = 49393, upnp = false, dht = false, lsd = false, utp = false), settings.torrents)
    }

    @Test
    fun `seeding forever survives a save`() {
        val forever = Settings(torrents = TorrentSettings(seedRatio = null))
        // The shared Json leaves nulls out, and the core would then restore its default ratio.
        assertFalse(json.encodeToString(forever).contains("seed_ratio"))
        val wire = encodeSettings(json, forever)
        assertTrue(wire, """"seed_ratio":null""" in wire)
        assertNull(json.decodeFromString<Settings>(wire).torrents.seedRatio)
    }

    private val summaryJson = """
        {"id":"1dc689206d6f6ef6021d558d95350ed09004b40c","name":"Test","state":"downloading","run_mode":"auto",
         "source":"magnet","progress":0.25,"size":1000,"have":250,"downloaded_total":300,"uploaded_total":10,
         "ratio":0.03,"down_bps":2048,"up_bps":64,"eta_secs":42,"peers":3,"seeds":1,"queue_pos":0,
         "save_dir":"/sdcard/Download","added_at":1794302000744,"completed_at":null,"done":false,"error":null}
    """.trimIndent()

    @Test
    fun `torrent summary parses`() {
        val list = json.decodeFromString<List<TorrentSummary>>("[$summaryJson]")
        val torrent = list.single()
        assertEquals("1dc689206d6f6ef6021d558d95350ed09004b40c", torrent.id)
        assertEquals(TorrentState.Downloading, torrent.state)
        assertEquals(RunMode.Auto, torrent.runMode)
        assertEquals(TorrentOrigin.Magnet, torrent.source)
        assertEquals(42L, torrent.etaSecs)
        assertNull(torrent.completedAt)
        assertNull(torrent.error)
    }

    @Test
    fun `torrent states match the core`() {
        val states = json.decodeFromString<List<TorrentState>>(
            """["metadata","checking","downloading","seeding","queued","paused","stopped","finished","done",
              "scheduled_off","scheduled_seed_only","on_battery","waiting_for_network","error"]""",
        )
        assertEquals(TorrentState.entries, states)
    }

    @Test
    fun `torrent details read the flattened summary and the extra fields`() {
        val details = json.decodeFromString<TorrentDetails>(
            summaryJson.removeSuffix("}") + """,
              "files":[{"index":0,"path":"a/b.iso","size":1000,"have":250,"selected":true}],
              "trackers":["udp://tracker.example:1337"],"limits":{"down_kib":null,"up_kib":128},"peer_limit":50,
              "seed":{"ratio":1.5,"minutes":null},"magnet":"magnet:?xt=urn:btih:1dc689206d6f6ef6021d558d95350ed09004b40c",
              "result_id":null,"seeding_since":null,"future_field":1}""",
        )
        assertEquals("1dc689206d6f6ef6021d558d95350ed09004b40c", details.id)
        assertEquals(TorrentState.Downloading, details.summary.state)
        assertEquals("a/b.iso", details.files.single().path)
        assertEquals(SpeedLimits(upKib = 128), details.limits)
        assertEquals(50, details.peerLimit)
        assertEquals(1.5f, details.seed.ratio)
        assertEquals(listOf("udp://tracker.example:1337"), details.trackers)
        assertEquals(details, json.decodeFromString<TorrentDetails>(json.encodeToString(details)))
    }

    @Test
    fun `session stats and peers parse`() {
        val stats = json.decodeFromString<SessionStats>(
            """{"running":true,"down_bps":0,"up_bps":0,"listen_port":49393,"dht_nodes":null,"schedule_mode":"seed_only",
              "turtle":false,"on_battery":true,"network_allowed":false,"peers_connected":0,
              "counts":{"all":1,"downloading":0,"seeding":0,"queued":0,"paused":0,"stopped":0,"finished":0,"error":1}}""",
        )
        assertTrue(stats.running)
        assertEquals(49393, stats.listenPort)
        assertEquals(ScheduleMode.SeedOnly, stats.scheduleMode)
        assertTrue(stats.onBattery)
        assertFalse(stats.networkAllowed)
        assertEquals(1, stats.counts.error)

        val peers = json.decodeFromString<List<PeerInfo>>(
            """[{"addr":"10.0.0.2:51413","client":null,"conn_kind":"tcp","state":"live","downloaded":5,"uploaded":6}]""",
        )
        assertEquals("tcp", peers.single().connKind)
    }

    @Test
    fun `torrent requests encode like the core's serde model`() {
        assertEquals(
            """{"source":{"magnet":"magnet:?xt=urn:btih:abc"},"files":[0,2],"paused":true}""",
            json.encodeToString(AddRequest(AddSource.Magnet("magnet:?xt=urn:btih:abc"), files = listOf(0, 2), paused = true)),
        )
        assertEquals(
            """{"source":{"torrent":"ZDg6"},"save_dir":"/sdcard/x","paused":false}""",
            json.encodeToString(AddRequest(AddSource.Torrent("ZDg6"), saveDir = "/sdcard/x")),
        )
        assertEquals(
            """{"action":"pause"}""",
            json.encodeToString(BulkRequest(action = BulkAction.Pause)),
        )
        assertEquals(
            """{"ids":["a"],"action":{"remove":{"delete_files":true}}}""",
            json.encodeToString(BulkRequest(ids = listOf("a"), action = BulkAction.Remove(deleteFiles = true))),
        )
        assertEquals(
            """{"action":{"turtle":true}}""",
            json.encodeToString(BulkRequest(action = BulkAction.Turtle(true))),
        )
        assertEquals(
            """{"action":"force_start","limits":{"down_kib":100},"queue_pos":"top"}""",
            json.encodeToString(
                TorrentPatch(
                    action = TorrentAction.ForceStart,
                    limits = SpeedLimits(downKib = 100),
                    queuePos = QueueMove.Step(QueueStep.Top),
                ),
            ),
        )
        assertEquals("""{"queue_pos":3}""", json.encodeToString(TorrentPatch(queuePos = QueueMove.Index(3))))
        for ((action, wire) in listOf(BulkAction.Reannounce to "reannounce", BulkAction.Recheck to "recheck")) {
            val request = BulkRequest(ids = listOf("a"), action = action)
            val encoded = json.encodeToString(request)
            assertEquals("""{"ids":["a"],"action":"$wire"}""", encoded)
            assertEquals(request, json.decodeFromString<BulkRequest>(encoded))
        }
        val trackers = TorrentPatch(trackers = listOf("udp://t.example:1337/announce", "https://t.example/announce"))
        val trackersJson = json.encodeToString(trackers)
        assertEquals(
            """{"trackers":["udp://t.example:1337/announce","https://t.example/announce"]}""",
            trackersJson,
        )
        assertEquals(trackers, json.decodeFromString<TorrentPatch>(trackersJson))
        assertEquals("""{"trackers":[]}""", json.encodeToString(TorrentPatch(trackers = emptyList())))
        val bulk = json.decodeFromString<BulkResult>("""{"ok":["a"],"failed":[{"id":"b","message":"not found"}]}""")
        assertEquals("b", bulk.failed.single().id)
    }

    @Test
    fun `query emptiness matches the core`() {
        assertTrue(SearchQuery(text = "  ").isEmpty)
        assertFalse(SearchQuery(text = "ubuntu").isEmpty)
        assertFalse(SearchQuery(text = "", imdbId = "tt0063350").isEmpty)
    }
}
