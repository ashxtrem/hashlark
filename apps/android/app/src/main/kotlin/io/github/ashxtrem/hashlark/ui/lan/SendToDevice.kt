// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark.ui.lan

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Computer
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material.icons.filled.Smartphone
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.ListItem
import androidx.compose.material3.ListItemDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.github.ashxtrem.hashlark.core.HashlarkFailure
import io.github.ashxtrem.hashlark.core.LanDevice
import io.github.ashxtrem.hashlark.core.LanPlatform
import io.github.ashxtrem.hashlark.system.LanController
import kotlinx.coroutines.launch

/** A magnet waiting for the user to pick where to send it. */
data class SendTarget(val title: String, val url: String)

/** Paired devices (those on the network first), then devices that could be paired. */
internal fun pickerSections(devices: List<LanDevice>): Pair<List<LanDevice>, List<LanDevice>> {
    val byName = compareBy<LanDevice> { it.name.lowercase() }
    val paired = devices.filter { it.paired }.sortedWith(compareBy<LanDevice> { !it.online }.then(byName))
    val others = devices.filter { !it.paired }.sortedWith(byName)
    return paired to others
}

internal fun isPin(text: String): Boolean = text.length == 6 && text.all(Char::isDigit)

/** "123 456": easier to read out and type. */
internal fun formatPin(pin: String): String = if (pin.length == 6) "${pin.take(3)} ${pin.drop(3)}" else pin

/** "1:05" for 65 000 ms; never negative. */
internal fun formatCountdown(ms: Long): String {
    val total = ((ms.coerceAtLeast(0) + 999) / 1000)
    return "${total / 60}:${(total % 60).toString().padStart(2, '0')}"
}

/** Where the other device shows its PIN. */
internal fun pinHint(device: LanDevice): String = when (device.platform) {
    LanPlatform.Desktop -> "On ${device.name}, open Hashlark → Settings → Devices on this network → Pair a phone, then enter the PIN it shows."
    LanPlatform.Android -> "On ${device.name}, open Hashlark → Settings → Devices on this network → Show pairing PIN, then enter the PIN it shows."
}

/** What to tell the user when talking to [deviceName] failed. */
internal fun lanErrorMessage(e: HashlarkFailure, deviceName: String): String = when (e.code) {
    "unreachable" -> "Couldn't reach $deviceName. Check that it's on the same Wi-Fi and Hashlark is open there."
    "rejected" -> "$deviceName no longer recognises this phone. Pair again."
    "pairing_closed" -> "$deviceName isn't ready to pair yet, or its PIN expired. Show a new PIN there and try again."
    "wrong_pin" -> "That PIN isn't right. Check the PIN on $deviceName."
    "rate_limited" -> "Too many attempts. Wait a minute and try again."
    else -> e.message
}

private fun icon(device: LanDevice): ImageVector =
    if (device.platform == LanPlatform.Android) Icons.Filled.Smartphone else Icons.Filled.Computer

/**
 * "Send to…": picks a computer on the same network and sends it [target],
 * pairing first (with the PIN the computer shows) when needed.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SendToDeviceSheet(target: SendTarget, lan: LanController, say: (String) -> Unit, onDismiss: () -> Unit) {
    val state by lan.state.collectAsStateWithLifecycle()
    val scope = rememberCoroutineScope()
    var busy by remember { mutableStateOf<String?>(null) }
    var pinFor by remember { mutableStateOf<LanDevice?>(null) }
    var pinError by remember { mutableStateOf<String?>(null) }
    var askAddress by remember { mutableStateOf(false) }
    var addressError by remember { mutableStateOf<String?>(null) }

    fun send(device: LanDevice) {
        if (busy != null) return
        busy = device.id
        scope.launch {
            try {
                lan.send(device.id, target.url, target.title)
                say("Sent to ${device.name}")
                onDismiss()
            } catch (e: HashlarkFailure) {
                say(lanErrorMessage(e, device.name))
                if (e.code == "rejected") {
                    pinError = null
                    pinFor = device
                }
            } finally {
                busy = null
            }
        }
    }

    fun pairThenSend(device: LanDevice, pin: String) {
        if (busy != null) return
        busy = device.id
        pinError = null
        scope.launch {
            val paired = try {
                lan.pair(device.id, pin)
            } catch (e: HashlarkFailure) {
                pinError = lanErrorMessage(e, device.name)
                null
            } finally {
                busy = null
            }
            if (paired != null) {
                pinFor = null
                send(paired)
            }
        }
    }

    fun addAddress(text: String) {
        addressError = null
        scope.launch {
            try {
                val device = lan.addAddress(text)
                askAddress = false
                if (device.paired) {
                    send(device)
                } else {
                    pinError = null
                    pinFor = device
                }
            } catch (e: HashlarkFailure) {
                addressError = lanErrorMessage(e, "that address")
            }
        }
    }

    ModalBottomSheet(onDismissRequest = onDismiss) {
        Column(Modifier.padding(horizontal = 16.dp).padding(bottom = 24.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Text("Send to…", style = MaterialTheme.typography.titleLarge)
            Text(
                target.title,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
            )
            Spacer(Modifier.height(8.dp))
            when {
                !state.enabled -> {
                    Text("Finding computers on this network is turned off.")
                    TextButton(onClick = { scope.launch { lan.setEnabled(true) } }) { Text("Turn on") }
                }
                state.error != null -> Text("Couldn't start: ${state.error}", color = MaterialTheme.colorScheme.error)
                !state.running -> Waiting("Starting…")
                else -> {
                    val (paired, others) = pickerSections(state.devices)
                    paired.forEach { device ->
                        DeviceItem(
                            device,
                            supporting = if (device.online) "On this network" else "Not seen right now; will try where it was last",
                            busy = busy == device.id,
                            onClick = { send(device) },
                        )
                    }
                    if (others.isNotEmpty()) {
                        Text(
                            "Pair with",
                            style = MaterialTheme.typography.titleSmall,
                            color = MaterialTheme.colorScheme.primary,
                            modifier = Modifier.padding(top = 8.dp),
                        )
                        others.forEach { device ->
                            DeviceItem(device, supporting = "Tap to pair", busy = busy == device.id, onClick = {
                                pinError = null
                                pinFor = device
                            })
                        }
                    }
                    if (paired.isEmpty() && others.isEmpty()) {
                        Waiting("Looking for computers on this Wi-Fi…")
                        Text(
                            "On your computer, open Hashlark and turn on Settings → Devices on this network.",
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                    if (state.discoveryError != null) {
                        Text(
                            "This phone can't look for computers automatically here, so enter the computer's address.",
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                    ListItem(
                        headlineContent = { Text("Enter address…") },
                        supportingContent = { Text("If your computer isn't listed") },
                        leadingContent = { Icon(Icons.Filled.Edit, contentDescription = null) },
                        colors = ListItemDefaults.colors(containerColor = MaterialTheme.colorScheme.surfaceContainerLow),
                        modifier = Modifier.clickable {
                            addressError = null
                            askAddress = true
                        }.testTag("lan-enter-address"),
                    )
                }
            }
        }
    }

    pinFor?.let { device ->
        PinDialog(device, error = pinError, busy = busy == device.id, onPair = { pin -> pairThenSend(device, pin) }, onDismiss = { pinFor = null })
    }
    if (askAddress) {
        AddressDialog(error = addressError, onAdd = ::addAddress, onDismiss = { askAddress = false })
    }
}

@Composable
private fun Waiting(text: String) {
    Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.padding(vertical = 8.dp)) {
        CircularProgressIndicator(Modifier.size(18.dp), strokeWidth = 2.dp)
        Text(text, Modifier.padding(start = 12.dp))
    }
}

@Composable
private fun DeviceItem(device: LanDevice, supporting: String, busy: Boolean, onClick: () -> Unit) {
    ListItem(
        headlineContent = { Text(device.name) },
        supportingContent = { Text(supporting) },
        leadingContent = { Icon(icon(device), contentDescription = null) },
        trailingContent = { if (busy) CircularProgressIndicator(Modifier.size(20.dp), strokeWidth = 2.dp) },
        colors = ListItemDefaults.colors(containerColor = MaterialTheme.colorScheme.surfaceContainerLow),
        modifier = Modifier.clickable(enabled = !busy, onClick = onClick).testTag("lan-device"),
    )
}

@Composable
private fun PinDialog(device: LanDevice, error: String?, busy: Boolean, onPair: (String) -> Unit, onDismiss: () -> Unit) {
    var pin by remember(device.id) { mutableStateOf("") }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Pair with ${device.name}") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Text(pinHint(device))
                OutlinedTextField(
                    value = pin,
                    onValueChange = { pin = it.filter(Char::isDigit).take(6) },
                    label = { Text("PIN") },
                    singleLine = true,
                    isError = error != null,
                    supportingText = error?.let { { Text(it) } },
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.NumberPassword, imeAction = ImeAction.Done),
                    keyboardActions = KeyboardActions(onDone = { if (isPin(pin) && !busy) onPair(pin) }),
                    modifier = Modifier.testTag("lan-pin"),
                )
            }
        },
        confirmButton = {
            Button(onClick = { onPair(pin) }, enabled = isPin(pin) && !busy) { Text(if (busy) "Pairing…" else "Pair and send") }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel") } },
    )
}

@Composable
private fun AddressDialog(error: String?, onAdd: (String) -> Unit, onDismiss: () -> Unit) {
    var address by remember { mutableStateOf("") }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Computer's address") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Text("Hashlark on the computer shows its address under Settings → Devices on this network.")
                OutlinedTextField(
                    value = address,
                    onValueChange = { address = it.trim() },
                    label = { Text("Address") },
                    placeholder = { Text("192.168.1.20:47821") },
                    singleLine = true,
                    isError = error != null,
                    supportingText = error?.let { { Text(it) } },
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri, imeAction = ImeAction.Done),
                    keyboardActions = KeyboardActions(onDone = { if (address.isNotBlank()) onAdd(address) }),
                    modifier = Modifier.testTag("lan-address"),
                )
            }
        },
        confirmButton = { Button(onClick = { onAdd(address) }, enabled = address.isNotBlank()) { Text("Connect") } },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel") } },
    )
}
