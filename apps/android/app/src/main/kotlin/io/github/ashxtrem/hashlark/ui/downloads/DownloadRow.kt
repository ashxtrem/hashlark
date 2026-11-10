// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark.ui.downloads

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.filled.DragHandle
import androidx.compose.material.icons.outlined.Circle
import androidx.compose.material3.Icon
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.customActions
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.github.ashxtrem.hashlark.core.Format
import io.github.ashxtrem.hashlark.core.TorrentState
import io.github.ashxtrem.hashlark.core.TorrentSummary
import io.github.ashxtrem.hashlark.ui.theme.LocalStatusColors

/** What a drag handle reports: started, moved by a number of pixels, ended, cancelled. */
class DragCallbacks(
    val onStart: () -> Unit,
    val onDrag: (Float) -> Unit,
    val onEnd: () -> Unit,
    val onCancel: () -> Unit,
    val onMoveUp: (() -> Unit)?,
    val onMoveDown: (() -> Unit)?,
)

/** One torrent: name, progress, and a line with the status, size, speeds and time left. */
@OptIn(ExperimentalFoundationApi::class)
@Composable
fun DownloadRow(
    t: TorrentSummary,
    selecting: Boolean,
    selected: Boolean,
    onClick: () -> Unit,
    onLongClick: () -> Unit,
    modifier: Modifier = Modifier,
    drag: DragCallbacks? = null,
) {
    val status = DownloadsLogic.statusLabel(t)
    val tone = toneOf(t.state)
    val line = buildList {
        add(DownloadsLogic.percent(t.progress))
        add(if (t.size > 0) "${Format.bytes(t.have)} of ${Format.bytes(t.size)}" else "Size unknown")
        if (t.downBps > 0) add("↓ ${DownloadsLogic.speed(t.downBps)}")
        if (t.upBps > 0) add("↑ ${DownloadsLogic.speed(t.upBps)}")
        if (t.state == TorrentState.Downloading && t.etaSecs != null) add("${DownloadsLogic.eta(t.etaSecs)} left")
    }.joinToString(" · ")
    Surface(
        color = if (selected) MaterialTheme.colorScheme.primaryContainer else MaterialTheme.colorScheme.surface,
        modifier = modifier
            .fillMaxWidth()
            .combinedClickable(
                onClickLabel = if (selecting) "Select" else "Show details",
                onClick = onClick,
                onLongClickLabel = "Select",
                onLongClick = onLongClick,
            )
            .semantics(mergeDescendants = true) {
                contentDescription = "${t.name}. $status. $line."
                this.selected = selected
            },
    ) {
        Row(
            Modifier.heightIn(min = 72.dp).padding(start = 16.dp, end = 4.dp, top = 8.dp, bottom = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (selecting) {
                Icon(
                    if (selected) Icons.Filled.CheckCircle else Icons.Outlined.Circle,
                    contentDescription = null,
                    tint = if (selected) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.outline,
                    modifier = Modifier.padding(end = 12.dp).size(24.dp),
                )
            }
            Column(Modifier.weight(1f).padding(end = 8.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Text(
                    t.name,
                    style = MaterialTheme.typography.bodyLarge.copy(fontWeight = FontWeight.Medium),
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )
                LinearProgressIndicator(
                    progress = { t.progress.coerceIn(0.0, 1.0).toFloat() },
                    modifier = Modifier.fillMaxWidth(),
                    color = if (tone == Color.Unspecified) MaterialTheme.colorScheme.primary else tone,
                )
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text(
                        status,
                        style = MaterialTheme.typography.bodyMedium.copy(fontWeight = FontWeight.Medium),
                        color = if (tone == Color.Unspecified) MaterialTheme.colorScheme.onSurfaceVariant else tone,
                        maxLines = 1,
                    )
                    Text(
                        line,
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        maxLines = 2,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
                t.error?.let {
                    Text(it, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.error, maxLines = 2, overflow = TextOverflow.Ellipsis)
                }
            }
            if (drag != null) DragHandle(drag)
        }
    }
}

@Composable
private fun DragHandle(drag: DragCallbacks) {
    Box(
        Modifier
            .size(48.dp)
            .pointerInput(drag) {
                detectDragGestures(
                    onDragStart = { drag.onStart() },
                    onDragEnd = { drag.onEnd() },
                    onDragCancel = { drag.onCancel() },
                    onDrag = { change, amount ->
                        change.consume()
                        drag.onDrag(amount.y)
                    },
                )
            }
            .semantics {
                contentDescription = "Reorder"
                // Dragging needs a pointer; screen readers move one place at a time instead.
                customActions = listOfNotNull(
                    drag.onMoveUp?.let { up -> CustomAccessibilityAction("Move up") { up(); true } },
                    drag.onMoveDown?.let { down -> CustomAccessibilityAction("Move down") { down(); true } },
                )
            },
        contentAlignment = Alignment.Center,
    ) {
        Icon(Icons.Filled.DragHandle, contentDescription = null, tint = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

/** The colour of a state: errors red, held-back states amber, finished green, the rest default. */
@Composable
private fun toneOf(state: TorrentState): Color {
    val status = LocalStatusColors.current
    return when (state) {
        TorrentState.Error -> MaterialTheme.colorScheme.error
        TorrentState.Seeding, TorrentState.Finished, TorrentState.Done -> status.ok
        TorrentState.ScheduledOff, TorrentState.ScheduledSeedOnly, TorrentState.OnBattery, TorrentState.WaitingForNetwork -> status.warn
        else -> Color.Unspecified
    }
}
