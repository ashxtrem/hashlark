// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark.ui.search

import android.view.View
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.draganddrop.dragAndDropSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.ContentCopy
import androidx.compose.material.icons.filled.Download
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.Star
import androidx.compose.material.icons.outlined.StarBorder
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.ExperimentalComposeUiApi
import androidx.compose.ui.Modifier
import androidx.compose.ui.draganddrop.DragAndDropTransferData
import androidx.compose.ui.draganddrop.mimeTypes
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.isCtrlPressed
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.isSecondaryPressed
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.github.ashxtrem.hashlark.core.Format
import io.github.ashxtrem.hashlark.core.MergedResult
import io.github.ashxtrem.hashlark.ui.ResultActions

/** How much a row shows, decided by the width it has (not by the window: it may sit inside a pane). */
enum class RowDensity {
    /** Title on two lines, then one meta line. */
    Narrow,

    /** Title on one line with the meta line below and action icons on the right. */
    Medium,

    /** A table row with a column each for size, seeders, peers, age and sources. */
    Wide,
    ;

    companion object {
        fun forWidth(widthDp: Float): RowDensity = when {
            widthDp < 440f -> Narrow
            widthDp < 640f -> Medium
            else -> Wide
        }
    }
}

/** Column widths of the wide table, shared by the header and the rows. */
object TableColumns {
    val size = 84.dp
    val seeders = 72.dp
    val peers = 72.dp
    val age = 56.dp
    val sources = 64.dp
    val actions = 132.dp
}

@OptIn(ExperimentalFoundationApi::class, ExperimentalComposeUiApi::class)
@Composable
fun ResultRow(
    result: MergedResult,
    density: RowDensity,
    selected: Boolean,
    favorite: Boolean,
    actions: ResultActions,
    onClick: () -> Unit,
    onToggleFavorite: () -> Unit,
    modifier: Modifier = Modifier,
) {
    var menuOpen by remember { mutableStateOf(false) }
    val dragText = remember(result) { actions.dragText(result) }
    val primary = result.primary

    Surface(
        color = if (selected) MaterialTheme.colorScheme.primaryContainer else MaterialTheme.colorScheme.surface,
        modifier = modifier
            .fillMaxWidth()
            // Keyboard: Ctrl+C copies the magnet of the focused row.
            .onPreviewKeyEvent { event ->
                if (event.type == KeyEventType.KeyDown && event.isCtrlPressed && event.key == Key.C) {
                    actions.copyMagnetOffline(result)
                    true
                } else {
                    false
                }
            }
            // Mouse and trackpad: right-click opens the same menu as long-press.
            .pointerInput(Unit) {
                awaitEachGesture {
                    val down = awaitFirstDown(requireUnconsumed = false, pass = PointerEventPass.Initial)
                    if (down.type == androidx.compose.ui.input.pointer.PointerType.Mouse &&
                        currentEvent.buttons.isSecondaryPressed
                    ) {
                        down.consume()
                        menuOpen = true
                    }
                }
            }
            // Drag the magnet into another app in split screen (a torrent client, a notes app).
            .then(
                if (dragText == null) {
                    Modifier
                } else {
                    Modifier.dragAndDropSource(transferData = { _ ->
                        DragAndDropTransferData(
                            clipData = android.content.ClipData.newPlainText("Magnet link", dragText),
                            flags = View.DRAG_FLAG_GLOBAL,
                        )
                    })
                },
            )
            .combinedClickable(onClick = onClick, onLongClick = { menuOpen = true })
            .semantics(mergeDescendants = true) {
                contentDescription = "${primary.title}, ${Format.bytes(primary.sizeBytes)}, ${Format.count(result.seeders)} seeders"
            },
    ) {
        Box {
            when (density) {
                RowDensity.Narrow -> NarrowRow(result, favorite, actions, onToggleFavorite) { menuOpen = true }
                RowDensity.Medium -> MediumRow(result, favorite, actions, onToggleFavorite) { menuOpen = true }
                RowDensity.Wide -> WideRow(result, favorite, actions, onToggleFavorite) { menuOpen = true }
            }
            ResultMenu(
                expanded = menuOpen,
                onDismiss = { menuOpen = false },
                result = result,
                favorite = favorite,
                actions = actions,
                onToggleFavorite = onToggleFavorite,
                onDetails = onClick,
            )
        }
    }
}

@Composable
private fun NarrowRow(
    result: MergedResult,
    favorite: Boolean,
    actions: ResultActions,
    onToggleFavorite: () -> Unit,
    onMenu: () -> Unit,
) {
    Row(Modifier.padding(start = 16.dp, top = 8.dp, bottom = 8.dp, end = 4.dp), verticalAlignment = Alignment.CenterVertically) {
        Column(Modifier.weight(1f)) {
            Text(result.primary.title, style = MaterialTheme.typography.bodyLarge, maxLines = 2, overflow = TextOverflow.Ellipsis)
            MetaLine(result)
        }
        IconButton(onClick = { actions.open(result) }) {
            Icon(Icons.Filled.Download, contentDescription = "Open in torrent client", tint = MaterialTheme.colorScheme.primary)
        }
        IconButton(onClick = onMenu) { Icon(Icons.Filled.MoreVert, contentDescription = "More actions") }
    }
}

@Composable
private fun MediumRow(
    result: MergedResult,
    favorite: Boolean,
    actions: ResultActions,
    onToggleFavorite: () -> Unit,
    onMenu: () -> Unit,
) {
    Row(Modifier.padding(start = 16.dp, top = 6.dp, bottom = 6.dp, end = 4.dp), verticalAlignment = Alignment.CenterVertically) {
        Column(Modifier.weight(1f)) {
            Text(result.primary.title, style = MaterialTheme.typography.bodyLarge, maxLines = 1, overflow = TextOverflow.Ellipsis)
            MetaLine(result)
        }
        RowActions(result, favorite, actions, onToggleFavorite, onMenu)
    }
}

@Composable
private fun WideRow(
    result: MergedResult,
    favorite: Boolean,
    actions: ResultActions,
    onToggleFavorite: () -> Unit,
    onMenu: () -> Unit,
) {
    val p = result.primary
    Row(Modifier.padding(start = 16.dp, top = 4.dp, bottom = 4.dp, end = 4.dp), verticalAlignment = Alignment.CenterVertically) {
        Column(Modifier.weight(1f).padding(end = 12.dp)) {
            Text(p.title, style = MaterialTheme.typography.bodyMedium, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(
                p.category?.label ?: "Other",
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        Cell(Format.bytes(p.sizeBytes), TableColumns.size)
        Cell(Format.count(result.seeders), TableColumns.seeders, color = MaterialTheme.colorScheme.primary)
        Cell(Format.count(result.leechers), TableColumns.peers)
        Cell(Format.age(p.published), TableColumns.age)
        Cell(result.sources.size.toString(), TableColumns.sources)
        Box(Modifier.width(TableColumns.actions), contentAlignment = Alignment.CenterEnd) {
            RowActions(result, favorite, actions, onToggleFavorite, onMenu)
        }
    }
}

@Composable
private fun Cell(text: String, width: androidx.compose.ui.unit.Dp, color: androidx.compose.ui.graphics.Color = androidx.compose.ui.graphics.Color.Unspecified) {
    Text(
        text,
        modifier = Modifier.width(width),
        textAlign = TextAlign.End,
        style = MaterialTheme.typography.bodyMedium,
        color = color,
        maxLines = 1,
    )
}

@Composable
private fun RowActions(
    result: MergedResult,
    favorite: Boolean,
    actions: ResultActions,
    onToggleFavorite: () -> Unit,
    onMenu: () -> Unit,
) {
    Row {
        IconButton(onClick = { actions.open(result) }) {
            Icon(Icons.Filled.Download, contentDescription = "Open in torrent client", tint = MaterialTheme.colorScheme.primary)
        }
        IconButton(onClick = onToggleFavorite) {
            Icon(
                if (favorite) Icons.Filled.Star else Icons.Outlined.StarBorder,
                contentDescription = if (favorite) "Remove from favourites" else "Save to favourites",
                tint = if (favorite) MaterialTheme.colorScheme.tertiary else MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        IconButton(onClick = onMenu) { Icon(Icons.Filled.MoreVert, contentDescription = "More actions") }
    }
}

/** `5.7 GiB · ↑ 812 · 3d · 2 sources`. */
@Composable
private fun MetaLine(result: MergedResult) {
    val p = result.primary
    val parts = buildList {
        add(Format.bytes(p.sizeBytes))
        add("↑ ${Format.count(result.seeders)}")
        add("↓ ${Format.count(result.leechers)}")
        add(Format.age(p.published))
        add(if (result.sources.size == 1) "1 source" else "${result.sources.size} sources")
    }
    Text(
        parts.joinToString("  ·  "),
        style = MaterialTheme.typography.bodySmall,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        maxLines = 1,
        overflow = TextOverflow.Ellipsis,
    )
}

@Composable
private fun ResultMenu(
    expanded: Boolean,
    onDismiss: () -> Unit,
    result: MergedResult,
    favorite: Boolean,
    actions: ResultActions,
    onToggleFavorite: () -> Unit,
    onDetails: () -> Unit,
) {
    DropdownMenu(expanded = expanded, onDismissRequest = onDismiss) {
        MenuItem("Open in torrent client", onDismiss) { actions.open(result) }
        MenuItem("Details", onDismiss, onDetails)
        if (result.hasMagnet) {
            MenuItem("Copy magnet link", onDismiss) { actions.copyMagnet(result) }
            MenuItem("Share magnet link", onDismiss) { actions.share(result) }
        }
        if (result.hasTorrentFile) {
            MenuItem("Save .torrent", onDismiss) { actions.saveTorrent(result) }
            MenuItem("Save .torrent as…", onDismiss) { actions.saveTorrent(result, saveAs = true) }
        }
        MenuItem(if (favorite) "Remove from favourites" else "Save to favourites", onDismiss, onToggleFavorite)
        result.primary.detailsUrl?.let { url -> MenuItem("View on site", onDismiss) { actions.viewOnSite(url) } }
    }
}

@Composable
private fun MenuItem(text: String, dismiss: () -> Unit, action: () -> Unit) {
    DropdownMenuItem(text = { Text(text) }, onClick = {
        dismiss()
        action()
    })
}
