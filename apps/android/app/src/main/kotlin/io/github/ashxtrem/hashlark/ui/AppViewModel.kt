// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark.ui

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import io.github.ashxtrem.hashlark.BuildConfig
import io.github.ashxtrem.hashlark.core.AvailableUpdate
import io.github.ashxtrem.hashlark.core.HashlarkApi
import io.github.ashxtrem.hashlark.core.HashlarkFailure
import io.github.ashxtrem.hashlark.core.HashlarkRuntime
import io.github.ashxtrem.hashlark.core.ProviderView
import io.github.ashxtrem.hashlark.core.Settings
import io.github.ashxtrem.hashlark.core.TorStatus
import io.github.ashxtrem.hashlark.core.UpdateChecker
import io.github.ashxtrem.hashlark.system.AppPrefs
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch

/** A snackbar message, optionally with one action (e.g. "Open with"). */
data class UiMessage(
    val text: String,
    val actionLabel: String? = null,
    val onAction: (() -> Unit)? = null,
)

/** What the rest of the app wants the UI to do, from intents and shortcuts. */
sealed interface AppEvent {
    data class Navigate(val destination: Destination) : AppEvent
    data class RunSearch(val query: io.github.ashxtrem.hashlark.core.SearchQuery) : AppEvent
    data class ImportRepo(val url: String) : AppEvent
    data class ImportDefinition(val yaml: String, val fileName: String?) : AppEvent
}

/**
 * State shared by every screen: settings, the provider list, messages for the
 * snackbar, update notice. It lives in the activity's ViewModel store, so it
 * survives rotation, folding and recreation.
 */
class AppViewModel(
    private val runtime: HashlarkRuntime,
    private val prefs: AppPrefs,
    /** Looks for a newer release; a parameter so tests never touch the network. */
    private val checkForNewerRelease: suspend () -> AvailableUpdate? = {
        UpdateChecker(currentVersion = BuildConfig.VERSION_NAME.substringBefore('-')).check()
    },
) : ViewModel() {
    private val _settings = MutableStateFlow<Settings?>(null)
    val settings: StateFlow<Settings?> = _settings.asStateFlow()

    private val _providers = MutableStateFlow<List<ProviderView>?>(null)
    val providers: StateFlow<List<ProviderView>?> = _providers.asStateFlow()

    /** Set when the engine could not be opened (e.g. a corrupt database). */
    private val _startError = MutableStateFlow<String?>(null)
    val startError: StateFlow<String?> = _startError.asStateFlow()

    private val _update = MutableStateFlow<AvailableUpdate?>(null)
    val update: StateFlow<AvailableUpdate?> = _update.asStateFlow()

    private val _messages = MutableSharedFlow<UiMessage>(extraBufferCapacity = 16, onBufferOverflow = BufferOverflow.DROP_OLDEST)

    /** One-off messages for the snackbar. */
    val messages: SharedFlow<UiMessage> = _messages.asSharedFlow()

    private val eventChannel = Channel<AppEvent>(Channel.BUFFERED)

    /** Intents turned into actions; each is delivered once. */
    val events = eventChannel.receiveAsFlow()

    val providerNames: StateFlow<Map<String, String>> = _providers
        .map { list -> list.orEmpty().associate { it.id to it.name } }
        .stateIn(viewModelScope, SharingStarted.Eagerly, emptyMap())

    val enabledProviderCount: StateFlow<Int> = _providers
        .map { list -> list.orEmpty().count { it.enabled && it.error == null } }
        .stateIn(viewModelScope, SharingStarted.Eagerly, 0)

    /** The update notice, unless the user already dismissed that version. */
    val visibleUpdate: StateFlow<AvailableUpdate?> =
        combine(_update, prefs.dismissedUpdate) { update, dismissed ->
            update?.takeUnless { it.version == dismissed }
        }.stateIn(viewModelScope, SharingStarted.Eagerly, null)

    init {
        viewModelScope.launch { bootstrap() }
    }

    private suspend fun bootstrap() {
        try {
            val api = runtime.api()
            _settings.value = api.settings()
            _providers.value = api.providers()
            checkForUpdate()
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            _startError.value = describe(e)
        }
    }

    /** Opens the engine again after a failed start. */
    fun retryStart() {
        _startError.value = null
        viewModelScope.launch { bootstrap() }
    }

    private suspend fun checkForUpdate() {
        if (_settings.value?.updates?.checkForUpdates != true) {
            _update.value = null
            return
        }
        _update.value = checkForNewerRelease()
    }

    fun dismissUpdate() {
        val version = _update.value?.version ?: return
        viewModelScope.launch { prefs.dismissUpdate(version) }
    }

    fun say(message: String, actionLabel: String? = null, onAction: (() -> Unit)? = null) {
        _messages.tryEmit(UiMessage(message, actionLabel, onAction))
    }

    /** Shows a failure to the user. */
    fun report(error: Throwable, prefix: String? = null) {
        val text = describe(error)
        say(if (prefix == null) text else "$prefix: $text")
    }

    fun post(event: AppEvent) {
        eventChannel.trySend(event)
    }

    // ----- settings -------------------------------------------------------------------

    /** Saves [settings]; returns the settings as the core applied them, or `null` on failure. */
    suspend fun saveSettings(settings: Settings): Settings? = try {
        val saved = runtime.api().setSettings(settings)
        _settings.value = saved
        // The network settings can change which providers work (Tor, proxy).
        _providers.value = runtime.api().providers()
        checkForUpdate()
        saved
    } catch (e: CancellationException) {
        throw e
    } catch (e: Exception) {
        report(e)
        null
    }

    /** Small edits that save at once, like the first-run notice. */
    fun updateSettings(transform: (Settings) -> Settings) {
        val current = _settings.value ?: return
        viewModelScope.launch { saveSettings(transform(current)) }
    }

    suspend fun torStatus(): TorStatus? = try {
        runtime.api().torStatus()
    } catch (e: CancellationException) {
        throw e
    } catch (_: Exception) {
        null
    }

    // ----- providers --------------------------------------------------------------------

    fun refreshProviders() {
        viewModelScope.launch {
            try {
                _providers.value = runtime.api().providers()
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                report(e)
            }
        }
    }

    /** Replaces one provider in the list after a change. */
    fun putProvider(provider: ProviderView) {
        _providers.value = _providers.value.orEmpty().let { list ->
            if (list.any { it.id == provider.id }) list.map { if (it.id == provider.id) provider else it } else list + provider
        }
    }

    fun removeProviderLocally(id: String) {
        _providers.value = _providers.value.orEmpty().filterNot { it.id == id }
    }

    /** Runs [block] with the engine, showing any failure as a message. Returns `null` on failure. */
    suspend fun <T> withApi(prefix: String? = null, block: suspend (HashlarkApi) -> T): T? = try {
        block(runtime.api())
    } catch (e: CancellationException) {
        throw e
    } catch (e: Exception) {
        report(e, prefix)
        null
    }

    companion object {
        fun describe(error: Throwable): String = when (error) {
            is HashlarkFailure -> if (error.details.isEmpty()) error.message else error.message + ": " + error.details.joinToString("; ")
            else -> error.message ?: error::class.simpleName ?: "Something went wrong"
        }
    }
}

/** Waits for the first non-null settings; used by code that needs them once. */
suspend fun AppViewModel.awaitSettings(): Settings = settings.first { it != null }!!
