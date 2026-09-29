// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark

import io.github.ashxtrem.hashlark.core.AddRepoResponse
import io.github.ashxtrem.hashlark.core.DefinitionCheck
import io.github.ashxtrem.hashlark.core.DownloadTarget
import io.github.ashxtrem.hashlark.core.Favorite
import io.github.ashxtrem.hashlark.core.HashlarkApi
import io.github.ashxtrem.hashlark.core.HistoryEntry
import io.github.ashxtrem.hashlark.core.MergedResult
import io.github.ashxtrem.hashlark.core.ProviderPatch
import io.github.ashxtrem.hashlark.core.ProviderKind
import io.github.ashxtrem.hashlark.core.ProviderView
import io.github.ashxtrem.hashlark.core.RepoPreview
import io.github.ashxtrem.hashlark.core.RepoView
import io.github.ashxtrem.hashlark.core.SearchEvent
import io.github.ashxtrem.hashlark.core.SearchQuery
import io.github.ashxtrem.hashlark.core.SearchResult
import io.github.ashxtrem.hashlark.core.Session
import io.github.ashxtrem.hashlark.core.Settings
import io.github.ashxtrem.hashlark.core.SyncReport
import io.github.ashxtrem.hashlark.core.TargetPreference
import io.github.ashxtrem.hashlark.core.TestReport
import io.github.ashxtrem.hashlark.core.TorStatus
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow

fun mergedResult(
    id: String,
    title: String = "Result $id",
    seeders: Int? = 10,
    size: Long? = 1_500_000_000,
    provider: String = "alpha",
    published: String? = "2026-08-01T10:00:00Z",
    magnet: String? = "magnet:?xt=urn:btih:$id",
) = MergedResult(
    id = id,
    primary = SearchResult(
        title = title,
        sizeBytes = size,
        seeders = seeders,
        leechers = 2,
        magnet = magnet,
        published = published,
        providerId = provider,
    ),
    sources = listOf(provider),
    seeders = seeders,
    leechers = 2,
    score = 0.5f,
)

fun providerView(id: String, name: String = id.replaceFirstChar { it.uppercase() }, enabled: Boolean = true) =
    ProviderView(id = id, name = name, kind = ProviderKind.Definition, enabled = enabled)

/** A scripted engine for tests: `search` replays [script] for each query. */
class FakeApi(
    var settings: Settings = Settings(),
    var providers: List<ProviderView> = listOf(providerView("alpha"), providerView("beta")),
    /** The events of one search, given the query. */
    var script: (SearchQuery) -> List<SearchEvent> = { emptyList() },
) : HashlarkApi {
    val queries = mutableListOf<SearchQuery>()
    val favorites = mutableListOf<Favorite>()
    var cancelledSearches = 0
    var failWith: Exception? = null

    /** Set to make `search` wait after replaying the script (a search that is still running). */
    var stayOpen = false

    override fun search(query: SearchQuery): Flow<SearchEvent> = flow {
        queries += query
        failWith?.let { throw it }
        try {
            script(query).forEach { emit(it) }
            if (stayOpen) kotlinx.coroutines.awaitCancellation()
        } catch (e: kotlinx.coroutines.CancellationException) {
            cancelledSearches += 1
            throw e
        }
    }

    override suspend fun result(id: String) = error("unused")
    override suspend fun resolve(resultId: String, prefer: TargetPreference?): DownloadTarget = DownloadTarget.Magnet("magnet:?xt=urn:btih:$resultId")
    override suspend fun fetchTorrent(url: String): ByteArray = byteArrayOf('d'.code.toByte())
    override suspend fun providers() = providers
    override suspend fun updateProvider(id: String, patch: ProviderPatch) = providers.first { it.id == id }.copy(enabled = patch.enabled ?: true)
    override suspend fun removeProvider(id: String) = Unit
    override suspend fun testProvider(id: String) = TestReport(ok = true, resultCount = 3, latencyMs = 120)
    override suspend fun addTorznab(name: String, url: String, apiKey: String?) = providerView(name.lowercase(), name)
    override suspend fun addDefinition(yaml: String) = providerView("imported")
    override suspend fun checkDefinition(yaml: String) = DefinitionCheck(ok = true, id = "imported", name = "Imported", version = 1)
    override suspend fun setSession(providerId: String, session: Session) = providers.first { it.id == providerId }
    override suspend fun settings() = settings
    override suspend fun setSettings(settings: Settings): Settings = settings.also { this.settings = it }
    override suspend fun torStatus() = TorStatus(builtIn = true)
    override suspend fun history(limit: Int): List<HistoryEntry> = emptyList()
    override suspend fun clearHistory() = Unit
    override suspend fun favorites() = favorites.toList()
    override suspend fun addFavorite(resultId: String) = Favorite(mergedResult(resultId), savedAt = 1).also { favorites += it }
    override suspend fun removeFavorite(resultId: String) {
        favorites.removeAll { it.result.id == resultId }
    }
    override suspend fun repos() = emptyList<RepoView>()
    override suspend fun previewRepo(url: String) = RepoPreview(url, "Repo", "AB12-CD34", 2)
    override suspend fun addRepo(url: String) = AddRepoResponse(RepoView("r", url), SyncReport())
    override suspend fun syncRepo(id: String) = SyncReport()
    override suspend fun removeRepo(id: String) = Unit
    override suspend fun refreshTrackers() = 0
    override suspend fun syncAll() = 0
}
