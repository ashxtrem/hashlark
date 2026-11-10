// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark

import android.app.Application
import android.content.Context
import android.util.Log
import io.github.ashxtrem.hashlark.core.HashlarkApi
import io.github.ashxtrem.hashlark.core.HashlarkRuntime
import io.github.ashxtrem.hashlark.system.AppPrefs
import io.github.ashxtrem.hashlark.system.LanController
import io.github.ashxtrem.hashlark.system.TorrentService
import io.github.ashxtrem.hashlark.system.TorrentServiceLogic
import io.github.ashxtrem.hashlark.system.TorrentStorage
import io.github.ashxtrem.hashlark.work.SyncScheduler
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext

/**
 * Owns what must outlive any activity: the engine and the preferences. An
 * activity that is rotated, folded or recreated finds the same running
 * engine, so an ongoing search is never lost.
 */
class HashlarkApplication : Application() {
    val runtime: HashlarkRuntime by lazy { HashlarkRuntime.forContext(this) }
    val prefs: AppPrefs by lazy { AppPrefs(this) }

    /** Work that outlives any screen (LAN sharing starting and stopping). */
    val scope = CoroutineScope(SupervisorJob() + Dispatchers.Default)
    val lan: LanController by lazy { LanController(this, prefs, scope) }

    private val torrentsLock = Mutex()

    @Volatile
    private var torrentsStarted = false

    override fun onCreate() {
        super.onCreate()
        SyncScheduler.schedule(this)
    }

    /**
     * The engine with the built-in torrent client started (once per process),
     * saving into [TorrentStorage.resolve]. Settings saves only reach the
     * client after this. Background sync doesn't call it, so it never starts
     * torrents without the service.
     */
    suspend fun startTorrents(): HashlarkApi {
        val api = runtime.api()
        if (torrentsStarted) return api
        torrentsLock.withLock {
            if (!torrentsStarted) {
                val dir = withContext(Dispatchers.IO) { TorrentStorage.resolve(this@HashlarkApplication) }
                api.startTorrents(dir.absolutePath)
                torrentsStarted = true
            }
        }
        return api
    }

    /** At app start: starts the client, and the service when torrents are unfinished. */
    fun resumeTorrents() {
        scope.launch {
            try {
                val api = startTorrents()
                val stats = api.torrentStats()
                if (TorrentServiceLogic.startsOnOpen(stats, api.torrents())) TorrentService.start(this@HashlarkApplication)
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                Log.w("Hashlark", "could not start the torrent client", e)
            }
        }
    }
}

val Context.hashlark: HashlarkApplication
    get() = applicationContext as HashlarkApplication
