// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark.core

import io.github.ashxtrem.hashlark.ffi.HashlarkEngine
import io.github.ashxtrem.hashlark.ffi.HashlarkException
import io.github.ashxtrem.hashlark.ffi.SearchListener
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.buffer
import kotlinx.coroutines.flow.callbackFlow
import kotlinx.serialization.decodeFromString
import kotlinx.serialization.encodeToString

/** A failure reported by the core. [code] is the HTTP API's stable error code. */
class HashlarkFailure(
    val code: String,
    override val message: String,
    val details: List<String> = emptyList(),
    val providerError: ErrorKind? = null,
) : Exception(message) {
    val isChallenge: Boolean get() = providerError == ErrorKind.ChallengeRequired
}

/**
 * Everything the app asks of the core. The interface exists so screens and
 * view models can be tested with a fake; [EngineApi] is the real one.
 */
interface HashlarkApi {
    /**
     * Starts a search. The flow emits events as providers answer, ends after
     * [SearchEvent.Done], and cancels the search when collection stops.
     */
    fun search(query: SearchQuery): Flow<SearchEvent>

    suspend fun result(id: String): MergedResult
    suspend fun resolve(resultId: String, prefer: TargetPreference? = null): DownloadTarget
    suspend fun fetchTorrent(url: String): ByteArray

    suspend fun providers(): List<ProviderView>
    suspend fun updateProvider(id: String, patch: ProviderPatch): ProviderView
    suspend fun removeProvider(id: String)
    suspend fun testProvider(id: String): TestReport
    suspend fun addTorznab(name: String, url: String, apiKey: String?): ProviderView
    suspend fun addDefinition(yaml: String): ProviderView
    suspend fun checkDefinition(yaml: String): DefinitionCheck
    suspend fun setSession(providerId: String, session: Session): ProviderView

    suspend fun settings(): Settings
    suspend fun setSettings(settings: Settings): Settings
    suspend fun torStatus(): TorStatus

    suspend fun history(limit: Int = 50): List<HistoryEntry>
    suspend fun clearHistory()
    suspend fun favorites(): List<Favorite>
    suspend fun addFavorite(resultId: String): Favorite
    suspend fun removeFavorite(resultId: String)

    suspend fun repos(): List<RepoView>
    suspend fun previewRepo(url: String): RepoPreview
    suspend fun addRepo(url: String): AddRepoResponse
    suspend fun syncRepo(id: String): SyncReport
    suspend fun removeRepo(id: String)

    suspend fun refreshTrackers(): Int

    /** Syncs every repository and the tracker list; returns how many repositories failed. */
    suspend fun syncAll(): Int
}

enum class TargetPreference(val wire: String) {
    Magnet("magnet"),
    TorrentFile("torrent_file"),
}

/** [HashlarkApi] over the UniFFI engine. */
class EngineApi(private val engine: HashlarkEngine) : HashlarkApi {
    private val json = HashlarkJson

    override fun search(query: SearchQuery): Flow<SearchEvent> = callbackFlow {
        val listener = object : SearchListener {
            override fun onEvent(eventJson: String) {
                val event = runCatching { json.decodeFromString<SearchEvent>(eventJson) }.getOrNull()
                    ?: return
                trySend(event)
                if (event is SearchEvent.Done) close()
            }

            // Also ends a cancelled or abandoned search, which sends no `done`.
            override fun onEnd() {
                close()
            }
        }
        val handle = call { engine.search(json.encodeToString(query), listener) }
        awaitClose { handle.cancel() }
    }.buffer(Channel.UNLIMITED)

    override suspend fun result(id: String) = decode<MergedResult> { engine.result(id) }

    override suspend fun resolve(resultId: String, prefer: TargetPreference?) =
        decode<DownloadTarget> { engine.resolve(resultId, prefer?.wire) }

    override suspend fun fetchTorrent(url: String): ByteArray = call { engine.fetchTorrent(url) }

    override suspend fun providers() = decodeList<ProviderView> { engine.providers() }

    override suspend fun updateProvider(id: String, patch: ProviderPatch) =
        decode<ProviderView> { engine.updateProvider(id, json.encodeToString(patch)) }

    override suspend fun removeProvider(id: String) = call { engine.removeProvider(id) }

    override suspend fun testProvider(id: String) = decode<TestReport> { engine.testProvider(id) }

    override suspend fun addTorznab(name: String, url: String, apiKey: String?) =
        decode<ProviderView> { engine.addTorznab(name, url, apiKey?.takeIf { it.isNotBlank() }) }

    override suspend fun addDefinition(yaml: String) = decode<ProviderView> { engine.addDefinition(yaml) }

    override suspend fun checkDefinition(yaml: String): DefinitionCheck =
        json.decodeFromString(call { io.github.ashxtrem.hashlark.ffi.checkDefinition(yaml) })

    override suspend fun setSession(providerId: String, session: Session) =
        decode<ProviderView> { engine.setSession(providerId, json.encodeToString(session)) }

    override suspend fun settings(): Settings = json.decodeFromString(call { engine.settings() })

    override suspend fun setSettings(settings: Settings) =
        decode<Settings> { engine.setSettings(json.encodeToString(settings)) }

    override suspend fun torStatus() = decode<TorStatus> { engine.torStatus() }

    override suspend fun history(limit: Int) = decodeList<HistoryEntry> { engine.history(limit.toUInt()) }

    override suspend fun clearHistory() = call { engine.clearHistory() }

    override suspend fun favorites() = decodeList<Favorite> { engine.favorites() }

    override suspend fun addFavorite(resultId: String) = decode<Favorite> { engine.addFavorite(resultId) }

    override suspend fun removeFavorite(resultId: String) = call { engine.removeFavorite(resultId) }

    override suspend fun repos() = decodeList<RepoView> { engine.repos() }

    override suspend fun previewRepo(url: String) = decode<RepoPreview> { engine.previewRepo(url) }

    override suspend fun addRepo(url: String) = decode<AddRepoResponse> { engine.addRepo(url) }

    override suspend fun syncRepo(id: String) = decode<SyncReport> { engine.syncRepo(id) }

    override suspend fun removeRepo(id: String) = call { engine.removeRepo(id) }

    override suspend fun refreshTrackers(): Int = call { engine.refreshTrackers() }.toInt()

    override suspend fun syncAll(): Int = call { engine.syncAll() }.toInt()

    private suspend inline fun <reified T> decode(crossinline block: suspend () -> String): T =
        json.decodeFromString(call { block() })

    private suspend inline fun <reified T> decodeList(crossinline block: suspend () -> String): List<T> =
        json.decodeFromString<List<T>>(call { block() })
}

/** Runs a call into the core, turning its exception into [HashlarkFailure]. */
internal suspend inline fun <T> call(crossinline block: suspend () -> T): T =
    try {
        block()
    } catch (e: HashlarkException.Failed) {
        throw HashlarkFailure(
            code = e.code,
            message = e.description,
            details = e.details,
            providerError = e.providerError?.let(::errorKindOf),
        )
    }

private fun errorKindOf(wire: String): ErrorKind? =
    runCatching { HashlarkJson.decodeFromString<ErrorKind>("\"$wire\"") }.getOrNull()
