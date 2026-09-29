// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark.system

import android.content.Context
import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.booleanPreferencesKey
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import androidx.datastore.preferences.preferencesDataStore
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map

private val Context.dataStore: DataStore<Preferences> by preferencesDataStore(name = "hashlark_prefs")

/**
 * Preferences that only make sense on this device and are not part of the
 * core's settings: which torrent client to use, whether to use system colours,
 * which update notice was dismissed.
 */
class AppPrefs(context: Context) {
    private val store = context.applicationContext.dataStore

    /** Package of the torrent client chosen for magnet links; `null` lets Android ask. */
    val magnetPackage: Flow<String?> = store.data.map { it[MAGNET_PACKAGE] }

    /** Use Material You colours on Android 12 and newer. */
    val dynamicColor: Flow<Boolean> = store.data.map { it[DYNAMIC_COLOR] ?: true }

    /** Version of the update notice the user dismissed. */
    val dismissedUpdate: Flow<String?> = store.data.map { it[DISMISSED_UPDATE] }

    suspend fun setMagnetPackage(packageName: String?) = store.edit {
        if (packageName == null) it.remove(MAGNET_PACKAGE) else it[MAGNET_PACKAGE] = packageName
    }

    suspend fun setDynamicColor(enabled: Boolean) = store.edit { it[DYNAMIC_COLOR] = enabled }

    suspend fun dismissUpdate(version: String) = store.edit { it[DISMISSED_UPDATE] = version }

    private companion object {
        val MAGNET_PACKAGE = stringPreferencesKey("magnet_package")
        val DYNAMIC_COLOR = booleanPreferencesKey("dynamic_color")
        val DISMISSED_UPDATE = stringPreferencesKey("dismissed_update")
    }
}
