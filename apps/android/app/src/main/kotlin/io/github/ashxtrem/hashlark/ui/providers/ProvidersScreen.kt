// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark.ui.providers

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.filled.OpenInNew
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Storage
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExtendedFloatingActionButton
import androidx.compose.material3.FilterChip
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
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
import androidx.compose.runtime.snapshots.SnapshotStateMap
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.github.ashxtrem.hashlark.core.ErrorKind
import io.github.ashxtrem.hashlark.core.HealthState
import io.github.ashxtrem.hashlark.core.NetworkPolicy
import io.github.ashxtrem.hashlark.core.ProviderKind
import io.github.ashxtrem.hashlark.core.ProviderPatch
import io.github.ashxtrem.hashlark.core.ProviderSource
import io.github.ashxtrem.hashlark.core.ProviderView
import io.github.ashxtrem.hashlark.core.Route
import io.github.ashxtrem.hashlark.core.SettingKind
import io.github.ashxtrem.hashlark.core.SettingView
import io.github.ashxtrem.hashlark.core.Format
import io.github.ashxtrem.hashlark.ui.AppViewModel
import io.github.ashxtrem.hashlark.ui.ResultActions
import io.github.ashxtrem.hashlark.ui.common.AdaptiveListDetail
import io.github.ashxtrem.hashlark.ui.common.CenteredMessage
import io.github.ashxtrem.hashlark.ui.common.ConfirmDialog
import io.github.ashxtrem.hashlark.ui.common.HealthDot
import io.github.ashxtrem.hashlark.ui.common.LoadingBox
import io.github.ashxtrem.hashlark.ui.common.ScreenTitle
import io.github.ashxtrem.hashlark.ui.common.SectionTitle
import io.github.ashxtrem.hashlark.ui.common.SwitchRow
import io.github.ashxtrem.hashlark.ui.theme.LocalStatusColors
import kotlinx.coroutines.launch
import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.time.format.FormatStyle

/** Providers and their repositories: a list, with the selected provider's settings beside it. */
@Composable
fun ProvidersScreen(
    app: AppViewModel,
    providersVm: ProvidersViewModel,
    reposVm: ReposViewModel,
    actions: ResultActions,
    modifier: Modifier = Modifier,
) {
    val providers by app.providers.collectAsStateWithLifecycle()
    var selected by rememberSaveable { mutableStateOf<String?>(null) }
    var showAdd by rememberSaveable { mutableStateOf(false) }
    var showRepos by rememberSaveable { mutableStateOf(false) }
    LaunchedEffect(Unit) { app.refreshProviders() }

    if (showRepos) {
        RepositoriesScreen(reposVm, onBack = { showRepos = false }, modifier = modifier)
        return
    }

    Box(modifier.fillMaxSize()) {
        Column(Modifier.fillMaxSize()) {
            ScreenTitle("Providers") {
                TextButton(onClick = { showRepos = true }) {
                    Icon(Icons.Filled.Storage, contentDescription = null, Modifier.size(18.dp))
                    Text("Repositories", Modifier.padding(start = 8.dp))
                }
            }
            val list = providers
            if (list == null) {
                LoadingBox()
            } else {
                AdaptiveListDetail(
                    selected = selected,
                    onDeselect = { selected = null },
                    list = {
                        LazyColumn(contentPadding = androidx.compose.foundation.layout.PaddingValues(bottom = 96.dp)) {
                            items(list, key = { it.id }) { provider ->
                                ProviderRow(provider, selected == provider.id, onClick = { selected = provider.id }) {
                                    providersVm.setEnabled(provider, it)
                                }
                                HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
                            }
                        }
                    },
                    detail = { key, onBack ->
                        val provider = list.firstOrNull { it.id == key }
                        if (provider == null) {
                            CenteredMessage("This provider was removed")
                        } else {
                            ProviderDetail(provider, providersVm, actions, onBack = onBack, onRemoved = { selected = null })
                        }
                    },
                    placeholder = "Select a provider",
                )
            }
        }
        ExtendedFloatingActionButton(
            onClick = { showAdd = true },
            icon = { Icon(Icons.Filled.Add, contentDescription = null) },
            text = { Text("Add") },
            modifier = Modifier.align(Alignment.BottomEnd).padding(16.dp),
        )
    }
    if (showAdd) AddProviderDialog(providersVm, onDismiss = { showAdd = false }, onAdded = { selected = it.id })
}

@Composable
private fun ProviderRow(provider: ProviderView, selected: Boolean, onClick: () -> Unit, onEnabled: (Boolean) -> Unit) {
    Surface(color = if (selected) MaterialTheme.colorScheme.primaryContainer else MaterialTheme.colorScheme.surface) {
        ListItem(
            leadingContent = { HealthDot(provider.health, provider.enabled) },
            headlineContent = { Text(provider.name) },
            supportingContent = {
                Text(
                    when {
                        provider.error != null -> "Can't load: ${provider.error}"
                        provider.health.state == HealthState.AutoDisabled -> HealthState.AutoDisabled.label
                        else -> listOfNotNull(
                            kindLabel(provider),
                            provider.categories.takeIf { it.isNotEmpty() }?.joinToString(", ") { it.label },
                        ).joinToString(" · ")
                    },
                    maxLines = 1,
                    color = if (provider.error != null) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.onSurfaceVariant,
                )
            },
            trailingContent = { Switch(checked = provider.enabled, onCheckedChange = onEnabled) },
            modifier = Modifier.clickable(onClick = onClick),
        )
    }
}

private fun kindLabel(provider: ProviderView): String = when (provider.kind) {
    ProviderKind.Native -> "Built in"
    ProviderKind.Definition -> when ((provider.source as? ProviderSource.Definition)?.siteType) {
        io.github.ashxtrem.hashlark.core.SiteType.Private -> "Private site"
        io.github.ashxtrem.hashlark.core.SiteType.SemiPrivate -> "Semi-private site"
        else -> "Definition"
    }
    ProviderKind.Torznab -> "Torznab"
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun ProviderDetail(
    provider: ProviderView,
    vm: ProvidersViewModel,
    actions: ResultActions,
    onBack: (() -> Unit)?,
    onRemoved: () -> Unit,
) {
    val testing by vm.testing.collectAsStateWithLifecycle()
    val reports by vm.reports.collectAsStateWithLifecycle()
    var confirmRemove by rememberSaveable(provider.id) { mutableStateOf(false) }
    val status = LocalStatusColors.current

    Surface(Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.surface) {
        Column(Modifier.verticalScroll(rememberScrollState()).padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                if (onBack != null) {
                    IconButton(onClick = onBack) { Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back to providers") }
                }
                Column(Modifier.weight(1f)) {
                    Text(provider.name, style = MaterialTheme.typography.titleLarge)
                    Text(kindLabel(provider), color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
            }
            if (provider.description.isNotBlank()) Text(provider.description)
            SwitchRow("Enabled", provider.enabled, { vm.setEnabled(provider, it) })
            provider.error?.let {
                Text("This provider can't be loaded: $it", color = MaterialTheme.colorScheme.error)
            }

            // Health
            SectionTitle("Health")
            val health = provider.health
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                HealthDot(health, provider.enabled)
                Text(health.state.label)
            }
            val facts = buildList {
                health.successRate?.let { add("${(it * 100).toInt()}% of recent searches worked") }
                if (health.p50Ms != null) add("Typical ${Format.duration(health.p50Ms)}, slowest ${Format.duration(health.p95Ms)}")
                health.lastErrorKind?.let { add("Last problem: ${it.label}") }
                health.disabledUntil?.let { add("Paused until ${dateTime(it)}") }
                health.lastCheckedAt?.let { add("Last checked ${dateTime(it)}") }
            }
            facts.forEach { Text(it, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }

            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                OutlinedButton(onClick = { vm.test(provider) }, enabled = provider.id !in testing) {
                    if (provider.id in testing) CircularProgressIndicator(Modifier.size(16.dp), strokeWidth = 2.dp)
                    Text("Test now", Modifier.padding(start = if (provider.id in testing) 8.dp else 0.dp))
                }
                if (provider.siteUrl != null) {
                    val needsCheck = health.lastErrorKind == ErrorKind.ChallengeRequired
                    val label = if (needsCheck) "Browser check…" else "Open site"
                    OutlinedButton(onClick = { if (needsCheck) actions.challenge(provider.id) else actions.viewOnSite(provider.siteUrl!!) }) {
                        Text(label)
                        Icon(Icons.AutoMirrored.Filled.OpenInNew, contentDescription = null, Modifier.padding(start = 6.dp).size(16.dp))
                    }
                }
            }
            reports[provider.id]?.let { report ->
                Text(
                    if (report.ok) "Worked: ${report.resultCount} results in ${Format.duration(report.latencyMs)}"
                    else "Failed: ${report.errorKind?.label ?: "error"}${report.message?.let { " - $it" }.orEmpty()}",
                    color = if (report.ok) status.ok else MaterialTheme.colorScheme.error,
                )
            }

            // Provider settings (logins, options)
            if (provider.settings.isNotEmpty()) {
                SectionTitle("Settings")
                ProviderSettingsForm(provider, vm)
            }

            // Network
            SectionTitle("Network")
            NetworkPolicyEditor(provider, vm)

            if (!provider.builtin) {
                Spacer(Modifier.width(1.dp))
                OutlinedButton(onClick = { confirmRemove = true }) {
                    Text("Remove provider", color = MaterialTheme.colorScheme.error)
                }
            }
        }
    }
    if (confirmRemove) {
        ConfirmDialog(
            title = "Remove ${provider.name}?",
            text = "Its saved settings and logins are deleted from this device.",
            confirmLabel = "Remove",
            destructive = true,
            onConfirm = {
                confirmRemove = false
                vm.remove(provider)
                onRemoved()
            },
            onDismiss = { confirmRemove = false },
        )
    }
}

private fun dateTime(ms: Long): String =
    DateTimeFormatter.ofLocalizedDateTime(FormatStyle.MEDIUM).withZone(ZoneId.systemDefault()).format(Instant.ofEpochMilli(ms))

/**
 * Login and option fields of a provider. An empty password field means "keep
 * the saved one"; the core never returns a saved password.
 */
@Composable
private fun ProviderSettingsForm(provider: ProviderView, vm: ProvidersViewModel) {
    val saving by vm.saving.collectAsStateWithLifecycle()
    val scope = rememberCoroutineScope()
    val values: SnapshotStateMap<String, String> = remember(provider.id, provider.settings) {
        mutableStateMapOf<String, String>().apply {
            provider.settings.forEach { put(it.name, if (it.kind == SettingKind.Password) "" else it.value.orEmpty()) }
        }
    }
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        provider.settings.forEach { setting ->
            SettingField(setting, values[setting.name].orEmpty()) { values[setting.name] = it }
            if (setting.kind == SettingKind.Password && setting.isSet) {
                TextButton(onClick = {
                    scope.launch { vm.update(provider, ProviderPatch(settings = mapOf(setting.name to null)), "Removed") }
                }) { Text("Remove saved ${setting.label.lowercase()}") }
            }
        }
        Button(
            enabled = !saving,
            onClick = {
                val changes = mutableMapOf<String, String?>()
                provider.settings.forEach { s ->
                    val v = values[s.name].orEmpty()
                    if (s.kind == SettingKind.Password) {
                        if (v.isNotEmpty()) changes[s.name] = v
                    } else if (v != s.value.orEmpty()) {
                        changes[s.name] = v.ifEmpty { null }
                    }
                }
                scope.launch { vm.update(provider, ProviderPatch(settings = changes), "Provider settings saved") }
            },
        ) { Text("Save settings") }
        Text(
            "Passwords are kept in the Android Keystore, encrypted with a key that never leaves this device.",
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

@Composable
private fun SettingField(setting: SettingView, value: String, onChange: (String) -> Unit) {
    val label = setting.label + if (setting.required) " *" else ""
    when (setting.kind) {
        SettingKind.Select -> {
            var open by remember { mutableStateOf(false) }
            Box {
                OutlinedButton(onClick = { open = true }, Modifier.fillMaxWidth()) {
                    Text("$label: ${setting.options[value] ?: "Default"}", Modifier.weight(1f))
                }
                DropdownMenu(expanded = open, onDismissRequest = { open = false }) {
                    DropdownMenuItem(text = { Text("Default") }, onClick = { onChange(""); open = false })
                    setting.options.forEach { (key, text) ->
                        DropdownMenuItem(text = { Text(text) }, onClick = { onChange(key); open = false })
                    }
                }
            }
        }
        SettingKind.Checkbox -> SwitchRow(label, value == "true", { onChange(if (it) "true" else "") })
        SettingKind.Password -> OutlinedTextField(
            value = value,
            onValueChange = onChange,
            label = { Text(label) },
            placeholder = { if (setting.isSet) Text("Saved; leave empty to keep") },
            singleLine = true,
            visualTransformation = PasswordVisualTransformation(),
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password),
            modifier = Modifier.fillMaxWidth(),
        )
        SettingKind.Text -> OutlinedTextField(
            value = value,
            onValueChange = onChange,
            label = { Text(label) },
            singleLine = true,
            modifier = Modifier.fillMaxWidth(),
        )
    }
}

/** How this provider reaches the internet: the global setting, direct, its own proxy, or Tor. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun NetworkPolicyEditor(provider: ProviderView, vm: ProvidersViewModel) {
    val scope = rememberCoroutineScope()
    var route by remember(provider.id, provider.networkPolicy) { mutableStateOf(provider.networkPolicy.route) }
    var proxy by remember(provider.id, provider.networkPolicy) { mutableStateOf(provider.networkPolicy.proxy.orEmpty()) }
    val changed = route != provider.networkPolicy.route || (route == Route.Proxy && proxy.trim() != provider.networkPolicy.proxy.orEmpty())
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            listOf(
                Route.Default to "Follow settings",
                Route.Direct to "Direct",
                Route.Proxy to "Own proxy",
                Route.Tor to "Tor",
            ).forEach { (value, text) ->
                FilterChip(selected = route == value, onClick = { route = value }, label = { Text(text) })
            }
        }
        if (route == Route.Proxy) {
            OutlinedTextField(
                value = proxy,
                onValueChange = { proxy = it },
                label = { Text("Proxy address") },
                placeholder = { Text("socks5h://127.0.0.1:9050 or http://proxy:8080") },
                singleLine = true,
                modifier = Modifier.fillMaxWidth(),
            )
        }
        Text(
            when (route) {
                Route.Default -> "Uses the network settings for every provider (encrypted DNS, proxy, Tor)."
                Route.Direct -> "Never uses a proxy or Tor for this provider."
                Route.Proxy -> "Uses this proxy for this provider only."
                Route.Tor -> "Always goes through Tor; slower, but reaches sites blocked on your network."
            },
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        if (changed) {
            Button(onClick = {
                scope.launch {
                    vm.update(
                        provider,
                        ProviderPatch(networkPolicy = NetworkPolicy(route, proxy.trim().ifEmpty { null }.takeIf { route == Route.Proxy })),
                        "Network setting saved",
                    )
                }
            }) { Text("Save network setting") }
        }
    }
}
