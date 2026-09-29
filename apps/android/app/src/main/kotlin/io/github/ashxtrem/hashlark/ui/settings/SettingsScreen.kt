// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark.ui.settings

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Info
import androidx.compose.material.icons.filled.Lock
import androidx.compose.material.icons.filled.Palette
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.filled.Tag
import androidx.compose.material3.Button
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.FilterChip
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.github.ashxtrem.hashlark.BuildConfig
import io.github.ashxtrem.hashlark.core.DohResolver
import io.github.ashxtrem.hashlark.core.Settings
import io.github.ashxtrem.hashlark.core.Theme
import io.github.ashxtrem.hashlark.core.TorMode
import io.github.ashxtrem.hashlark.core.TorStatus
import io.github.ashxtrem.hashlark.ffi.coreVersion
import io.github.ashxtrem.hashlark.hashlark
import io.github.ashxtrem.hashlark.system.MagnetLauncher
import io.github.ashxtrem.hashlark.ui.AppViewModel
import io.github.ashxtrem.hashlark.ui.common.AdaptiveListDetail
import io.github.ashxtrem.hashlark.ui.common.LoadingBox
import io.github.ashxtrem.hashlark.ui.common.ScreenTitle
import io.github.ashxtrem.hashlark.ui.common.SectionTitle
import io.github.ashxtrem.hashlark.ui.common.SwitchRow
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

private enum class Section(val title: String, val summary: String, val icon: ImageVector) {
    Search("Search", "Timeouts, concurrency, history", Icons.Filled.Search),
    Network("Network and privacy", "Encrypted DNS, proxy, Tor", Icons.Filled.Lock),
    Magnets("Magnets and downloads", "Torrent client, trackers, .torrent files", Icons.Filled.Tag),
    Appearance("Appearance and updates", "Theme, colours, update check", Icons.Filled.Palette),
    About("About", "Version, licence, privacy", Icons.Filled.Info),
}

/** Settings: a list of sections with the selected section beside it (or replacing it, on a narrow window). */
@Composable
fun SettingsScreen(app: AppViewModel, vm: SettingsViewModel, modifier: Modifier = Modifier) {
    val draft by vm.draft.collectAsStateWithLifecycle()
    val dirty by vm.dirty.collectAsStateWithLifecycle()
    val saving by vm.saving.collectAsStateWithLifecycle()
    var selected by rememberSaveable { mutableStateOf<String?>(null) }

    Column(modifier.fillMaxSize()) {
        ScreenTitle("Settings")
        val current = draft
        if (current == null) {
            LoadingBox()
            return@Column
        }
        Box(Modifier.weight(1f)) {
            AdaptiveListDetail(
                selected = selected,
                onDeselect = { selected = null },
                list = {
                    Column {
                        Section.entries.forEach { section ->
                            Surface(color = if (selected == section.name) MaterialTheme.colorScheme.primaryContainer else MaterialTheme.colorScheme.surface) {
                                ListItem(
                                    leadingContent = { Icon(section.icon, contentDescription = null) },
                                    headlineContent = { Text(section.title) },
                                    supportingContent = { Text(section.summary) },
                                    modifier = Modifier.clickable { selected = section.name },
                                )
                            }
                            HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
                        }
                    }
                },
                detail = { key, onBack ->
                    val section = Section.entries.firstOrNull { it.name == key } ?: Section.Search
                    SectionScreen(section, current, vm, app, onBack)
                },
                placeholder = "Select a section",
            )
        }
        if (dirty) {
            Surface(tonalElevation = 3.dp, shadowElevation = 3.dp) {
                Row(
                    Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    Text("You have unsaved changes.", Modifier.weight(1f), color = MaterialTheme.colorScheme.onSurfaceVariant)
                    TextButton(onClick = vm::discard, enabled = !saving) { Text("Discard") }
                    Button(onClick = vm::save, enabled = !saving) { Text("Save changes") }
                }
            }
        }
    }
}

@Composable
private fun SectionScreen(section: Section, settings: Settings, vm: SettingsViewModel, app: AppViewModel, onBack: (() -> Unit)?) {
    Surface(Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.surface) {
        Column(Modifier.verticalScroll(rememberScrollState()).padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                if (onBack != null) {
                    IconButton(onClick = onBack) { Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back to settings") }
                }
                Text(section.title, style = MaterialTheme.typography.titleLarge)
            }
            when (section) {
                Section.Search -> SearchSection(settings, vm)
                Section.Network -> NetworkSection(settings, vm, app)
                Section.Magnets -> MagnetsSection(settings, vm, app)
                Section.Appearance -> AppearanceSection(settings, vm)
                Section.About -> AboutSection()
            }
        }
    }
}

@Composable
private fun SearchSection(settings: Settings, vm: SettingsViewModel) {
    val s = settings.search
    NumberField("Provider timeout (seconds)", s.providerTimeoutSecs, 1..120) { v -> vm.edit { it.copy(search = it.search.copy(providerTimeoutSecs = v)) } }
    NumberField("Providers searched at once", s.maxConcurrency.toLong(), 1..64) { v -> vm.edit { it.copy(search = it.search.copy(maxConcurrency = v.toInt())) } }
    NumberField("Cache identical searches (seconds; 0 turns it off)", s.cacheTtlSecs, 0..86_400) { v -> vm.edit { it.copy(search = it.search.copy(cacheTtlSecs = v)) } }
    SwitchRow(
        "Save search history",
        s.saveHistory,
        { v -> vm.edit { it.copy(search = it.search.copy(saveHistory = v)) } },
        description = "Kept only on this device.",
    )
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun NetworkSection(settings: Settings, vm: SettingsViewModel, app: AppViewModel) {
    val net = settings.network
    SwitchRow(
        "Encrypted DNS (DNS-over-HTTPS)",
        net.doh.enabled,
        { v -> vm.edit { it.copy(network = it.network.copy(doh = it.network.doh.copy(enabled = v))) } },
        description = "Looks up provider addresses privately instead of through your network's DNS.",
    )
    if (net.doh.enabled) {
        Text("DNS provider", style = MaterialTheme.typography.labelLarge)
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            DohResolver.entries.forEach { resolver ->
                FilterChip(
                    selected = net.doh.resolver == resolver,
                    onClick = { vm.edit { it.copy(network = it.network.copy(doh = it.network.doh.copy(resolver = resolver))) } },
                    label = { Text(resolver.label) },
                )
            }
        }
        if (net.doh.resolver == DohResolver.Custom) {
            TextValue("DoH address", net.doh.customUrl, "https://dns.example/dns-query") { v ->
                vm.edit { it.copy(network = it.network.copy(doh = it.network.doh.copy(customUrl = v))) }
            }
        }
        SwitchRow(
            "Fall back to system DNS if encrypted DNS fails",
            net.doh.fallbackToSystem,
            { v -> vm.edit { it.copy(network = it.network.copy(doh = it.network.doh.copy(fallbackToSystem = v))) } },
        )
    }

    SectionTitle("Proxy")
    TextValue("Proxy address", net.proxy, "socks5h://127.0.0.1:9050 or http://proxy:8080") { v ->
        vm.edit { it.copy(network = it.network.copy(proxy = v)) }
    }
    Text(
        "Used for every provider unless a provider has its own network setting. Leave empty for none.",
        style = MaterialTheme.typography.bodySmall,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )

    SectionTitle("Tor")
    SwitchRow(
        "Route everything through Tor",
        net.tor.enabled,
        { v -> vm.edit { it.copy(network = it.network.copy(tor = it.network.tor.copy(enabled = v))) } },
        description = "Slower, but reaches sites blocked on your network and .onion mirrors. Providers can also use Tor one by one (Providers, then Network).",
    )
    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        FilterChip(
            selected = net.tor.mode == TorMode.Embedded,
            onClick = { vm.edit { it.copy(network = it.network.copy(tor = it.network.tor.copy(mode = TorMode.Embedded))) } },
            label = { Text("Built into Hashlark") },
        )
        FilterChip(
            selected = net.tor.mode == TorMode.External,
            onClick = { vm.edit { it.copy(network = it.network.copy(tor = it.network.tor.copy(mode = TorMode.External))) } },
            label = { Text("Orbot or my own Tor") },
        )
    }
    if (net.tor.mode == TorMode.External) {
        TextValue("Tor SOCKS address", net.tor.socksUrl, "socks5h://127.0.0.1:9050") { v ->
            vm.edit { it.copy(network = it.network.copy(tor = it.network.tor.copy(socksUrl = v))) }
        }
        Text(
            "Orbot listens on 127.0.0.1:9050 (start Orbot and turn on its VPN or proxy first). Tor Browser uses port 9150.",
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    } else {
        TorStatusText(app)
        Text(
            "Built-in Tor starts only when Tor is turned on or a provider uses it, and never keeps running in the background. " +
                "If Tor cannot connect on your network, use Orbot, which supports bridges.",
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }

    NumberField("Requests per second to one site", net.perHostRate.toLong(), 1..20) { v ->
        vm.edit { it.copy(network = it.network.copy(perHostRate = v.toInt())) }
    }
}

/** Shows how far built-in Tor is while this section is open. */
@Composable
private fun TorStatusText(app: AppViewModel) {
    var status by remember { mutableStateOf<TorStatus?>(null) }
    LaunchedEffect(Unit) {
        while (true) {
            status = app.torStatus()
            delay(if (status?.running == true && status?.ready != true) 2_000 else 10_000)
        }
    }
    val s = status ?: return
    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Text(
            when {
                !s.builtIn -> "This build has no built-in Tor."
                !s.running -> "Not running yet. It starts when Tor is turned on or a provider uses it."
                s.ready -> "Connected to Tor."
                else -> "Connecting to Tor… ${(s.progress * 100).toInt()}%"
            },
            color = if (s.ready) io.github.ashxtrem.hashlark.ui.theme.LocalStatusColors.current.ok else MaterialTheme.colorScheme.onSurfaceVariant,
        )
        if (s.running && !s.ready) LinearProgressIndicator(progress = { s.progress }, modifier = Modifier.fillMaxWidth())
    }
}

@Composable
private fun MagnetsSection(settings: Settings, vm: SettingsViewModel, app: AppViewModel) {
    val context = LocalContext.current
    val prefs = context.hashlark.prefs
    val scope = rememberCoroutineScope()
    val chosen by prefs.magnetPackage.collectAsStateWithLifecycle(initialValue = null)
    val handlers = remember { MagnetLauncher.handlers(context) }
    var menu by remember { mutableStateOf(false) }

    Text("Torrent client", style = MaterialTheme.typography.labelLarge)
    if (handlers.isEmpty()) {
        Text(
            "No app that opens magnet links is installed. Install a torrent client; until then, magnets can be copied or shared.",
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    } else {
        Box {
            OutlinedButton(onClick = { menu = true }, Modifier.fillMaxWidth()) {
                Text(handlers.firstOrNull { it.packageName == chosen }?.label ?: "Ask each time (Android decides)", Modifier.weight(1f))
            }
            DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
                DropdownMenuItem(text = { Text("Ask each time (Android decides)") }, onClick = { menu = false; scope.launch { prefs.setMagnetPackage(null) } })
                handlers.forEach { h ->
                    DropdownMenuItem(text = { Text(h.label) }, onClick = { menu = false; scope.launch { prefs.setMagnetPackage(h.packageName) } })
                }
            }
        }
    }
    SwitchRow(
        "Add public trackers to every magnet",
        settings.magnets.appendDefaultTrackers,
        { v -> vm.edit { it.copy(magnets = it.magnets.copy(appendDefaultTrackers = v)) } },
        description = "Can help find peers faster. Magnets without trackers always get them.",
    )
    TextValue("Tracker list address", settings.magnets.trackersUrl, "https://example.org/trackers.txt") { v ->
        vm.edit { it.copy(magnets = it.magnets.copy(trackersUrl = v)) }
    }
    Text(
        "Optional plain-text list, one tracker per line, refreshed about once a day on Wi-Fi.",
        style = MaterialTheme.typography.bodySmall,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )
    val savedSettings by app.settings.collectAsStateWithLifecycle()
    if (savedSettings?.magnets?.trackersUrl != null) {
        OutlinedButton(onClick = {
            scope.launch { app.withApi("Could not update the tracker list") { it.refreshTrackers() }?.let { app.say("Tracker list updated") } }
        }) { Text("Refresh tracker list now") }
    }

    SectionTitle(".torrent files")
    Text(
        "Saved into your Downloads folder" + if (android.os.Build.VERSION.SDK_INT >= 29) "." else ": Android 9 and older ask where to save each file.",
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun AppearanceSection(settings: Settings, vm: SettingsViewModel) {
    val context = LocalContext.current
    val prefs = context.hashlark.prefs
    val scope = rememberCoroutineScope()
    val dynamic by prefs.dynamicColor.collectAsStateWithLifecycle(initialValue = true)
    Text("Theme", style = MaterialTheme.typography.labelLarge)
    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Theme.entries.forEach { theme ->
            FilterChip(
                selected = settings.ui.theme == theme,
                onClick = { vm.edit { it.copy(ui = it.ui.copy(theme = theme)) } },
                label = { Text(theme.label) },
            )
        }
    }
    if (android.os.Build.VERSION.SDK_INT >= 31) {
        SwitchRow(
            "Use system colours",
            dynamic,
            { scope.launch { prefs.setDynamicColor(it) } },
            description = "Material You colours from your wallpaper (Android 12 and newer).",
        )
    }
    SwitchRow(
        "Check for updates",
        settings.updates.checkForUpdates,
        { v -> vm.edit { it.copy(updates = it.updates.copy(checkForUpdates = v)) } },
        description = "Asks GitHub for the latest published release. Nothing else is sent, and Hashlark never installs updates itself.",
    )
}

@Composable
private fun AboutSection() {
    Text("Hashlark ${BuildConfig.VERSION_NAME} (core ${coreVersion()})")
    Text("Licensed under GPL-3.0-or-later.", color = MaterialTheme.colorScheme.onSurfaceVariant)
    Text(
        "No telemetry: Hashlark sends no analytics, crash reports or usage data. It only contacts the providers you enable, " +
            "definition repositories you add, your tracker list address, and GitHub for the update check.",
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )
    Text(
        "Hashlark is a neutral search tool and hosts no content. You are responsible for complying with the law and copyright where you live.",
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )
}

/** A whole-number field that only reports values inside [range]. */
@Composable
private fun NumberField(label: String, value: Long, range: IntRange, onValid: (Long) -> Unit) {
    var text by remember(value) { mutableStateOf(value.toString()) }
    val parsed = text.toLongOrNull()
    val valid = parsed != null && parsed in range.first.toLong()..range.last.toLong()
    OutlinedTextField(
        value = text,
        onValueChange = {
            text = it.filter(Char::isDigit).take(6)
            text.toLongOrNull()?.takeIf { v -> v in range.first.toLong()..range.last.toLong() }?.let(onValid)
        },
        label = { Text(label) },
        supportingText = { if (!valid) Text("Enter a number from ${range.first} to ${range.last}") },
        isError = !valid,
        singleLine = true,
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
        modifier = Modifier.fillMaxWidth(),
    )
}

/** An optional text setting: empty means "not set". */
@Composable
private fun TextValue(label: String, value: String?, placeholder: String, onChange: (String?) -> Unit) {
    OutlinedTextField(
        value = value.orEmpty(),
        onValueChange = { onChange(it.ifBlank { null }) },
        label = { Text(label) },
        placeholder = { Text(placeholder) },
        singleLine = true,
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri),
        modifier = Modifier.fillMaxWidth(),
    )
}
