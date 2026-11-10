// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark.ui.settings

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.FilterChip
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import io.github.ashxtrem.hashlark.core.KeepAwake
import io.github.ashxtrem.hashlark.core.OnBattery
import io.github.ashxtrem.hashlark.core.Settings
import io.github.ashxtrem.hashlark.core.SpeedLimits
import io.github.ashxtrem.hashlark.core.TorrentSettings
import io.github.ashxtrem.hashlark.ui.common.SectionTitle
import io.github.ashxtrem.hashlark.ui.common.SwitchRow

/** Ranges the core's `TorrentSettings::validate` accepts; fields outside them aren't saved. */
object TorrentLimits {
    val PEERS_GLOBAL = 20..10_000
    val PEERS_TORRENT = 10..2_000
    val MAX_ACTIVE = 1..100
    const val MAX_RATIO = 100f

    /** A SOCKS5 proxy for torrents, or none. */
    fun proxyValid(proxy: String?): Boolean {
        if (proxy == null) return true
        val p = proxy.trim().lowercase()
        val rest = p.removePrefix("socks5h://").removePrefix("socks5://")
        return rest != p && rest.isNotBlank()
    }

    /** The global connection limit can't be below the per-torrent one. */
    fun peersValid(t: TorrentSettings): Boolean = t.peerLimitGlobal >= t.peerLimitTorrent
}

/** The "Torrent client" settings section: on/off, folder, speed, schedule, connections, queue, seeding, power, proxy. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
internal fun TorrentSection(settings: Settings, vm: SettingsViewModel) {
    val t = settings.torrents
    fun edit(transform: (TorrentSettings) -> TorrentSettings) = vm.edit { it.copy(torrents = transform(it.torrents)) }

    SwitchRow(
        "Use the built-in client",
        t.enabled,
        { v -> edit { it.copy(enabled = v) } },
        description = "Download results inside Hashlark. When it's off, results open in your torrent app.",
    )
    SwitchRow(
        "Ask before each download",
        t.askOnAdd,
        { v -> edit { it.copy(askOnAdd = v) } },
        description = "Shows the folder and \"Start paused\" first. A long-press on Download does this for one download.",
    )
    Text("Download folder", style = MaterialTheme.typography.labelLarge)
    Text(t.downloadDir ?: "Download/Hashlark (default)", color = MaterialTheme.colorScheme.onSurfaceVariant)
    if (t.downloadDir != null) {
        OutlinedButton(onClick = { edit { it.copy(downloadDir = null) } }) { Text("Use the default folder") }
    }
    SwitchRow(
        "Download on mobile data",
        !t.unmeteredOnly,
        { v -> edit { it.copy(unmeteredOnly = !v) } },
        description = "Off: downloads wait for Wi-Fi or another unmetered network.",
    )

    SectionTitle("Speed")
    LimitFields("Download limit (KiB/s)", "Upload limit (KiB/s)", t.limits) { l -> edit { it.copy(limits = l) } }
    Text("Leave empty for no limit.", style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
    LimitFields("Alternative download (KiB/s)", "Alternative upload (KiB/s)", t.altLimits) { l -> edit { it.copy(altLimits = l) } }
    SwitchRow(
        "Use alternative speed now",
        t.turtle,
        { v -> edit { it.copy(turtle = v) } },
        description = "Also on the Downloads screen. The schedule and battery can switch to it on their own.",
    )

    SectionTitle("Schedule")
    SwitchRow(
        "Weekly schedule",
        t.schedule != null,
        { v -> edit { it.copy(schedule = if (v) ScheduleGrid.ALL_NORMAL else null) } },
        description = "Pick, hour by hour, when to use normal speed, alternative speed, seed only, or nothing.",
    )
    if (t.schedule != null) {
        ScheduleGridEditor(t.schedule) { s -> edit { it.copy(schedule = s) } }
    }

    SectionTitle("Connections")
    NumberField("Connections in total", t.peerLimitGlobal.toLong(), TorrentLimits.PEERS_GLOBAL) { v -> edit { it.copy(peerLimitGlobal = v.toInt()) } }
    NumberField("Connections per torrent", t.peerLimitTorrent.toLong(), TorrentLimits.PEERS_TORRENT) { v -> edit { it.copy(peerLimitTorrent = v.toInt()) } }
    if (!TorrentLimits.peersValid(t)) {
        Text("The total can't be lower than the limit per torrent.", color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall)
    }

    SectionTitle("Queue")
    NumberField("Downloads at once", t.maxActiveDownloads.toLong(), TorrentLimits.MAX_ACTIVE) { v -> edit { it.copy(maxActiveDownloads = v.toInt()) } }
    NumberField("Torrents seeding at once", t.maxActiveSeeds.toLong(), TorrentLimits.MAX_ACTIVE) { v -> edit { it.copy(maxActiveSeeds = v.toInt()) } }

    SectionTitle("Seeding")
    RatioField(t.seedRatio) { r -> edit { it.copy(seedRatio = r) } }
    OptionalNumber("Stop seeding after (minutes)", t.seedMinutes?.toLong(), 1..10_000_000) { v -> edit { it.copy(seedMinutes = v?.toInt()) } }
    Text(
        "0 stops seeding as soon as a download finishes. Empty seeds without a limit.",
        style = MaterialTheme.typography.bodySmall,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )

    SectionTitle("Power")
    Text("Keep the phone awake", style = MaterialTheme.typography.labelLarge)
    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        listOf(KeepAwake.Off to "Off", KeepAwake.Downloading to "While downloading", KeepAwake.Active to "While downloading or seeding")
            .forEach { (value, label) ->
                FilterChip(selected = t.keepAwake == value, onClick = { edit { it.copy(keepAwake = value) } }, label = { Text(label) })
            }
    }
    Text("On battery", style = MaterialTheme.typography.labelLarge)
    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        listOf(OnBattery.Nothing to "Do nothing", OnBattery.AltSpeed to "Alternative speed", OnBattery.Pause to "Pause all")
            .forEach { (value, label) ->
                FilterChip(selected = t.onBattery == value, onClick = { edit { it.copy(onBattery = value) } }, label = { Text(label) })
            }
    }
    Text(
        "\"On battery\" means battery saver is on, or the battery is below 15%, while not charging.",
        style = MaterialTheme.typography.bodySmall,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )

    SectionTitle("Proxy")
    val proxyOk = TorrentLimits.proxyValid(t.proxy)
    OutlinedTextField(
        value = t.proxy.orEmpty(),
        onValueChange = { v -> edit { it.copy(proxy = v.ifBlank { null }) } },
        label = { Text("SOCKS5 proxy for downloads") },
        placeholder = { Text("socks5://127.0.0.1:1080") },
        isError = !proxyOk,
        supportingText = { if (!proxyOk) Text("Use socks5:// or socks5h:// followed by the address.") },
        singleLine = true,
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri),
        modifier = Modifier.fillMaxWidth(),
    )
    Text(
        "Downloads never use Tor or the search proxy. Even with this proxy, DHT and uTP (UDP) still connect directly.",
        style = MaterialTheme.typography.bodySmall,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )
}

/** Download and upload limits in KiB/s; empty is unlimited. */
@Composable
private fun LimitFields(downLabel: String, upLabel: String, limits: SpeedLimits, onChange: (SpeedLimits) -> Unit) {
    OptionalNumber(downLabel, limits.downKib, 1..10_000_000) { onChange(limits.copy(downKib = it)) }
    OptionalNumber(upLabel, limits.upKib, 1..10_000_000) { onChange(limits.copy(upKib = it)) }
}

/** A whole number that may be empty (no value); reports only empty or values inside [range]. */
@Composable
private fun OptionalNumber(label: String, value: Long?, range: IntRange, onValid: (Long?) -> Unit) {
    var text by remember(value) { mutableStateOf(value?.toString().orEmpty()) }
    val parsed = text.toLongOrNull()
    val valid = text.isEmpty() || (parsed != null && parsed in range.first.toLong()..range.last.toLong())
    OutlinedTextField(
        value = text,
        onValueChange = {
            text = it.filter(Char::isDigit).take(8)
            val v = text.toLongOrNull()
            when {
                text.isEmpty() -> onValid(null)
                v != null && v in range.first.toLong()..range.last.toLong() -> onValid(v)
            }
        },
        label = { Text(label) },
        supportingText = { if (!valid) Text("Enter a number from ${range.first} to ${range.last}, or leave it empty") },
        isError = !valid,
        singleLine = true,
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
        modifier = Modifier.fillMaxWidth(),
    )
}

/** The share ratio to stop seeding at, 0 to 100; empty seeds forever. */
@Composable
private fun RatioField(value: Float?, onValid: (Float?) -> Unit) {
    var text by remember(value) { mutableStateOf(value?.let { if (it == it.toInt().toFloat()) it.toInt().toString() else it.toString() }.orEmpty()) }
    val parsed = text.replace(',', '.').toFloatOrNull()
    val valid = text.isEmpty() || (parsed != null && parsed in 0f..TorrentLimits.MAX_RATIO)
    OutlinedTextField(
        value = text,
        onValueChange = {
            text = it.filter { c -> c.isDigit() || c == '.' || c == ',' }.take(6)
            val v = text.replace(',', '.').toFloatOrNull()
            when {
                text.isEmpty() -> onValid(null)
                v != null && v in 0f..TorrentLimits.MAX_RATIO -> onValid(v)
            }
        },
        label = { Text("Stop seeding at share ratio") },
        supportingText = { if (!valid) Text("Enter a ratio from 0 to 100, or leave it empty") },
        isError = !valid,
        singleLine = true,
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Decimal),
        modifier = Modifier.fillMaxWidth(),
    )
}
