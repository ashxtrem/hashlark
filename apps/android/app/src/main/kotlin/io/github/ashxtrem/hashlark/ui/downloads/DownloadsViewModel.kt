// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark.ui.downloads

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import io.github.ashxtrem.hashlark.core.BulkAction
import io.github.ashxtrem.hashlark.core.BulkRequest
import io.github.ashxtrem.hashlark.core.PeerInfo
import io.github.ashxtrem.hashlark.core.QueueMove
import io.github.ashxtrem.hashlark.core.QueueStep
import io.github.ashxtrem.hashlark.core.SessionStats
import io.github.ashxtrem.hashlark.core.TorrentAction
import io.github.ashxtrem.hashlark.core.TorrentDetails
import io.github.ashxtrem.hashlark.core.TorrentPatch
import io.github.ashxtrem.hashlark.core.TorrentSummary
import io.github.ashxtrem.hashlark.ui.AppViewModel
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

/**
 * The built-in client's torrents for the Downloads screen. The screen polls [refresh]
 * once a second while it is visible (plan §2: no FFI callback), and every action
 * refreshes at once so the list answers without waiting for the next tick.
 */
class DownloadsViewModel(private val app: AppViewModel) : ViewModel() {
    private val _list = MutableStateFlow<List<TorrentSummary>?>(null)
    val list: StateFlow<List<TorrentSummary>?> = _list.asStateFlow()

    private val _stats = MutableStateFlow<SessionStats?>(null)
    val stats: StateFlow<SessionStats?> = _stats.asStateFlow()

    /** Why the list couldn't be read (the client isn't running), shown in place of the list. */
    private val _error = MutableStateFlow<String?>(null)
    val error: StateFlow<String?> = _error.asStateFlow()

    suspend fun refresh() {
        try {
            val api = app.torrents()
            val torrents = api.torrents()
            _stats.value = api.torrentStats()
            _list.value = torrents
            _error.value = null
            app.setTorrentIds(torrents.map { it.id }.toSet())
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            _error.value = AppViewModel.describe(e)
        }
    }

    private fun act(prefix: String, block: suspend (io.github.ashxtrem.hashlark.core.HashlarkApi) -> Unit) {
        viewModelScope.launch {
            app.withTorrents(prefix) { block(it) }
            refresh()
        }
    }

    /** Pause, Resume, Stop, Start or Force start on [ids]. */
    fun run(ids: Collection<String>, action: TorrentAction) {
        val bulk = when (action) {
            TorrentAction.Pause -> BulkAction.Pause
            TorrentAction.Resume -> BulkAction.Resume
            TorrentAction.Stop -> BulkAction.Stop
            TorrentAction.Start -> BulkAction.Start
            TorrentAction.ForceStart -> BulkAction.ForceStart
        }
        act("Could not change the torrents") { api ->
            val result = api.torrentBulk(BulkRequest(ids.toList(), bulk))
            result.failed.firstOrNull()?.let { app.say("${result.failed.size} failed: ${it.message}") }
        }
    }

    /** Pauses or resumes everything. */
    fun all(action: BulkAction) = act("Could not change the torrents") { it.torrentBulk(BulkRequest(null, action)) }

    fun setTurtle(on: Boolean) = act("Could not change the speed") { it.torrentBulk(BulkRequest(null, BulkAction.Turtle(on))) }

    /** Deletes [ids], with their downloaded files when [deleteFiles]. */
    fun remove(ids: Collection<String>, deleteFiles: Boolean) {
        act("Could not delete") { api ->
            val result = api.torrentBulk(BulkRequest(ids.toList(), BulkAction.Remove(deleteFiles)))
            if (result.failed.isEmpty()) {
                val n = result.ok.size
                app.say(if (n == 1) "Deleted 1 torrent" else "Deleted $n torrents")
            } else {
                app.say("${result.failed.size} could not be deleted: ${result.failed.first().message}")
            }
        }
    }

    /** One queue step for every selected torrent, in an order that keeps the selection's own order. */
    fun move(ids: Collection<String>, step: QueueStep) {
        val all = _list.value.orEmpty()
        act("Could not move") { api ->
            DownloadsLogic.moveOrder(all, ids, step).forEach { id ->
                api.updateTorrent(id, TorrentPatch(queuePos = QueueMove.Step(step)))
            }
        }
    }

    /** Drops [id] at [index] of the whole queue (a drag in the Queued filter). */
    fun moveTo(id: String, index: Int) = act("Could not move") { api ->
        api.updateTorrent(id, TorrentPatch(queuePos = QueueMove.Index(index)))
    }

    /** Reads [id] quietly (the details sheet polls this once a second): `null` when it can't be read, no message. */
    suspend fun details(id: String): TorrentDetails? = try {
        app.torrents().torrent(id)
    } catch (e: CancellationException) {
        throw e
    } catch (_: Exception) {
        null
    }

    suspend fun peers(id: String): List<PeerInfo> = try {
        app.torrents().torrentPeers(id)
    } catch (e: CancellationException) {
        throw e
    } catch (_: Exception) {
        emptyList()
    }

    /** Applies [patch] to [id]; returns the new details, or `null` on failure (already reported). */
    suspend fun update(id: String, patch: TorrentPatch): TorrentDetails? =
        app.withTorrents("Could not change the torrent") { it.updateTorrent(id, patch) }.also { refresh() }
}
