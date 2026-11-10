// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark.core

import android.content.Context
import io.github.ashxtrem.hashlark.ffi.HashlarkLan
import io.github.ashxtrem.hashlark.ffi.LanListener
import io.github.ashxtrem.hashlark.ffi.lanLocalAddresses
import io.github.ashxtrem.hashlark.ffi.openLan
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.decodeFromString

// Sending results to Hashlark on a computer on the same network, and receiving
// them (crates/hashlark-lan, docs/lan-sharing-plan.md).

@Serializable
enum class LanPlatform {
    @SerialName("desktop") Desktop,
    @SerialName("android") Android,
}

/** Another device: paired (a key is stored for it) and/or seen on the network. */
@Serializable
data class LanDevice(
    val id: String,
    val name: String,
    val platform: LanPlatform,
    val paired: Boolean,
    val online: Boolean,
    /** `ip:port` it was last seen or reached at. */
    val addr: String? = null,
)

/** The PIN shown while this device accepts a pairing. */
@Serializable
data class PairingCode(
    val pin: String,
    /** Unix milliseconds. */
    @SerialName("expires_at") val expiresAt: Long,
)

/** A link a paired device sent. */
@Serializable
data class LanReceived(val from: LanDevice, val url: String, val title: String)

/** This device as others see it. */
@Serializable
data class LanIdentity(val id: String, val name: String, val platform: LanPlatform, val port: Int)

/** What the LAN service reports. Called on a background thread, in order. */
interface LanEvents {
    fun received(item: LanReceived)
    fun devicesChanged()
    fun paired(device: LanDevice)
    fun pairingChanged(code: PairingCode?)
}

/**
 * LAN sharing while the app is open. Failures are [HashlarkFailure]s whose
 * [HashlarkFailure.code] is one of `wrong_pin`, `pairing_closed`, `not_paired`,
 * `rejected`, `unreachable`, `rate_limited`, `invalid`, `protocol`, `storage`.
 */
interface LanApi {
    val identity: LanIdentity

    /** Why automatic discovery isn't running, if it failed to start. */
    val discoveryError: String?

    /** `ip:port` to type on another device when it can't find this one. */
    fun addresses(): List<String>

    fun devices(): List<LanDevice>
    suspend fun addAddress(address: String): LanDevice
    fun openPairing(): PairingCode
    fun closePairing()
    fun pairing(): PairingCode?
    suspend fun pair(deviceId: String, pin: String): LanDevice
    suspend fun forget(deviceId: String)
    suspend fun sendMagnet(deviceId: String, url: String, title: String)
    suspend fun shutdown()

    companion object {
        /** Starts LAN sharing as [name]; pairings go in the same Keystore-backed store as the engine's secrets. */
        suspend fun open(context: Context, name: String, events: LanEvents): LanApi = call {
            FfiLan(openLan(name, KeystoreSecretStore(context.applicationContext), Listener(events)))
        }
    }
}

private class Listener(private val events: LanEvents) : LanListener {
    private val json = HashlarkJson

    override fun onReceived(receivedJson: String) = events.received(json.decodeFromString(receivedJson))

    override fun onDevicesChanged() = events.devicesChanged()

    override fun onPaired(deviceJson: String) = events.paired(json.decodeFromString(deviceJson))

    override fun onPairingChanged(codeJson: String?) =
        events.pairingChanged(codeJson?.let { json.decodeFromString<PairingCode>(it) })
}

/** [LanApi] over the UniFFI object. */
private class FfiLan(private val lan: HashlarkLan) : LanApi {
    private val json = HashlarkJson

    override val identity: LanIdentity = json.decodeFromString(lan.identity())

    override val discoveryError: String? = lan.discoveryError()

    override fun addresses(): List<String> = lanLocalAddresses().map { "$it:${identity.port}" }

    override fun devices(): List<LanDevice> = json.decodeFromString(lan.devices())

    override suspend fun addAddress(address: String): LanDevice =
        json.decodeFromString(call { lan.addAddress(address) })

    override fun openPairing(): PairingCode = json.decodeFromString(lan.openPairing())

    override fun closePairing() = lan.closePairing()

    override fun pairing(): PairingCode? = lan.pairing()?.let { json.decodeFromString<PairingCode>(it) }

    override suspend fun pair(deviceId: String, pin: String): LanDevice =
        json.decodeFromString(call { lan.pair(deviceId, pin) })

    override suspend fun forget(deviceId: String) = call { lan.forget(deviceId) }

    override suspend fun sendMagnet(deviceId: String, url: String, title: String) =
        call { lan.sendMagnet(deviceId, url, title) }

    override suspend fun shutdown() = call { lan.shutdown() }
}
