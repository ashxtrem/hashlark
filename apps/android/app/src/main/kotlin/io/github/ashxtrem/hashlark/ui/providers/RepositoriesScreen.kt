// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark.ui.providers

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material.icons.filled.VerifiedUser
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.github.ashxtrem.hashlark.core.RepoPreview
import io.github.ashxtrem.hashlark.core.RepoView
import io.github.ashxtrem.hashlark.ui.common.ConfirmDialog
import io.github.ashxtrem.hashlark.ui.common.LoadingBox
import io.github.ashxtrem.hashlark.ui.common.ScreenTitle
import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.time.format.FormatStyle

/**
 * Signed collections of provider definitions. Adding one first shows its
 * signing key: the key you accept is remembered (trust on first use), and later
 * updates signed with another key are refused.
 */
@Composable
fun RepositoriesScreen(vm: ReposViewModel, onBack: () -> Unit, modifier: Modifier = Modifier) {
    val repos by vm.repos.collectAsStateWithLifecycle()
    val busy by vm.busy.collectAsStateWithLifecycle()
    val preview by vm.preview.collectAsStateWithLifecycle()
    var url by rememberSaveable { mutableStateOf("") }
    var removing by rememberSaveable { mutableStateOf<String?>(null) }

    Column(modifier.fillMaxSize()) {
        ScreenTitle("Definition repositories") {
            IconButton(onClick = onBack) { Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back to providers") }
        }
        Text(
            "Signed collections of provider definitions, synced about once a day on Wi-Fi. New providers start disabled.",
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.padding(horizontal = 16.dp),
        )
        Row(Modifier.padding(16.dp), horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
            OutlinedTextField(
                value = url,
                onValueChange = { url = it },
                modifier = Modifier.weight(1f),
                label = { Text("Repository address") },
                placeholder = { Text("https://example.org/hashlark-definitions/") },
                singleLine = true,
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri, imeAction = ImeAction.Go),
                keyboardActions = KeyboardActions(onGo = { if (url.isNotBlank()) vm.requestAdd(url) }),
            )
            Button(onClick = { vm.requestAdd(url) }, enabled = url.isNotBlank() && "" !in busy) {
                if ("" in busy) CircularProgressIndicator(Modifier.size(16.dp), strokeWidth = 2.dp) else Text("Add")
            }
        }
        val list = repos
        if (list == null) {
            LoadingBox()
        } else {
            LazyColumn {
                items(list, key = { it.id }) { repo ->
                    RepoRow(repo, syncing = repo.id in busy, onSync = { vm.sync(repo) }, onRemove = { removing = repo.id })
                    HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
                }
            }
        }
    }

    preview?.let { TrustDialog(it, onTrust = vm::confirmAdd, onDismiss = vm::cancelAdd) }
    removing?.let { id ->
        val repo = repos.orEmpty().firstOrNull { it.id == id }
        if (repo == null) {
            removing = null
        } else {
            ConfirmDialog(
                title = "Remove ${repo.name ?: repo.url}?",
                text = "Its ${repo.definitions} provider(s) are removed too.",
                confirmLabel = "Remove",
                destructive = true,
                onConfirm = {
                    removing = null
                    vm.remove(repo)
                },
                onDismiss = { removing = null },
            )
        }
    }
}

/** The trust-on-first-use prompt: shows the signing key before it is remembered. */
@Composable
fun TrustDialog(preview: RepoPreview, onTrust: () -> Unit, onDismiss: () -> Unit) {
    AlertDialog(
        onDismissRequest = onDismiss,
        icon = { Icon(Icons.Filled.VerifiedUser, contentDescription = null) },
        title = { Text("Trust this repository?") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(preview.name, style = MaterialTheme.typography.titleSmall)
                Text(preview.url, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                Text("${preview.definitions} provider definition${if (preview.definitions == 1) "" else "s"}")
                Text("Signing key", style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
                Text(preview.fingerprint, fontFamily = FontFamily.Monospace)
                Text(
                    "Hashlark remembers this key. Later updates signed with any other key are refused. " +
                        "Only add repositories from people you trust; new providers start disabled.",
                    style = MaterialTheme.typography.bodySmall,
                )
            }
        },
        confirmButton = { TextButton(onClick = onTrust) { Text("Trust and add") } },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel") } },
    )
}

@Composable
private fun RepoRow(repo: RepoView, syncing: Boolean, onSync: () -> Unit, onRemove: () -> Unit) {
    Row(Modifier.fillMaxWidth().padding(start = 16.dp, top = 8.dp, bottom = 8.dp, end = 4.dp), verticalAlignment = Alignment.CenterVertically) {
        Column(Modifier.weight(1f)) {
            Text(repo.name ?: repo.url, style = MaterialTheme.typography.bodyLarge)
            Text(
                listOfNotNull(
                    if (repo.builtin) "Shipped with Hashlark" else repo.url,
                    "${repo.definitions} definition${if (repo.definitions == 1L) "" else "s"}",
                    repo.lastSyncAt?.let { "synced " + DateTimeFormatter.ofLocalizedDateTime(FormatStyle.MEDIUM).withZone(ZoneId.systemDefault()).format(Instant.ofEpochMilli(it)) },
                ).joinToString(" · "),
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            repo.fingerprint?.let {
                Text("Key $it", style = MaterialTheme.typography.bodySmall, fontFamily = FontFamily.Monospace, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
        }
        if (!repo.builtin) {
            IconButton(onClick = onSync, enabled = !syncing) {
                if (syncing) CircularProgressIndicator(Modifier.size(20.dp), strokeWidth = 2.dp) else Icon(Icons.Filled.Refresh, contentDescription = "Sync now")
            }
            IconButton(onClick = onRemove) { Icon(Icons.Filled.Delete, contentDescription = "Remove repository", tint = MaterialTheme.colorScheme.error) }
        }
    }
}
