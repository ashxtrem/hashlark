// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark.ui.downloads

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.AddLink
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.SelectAll
import androidx.compose.material.icons.filled.Speed
import androidx.compose.material3.Button
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.FilterChip
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.IconToggleButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalLifecycleOwner
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.zIndex
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.repeatOnLifecycle
import io.github.ashxtrem.hashlark.core.AddSource
import io.github.ashxtrem.hashlark.core.BulkAction
import io.github.ashxtrem.hashlark.core.QueueStep
import io.github.ashxtrem.hashlark.core.ScheduleMode
import io.github.ashxtrem.hashlark.core.SessionStats
import io.github.ashxtrem.hashlark.core.TorrentAction
import io.github.ashxtrem.hashlark.core.TorrentSummary
import io.github.ashxtrem.hashlark.ui.AppViewModel
import io.github.ashxtrem.hashlark.ui.common.CenteredMessage
import io.github.ashxtrem.hashlark.ui.common.LoadingBox
import io.github.ashxtrem.hashlark.ui.common.ScreenTitle
import kotlinx.coroutines.delay
import kotlin.math.roundToInt

/** Test tag of the torrent list. */
const val DOWNLOADS_TAG = "downloads"

/** How often the list is read while the screen is visible. */
private const val POLL_MS = 1_000L

/**
 * The built-in client's torrents: filter chips with counts, a list that updates every second,
 * multi-select by long-press with a bottom action bar, and drag-to-reorder in the Queued filter.
 */
@Composable
fun DownloadsScreen(app: AppViewModel, vm: DownloadsViewModel, modifier: Modifier = Modifier) {
    val settings by app.settings.collectAsStateWithLifecycle()
    val list by vm.list.collectAsStateWithLifecycle()
    val stats by vm.stats.collectAsStateWithLifecycle()
    val error by vm.error.collectAsStateWithLifecycle()
    val enabled = settings?.torrents?.enabled == true

    var filter by rememberSaveable { mutableStateOf(DownloadFilter.All) }
    var selection by rememberSaveable { mutableStateOf(emptySet<String>()) }
    var details by rememberSaveable { mutableStateOf<String?>(null) }
    var deleting by remember { mutableStateOf<List<TorrentSummary>?>(null) }
    var addingMagnet by rememberSaveable { mutableStateOf(false) }

    // Read once a second, but only while the app is on screen.
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    LaunchedEffect(enabled) {
        if (!enabled) return@LaunchedEffect
        lifecycle.repeatOnLifecycle(Lifecycle.State.STARTED) {
            while (true) {
                vm.refresh()
                delay(POLL_MS)
            }
        }
    }

    val all = list.orEmpty()
    // Torrents that went away (deleted elsewhere) leave the selection.
    val selected = all.filter { it.id in selection }
    BackHandler(enabled = selection.isNotEmpty()) { selection = emptySet() }

    Column(modifier.fillMaxSize()) {
        if (selection.isEmpty()) {
            ScreenTitle("Downloads") {
                if (enabled) {
                    TitleActions(stats, onAdd = { addingMagnet = true }, vm = vm)
                }
            }
        } else {
            SelectionTitle(
                count = selection.size,
                onClear = { selection = emptySet() },
                onSelectAll = { selection = DownloadsLogic.visible(all, filter).map { it.id }.toSet() },
            )
        }
        when {
            !enabled -> CenteredMessage(
                "The built-in client is off",
                body = "Results open in your torrent app instead. Turn the built-in client on to download inside Hashlark.",
                action = { Button(onClick = { app.turnOnBuiltInClient() }) { Text("Turn on the built-in client") } },
            )
            list == null && error != null -> CenteredMessage("Downloads aren't available", body = error)
            list == null -> LoadingBox()
            else -> {
                stats?.let { SessionLine(it) }
                FilterChips(all, filter) {
                    filter = it
                    selection = emptySet()
                }
                val rows = DownloadsLogic.visible(all, filter)
                Box(Modifier.weight(1f).fillMaxWidth()) {
                    if (rows.isEmpty()) {
                        CenteredMessage(
                            if (filter == DownloadFilter.All) "Nothing downloading" else "No ${filter.label.lowercase()} torrents",
                            body = if (filter == DownloadFilter.All) "Download a search result, or open a magnet link or .torrent file with Hashlark." else null,
                        )
                    } else {
                        TorrentList(
                            rows = rows,
                            all = all,
                            reorder = filter == DownloadFilter.Queued && selection.isEmpty(),
                            selection = selection,
                            onClick = { t ->
                                if (selection.isNotEmpty()) selection = selection.toggle(t.id) else details = t.id
                            },
                            onLongClick = { t -> selection = selection.toggle(t.id) },
                            vm = vm,
                        )
                    }
                }
                if (selection.isNotEmpty()) {
                    SelectionBar(
                        selected = selected,
                        onRun = { vm.run(selection, it) },
                        onMove = { vm.move(selection, it) },
                        onBulk = { vm.run(selection, it) },
                        onDelete = { deleting = selected },
                    )
                }
            }
        }
    }

    details?.let { id ->
        TorrentDetailSheet(
            id = id,
            vm = vm,
            onDelete = { t -> deleting = listOf(t) },
            onDismiss = { details = null },
        )
    }
    deleting?.let { torrents ->
        DeleteTorrentDialog(
            names = torrents.map { it.name },
            onConfirm = { deleteFiles ->
                vm.remove(torrents.map { it.id }, deleteFiles)
                selection = selection - torrents.map { it.id }.toSet()
                if (details in torrents.map { it.id }) details = null
                deleting = null
            },
            onDismiss = { deleting = null },
        )
    }
    if (addingMagnet) {
        AddMagnetDialog(
            onAdd = { magnet, withOptions ->
                addingMagnet = false
                app.requestAdd(AddSource.Magnet(magnet), withOptions = withOptions)
            },
            onDismiss = { addingMagnet = false },
        )
    }
}

private fun Set<String>.toggle(id: String): Set<String> = if (id in this) this - id else this + id

@Composable
private fun TitleActions(stats: SessionStats?, onAdd: () -> Unit, vm: DownloadsViewModel) {
    var menu by remember { mutableStateOf(false) }
    IconButton(onClick = onAdd) { Icon(Icons.Filled.AddLink, contentDescription = "Add magnet link") }
    val turtle = stats?.turtle == true
    IconToggleButton(checked = turtle, onCheckedChange = vm::setTurtle) {
        Icon(
            Icons.Filled.Speed,
            contentDescription = if (turtle) "Alternative speed is on" else "Alternative speed is off",
            tint = if (turtle) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
    Box {
        IconButton(onClick = { menu = true }) { Icon(Icons.Filled.MoreVert, contentDescription = "More") }
        DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
            DropdownMenuItem(text = { Text("Pause all") }, onClick = { menu = false; vm.all(BulkAction.Pause) })
            DropdownMenuItem(text = { Text("Resume all") }, onClick = { menu = false; vm.all(BulkAction.Resume) })
        }
    }
}

@Composable
private fun SelectionTitle(count: Int, onClear: () -> Unit, onSelectAll: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().padding(start = 4.dp, end = 8.dp, top = 4.dp, bottom = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        IconButton(onClick = onClear) { Icon(Icons.Filled.Close, contentDescription = "Clear selection") }
        Text("$count selected", style = MaterialTheme.typography.titleLarge, modifier = Modifier.weight(1f))
        IconButton(onClick = onSelectAll) { Icon(Icons.Filled.SelectAll, contentDescription = "Select all") }
    }
}

/** Total speeds and why transfers are held back (Wi-Fi, battery, schedule). */
@Composable
private fun SessionLine(stats: SessionStats) {
    val reasons = listOfNotNull(
        "Waiting for Wi-Fi".takeIf { !stats.networkAllowed },
        "On battery".takeIf { stats.onBattery },
        when (stats.scheduleMode) {
            ScheduleMode.Off -> "Scheduled: off"
            ScheduleMode.SeedOnly -> "Scheduled: seed only"
            ScheduleMode.Alt -> "Scheduled: alternative speed"
            ScheduleMode.Normal -> null
        },
        "Alternative speed".takeIf { stats.turtle },
    )
    Text(
        (listOf("↓ ${DownloadsLogic.speed(stats.downBps)}  ↑ ${DownloadsLogic.speed(stats.upBps)}") + reasons).joinToString(" · "),
        style = MaterialTheme.typography.bodyMedium,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        maxLines = 2,
        overflow = TextOverflow.Ellipsis,
        modifier = Modifier.padding(horizontal = 16.dp).semantics { liveRegion = LiveRegionMode.Polite },
    )
}

@Composable
private fun FilterChips(all: List<TorrentSummary>, current: DownloadFilter, onPick: (DownloadFilter) -> Unit) {
    val counts = DownloadsLogic.counts(all)
    Row(
        Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = 16.dp, vertical = 4.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        DownloadFilter.entries.forEach { f ->
            FilterChip(
                selected = f == current,
                onClick = { onPick(f) },
                label = { Text("${f.label} ${counts[f] ?: 0}") },
            )
        }
    }
}

@Composable
private fun TorrentList(
    rows: List<TorrentSummary>,
    all: List<TorrentSummary>,
    reorder: Boolean,
    selection: Set<String>,
    onClick: (TorrentSummary) -> Unit,
    onLongClick: (TorrentSummary) -> Unit,
    vm: DownloadsViewModel,
) {
    val state = rememberLazyListState()
    var dragId by remember { mutableStateOf<String?>(null) }
    var dragOffset by remember { mutableFloatStateOf(0f) }
    var rowHeight by remember { mutableIntStateOf(1) }
    val currentRows by rememberUpdatedState(rows)
    val currentAll by rememberUpdatedState(all)

    fun drop(id: String) {
        val from = currentRows.indexOfFirst { it.id == id }
        if (from >= 0) {
            val to = (from + (dragOffset / rowHeight).roundToInt()).coerceIn(0, currentRows.lastIndex)
            if (to != from) DownloadsLogic.dropIndex(currentAll, currentRows[to].id)?.let { vm.moveTo(id, it) }
        }
        dragId = null
        dragOffset = 0f
    }

    LazyColumn(state = state, modifier = Modifier.fillMaxSize().testTag(DOWNLOADS_TAG)) {
        itemsIndexed(rows, key = { _, t -> t.id }) { index, t ->
            val dragging = dragId == t.id
            DownloadRow(
                t = t,
                selecting = selection.isNotEmpty(),
                selected = t.id in selection,
                onClick = { onClick(t) },
                onLongClick = { onLongClick(t) },
                modifier = Modifier
                    .onSizeChanged { if (it.height > 0) rowHeight = it.height }
                    .then(if (dragging) Modifier.zIndex(1f).graphicsLayer { translationY = dragOffset; shadowElevation = 8f } else Modifier),
                drag = if (!reorder) {
                    null
                } else {
                    remember(t.id, index, rows.size) {
                        DragCallbacks(
                            onStart = { dragId = t.id; dragOffset = 0f },
                            onDrag = { dragOffset += it },
                            onEnd = { drop(t.id) },
                            onCancel = { dragId = null; dragOffset = 0f },
                            onMoveUp = if (index > 0) ({ vm.move(listOf(t.id), QueueStep.Up) }) else null,
                            onMoveDown = if (index < rows.lastIndex) ({ vm.move(listOf(t.id), QueueStep.Down) }) else null,
                        )
                    }
                },
            )
            HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
        }
    }
}

/** The actions on a multi-selection; Reannounce and Force recheck sit under More. */
@Composable
private fun SelectionBar(
    selected: List<TorrentSummary>,
    onRun: (TorrentAction) -> Unit,
    onMove: (QueueStep) -> Unit,
    onBulk: (BulkAction) -> Unit,
    onDelete: () -> Unit,
) {
    val pause = DownloadsLogic.pauseToggle(selected)
    val stop = DownloadsLogic.stopToggle(selected)
    Surface(tonalElevation = 3.dp, shadowElevation = 3.dp) {
        Row(
            Modifier.fillMaxWidth().heightIn(min = 64.dp).padding(horizontal = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            val each = Modifier.weight(1f)
            BarAction(pause.icon(), pause.label(), each) { onRun(pause) }
            BarAction(stop.icon(), stop.label(), each) { onRun(stop) }
            BarAction(TorrentIcons.forceStart, "Force start", each) { onRun(TorrentAction.ForceStart) }
            BarAction(TorrentIcons.up, "Move up", each) { onMove(QueueStep.Up) }
            BarAction(TorrentIcons.down, "Move down", each) { onMove(QueueStep.Down) }
            BarAction(TorrentIcons.delete, "Delete", each) { onDelete() }
            MoreTorrentActions(
                count = selected.size,
                canReannounce = DownloadsLogic.canReannounce(selected),
                canRecheck = DownloadsLogic.canRecheck(selected),
                onBulk = onBulk,
                modifier = each,
            )
        }
    }
}
