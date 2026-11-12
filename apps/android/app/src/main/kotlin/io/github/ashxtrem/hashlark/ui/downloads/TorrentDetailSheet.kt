// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark.ui.downloads

import android.content.Context
import android.content.Intent
import android.webkit.MimeTypeMap
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.OpenInNew
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material3.Button
import androidx.compose.material3.Checkbox
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.PrimaryScrollableTabRow
import androidx.compose.material3.Tab
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalLifecycleOwner
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.repeatOnLifecycle
import io.github.ashxtrem.hashlark.core.BulkAction
import io.github.ashxtrem.hashlark.core.Format
import io.github.ashxtrem.hashlark.core.PeerInfo
import io.github.ashxtrem.hashlark.core.SpeedLimits
import io.github.ashxtrem.hashlark.core.TorrentAction
import io.github.ashxtrem.hashlark.core.TorrentDetails
import io.github.ashxtrem.hashlark.core.TorrentFile
import io.github.ashxtrem.hashlark.core.TorrentPatch
import io.github.ashxtrem.hashlark.core.TorrentState
import io.github.ashxtrem.hashlark.core.TorrentSummary
import io.github.ashxtrem.hashlark.system.TorrentStorage
import io.github.ashxtrem.hashlark.ui.common.CenteredMessage
import io.github.ashxtrem.hashlark.ui.common.LoadingBox
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import java.io.File
import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.time.format.FormatStyle

private enum class DetailTab(val label: String) { Overview("Overview"), Files("Files"), Peers("Peers"), Trackers("Trackers") }

/** One torrent: Overview (with speed and peer limits), Files with checkboxes, Peers, Trackers; Open file and Delete. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun TorrentDetailSheet(id: String, vm: DownloadsViewModel, onDelete: (TorrentSummary) -> Unit, onDismiss: () -> Unit) {
    var details by remember(id) { mutableStateOf<TorrentDetails?>(null) }
    var peers by remember(id) { mutableStateOf<List<PeerInfo>>(emptyList()) }
    var failed by remember(id) { mutableStateOf(false) }
    var tab by rememberSaveable { mutableIntStateOf(0) }
    val scope = rememberCoroutineScope()

    // Read once a second, only while the app is on screen; a failed read is shown in the sheet, not as a message.
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    LaunchedEffect(id, tab) {
        lifecycle.repeatOnLifecycle(Lifecycle.State.STARTED) {
            while (true) {
                val read = vm.details(id)
                if (read != null) details = read else failed = details == null
                if (DetailTab.entries[tab] == DetailTab.Peers) peers = vm.peers(id)
                delay(1_000)
            }
        }
    }

    ModalBottomSheet(onDismissRequest = onDismiss, sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)) {
        val d = details
        when {
            d == null && failed -> CenteredMessage("This torrent is gone", Modifier.heightIn(max = 240.dp))
            d == null -> LoadingBox(Modifier.heightIn(max = 240.dp))
            else -> Column(Modifier.fillMaxWidth().navigationBarsPadding()) {
                Text(
                    d.summary.name,
                    style = MaterialTheme.typography.titleLarge,
                    maxLines = 3,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.padding(horizontal = 24.dp),
                )
                PrimaryScrollableTabRow(selectedTabIndex = tab, edgePadding = 16.dp) {
                    DetailTab.entries.forEachIndexed { i, t ->
                        Tab(selected = tab == i, onClick = { tab = i }, text = { Text(t.label) })
                    }
                }
                Column(
                    Modifier.fillMaxWidth().verticalScroll(rememberScrollState()).padding(horizontal = 24.dp, vertical = 16.dp),
                    verticalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                    when (DetailTab.entries[tab]) {
                        DetailTab.Overview -> Overview(
                            d,
                            onAction = { action -> vm.run(listOf(id), action) },
                            onBulk = { action -> vm.run(listOf(id), action) },
                            onSave = { patch -> scope.launch { vm.update(id, patch)?.let { details = it } } },
                            onDelete = { onDelete(d.summary) },
                        )
                        DetailTab.Files -> Files(d) { files ->
                            scope.launch { vm.update(id, TorrentPatch(files = files))?.let { details = it } }
                        }
                        DetailTab.Peers -> Peers(peers)
                        DetailTab.Trackers -> Trackers(d) { list ->
                            var error: String? = null
                            vm.update(id, TorrentPatch(trackers = list)) { error = it }?.let { details = it }
                            error
                        }
                    }
                }
            }
        }
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun Overview(
    d: TorrentDetails,
    onAction: (TorrentAction) -> Unit,
    onBulk: (BulkAction) -> Unit,
    onSave: (TorrentPatch) -> Unit,
    onDelete: () -> Unit,
) {
    val t = d.summary
    val context = LocalContext.current
    LinearProgressIndicator(progress = { t.progress.coerceIn(0.0, 1.0).toFloat() }, modifier = Modifier.fillMaxWidth())
    Text("${DownloadsLogic.statusLabel(t)} · ${DownloadsLogic.percent(t.progress)}", style = MaterialTheme.typography.titleSmall)
    t.error?.let { Text(it, color = MaterialTheme.colorScheme.error) }
    FlowRow(horizontalArrangement = Arrangement.spacedBy(4.dp)) {
        val pause = DownloadsLogic.pauseToggle(listOf(t))
        val stop = DownloadsLogic.stopToggle(listOf(t))
        BarAction(pause.icon(), pause.label()) { onAction(pause) }
        BarAction(stop.icon(), stop.label()) { onAction(stop) }
        BarAction(TorrentIcons.forceStart, "Force start") { onAction(TorrentAction.ForceStart) }
        openableFile(d)?.let { file ->
            BarAction(Icons.AutoMirrored.Filled.OpenInNew, "Open file") { openFile(context, file) }
        }
        BarAction(TorrentIcons.delete, "Delete", onClick = onDelete)
        MoreTorrentActions(
            count = 1,
            canReannounce = DownloadsLogic.canReannounce(listOf(t)),
            canRecheck = DownloadsLogic.canRecheck(listOf(t)),
            onBulk = onBulk,
        )
    }
    Fact("Size", if (t.size > 0) "${Format.bytes(t.have)} of ${Format.bytes(t.size)}" else "Not known yet")
    Fact("Speed", "↓ ${DownloadsLogic.speed(t.downBps)}  ↑ ${DownloadsLogic.speed(t.upBps)}")
    if (t.etaSecs != null) Fact("Time left", DownloadsLogic.eta(t.etaSecs))
    Fact("Downloaded / uploaded", "${Format.bytes(t.downloadedTotal)} / ${Format.bytes(t.uploadedTotal)}")
    Fact("Ratio", DownloadsLogic.ratio(t.ratio))
    Fact("Peers", "${t.peers} connected, ${t.seeds} seeds" + (d.peerLimit?.let { " (limit $it)" } ?: ""))
    Fact("Saved in", t.saveDir)
    Fact("Added", dateText(t.addedAt))
    t.completedAt?.let { Fact("Completed", dateText(it)) }
    Column {
        Text("Info hash", style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
        SelectionContainer { Text(t.id, fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodyMedium) }
    }
    Limits(d, onSave)
}

/** Per-torrent speed limits (KiB/s, empty is unlimited) and the connection limit (empty is the global default). */
@Composable
private fun Limits(d: TorrentDetails, onSave: (TorrentPatch) -> Unit) {
    var down by remember(d.id, d.limits) { mutableStateOf(d.limits.downKib?.toString().orEmpty()) }
    var up by remember(d.id, d.limits) { mutableStateOf(d.limits.upKib?.toString().orEmpty()) }
    var peers by remember(d.id, d.peerLimit) { mutableStateOf(d.peerLimit?.toString().orEmpty()) }
    val peerValue = peers.toIntOrNull()
    val peersValid = peers.isEmpty() || (peerValue != null && peerValue in 10..2000)
    Text("Limits", style = MaterialTheme.typography.titleSmall, color = MaterialTheme.colorScheme.primary)
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        DigitsField("Download KiB/s", down, Modifier.weight(1f)) { down = it }
        DigitsField("Upload KiB/s", up, Modifier.weight(1f)) { up = it }
    }
    DigitsField("Connections (10 to 2000)", peers, Modifier.fillMaxWidth(), isError = !peersValid) { peers = it }
    Text("Leave a field empty for no limit, or the global connection limit.", style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
    OutlinedButton(
        enabled = peersValid,
        onClick = {
            onSave(
                TorrentPatch(
                    limits = SpeedLimits(down.toLongOrNull()?.takeIf { it > 0 }, up.toLongOrNull()?.takeIf { it > 0 }),
                    // 0 returns to the global default.
                    peerLimit = peerValue ?: 0,
                ),
            )
        },
    ) { Text("Save limits") }
}

@Composable
private fun DigitsField(label: String, value: String, modifier: Modifier, isError: Boolean = false, onChange: (String) -> Unit) {
    OutlinedTextField(
        value = value,
        onValueChange = { onChange(it.filter(Char::isDigit).take(7)) },
        label = { Text(label) },
        isError = isError,
        singleLine = true,
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
        modifier = modifier,
    )
}

@Composable
private fun Files(d: TorrentDetails, onSelect: (List<Int>) -> Unit) {
    val context = LocalContext.current
    if (d.files.isEmpty()) {
        Text("The file list arrives with the torrent's metadata.", color = MaterialTheme.colorScheme.onSurfaceVariant)
        return
    }
    val selected = d.files.filter { it.selected }.map { it.index }.toSet()
    d.files.forEach { f ->
        Row(verticalAlignment = Alignment.CenterVertically) {
            Row(
                Modifier
                    .weight(1f)
                    .heightIn(min = 48.dp)
                    // At least one file stays selected (the core refuses none).
                    .toggleable(value = f.selected, role = Role.Checkbox, enabled = !(f.selected && selected.size == 1)) { on ->
                        onSelect((if (on) selected + f.index else selected - f.index).sorted())
                    },
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Checkbox(checked = f.selected, onCheckedChange = null)
                Column(Modifier.padding(start = 12.dp)) {
                    Text(f.path, maxLines = 2, overflow = TextOverflow.Ellipsis)
                    Text(
                        "${Format.bytes(f.have)} of ${Format.bytes(f.size)}",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
            if (f.size > 0 && f.have >= f.size) {
                IconButton(onClick = { openFile(context, File(d.summary.saveDir, f.path)) }) {
                    Icon(Icons.AutoMirrored.Filled.OpenInNew, contentDescription = "Open ${f.path}")
                }
            }
        }
    }
}

@Composable
private fun Peers(peers: List<PeerInfo>) {
    if (peers.isEmpty()) {
        Text("No peers connected.", color = MaterialTheme.colorScheme.onSurfaceVariant)
        return
    }
    peers.forEach { p ->
        Column {
            Text(p.addr, fontFamily = FontFamily.Monospace)
            Text(
                listOfNotNull(p.client, p.connKind, p.state.ifBlank { null }, "↓ ${Format.bytes(p.downloaded)}", "↑ ${Format.bytes(p.uploaded)}").joinToString(" · "),
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

/**
 * The tracker list, edited in place: each row can be edited or removed, many can be pasted at once,
 * and nothing reaches the engine until Save (each save re-adds the torrent, so peers drop for a moment).
 * [onSave] returns the engine's error, or `null` once saved.
 */
@Composable
private fun Trackers(d: TorrentDetails, onSave: suspend (List<String>) -> String?) {
    val scope = rememberCoroutineScope()
    // `null` while nothing is staged: the list follows the torrent.
    var staged by remember(d.id) { mutableStateOf<List<String>?>(null) }
    var editing by remember(d.id) { mutableStateOf<Int?>(null) }
    var editText by remember(d.id) { mutableStateOf("") }
    var adding by remember(d.id) { mutableStateOf("") }
    var saveError by remember(d.id) { mutableStateOf<String?>(null) }
    var saving by remember(d.id) { mutableStateOf(false) }
    val muted = MaterialTheme.colorScheme.onSurfaceVariant

    if (d.summary.state == TorrentState.Metadata) {
        Text("Trackers can be edited once the metadata is known.", color = muted)
        SelectionContainer {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                d.trackers.forEach { Text(it, style = MaterialTheme.typography.bodyMedium) }
            }
        }
        return
    }

    val list = staged ?: d.trackers
    val changed = staged != null && staged != d.trackers
    fun stage(next: List<String>) {
        staged = next
        saveError = null
    }

    if (list.isEmpty()) Text("No trackers: peers are found through DHT and peer exchange.", color = muted)
    list.forEachIndexed { i, url ->
        if (editing == i) {
            val valid = TrackerEdits.isValid(editText.trim())
            OutlinedTextField(
                value = editText,
                onValueChange = { editText = it },
                label = { Text("Tracker URL") },
                isError = !valid,
                singleLine = true,
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri),
                trailingIcon = {
                    Row {
                        IconButton(
                            enabled = valid,
                            onClick = {
                                stage(TrackerEdits.edit(list, i, editText))
                                editing = null
                            },
                        ) { Icon(Icons.Filled.Check, contentDescription = "Keep") }
                        IconButton(onClick = { editing = null }) { Icon(Icons.Filled.Close, contentDescription = "Cancel") }
                    }
                },
                supportingText = TrackerEdits.problem(listOf(editText.trim()).filterNot(TrackerEdits::isValid))?.let { { Text(it) } },
                modifier = Modifier.fillMaxWidth(),
            )
        } else {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(url, style = MaterialTheme.typography.bodyMedium, modifier = Modifier.weight(1f))
                IconButton(
                    enabled = !saving && editing == null,
                    onClick = {
                        editText = url
                        editing = i
                    },
                ) { Icon(Icons.Filled.Edit, contentDescription = "Edit tracker $url") }
                IconButton(
                    enabled = !saving && editing == null,
                    onClick = { stage(TrackerEdits.remove(list, i)) },
                ) { Icon(TorrentIcons.delete, contentDescription = "Remove tracker $url") }
            }
        }
    }

    val parsed = TrackerEdits.parse(adding)
    val addProblem = TrackerEdits.problem(parsed.invalid)
    OutlinedTextField(
        value = adding,
        onValueChange = { adding = it },
        label = { Text("Add trackers (one per line)") },
        isError = addProblem != null,
        minLines = 2,
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri),
        supportingText = addProblem?.let { { Text(it) } },
        modifier = Modifier.fillMaxWidth(),
    )
    OutlinedButton(
        enabled = !saving && editing == null && parsed.valid.isNotEmpty() && addProblem == null,
        onClick = {
            stage(TrackerEdits.add(list, parsed.valid))
            adding = ""
        },
    ) { Text("Add") }

    val tooMany = list.size > TrackerEdits.MAX
    if (tooMany) Text("At most ${TrackerEdits.MAX} trackers.", color = MaterialTheme.colorScheme.error)
    saveError?.let { Text(it, color = MaterialTheme.colorScheme.error) }
    if (changed) {
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Button(
                enabled = !saving && !tooMany && editing == null,
                onClick = {
                    val next = list
                    saving = true
                    scope.launch {
                        val error = onSave(next)
                        saving = false
                        // A failed save keeps the staged list so it can be fixed.
                        if (error == null) staged = null else saveError = error
                    }
                },
            ) { Text("Save") }
            OutlinedButton(
                enabled = !saving,
                onClick = {
                    staged = null
                    editing = null
                    saveError = null
                },
            ) { Text("Discard") }
        }
    }
    Text("The engine doesn't report each tracker's status.", style = MaterialTheme.typography.bodySmall, color = muted)
}

@Composable
private fun Fact(label: String, value: String) {
    Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        Text(label, Modifier.weight(0.4f), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
        Text(value, Modifier.weight(0.6f), style = MaterialTheme.typography.bodyMedium)
    }
}

private fun dateText(ms: Long): String =
    DateTimeFormatter.ofLocalizedDateTime(FormatStyle.MEDIUM).withZone(ZoneId.systemDefault()).format(Instant.ofEpochMilli(ms))

/** The file "Open file" opens: the only selected file, once it is complete. */
private fun openableFile(d: TorrentDetails): File? {
    val chosen: TorrentFile = d.files.filter { it.selected }.singleOrNull() ?: return null
    if (chosen.size <= 0 || chosen.have < chosen.size) return null
    return File(d.summary.saveDir, chosen.path)
}

/** Opens a downloaded file in another app, through the FileProvider. */
private fun openFile(context: Context, file: File) {
    val opened = runCatching {
        val uri = TorrentStorage.uriFor(context, file)
        val type = MimeTypeMap.getSingleton().getMimeTypeFromExtension(file.extension.lowercase()) ?: "*/*"
        val intent = Intent(Intent.ACTION_VIEW).setDataAndType(uri, type)
            .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_ACTIVITY_NEW_TASK)
        context.startActivity(Intent.createChooser(intent, "Open with").addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
    }.isSuccess
    if (!opened) android.widget.Toast.makeText(context, "No app can open this file", android.widget.Toast.LENGTH_SHORT).show()
}
