// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark.ui.downloads

import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Checkbox
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.github.ashxtrem.hashlark.core.AddSource
import io.github.ashxtrem.hashlark.system.TorrentService
import io.github.ashxtrem.hashlark.system.TorrentStorage
import io.github.ashxtrem.hashlark.ui.AddStage
import io.github.ashxtrem.hashlark.ui.AppViewModel
import io.github.ashxtrem.hashlark.ui.PendingAdd
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/** The one-time notice before the first download (plan §2). */
const val DOWNLOAD_NOTICE =
    "Downloading shares pieces with other people and shows your IP address to them. Only download what you have the right to."

/**
 * Runs [AppViewModel.pendingAdd] to its end: the notice, the Add sheet, then the permission
 * prompts (notifications on Android 13+, storage on Android 8-9) before the add. Placed once,
 * in the app frame, so every add (results, intents, LAN links, the Downloads screen) uses it.
 */
@Composable
fun AddTorrentFlow(app: AppViewModel) {
    val pending by app.pendingAdd.collectAsStateWithLifecycle()
    val settings by app.settings.collectAsStateWithLifecycle()
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val permissions = rememberLauncherForActivityResult(ActivityResultContracts.RequestMultiplePermissions()) { results ->
        val storage = results[TorrentStorage.STORAGE_PERMISSION] == true
        scope.launch {
            // The client resolved its folder before the grant: name `Download/Hashlark` so it is used now.
            val dir = if (storage) {
                withContext(Dispatchers.IO) { TorrentStorage.publicDir().takeIf(TorrentStorage::canWrite)?.absolutePath }
            } else {
                null
            }
            app.finishAdd(dir)
        }
    }
    val p = pending ?: return
    when (p.stage) {
        AddStage.Notice -> DownloadNoticeDialog(onAccept = app::acceptNotice, onDismiss = app::cancelAdd)
        AddStage.Options -> AddTorrentSheet(
            pending = p,
            folder = settings?.torrents?.downloadDir,
            onAdd = app::confirmAddOptions,
            onDismiss = app::cancelAdd,
        )
        AddStage.Permissions -> LaunchedEffect(p) {
            // Recreated while the dialog is open: its answer (delivered to the new launcher) finishes the add.
            if (app.awaitingPermissions) return@LaunchedEffect
            val needed = if (app.takePermissionPrompt()) {
                listOfNotNull(
                    TorrentService.NOTIFICATION_PERMISSION.takeIf { TorrentService.needsNotificationPermission(context) },
                    TorrentStorage.STORAGE_PERMISSION.takeIf { TorrentStorage.needsStoragePermission(context) },
                )
            } else {
                emptyList()
            }
            if (needed.isEmpty()) {
                app.finishAdd()
            } else {
                app.permissionPromptShown()
                permissions.launch(needed.toTypedArray())
            }
        }
    }
}

@Composable
fun DownloadNoticeDialog(onAccept: () -> Unit, onDismiss: () -> Unit) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Before you download") },
        text = { Text(DOWNLOAD_NOTICE) },
        confirmButton = { Button(onClick = onAccept) { Text("I understand") } },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel") } },
    )
}

/** What a source is called before its metadata is known. */
fun sourceName(pending: PendingAdd): String = pending.title ?: when (val s = pending.request.source) {
    is AddSource.Magnet -> magnetName(s.uri) ?: "Magnet link"
    is AddSource.Torrent -> ".torrent file"
    is AddSource.ResultId -> "Search result"
}

/** The `dn` (display name) of a magnet link. */
fun magnetName(magnet: String): String? = magnet.substringAfter('?', "").split('&')
    .firstOrNull { it.startsWith("dn=") }
    ?.removePrefix("dn=")
    ?.let { runCatching { java.net.URLDecoder.decode(it, "UTF-8") }.getOrNull() }
    ?.takeIf { it.isNotBlank() }

/**
 * Options for one add: the folder (empty is the default folder) and "Start paused". Files are
 * picked in the torrent's details once its metadata has arrived.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AddTorrentSheet(pending: PendingAdd, folder: String?, onAdd: (saveDir: String?, paused: Boolean) -> Unit, onDismiss: () -> Unit) {
    var dir by rememberSaveable { mutableStateOf("") }
    var paused by rememberSaveable { mutableStateOf(false) }
    ModalBottomSheet(onDismissRequest = onDismiss, sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)) {
        Column(
            Modifier.fillMaxWidth().verticalScroll(rememberScrollState()).padding(horizontal = 24.dp).padding(bottom = 16.dp).navigationBarsPadding(),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Text("Add torrent", style = MaterialTheme.typography.titleLarge)
            Text(sourceName(pending), style = MaterialTheme.typography.bodyLarge, maxLines = 3, overflow = TextOverflow.Ellipsis)
            OutlinedTextField(
                value = dir,
                onValueChange = { dir = it },
                label = { Text("Save to") },
                placeholder = { Text(folder ?: "Default folder (Download/Hashlark)") },
                supportingText = { Text("Leave empty for the default folder.") },
                singleLine = true,
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri),
                modifier = Modifier.fillMaxWidth(),
            )
            Row(
                Modifier.fillMaxWidth().heightIn(min = 48.dp).toggleable(value = paused, role = Role.Checkbox, onValueChange = { paused = it }),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Checkbox(checked = paused, onCheckedChange = null)
                Text("Start paused", Modifier.padding(start = 12.dp))
            }
            Text(
                "To download only some files, open the torrent in Downloads once its file list has arrived.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.End)) {
                TextButton(onClick = onDismiss) { Text("Cancel") }
                Button(onClick = { onAdd(dir, paused) }) { Text("Download") }
            }
        }
    }
}

/** Pastes a magnet link on the Downloads screen. */
@Composable
fun AddMagnetDialog(onAdd: (magnet: String, withOptions: Boolean) -> Unit, onDismiss: () -> Unit) {
    var text by rememberSaveable { mutableStateOf("") }
    val valid = text.trim().startsWith("magnet:?", ignoreCase = true)
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Add magnet link") },
        text = {
            OutlinedTextField(
                value = text,
                onValueChange = { text = it },
                label = { Text("Magnet link") },
                placeholder = { Text("magnet:?xt=urn:btih:…") },
                isError = text.isNotBlank() && !valid,
                supportingText = { if (text.isNotBlank() && !valid) Text("A magnet link starts with magnet:?") },
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri),
                modifier = Modifier.fillMaxWidth(),
            )
        },
        confirmButton = {
            Row {
                TextButton(onClick = { onAdd(text.trim(), true) }, enabled = valid) { Text("Options…") }
                TextButton(onClick = { onAdd(text.trim(), false) }, enabled = valid) { Text("Download") }
            }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel") } },
    )
}
