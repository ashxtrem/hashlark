// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark.ui

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import io.github.ashxtrem.hashlark.BuildConfig
import io.github.ashxtrem.hashlark.core.AddRequest
import io.github.ashxtrem.hashlark.core.AddSource
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

    /** A magnet link or `.torrent` file opened from another app, for the built-in client. */
    data class AddTorrent(val source: AddSource) : AppEvent
}

/** Where an add stands. */
enum class AddStage {
    /** The one-time notice about sharing pieces and showing the IP address. */
    Notice,

    /** The Add sheet: folder and "Start paused". */
    Options,

    /** Asking for notifications (Android 13+) and storage (Android 8-9), then adding. */
    Permissions,
}

/** An add on its way through [AppViewModel.requestAdd]. */
data class PendingAdd(
    val request: AddRequest,
    /** A name to show in the sheet, when known (a result's title). */
    val title: String?,
    /** The paired device that sent the link, for the message. */
    val from: String?,
    val withOptions: Boolean,
    val stage: AddStage,
)

/**
 * State shared by every screen: settings, the provider list, messages for the
 * snackbar, update notice. It lives in the activity's ViewModel store, so it
 * survives rotation, folding and recreation.
 */
class AppViewModel(
    private val runtime: HashlarkRuntime,
    private val prefs: AppPrefs,
    /** The engine with the torrent client started (`HashlarkApplication.startTorrents`). */
    private val torrentApi: suspend () -> HashlarkApi = { runtime.api() },
    /** Called after a torrent was added: starts the download service. */
    private val onTorrentAdded: () -> Unit = {},
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

    // ----- torrents ---------------------------------------------------------------------

    private val _pendingAdd = MutableStateFlow<PendingAdd?>(null)

    /** The add that waits for the one-time notice, the Add sheet or the permission prompts. */
    val pendingAdd: StateFlow<PendingAdd?> = _pendingAdd.asStateFlow()

    private val _torrentIds = MutableStateFlow<Set<String>>(emptySet())

    /** Info hashes in the built-in client, for the "In downloads" badge on results. */
    val torrentIds: StateFlow<Set<String>> = _torrentIds.asStateFlow()

    /** Permissions are asked at most once per run, so a refusal isn't asked again on every add. */
    private var permissionsAsked = false

    /** Whether results go to the built-in client (else to the user's torrent app). */
    val builtInClient: Boolean get() = _settings.value?.torrents?.enabled == true

    /**
     * Starts the add flow: the one-time notice (first add only), then the Add sheet when
     * `ask_on_add` is on or [withOptions] (a long-press), then the permission prompts, then
     * the add itself. Intents, LAN links and results all come through here.
     */
    fun requestAdd(source: AddSource, title: String? = null, withOptions: Boolean = false, from: String? = null) {
        val torrents = _settings.value?.torrents ?: return
        val options = withOptions || torrents.askOnAdd
        _pendingAdd.value = PendingAdd(
            request = AddRequest(source),
            title = title,
            from = from,
            withOptions = options,
            stage = when {
                !torrents.noticeAccepted -> AddStage.Notice
                options -> AddStage.Options
                else -> AddStage.Permissions
            },
        )
    }

    /** The user accepted the one-time notice; it is stored so it never shows again. */
    fun acceptNotice() {
        val pending = _pendingAdd.value ?: return
        viewModelScope.launch {
            val current = _settings.value ?: return@launch
            saveSettings(current.copy(torrents = current.torrents.copy(noticeAccepted = true))) ?: return@launch
            _pendingAdd.value = pending.copy(stage = if (pending.withOptions) AddStage.Options else AddStage.Permissions)
        }
    }

    /** The Add sheet was confirmed with these options. */
    fun confirmAddOptions(saveDir: String?, paused: Boolean) {
        val pending = _pendingAdd.value ?: return
        _pendingAdd.value = pending.copy(
            request = pending.request.copy(saveDir = saveDir?.trim()?.ifBlank { null }, paused = paused),
            stage = AddStage.Permissions,
        )
    }

    fun cancelAdd() {
        _pendingAdd.value = null
    }

    /**
     * Whether to show the permission prompts for this add: only the first time in a run, so a
     * refusal isn't asked again on every add (and a rotation doesn't ask twice).
     */
    fun takePermissionPrompt(): Boolean = (!permissionsAsked).also { permissionsAsked = true }

    /**
     * The permission prompts are over: adds the torrent. [publicDir] is `Download/Hashlark` when
     * storage access was just granted (Android 8-9); the client resolved its default folder before
     * that, so the folder is saved as the download folder for the grant to count at once.
     */
    fun finishAdd(publicDir: String? = null) {
        permissionsAsked = true
        val pending = _pendingAdd.value ?: return
        _pendingAdd.value = null
        viewModelScope.launch {
            val current = _settings.value
            if (publicDir != null && current != null && current.torrents.downloadDir == null) {
                saveSettings(current.copy(torrents = current.torrents.copy(downloadDir = publicDir)))
            }
            add(pending)
        }
    }

    private suspend fun add(pending: PendingAdd) {
        try {
            val response = torrentApi().addTorrent(pending.request)
            val torrent = response.torrent
            _torrentIds.value = _torrentIds.value + torrent.id
            val from = pending.from?.let { " from $it" }.orEmpty()
            val text = if (response.already) "Already in downloads: ${torrent.name}" else "Downloading ${torrent.name}$from"
            say(text, "Show") { post(AppEvent.Navigate(Destination.Downloads)) }
            onTorrentAdded()
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            report(e, "Could not add the torrent")
        }
    }

    /**
     * Adds a torrent with the default options, through the same flow as every other add
     * (notice, `ask_on_add`, permissions).
     */
    fun addTorrent(source: AddSource) = requestAdd(source)

    /** "Turn on the built-in client" in the No torrent client dialog: saves the setting, then retries [retry]. */
    fun turnOnBuiltInClient(retry: AddSource? = null, title: String? = null) {
        viewModelScope.launch {
            val current = _settings.value ?: return@launch
            saveSettings(current.copy(torrents = current.torrents.copy(enabled = true))) ?: return@launch
            if (retry != null) requestAdd(retry, title)
        }
    }

    /** Records the torrents the client has, for the badges on results. */
    fun setTorrentIds(ids: Set<String>) {
        _torrentIds.value = ids
    }

    /** Reads the client's torrents once (the Search screen calls this, so the badges are current). */
    fun refreshTorrentIds() {
        if (!builtInClient) return
        viewModelScope.launch {
            try {
                _torrentIds.value = torrentApi().torrents().map { it.id }.toSet()
            } catch (e: CancellationException) {
                throw e
            } catch (_: Exception) {
                // The badges are a hint; a client that isn't ready yet just shows none.
            }
        }
    }

    /** Runs [block] with the engine that has the torrent client started. Returns `null` on failure. */
    suspend fun <T> withTorrents(prefix: String? = null, block: suspend (HashlarkApi) -> T): T? = try {
        block(torrentApi())
    } catch (e: CancellationException) {
        throw e
    } catch (e: Exception) {
        report(e, prefix)
        null
    }

    /** Like [withTorrents], but the failure is thrown, for callers that show it themselves. */
    suspend fun torrents(): HashlarkApi = torrentApi()

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
