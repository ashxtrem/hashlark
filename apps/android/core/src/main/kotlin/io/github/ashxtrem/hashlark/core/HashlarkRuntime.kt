// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark.core

import android.content.Context
import io.github.ashxtrem.hashlark.ffi.openEngine
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

/**
 * Owns the one engine of the process. It lives in `Application` scope, so
 * rotating, folding or recreating an activity never restarts a search or
 * reopens the database.
 */
class HashlarkRuntime(private val opener: suspend () -> HashlarkApi) {
    private val mutex = Mutex()

    @Volatile
    private var api: HashlarkApi? = null

    /** The engine, opened on first use. A failed open is retried on the next call. */
    suspend fun api(): HashlarkApi = api ?: mutex.withLock { api ?: opener().also { api = it } }

    companion object {
        fun forContext(context: Context): HashlarkRuntime {
            val app = context.applicationContext
            return HashlarkRuntime {
                val engine = call {
                    openEngine(app.filesDir.absolutePath, KeystoreSecretStore(app))
                }
                EngineApi(engine)
            }
        }
    }
}
