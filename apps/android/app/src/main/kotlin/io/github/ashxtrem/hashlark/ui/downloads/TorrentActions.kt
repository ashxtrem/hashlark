// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark.ui.downloads

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.ArrowDownward
import androidx.compose.material.icons.filled.ArrowUpward
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.FastForward
import androidx.compose.material.icons.filled.Pause
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material.icons.filled.Stop
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.ui.draw.clip
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
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

/** An icon with its label under it, so the action is clear without a tooltip. Six fit a 360 dp bar. */
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
