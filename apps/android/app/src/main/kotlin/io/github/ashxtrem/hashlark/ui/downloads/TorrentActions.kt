// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark.ui.downloads

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.ArrowDownward
import androidx.compose.material.icons.filled.ArrowUpward
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.FastForward
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.Pause
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material.icons.filled.Stop
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.ui.draw.clip
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.github.ashxtrem.hashlark.core.BulkAction
import io.github.ashxtrem.hashlark.core.TorrentAction

/** Icons of the torrent actions, shared by the selection bar and the detail sheet. */
object TorrentIcons {
    val pause: ImageVector = Icons.Filled.Pause
    val resume: ImageVector = Icons.Filled.PlayArrow
    val stop: ImageVector = Icons.Filled.Stop
    val start: ImageVector = Icons.Filled.PlayArrow
    val forceStart: ImageVector = Icons.Filled.FastForward
    val up: ImageVector = Icons.Filled.ArrowUpward
    val down: ImageVector = Icons.Filled.ArrowDownward
    val delete: ImageVector = Icons.Filled.Delete
    val more: ImageVector = Icons.Filled.MoreVert
}

fun TorrentAction.label(): String = when (this) {
    TorrentAction.Pause -> "Pause"
    TorrentAction.Resume -> "Resume"
    TorrentAction.Stop -> "Stop"
    TorrentAction.Start -> "Start"
    TorrentAction.ForceStart -> "Force start"
}

fun TorrentAction.icon(): ImageVector = when (this) {
    TorrentAction.Pause -> TorrentIcons.pause
    TorrentAction.Resume -> TorrentIcons.resume
    TorrentAction.Stop -> TorrentIcons.stop
    TorrentAction.Start -> TorrentIcons.start
    TorrentAction.ForceStart -> TorrentIcons.forceStart
}

/** An icon with its label under it, so the action is clear without a tooltip. Seven fit a 360 dp bar. */
@Composable
fun BarAction(icon: ImageVector, label: String, modifier: Modifier = Modifier, onClick: () -> Unit) {
    Column(
        modifier
            .heightIn(min = 56.dp)
            .widthIn(min = 56.dp)
            .clip(MaterialTheme.shapes.medium)
            .clickable(role = Role.Button, onClick = onClick)
            .padding(horizontal = 2.dp, vertical = 6.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.Center,
    ) {
        Icon(icon, contentDescription = null, tint = MaterialTheme.colorScheme.primary)
        Text(
            label,
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.primary,
            maxLines = 2,
            textAlign = TextAlign.Center,
            overflow = TextOverflow.Ellipsis,
            modifier = Modifier.padding(top = 2.dp),
        )
    }
}

/**
 * "More" with Reannounce and Force recheck, for the selection bar and the detail sheet.
 * Force recheck asks first; [count] is how many torrents it reaches.
 */
@Composable
fun MoreTorrentActions(
    count: Int,
    canReannounce: Boolean,
    canRecheck: Boolean,
    onBulk: (BulkAction) -> Unit,
    modifier: Modifier = Modifier,
) {
    var menu by rememberSaveable { mutableStateOf(false) }
    var confirming by rememberSaveable { mutableStateOf(false) }
    Box(modifier) {
        BarAction(TorrentIcons.more, "More", Modifier.fillMaxWidth()) { menu = true }
        DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
            DropdownMenuItem(
                text = { Text("Reannounce") },
                enabled = canReannounce,
                onClick = {
                    menu = false
                    onBulk(BulkAction.Reannounce)
                },
            )
            DropdownMenuItem(
                text = { Text("Force recheck…") },
                enabled = canRecheck,
                onClick = {
                    menu = false
                    confirming = true
                },
            )
        }
    }
    if (confirming) {
        RecheckTorrentDialog(
            count = count,
            onConfirm = {
                confirming = false
                onBulk(BulkAction.Recheck)
            },
            onDismiss = { confirming = false },
        )
    }
}

/** Confirms Force recheck: it re-reads every downloaded byte, which can take minutes. */
@Composable
fun RecheckTorrentDialog(count: Int, onConfirm: () -> Unit, onDismiss: () -> Unit) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Force recheck") },
        text = {
            Text(
                "Re-verify all downloaded data of ${if (count == 1) "1 torrent" else "$count torrents"}? " +
                    "Downloading waits until the check finishes.",
            )
        },
        confirmButton = { TextButton(onClick = onConfirm) { Text("Recheck") } },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel") } },
    )
}
