// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark.ui.providers

import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.PrimaryTabRow
import androidx.compose.material3.Tab
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.github.ashxtrem.hashlark.core.DefinitionCheck
import io.github.ashxtrem.hashlark.core.ProviderView
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/** Adds a Torznab endpoint (Jackett, Prowlarr, Bitmagnet) or a provider definition. */
@Composable
fun AddProviderDialog(
    vm: ProvidersViewModel,
    onDismiss: () -> Unit,
    onAdded: (ProviderView) -> Unit,
    initialYaml: String = "",
) {
    var tab by rememberSaveable { mutableIntStateOf(if (initialYaml.isNotEmpty()) 1 else 0) }
    var name by rememberSaveable { mutableStateOf("") }
    var url by rememberSaveable { mutableStateOf("") }
    var apiKey by rememberSaveable { mutableStateOf("") }
    var yaml by rememberSaveable { mutableStateOf(initialYaml) }
    var check by remember { mutableStateOf<DefinitionCheck?>(null) }
    val scope = rememberCoroutineScope()
    val context = LocalContext.current
    val saving by vm.saving.collectAsStateWithLifecycle()

    val pickFile = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        if (uri != null) {
            runCatching {
                context.contentResolver.openInputStream(uri)?.use { it.readBytes().decodeToString(throwOnInvalidSequence = false) }
            }.getOrNull()?.let { yaml = it.take(MAX_YAML_CHARS) }
        }
    }

    // Check the definition a moment after typing stops.
    LaunchedEffect(yaml, tab) {
        check = null
        if (tab == 1 && yaml.isNotBlank()) {
            delay(400)
            check = vm.checkDefinition(yaml)
        }
    }

    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Add a provider") },
        text = {
            Column(Modifier.verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                PrimaryTabRow(selectedTabIndex = tab) {
                    Tab(selected = tab == 0, onClick = { tab = 0 }, text = { Text("Torznab") })
                    Tab(selected = tab == 1, onClick = { tab = 1 }, text = { Text("Definition") })
                }
                if (tab == 0) {
                    Text(
                        "A Torznab endpoint from Jackett, Prowlarr or Bitmagnet that you run yourself.",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                    OutlinedTextField(name, { name = it }, label = { Text("Name") }, singleLine = true, modifier = Modifier.fillMaxWidth())
                    OutlinedTextField(
                        url, { url = it },
                        label = { Text("Torznab URL") },
                        placeholder = { Text("http://192.168.1.10:9117/api/v2.0/indexers/all/results/torznab/") },
                        singleLine = true,
                        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri),
                        modifier = Modifier.fillMaxWidth(),
                    )
                    OutlinedTextField(
                        apiKey, { apiKey = it },
                        label = { Text("API key (optional)") },
                        singleLine = true,
                        visualTransformation = PasswordVisualTransformation(),
                        modifier = Modifier.fillMaxWidth(),
                    )
                } else {
                    Text(
                        "Paste a provider definition (YAML), or pick a .yml file. Definitions are declarative; they run no code.",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                    OutlinedButton(onClick = { pickFile.launch(arrayOf("*/*")) }) { Text("Pick a file…") }
                    OutlinedTextField(
                        yaml, { yaml = it.take(MAX_YAML_CHARS) },
                        label = { Text("Definition") },
                        modifier = Modifier.fillMaxWidth().heightIn(min = 160.dp, max = 280.dp),
                    )
                    check?.let { result ->
                        if (result.ok) {
                            Text("Valid: ${result.name ?: result.id} (version ${result.version ?: 1})", color = io.github.ashxtrem.hashlark.ui.theme.LocalStatusColors.current.ok)
                        } else {
                            result.errors.forEach { Text(it, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall) }
                        }
                    }
                }
            }
        },
        confirmButton = {
            TextButton(
                enabled = !saving && if (tab == 0) name.isNotBlank() && url.isNotBlank() else check?.ok == true,
                onClick = {
                    scope.launch {
                        val added = if (tab == 0) vm.addTorznab(name, url, apiKey.ifBlank { null }) else vm.addDefinition(yaml)
                        if (added != null) {
                            onAdded(added)
                            onDismiss()
                        }
                    }
                },
            ) { Text("Add") }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel") } },
    )
}

private const val MAX_YAML_CHARS = 200_000
