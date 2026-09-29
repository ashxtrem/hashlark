// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark.ui.search

import android.view.View
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.draganddrop.dragAndDropSource
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.material.icons.Icons
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
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.isCtrlPressed
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.isSecondaryPressed
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.customActions
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import io.github.ashxtrem.hashlark.core.Format
import io.github.ashxtrem.hashlark.core.MergedResult
import io.github.ashxtrem.hashlark.ui.ResultActions

/** How much a row shows, decided by the width it has (not by the window: it may sit inside a pane). */
enum class RowDensity {
    /** Filename on up to two lines, then size, seeds and sources. */
    Narrow,

    /** The same, in a roomier window: age and provider show on a third line. */
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
    val age = 64.dp
    val sources = 72.dp
    val actions = 56.dp
}

/** `Seeds 812`, `Seeds 0`, or `Seeds unknown`: a provider that does not report seeders is not the same as none. */
fun seedsText(seeders: Int?): String = if (seeders == null) "Seeds unknown" else "Seeds ${Format.count(seeders)}"

fun sourcesText(count: Int): String = if (count == 1) "1 source" else "$count sources"

/** `3d ago`, or null when the provider gave no date. */
fun agoText(published: String?): String? {
    val age = Format.age(published)
    return when (age) {
        Format.UNKNOWN -> null
        "0h" -> "Less than an hour ago"
        else -> "$age ago"
    }
}

@OptIn(ExperimentalFoundationApi::class, ExperimentalComposeUiApi::class)
@Composable
fun ResultRow(
    result: MergedResult,
    density: RowDensity,
    selected: Boolean,
    favorite: Boolean,
    actions: ResultActions,
    providerNames: Map<String, String>,
    showExtras: Boolean,
    onClick: () -> Unit,
    onToggleFavorite: () -> Unit,
    modifier: Modifier = Modifier,
    focusRequester: FocusRequester? = null,
) {
    var menuOpen by remember { mutableStateOf(false) }
    val dragText = remember(result) { actions.dragText(result) }
    val primary = result.primary
    val accent = MaterialTheme.colorScheme.primary
    val description = remember(result) {
        "${primary.title}. ${Format.bytes(primary.sizeBytes)}. ${seedsText(result.seeders)}. ${sourcesText(result.sources.size)}."
    }

    Surface(
        color = if (selected) MaterialTheme.colorScheme.primaryContainer else MaterialTheme.colorScheme.surface,
        modifier = modifier
            .fillMaxWidth()
            .then(if (focusRequester != null) Modifier.focusRequester(focusRequester) else Modifier)
            // The leading bar says "selected" without relying on the background colour.
            .drawBehind {
                if (selected) drawRect(accent, size = Size(SELECTED_BAR.toPx(), size.height))
            }
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
            .combinedClickable(
                onClickLabel = "Show details",
                onClick = onClick,
                onLongClickLabel = "More actions",
                onLongClick = { menuOpen = true },
            )
            .semantics(mergeDescendants = true) {
                contentDescription = description
                this.selected = selected
                // The long-press menu, reachable from TalkBack's actions menu.
                customActions = buildList {
                    add(CustomAccessibilityAction("Open in torrent client") { actions.open(result); true })
                    if (result.hasMagnet) {
                        add(CustomAccessibilityAction("Copy magnet link") { actions.copyMagnet(result); true })
                        add(CustomAccessibilityAction("Share magnet link") { actions.share(result); true })
                    }
                    if (result.hasTorrentFile) add(CustomAccessibilityAction("Save .torrent") { actions.saveTorrent(result); true })
                    primary.detailsUrl?.let { url -> add(CustomAccessibilityAction("View on site") { actions.viewOnSite(url); true }) }
                }
            },
    ) {
        Box {
            when (density) {
                RowDensity.Wide -> WideRow(result, providerNames, favorite, onToggleFavorite)
                else -> CompactRow(result, providerNames, showExtras, favorite, onToggleFavorite)
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

private val SELECTED_BAR = 4.dp

/** The filename first (two lines), the facts in a labelled line, optionally when and where it was published. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun CompactRow(
    result: MergedResult,
    providerNames: Map<String, String>,
    showExtras: Boolean,
    favorite: Boolean,
    onToggleFavorite: () -> Unit,
) {
    val p = result.primary
    Row(
        Modifier.heightIn(min = 64.dp).padding(start = 16.dp, top = 8.dp, bottom = 8.dp, end = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(
                p.title,
                style = MaterialTheme.typography.bodyLarge.copy(fontWeight = FontWeight.Medium),
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
            )
            // Whole facts wrap onto the next line instead of being cut in the middle.
            FlowRow(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                Fact(Format.bytes(p.sizeBytes), strong = true)
                Fact(seedsText(result.seeders))
                Fact(sourcesText(result.sources.size))
            }
            if (showExtras) {
                val provider = providerNames[p.providerId] ?: p.providerId
                Text(
                    listOfNotNull(agoText(p.published), "via $provider").joinToString(" · "),
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
        SaveButton(favorite, onToggleFavorite)
    }
}

@Composable
private fun Fact(text: String, strong: Boolean = false) {
    Text(
        text,
        style = MaterialTheme.typography.bodyMedium,
        color = if (strong) MaterialTheme.colorScheme.onSurface else MaterialTheme.colorScheme.onSurfaceVariant,
    )
}

@Composable
private fun WideRow(
    result: MergedResult,
    providerNames: Map<String, String>,
    favorite: Boolean,
    onToggleFavorite: () -> Unit,
) {
    val p = result.primary
    Row(
        Modifier.heightIn(min = 56.dp).padding(start = 16.dp, top = 4.dp, bottom = 4.dp, end = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f).padding(end = 12.dp)) {
            // One line in a table: the middle is cut, so the end (release group, resolution) stays visible.
            Text(
                p.title,
                style = MaterialTheme.typography.bodyLarge.copy(fontWeight = FontWeight.Medium),
                maxLines = 1,
                overflow = TextOverflow.MiddleEllipsis,
            )
            Text(
                listOfNotNull(p.category?.label ?: "Other", providerNames[p.providerId] ?: p.providerId).joinToString(" · "),
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
        Cell(Format.bytes(p.sizeBytes), TableColumns.size)
        Cell(Format.count(result.seeders), TableColumns.seeders, color = MaterialTheme.colorScheme.primary)
        Cell(Format.count(result.leechers), TableColumns.peers)
        Cell(Format.age(p.published), TableColumns.age)
        Cell(result.sources.size.toString(), TableColumns.sources)
        Box(Modifier.width(TableColumns.actions), contentAlignment = Alignment.CenterEnd) { SaveButton(favorite, onToggleFavorite) }
    }
}

@Composable
private fun Cell(text: String, width: Dp, color: Color = Color.Unspecified) {
    Text(
        text,
        modifier = Modifier.width(width),
        textAlign = TextAlign.End,
        style = MaterialTheme.typography.bodyMedium,
        color = color,
        maxLines = 1,
    )
}

/** The one save action of a row; everything else is in the details and the long-press menu. */
@Composable
private fun SaveButton(favorite: Boolean, onToggleFavorite: () -> Unit) {
    IconButton(onClick = onToggleFavorite) {
        Icon(
            if (favorite) Icons.Filled.Star else Icons.Outlined.StarBorder,
            contentDescription = if (favorite) "Remove from favourites" else "Save to favourites",
            tint = if (favorite) MaterialTheme.colorScheme.tertiary else MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
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
