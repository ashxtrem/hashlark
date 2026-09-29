// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark.core

import kotlinx.serialization.ExperimentalSerializationApi
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonClassDiscriminator

/*
 * Mirrors the JSON schemas of the HTTP API (apps/desktop/src/lib/api/openapi.json).
 * The Rust core produces and accepts exactly these documents across the FFI,
 * so a field added there shows up here as a new property with a default.
 */

/** The one JSON configuration used for everything that crosses the FFI. */
val HashlarkJson: Json = Json {
    ignoreUnknownKeys = true
    explicitNulls = false
    encodeDefaults = true
    isLenient = false
}

@Serializable
enum class Category(val label: String) {
    @SerialName("movies") Movies("Movies"),
    @SerialName("tv") Tv("TV"),
    @SerialName("music") Music("Music"),
    @SerialName("books") Books("Books"),
    @SerialName("software") Software("Software"),
    @SerialName("games") Games("Games"),
    @SerialName("anime") Anime("Anime"),
    @SerialName("other") Other("Other"),
}

@Serializable
enum class SortOrder(val label: String) {
    @SerialName("relevance") Relevance("Relevance"),
    @SerialName("title") Title("Title"),
    @SerialName("seeders") Seeders("Seeders"),
    @SerialName("peers") Peers("Peers"),
    @SerialName("size") Size("Size"),
    @SerialName("date") Date("Date"),
}

@Serializable
enum class ErrorKind(val label: String) {
    @SerialName("timeout") Timeout("Timed out"),
    @SerialName("blocked") Blocked("Blocked"),
    @SerialName("challenge_required") ChallengeRequired("Needs browser check"),
    @SerialName("auth_failed") AuthFailed("Login failed"),
    @SerialName("rate_limited") RateLimited("Rate limited"),
    @SerialName("parse_failed") ParseFailed("Site changed"),
    @SerialName("http") Http("Server error"),
    @SerialName("network") Network("Network error"),
}

// ----- search -------------------------------------------------------------------

@Serializable
data class SearchQuery(
    val text: String,
    val categories: List<Category> = emptyList(),
    /** `null` searches every enabled provider. */
    val providers: List<String>? = null,
    @SerialName("imdb_id") val imdbId: String? = null,
    val page: Int = 1,
    val sort: SortOrder = SortOrder.Relevance,
) {
    val isEmpty: Boolean get() = text.isBlank() && imdbId.isNullOrBlank()
}

@Serializable
data class SearchResult(
    val title: String,
    @SerialName("size_bytes") val sizeBytes: Long? = null,
    val seeders: Int? = null,
    val leechers: Int? = null,
    @SerialName("info_hash") val infoHash: String? = null,
    val magnet: String? = null,
    @SerialName("torrent_url") val torrentUrl: String? = null,
    @SerialName("details_url") val detailsUrl: String? = null,
    /** RFC 3339. */
    val published: String? = null,
    val category: Category? = null,
    @SerialName("provider_id") val providerId: String,
    @SerialName("needs_resolve") val needsResolve: Boolean = false,
)

@Serializable
data class MergedResult(
    val id: String,
    val primary: SearchResult,
    val sources: List<String> = emptyList(),
    val seeders: Int? = null,
    val leechers: Int? = null,
    val score: Float = 0f,
) {
    /** Whether a magnet link is available without asking the provider. */
    val hasMagnet: Boolean get() = primary.magnet != null || primary.infoHash != null || primary.needsResolve

    /** Whether a `.torrent` file is available without asking the provider. */
    val hasTorrentFile: Boolean get() = primary.torrentUrl != null && !primary.needsResolve
}

@OptIn(ExperimentalSerializationApi::class)
@Serializable
@JsonClassDiscriminator("event")
sealed interface SearchEvent {
    @Serializable
    @SerialName("provider_started")
    data class ProviderStarted(val provider: String) : SearchEvent

    @Serializable
    @SerialName("results")
    data class Results(val provider: String, val items: List<MergedResult>) : SearchEvent

    @Serializable
    @SerialName("provider_finished")
    data class ProviderFinished(
        val provider: String,
        val count: Int,
        @SerialName("latency_ms") val latencyMs: Long,
    ) : SearchEvent

    @Serializable
    @SerialName("provider_failed")
    data class ProviderFailed(
        val provider: String,
        @SerialName("error_kind") val errorKind: ErrorKind,
        val message: String,
        @SerialName("latency_ms") val latencyMs: Long,
    ) : SearchEvent

    @Serializable
    @SerialName("done")
    data class Done(val total: Int, @SerialName("duration_ms") val durationMs: Long) : SearchEvent
}

@OptIn(ExperimentalSerializationApi::class)
@Serializable
@JsonClassDiscriminator("type")
sealed interface DownloadTarget {
    val url: String

    @Serializable
    @SerialName("magnet")
    data class Magnet(override val url: String) : DownloadTarget

    @Serializable
    @SerialName("torrent_file")
    data class TorrentFile(override val url: String) : DownloadTarget
}

// ----- providers ------------------------------------------------------------------

@Serializable
enum class ProviderKind {
    @SerialName("native") Native,
    @SerialName("definition") Definition,
    @SerialName("torznab") Torznab,
}

@Serializable
enum class HealthState(val label: String) {
    @SerialName("unknown") Unknown("Not checked"),
    @SerialName("ok") Ok("Working"),
    @SerialName("degraded") Degraded("Slow or flaky"),
    @SerialName("failing") Failing("Failing"),
    @SerialName("auto_disabled") AutoDisabled("Paused after failures"),
}

@Serializable
data class HealthSummary(
    val state: HealthState = HealthState.Unknown,
    @SerialName("success_rate") val successRate: Float? = null,
    @SerialName("p50_ms") val p50Ms: Long? = null,
    @SerialName("p95_ms") val p95Ms: Long? = null,
    @SerialName("last_error_kind") val lastErrorKind: ErrorKind? = null,
    @SerialName("last_checked_at") val lastCheckedAt: Long? = null,
    @SerialName("consecutive_failures") val consecutiveFailures: Int = 0,
    @SerialName("disabled_until") val disabledUntil: Long? = null,
)

@Serializable
enum class Route {
    @SerialName("default") Default,
    @SerialName("direct") Direct,
    @SerialName("proxy") Proxy,
    @SerialName("tor") Tor,
}

@Serializable
data class NetworkPolicy(
    val route: Route = Route.Default,
    val proxy: String? = null,
)

@Serializable
data class Capabilities(
    @SerialName("imdb_search") val imdbSearch: Boolean = false,
    val paging: Boolean = false,
    @SerialName("text_search") val textSearch: Boolean = true,
)

@Serializable
enum class SiteType {
    @SerialName("public") Public,
    @SerialName("semi_private") SemiPrivate,
    @SerialName("private") Private,
}

@OptIn(ExperimentalSerializationApi::class)
@Serializable
@JsonClassDiscriminator("type")
sealed interface ProviderSource {
    @Serializable
    @SerialName("native")
    data object Native : ProviderSource

    @Serializable
    @SerialName("definition")
    data class Definition(
        val version: Long = 0,
        @SerialName("repo_id") val repoId: String? = null,
        @SerialName("site_type") val siteType: SiteType? = null,
        val links: List<String> = emptyList(),
    ) : ProviderSource

    @Serializable
    @SerialName("torznab")
    data class Torznab(val url: String) : ProviderSource
}

@Serializable
enum class SettingKind {
    @SerialName("text") Text,
    @SerialName("password") Password,
    @SerialName("checkbox") Checkbox,
    @SerialName("select") Select,
}

@Serializable
data class SettingView(
    val name: String,
    val label: String,
    val kind: SettingKind = SettingKind.Text,
    val required: Boolean = false,
    val options: Map<String, String> = emptyMap(),
    /** Always `null` for passwords. */
    val value: String? = null,
    @SerialName("is_set") val isSet: Boolean = false,
)

@Serializable
data class ProviderView(
    val id: String,
    val name: String,
    val kind: ProviderKind,
    val description: String = "",
    val enabled: Boolean,
    val builtin: Boolean = false,
    val categories: List<Category> = emptyList(),
    val capabilities: Capabilities? = null,
    val health: HealthSummary = HealthSummary(),
    val error: String? = null,
    @SerialName("network_policy") val networkPolicy: NetworkPolicy = NetworkPolicy(),
    val source: ProviderSource = ProviderSource.Native,
    val settings: List<SettingView> = emptyList(),
) {
    /** First web page of the site, for the browser check. */
    val siteUrl: String?
        get() = (source as? ProviderSource.Definition)?.links?.firstOrNull()
}

/** Changes to a provider; properties left `null` stay as they are. */
@Serializable
data class ProviderPatch(
    val enabled: Boolean? = null,
    val name: String? = null,
    @SerialName("network_policy") val networkPolicy: NetworkPolicy? = null,
    /** Setting values to change; a `null` value clears one. */
    val settings: Map<String, String?>? = null,
)

@Serializable
data class TestReport(
    val ok: Boolean,
    @SerialName("result_count") val resultCount: Int = 0,
    @SerialName("latency_ms") val latencyMs: Long = 0,
    @SerialName("error_kind") val errorKind: ErrorKind? = null,
    val message: String? = null,
)

@Serializable
data class DefinitionCheck(
    val ok: Boolean,
    val errors: List<String> = emptyList(),
    val id: String? = null,
    val name: String? = null,
    val version: Int? = null,
)

@Serializable
data class SessionCookie(val name: String, val value: String)

/** Cookies from a browser check the user completed on the site. */
@Serializable
data class Session(
    val url: String,
    val cookies: List<SessionCookie>,
    @SerialName("user_agent") val userAgent: String? = null,
    @SerialName("expires_at") val expiresAt: Long? = null,
)

// ----- repositories, favourites, history -----------------------------------------------

@Serializable
data class RepoView(
    val id: String,
    val url: String,
    val name: String? = null,
    val fingerprint: String? = null,
    @SerialName("last_sync_at") val lastSyncAt: Long? = null,
    @SerialName("last_version") val lastVersion: Long? = null,
    val definitions: Long = 0,
    val builtin: Boolean = false,
)

@Serializable
data class SyncReport(
    val added: List<String> = emptyList(),
    val updated: List<String> = emptyList(),
    val removed: List<String> = emptyList(),
    val unchanged: Int = 0,
    val errors: List<String> = emptyList(),
)

/** A repository as fetched but not yet added: the signing key to trust. */
@Serializable
data class RepoPreview(
    val url: String,
    val name: String,
    val fingerprint: String,
    val definitions: Int = 0,
    val exists: Boolean = false,
)

@Serializable
data class AddRepoResponse(val repo: RepoView, val sync: SyncReport)

@Serializable
data class Favorite(
    val result: MergedResult,
    @SerialName("saved_at") val savedAt: Long,
)

@Serializable
data class HistoryEntry(
    val id: Long,
    val query: SearchQuery,
    /** Unix ms. */
    val ts: Long,
    @SerialName("result_count") val resultCount: Long = 0,
)

// ----- settings ---------------------------------------------------------------------------

@Serializable
data class Settings(
    val search: SearchSettings = SearchSettings(),
    val network: NetworkSettings = NetworkSettings(),
    val magnets: MagnetSettings = MagnetSettings(),
    val downloads: DownloadSettings = DownloadSettings(),
    val updates: UpdateSettings = UpdateSettings(),
    val ui: UiSettings = UiSettings(),
)

@Serializable
data class SearchSettings(
    @SerialName("provider_timeout_secs") val providerTimeoutSecs: Long = 10,
    @SerialName("max_concurrency") val maxConcurrency: Int = 16,
    @SerialName("cache_ttl_secs") val cacheTtlSecs: Long = 600,
    @SerialName("save_history") val saveHistory: Boolean = true,
)

@Serializable
data class NetworkSettings(
    val doh: DohSettings = DohSettings(),
    val proxy: String? = null,
    val tor: TorSettings = TorSettings(),
    @SerialName("per_host_rate") val perHostRate: Int = 1,
)

@Serializable
enum class DohResolver(val label: String) {
    @SerialName("cloudflare") Cloudflare("Cloudflare"),
    @SerialName("quad9") Quad9("Quad9"),
    @SerialName("google") Google("Google"),
    @SerialName("custom") Custom("Custom"),
}

@Serializable
data class DohSettings(
    val enabled: Boolean = true,
    val resolver: DohResolver = DohResolver.Cloudflare,
    @SerialName("custom_url") val customUrl: String? = null,
    @SerialName("fallback_to_system") val fallbackToSystem: Boolean = true,
)

@Serializable
enum class TorMode {
    @SerialName("embedded") Embedded,
    @SerialName("external") External,
}

@Serializable
data class TorSettings(
    val enabled: Boolean = false,
    val mode: TorMode = TorMode.Embedded,
    @SerialName("socks_url") val socksUrl: String? = "socks5h://127.0.0.1:9050",
)

@Serializable
data class MagnetSettings(
    @SerialName("append_default_trackers") val appendDefaultTrackers: Boolean = false,
    @SerialName("trackers_url") val trackersUrl: String? = null,
)

@Serializable
data class DownloadSettings(
    @SerialName("torrent_dir") val torrentDir: String? = null,
)

@Serializable
data class UpdateSettings(
    @SerialName("check_for_updates") val checkForUpdates: Boolean = true,
)

@Serializable
enum class Theme(val label: String) {
    @SerialName("system") System("System default"),
    @SerialName("light") Light("Light"),
    @SerialName("dark") Dark("Dark"),
}

@Serializable
data class UiSettings(
    val theme: Theme = Theme.System,
    @SerialName("first_run_done") val firstRunDone: Boolean = false,
)

@Serializable
data class TorStatus(
    @SerialName("built_in") val builtIn: Boolean = false,
    val running: Boolean = false,
    val ready: Boolean = false,
    val progress: Float = 0f,
    val message: String = "",
)
