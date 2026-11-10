// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark

import android.app.Application
import android.content.Context
import androidx.test.core.app.ApplicationProvider
import io.github.ashxtrem.hashlark.core.HashlarkFailure
import io.github.ashxtrem.hashlark.core.LanApi
import io.github.ashxtrem.hashlark.core.LanDevice
import io.github.ashxtrem.hashlark.core.LanEvents
import io.github.ashxtrem.hashlark.core.LanIdentity
import io.github.ashxtrem.hashlark.core.LanPlatform
import io.github.ashxtrem.hashlark.core.LanReceived
import io.github.ashxtrem.hashlark.core.PairingCode
import io.github.ashxtrem.hashlark.system.AppPrefs
import io.github.ashxtrem.hashlark.system.LanController
import io.github.ashxtrem.hashlark.ui.lan.formatCountdown
import io.github.ashxtrem.hashlark.ui.lan.formatPin
import io.github.ashxtrem.hashlark.ui.lan.isPin
import io.github.ashxtrem.hashlark.ui.lan.lanErrorMessage
import io.github.ashxtrem.hashlark.ui.lan.pickerSections
import io.github.ashxtrem.hashlark.ui.lan.pinHint
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

private fun device(name: String, paired: Boolean, online: Boolean, platform: LanPlatform = LanPlatform.Desktop) =
    LanDevice(id = name.lowercase(), name = name, platform = platform, paired = paired, online = online, addr = "192.168.1.2:47821")

class LanPickerTest {
    @Test
    fun `paired devices come first, those on the network before the rest`() {
        val (paired, others) = pickerSections(
            listOf(
                device("zeta", paired = false, online = true),
                device("Office", paired = true, online = false),
                device("desk", paired = true, online = true),
                device("Attic", paired = false, online = true),
            ),
        )
        assertEquals(listOf("desk", "Office"), paired.map { it.name })
        assertEquals(listOf("Attic", "zeta"), others.map { it.name })
    }

    @Test
    fun `a PIN is exactly six digits`() {
        assertTrue(isPin("012345"))
        assertFalse(isPin("12345"))
        assertFalse(isPin("1234567"))
        assertFalse(isPin("12a456"))
    }

    @Test
    fun `failures name the device and say what to do`() {
        val unreachable = lanErrorMessage(HashlarkFailure("unreachable", "x"), "Desk")
        assertTrue(unreachable, unreachable.startsWith("Couldn't reach Desk."))
        assertTrue(lanErrorMessage(HashlarkFailure("rejected", "x"), "Desk").contains("Pair again"))
        assertEquals("raw", lanErrorMessage(HashlarkFailure("protocol", "raw"), "Desk"))
    }

    @Test
    fun `PINs and countdowns read well`() {
        assertEquals("123 456", formatPin("123456"))
        assertEquals("2:00", formatCountdown(120_000))
        assertEquals("1:05", formatCountdown(65_000))
        assertEquals("0:01", formatCountdown(200))
        assertEquals("0:00", formatCountdown(-5_000))
    }

    @Test
    fun `the PIN hint points at the right screen on each platform`() {
        assertTrue(pinHint(device("Desk", false, true)).contains("Pair a phone"))
        assertTrue(pinHint(device("Pixel", false, true, LanPlatform.Android)).contains("Show pairing PIN"))
    }
}

/** The LAN service without the network or the native library. */
private class FakeLan : LanApi {
    var devices = listOf(device("Desk", paired = false, online = true))
    var shutDown = false
    val sent = mutableListOf<Triple<String, String, String>>()

    override val identity = LanIdentity("p1", "Test phone", LanPlatform.Android, 47821)
    override val discoveryError: String? = null

    override fun devices() = devices
    override suspend fun addAddress(address: String) = devices.first()
    override fun openPairing() = PairingCode("123456", 0)
    override fun closePairing() {}
    override fun pairing(): PairingCode? = null

    override suspend fun pair(deviceId: String, pin: String): LanDevice {
        if (pin != "123456") throw HashlarkFailure("wrong_pin", "wrong PIN")
        devices = devices.map { it.copy(paired = true) }
        return devices.first()
    }

    override suspend fun forget(deviceId: String) {
        devices = devices.map { it.copy(paired = false) }
    }

    override suspend fun sendMagnet(deviceId: String, url: String, title: String) {
        sent += Triple(deviceId, url, title)
    }

    override suspend fun shutdown() {
        shutDown = true
    }
}

@Config(application = Application::class)
@RunWith(RobolectricTestRunner::class)
class LanControllerTest {
    private val context: Context = ApplicationProvider.getApplicationContext()
    private val fake = FakeLan()
    private var opened = 0
    private var events: LanEvents? = null

    private fun controller() = LanController(context, AppPrefs(context), CoroutineScope(Dispatchers.Unconfined)) { _, ev ->
        opened++
        events = ev
        fake
    }

    private suspend fun LanController.running() = withTimeout(5_000) { state.first { it.running } }

    @Test
    fun `runs only while the app is in the foreground`() = runBlocking {
        val lan = controller()
        lan.setEnabled(true)
        lan.onForeground()
        val state = lan.running()
        assertEquals("Test phone", state.name)
        assertEquals(listOf("Desk"), state.devices.map { it.name })

        lan.onBackground()
        withTimeout(5_000) { lan.state.first { !it.running } }
        assertTrue(fake.shutDown)
        assertTrue(lan.state.value.devices.isEmpty())
    }

    @Test
    fun `turning it off stops it, and it stays off`() = runBlocking {
        val lan = controller()
        lan.setEnabled(true)
        lan.onForeground()
        lan.running()
        lan.setEnabled(false)
        assertFalse(lan.state.value.running)
        assertTrue(fake.shutDown)

        lan.onForeground()
        delay(300)
        assertEquals(1, opened)
        assertFalse(lan.state.value.running)
        lan.setEnabled(true)
    }

    @Test
    fun `pairing, sending and receiving go through the service`() = runBlocking {
        val lan = controller()
        lan.setEnabled(true)
        lan.onForeground()
        lan.running()

        try {
            lan.pair("desk", "000000")
            fail("a wrong PIN must fail")
        } catch (e: HashlarkFailure) {
            assertEquals("wrong_pin", e.code)
        }
        lan.pair("desk", "123456")
        assertTrue(lan.state.value.devices.single().paired)

        lan.send("desk", "magnet:?xt=urn:btih:00", "Title")
        assertEquals(Triple("desk", "magnet:?xt=urn:btih:00", "Title"), fake.sent.single())

        val got = async { withTimeout(5_000) { lan.received.first() } }
        delay(50)
        val item = LanReceived(device("Desk", paired = true, online = true), "magnet:?xt=urn:btih:11", "Back")
        events!!.received(item)
        assertEquals(item, got.await())

        val confirmed = async { withTimeout(5_000) { lan.paired.first() } }
        delay(50)
        events!!.paired(item.from)
        assertEquals("Desk", confirmed.await().name)

        lan.forget("desk")
        assertFalse(lan.state.value.devices.single().paired)
    }

    @Test
    fun `actions fail clearly while sharing is off`() = runBlocking {
        val lan = controller()
        try {
            lan.send("desk", "magnet:?xt=urn:btih:00", "x")
            fail("sending without the service must fail")
        } catch (e: HashlarkFailure) {
            assertEquals("not_running", e.code)
        }
    }
}
