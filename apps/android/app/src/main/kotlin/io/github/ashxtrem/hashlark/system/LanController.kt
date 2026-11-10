// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark.system

import android.content.Context
import android.net.wifi.WifiManager
import android.os.Build
import android.provider.Settings
import io.github.ashxtrem.hashlark.core.HashlarkFailure
import io.github.ashxtrem.hashlark.core.LanApi
import io.github.ashxtrem.hashlark.core.LanDevice
import io.github.ashxtrem.hashlark.core.LanEvents
import io.github.ashxtrem.hashlark.core.LanReceived
import io.github.ashxtrem.hashlark.core.PairingCode
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

/** What the device picker and Settings show. */
data class LanState(
    /** The user's choice (Settings → Devices on this network). */
    val enabled: Boolean = true,
    /** Listening and looking for devices right now. */
    val running: Boolean = false,
    /** This phone's name as computers see it. */
    val name: String? = null,
    /** `ip:port` to type on a computer that can't find this phone. */
    val addresses: List<String> = emptyList(),
    val devices: List<LanDevice> = emptyList(),
    /** The PIN shown while a computer may pair with this phone. */
    val pairing: PairingCode? = null,
    /** Why sharing couldn't start. */
    val error: String? = null,
    /** Why automatic discovery isn't working (typed addresses still are). */
    val discoveryError: String? = null,
)

/**
 * Runs LAN sharing while the app is in the foreground: finding computers,
 * pairing with them, sending them links, and receiving links from them.
 * One instance per process (see `HashlarkApplication`).
 *
 * Android drops multicast replies unless an app holds a multicast lock, so
 * the lock is held exactly while sharing runs.
 */
class LanController(
    private val context: Context,
    private val prefs: AppPrefs,
    private val scope: CoroutineScope,
    /** Opens the service; a parameter so tests can use a fake. */
    private val opener: suspend (name: String, events: LanEvents) -> LanApi = { name, events ->
        LanApi.open(context, name, events)
    },
) {
    private val _state = MutableStateFlow(LanState())
    val state: StateFlow<LanState> = _state.asStateFlow()

    private val _received = MutableSharedFlow<LanReceived>(extraBufferCapacity = 8, onBufferOverflow = BufferOverflow.DROP_OLDEST)

    /** Links paired computers sent; the UI opens them. */
    val received: SharedFlow<LanReceived> = _received.asSharedFlow()

    private val _paired = MutableSharedFlow<LanDevice>(extraBufferCapacity = 4, onBufferOverflow = BufferOverflow.DROP_OLDEST)

    /** Devices that just paired (from either side); the UI confirms it. */
    val paired: SharedFlow<LanDevice> = _paired.asSharedFlow()

    private val mutex = Mutex()
    private var lan: LanApi? = null
    private var foreground = false
    private var multicastLock: WifiManager.MulticastLock? = null

    private val events = object : LanEvents {
        override fun received(item: LanReceived) {
            _received.tryEmit(item)
        }

        override fun devicesChanged() = refresh()

        override fun paired(device: LanDevice) {
            refresh()
            _paired.tryEmit(device)
        }

        override fun pairingChanged(code: PairingCode?) = _state.update { it.copy(pairing = code) }
    }

    /** The app came to the foreground. */
    fun onForeground() {
        foreground = true
        scope.launch {
            val enabled = prefs.lanEnabled.first()
            _state.update { it.copy(enabled = enabled) }
            if (enabled && foreground) start()
        }
    }

    /** The app went to the background: stop listening (and save battery). */
    fun onBackground() {
        foreground = false
        scope.launch { stop() }
    }

    suspend fun setEnabled(enabled: Boolean) {
        prefs.setLanEnabled(enabled)
        _state.update { it.copy(enabled = enabled) }
        if (enabled && foreground) start() else stop()
    }

    private suspend fun start() = mutex.withLock {
        if (lan != null) return@withLock
        acquireMulticast()
        try {
            val opened = opener(deviceName(), events)
            lan = opened
            _state.update {
                it.copy(
                    running = true,
                    name = opened.identity.name,
                    addresses = opened.addresses(),
                    error = null,
                    discoveryError = opened.discoveryError,
                )
            }
            refresh()
        } catch (e: CancellationException) {
            releaseMulticast()
            throw e
        } catch (e: Exception) {
            releaseMulticast()
            _state.update { it.copy(running = false, error = e.message ?: e.toString()) }
        }
    }

    private suspend fun stop() = mutex.withLock {
        val current = lan ?: return@withLock
        lan = null
        try {
            current.shutdown()
        } catch (e: CancellationException) {
            throw e
        } catch (_: Exception) {
            // Stopping is best effort.
        }
        releaseMulticast()
        _state.update { it.copy(running = false, devices = emptyList(), pairing = null, addresses = emptyList()) }
    }

    /** Re-reads the device list and the PIN. */
    fun refresh() {
        val current = lan ?: return
        _state.update { it.copy(devices = current.devices(), pairing = current.pairing()) }
    }

    private fun require(): LanApi =
        lan ?: throw HashlarkFailure("not_running", "Turn on Settings → Devices on this network first.")

    suspend fun addAddress(address: String): LanDevice = require().addAddress(address).also { refresh() }

    suspend fun pair(deviceId: String, pin: String): LanDevice = require().pair(deviceId, pin).also { refresh() }

    suspend fun send(deviceId: String, url: String, title: String) = require().sendMagnet(deviceId, url, title)

    suspend fun forget(deviceId: String) {
        require().forget(deviceId)
        refresh()
    }

    fun openPairing(): PairingCode = require().openPairing().also { code -> _state.update { it.copy(pairing = code) } }

    fun closePairing() {
        lan?.closePairing()
        _state.update { it.copy(pairing = null) }
    }

    /** The name the user gave the phone ("Ashish's Pixel"), else the model. */
    private fun deviceName(): String =
        Settings.Global.getString(context.contentResolver, Settings.Global.DEVICE_NAME)
            ?.takeIf { it.isNotBlank() }
            ?: Build.MODEL

    private fun acquireMulticast() {
        if (multicastLock?.isHeld == true) return
        val wifi = context.applicationContext.getSystemService(WifiManager::class.java) ?: return
        multicastLock = wifi.createMulticastLock("hashlark-lan").apply {
            setReferenceCounted(false)
            acquire()
        }
    }

    private fun releaseMulticast() {
        multicastLock?.takeIf { it.isHeld }?.release()
        multicastLock = null
    }
}
