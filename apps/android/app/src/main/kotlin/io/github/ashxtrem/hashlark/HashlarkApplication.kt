// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark

import android.app.Application
import android.content.Context
import io.github.ashxtrem.hashlark.core.HashlarkRuntime
import io.github.ashxtrem.hashlark.system.AppPrefs
import io.github.ashxtrem.hashlark.work.SyncScheduler

/**
 * Owns what must outlive any activity: the engine and the preferences. An
 * activity that is rotated, folded or recreated finds the same running
 * engine, so an ongoing search is never lost.
 */
class HashlarkApplication : Application() {
    val runtime: HashlarkRuntime by lazy { HashlarkRuntime.forContext(this) }
    val prefs: AppPrefs by lazy { AppPrefs(this) }

    override fun onCreate() {
        super.onCreate()
        SyncScheduler.schedule(this)
    }
}

val Context.hashlark: HashlarkApplication
    get() = applicationContext as HashlarkApplication
