// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark.system

import android.content.Context
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import android.os.Build

/**
 * The Wi-Fi gate of the built-in torrent client. Torrents only transfer on an
 * unmetered network (Wi-Fi, Ethernet) unless the user allows mobile data
 * (`torrents.unmetered_only` off). The core pauses everything with
 * "Waiting for Wi-Fi" while [allowed] is false.
 *
 * [onChange] gets the new answer whenever it changes, and once at the start —
 * after the first [setUnmeteredOnly], so it never acts on a guessed setting.
 */
class NetworkGate(
    private val context: Context,
    private val onChange: (allowed: Boolean) -> Unit,
) {
    @Volatile
    private var unmetered: Boolean = false

    /** Null until the service has read the user's setting; nothing is published before that. */
    @Volatile
    private var unmeteredOnly: Boolean? = null

    private var last: Boolean? = null
    private var callback: ConnectivityManager.NetworkCallback? = null

    fun start() {
        if (callback != null) return
        val cm = context.getSystemService(ConnectivityManager::class.java) ?: return
        unmetered = cm.getNetworkCapabilities(cm.activeNetwork)?.let(::isUnmetered) ?: false
        val cb = object : ConnectivityManager.NetworkCallback() {
            override fun onCapabilitiesChanged(network: Network, caps: NetworkCapabilities) {
                unmetered = isUnmetered(caps)
                publish()
            }

            override fun onLost(network: Network) {
                unmetered = false
                publish()
            }
        }
        cm.registerDefaultNetworkCallback(cb)
        callback = cb
        publish()
    }

    fun stop() {
        val cb = callback ?: return
        callback = null
        runCatching { context.getSystemService(ConnectivityManager::class.java)?.unregisterNetworkCallback(cb) }
    }

    /** The user's "only on Wi-Fi" setting; re-read by the service on every tick. */
    fun setUnmeteredOnly(value: Boolean) {
        unmeteredOnly = value
        publish()
    }

    @Synchronized
    private fun publish() {
        // Guessing the setting would flash "Waiting for Wi-Fi" for users who allow mobile data.
        val only = unmeteredOnly ?: return
        val now = allowed(unmetered, only)
        if (now != last) {
            last = now
            onChange(now)
        }
    }

    companion object {
        /** Torrents may transfer on this network. */
        fun allowed(unmetered: Boolean, unmeteredOnly: Boolean): Boolean = unmetered || !unmeteredOnly

        fun isUnmetered(caps: NetworkCapabilities): Boolean =
            caps.hasCapability(NetworkCapabilities.NET_CAPABILITY_NOT_METERED) ||
                (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R &&
                    caps.hasCapability(NetworkCapabilities.NET_CAPABILITY_TEMPORARILY_NOT_METERED))
    }
}
