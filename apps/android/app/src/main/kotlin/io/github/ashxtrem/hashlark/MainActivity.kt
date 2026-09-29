// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark

import android.content.Intent
import android.graphics.Color
import android.net.Uri
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.SystemBarStyle
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.viewModels
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.runtime.remember
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.lifecycleScope
import io.github.ashxtrem.hashlark.core.Theme
import io.github.ashxtrem.hashlark.system.Shortcuts
import io.github.ashxtrem.hashlark.ui.AppViewModel
import io.github.ashxtrem.hashlark.ui.HashlarkRoot
import io.github.ashxtrem.hashlark.ui.LibraryViewModel
import io.github.ashxtrem.hashlark.ui.PendingDefinition
import io.github.ashxtrem.hashlark.ui.Screens
import io.github.ashxtrem.hashlark.ui.providers.ProvidersViewModel
import io.github.ashxtrem.hashlark.ui.providers.ReposViewModel
import io.github.ashxtrem.hashlark.ui.search.SearchViewModel
import io.github.ashxtrem.hashlark.ui.settings.SettingsViewModel
import io.github.ashxtrem.hashlark.ui.theme.HashlarkTheme
import io.github.ashxtrem.hashlark.ui.theme.isDark
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * The only activity. It is never locked to an orientation or an aspect ratio,
 * handles fold, unfold and window resizes itself, and keeps all state in
 * activity-scoped ViewModels, so a search keeps streaming while the window
 * changes shape.
 */
class MainActivity : ComponentActivity() {
    private val hashlarkApp get() = application as HashlarkApplication

    private val appVm: AppViewModel by viewModels { factory { AppViewModel(hashlarkApp.runtime, hashlarkApp.prefs) } }
    private val searchVm: SearchViewModel by viewModels { factory { SearchViewModel(hashlarkApp.runtime) } }
    private val libraryVm: LibraryViewModel by viewModels { factory { LibraryViewModel(appVm) } }
    private val providersVm: ProvidersViewModel by viewModels { factory { ProvidersViewModel(appVm) } }
    private val reposVm: ReposViewModel by viewModels { factory { ReposViewModel(appVm) } }
    private val settingsVm: SettingsViewModel by viewModels { factory { SettingsViewModel(appVm) } }

    private var pendingDefinition by mutableStateOf<PendingDefinition?>(null)

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        val screens = Screens(appVm, searchVm, libraryVm, providersVm, reposVm, settingsVm)
        if (savedInstanceState == null) handle(intent)

        // Recent searches become launcher shortcuts once a search has finished.
        lifecycleScope.launch {
            searchVm.state.map { it.progress.done && it.shown != null }.distinctUntilChanged().filter { it }.collect {
                appVm.withApi { api -> Shortcuts.publishRecent(applicationContext, api.history(3)) }
            }
        }

        setContent {
            val settings by appVm.settings.collectAsStateWithLifecycle()
            val dynamic by hashlarkApp.prefs.dynamicColor.collectAsStateWithLifecycle(initialValue = true)
            val theme = settings?.ui?.theme ?: Theme.System
            HashlarkTheme(theme = theme, dynamicColor = dynamic) {
                val dark = isDark(theme)
                LaunchedEffect(dark) {
                    // The in-app theme choice decides the icon colours of the system bars.
                    enableEdgeToEdge(
                        statusBarStyle = SystemBarStyle.auto(Color.TRANSPARENT, Color.TRANSPARENT) { dark },
                        navigationBarStyle = SystemBarStyle.auto(Color.TRANSPARENT, Color.TRANSPARENT) { dark },
                    )
                }
                HashlarkRoot(screens, pendingDefinition, onDefinitionHandled = { pendingDefinition = null })
            }
        }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        handle(intent)
    }

    private fun handle(intent: Intent) {
        val data = intent.data
        val isFile = intent.action == Intent.ACTION_VIEW && (data?.scheme == "content" || data?.scheme == "file")
        if (isFile && data != null) {
            // Reading a file is I/O: do it off the main thread, then show the confirmation.
            lifecycleScope.launch {
                val text = withContext(Dispatchers.IO) { readText(data) }
                if (text == null) appVm.say("Could not read this file") else pendingDefinition = PendingDefinition(text, data.lastPathSegment)
            }
            return
        }
        IntentRouter.route(intent) { null }?.let(appVm::post)
    }

    private fun readText(uri: Uri): String? = runCatching {
        contentResolver.openInputStream(uri)?.use { stream ->
            // Read at most one byte more than allowed, so a huge file is refused without loading it.
            val limit = IntentRouter.MAX_DEFINITION_BYTES
            val out = java.io.ByteArrayOutputStream()
            val buffer = ByteArray(8 * 1024)
            while (out.size() <= limit) {
                val n = stream.read(buffer, 0, minOf(buffer.size, limit + 1 - out.size()))
                if (n < 0) break
                out.write(buffer, 0, n)
            }
            if (out.size() > limit) null else out.toByteArray().decodeToString(throwOnInvalidSequence = false)
        }
    }.getOrNull()?.takeIf { it.isNotBlank() }

    private inline fun <reified T : ViewModel> factory(crossinline create: () -> T) = object : ViewModelProvider.Factory {
        @Suppress("UNCHECKED_CAST")
        override fun <V : ViewModel> create(modelClass: Class<V>): V = create() as V
    }
}
