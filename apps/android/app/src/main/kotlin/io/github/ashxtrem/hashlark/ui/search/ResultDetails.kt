// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark.ui.search

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.filled.OpenInNew
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.ContentCopy
import androidx.compose.material.icons.filled.Download
import androidx.compose.material.icons.filled.ExpandLess
import androidx.compose.material.icons.filled.ExpandMore
import androidx.compose.material.icons.filled.FileDownload
import androidx.compose.material.icons.filled.Share
import androidx.compose.material.icons.filled.Star
import androidx.compose.material.icons.outlined.StarBorder
import androidx.compose.material3.Button
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.paneTitle
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import io.github.ashxtrem.hashlark.core.Format
import io.github.ashxtrem.hashlark.core.MergedResult
import io.github.ashxtrem.hashlark.ui.ResultActions
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.time.format.FormatStyle

private const val NOT_REPORTED = "Not reported"

/**
 * Everything known about one result, with the actions that apply to it. [onBack] is set when the
 * details replace the list (a Back arrow) or sit beside it ([asPane]: a Close button).
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun ResultDetails(
    result: MergedResult,
    favorite: Boolean,
    names: Map<String, String>,
    actions: ResultActions,
    onToggleFavorite: () -> Unit,
    modifier: Modifier = Modifier,
    onBack: (() -> Unit)? = null,
    asPane: Boolean = false,
) {
    val p = result.primary
    var technical by rememberSaveable(result.id) { mutableStateOf(false) }
    Surface(modifier.fillMaxSize().semantics { paneTitle = "Result details" }, color = MaterialTheme.colorScheme.surface) {
        Column(Modifier.fillMaxSize()) {
            // Pinned: Back or Close is always in reach, however far the details are scrolled.
            if (onBack != null) {
                Row(Modifier.padding(start = 4.dp, end = 16.dp), verticalAlignment = Alignment.CenterVertically) {
                    IconButton(onClick = onBack) {
                        if (asPane) {
                            Icon(Icons.Filled.Close, contentDescription = "Close details")
                        } else {
                            Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back to results")
                        }
                    }
                    Text("Details", style = MaterialTheme.typography.titleMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
            }
            Column(
                Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(start = 16.dp, end = 16.dp, top = if (onBack == null) 16.dp else 4.dp, bottom = 24.dp),
                verticalArrangement = Arrangement.spacedBy(16.dp),
            ) {
                Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    // The complete original filename, selectable so it can be copied.
                    SelectionContainer { Text(p.title, style = MaterialTheme.typography.titleLarge) }
                    Text(
                        "${p.category?.label ?: "Category not reported"} · ${publishedLine(p.published)}",
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }

                Button(
                    onClick = { actions.open(result) },
                    modifier = Modifier.fillMaxWidth().heightIn(min = 52.dp),
                ) {
                    Icon(Icons.Filled.Download, contentDescription = null)
                    Text("Open in client", Modifier.padding(start = 8.dp))
                }

                // Only what this result supports: a result without a magnet has no copy or share for it.
                FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    OutlinedButton(onClick = onToggleFavorite) {
                        Icon(if (favorite) Icons.Filled.Star else Icons.Outlined.StarBorder, contentDescription = null)
                        Text(if (favorite) "Saved" else "Save", Modifier.padding(start = 8.dp))
                    }
                    if (result.hasMagnet) {
                        OutlinedButton(onClick = { actions.copyMagnet(result) }) {
                            Icon(Icons.Filled.ContentCopy, contentDescription = null)
                            Text("Copy magnet", Modifier.padding(start = 8.dp))
                        }
                        OutlinedButton(onClick = { actions.share(result) }) {
                            Icon(Icons.Filled.Share, contentDescription = null)
                            Text("Share", Modifier.padding(start = 8.dp))
                        }
                    }
                    if (result.hasTorrentFile) {
                        OutlinedButton(onClick = { actions.saveTorrent(result) }) {
                            Icon(Icons.Filled.FileDownload, contentDescription = null)
                            Text("Save .torrent", Modifier.padding(start = 8.dp))
                        }
                        OutlinedButton(onClick = { actions.saveTorrent(result, saveAs = true) }) {
                            Text("Save .torrent as…")
                        }
                    }
                }

                Surface(shape = MaterialTheme.shapes.large, color = MaterialTheme.colorScheme.surfaceContainerLow, border = androidx.compose.foundation.BorderStroke(1.dp, MaterialTheme.colorScheme.outlineVariant)) {
                    Column(Modifier.fillMaxWidth().padding(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                        Fact("Size", p.sizeBytes?.let { Format.bytes(it) } ?: NOT_REPORTED)
                        Fact("Seeds", result.seeders?.let { Format.count(it) } ?: NOT_REPORTED)
                        Fact("Peers", result.leechers?.let { Format.count(it) } ?: NOT_REPORTED)
                        Fact("Sources", result.sources.size.toString())
                    }
                }

                Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    Text("Found on", style = MaterialTheme.typography.titleSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    result.sources.forEach { source ->
                        Surface(shape = MaterialTheme.shapes.medium, color = MaterialTheme.colorScheme.surfaceContainerHigh) {
                            Row(
                                Modifier.fillMaxWidth().heightIn(min = 48.dp).padding(start = 12.dp, end = 4.dp),
                                verticalAlignment = Alignment.CenterVertically,
                            ) {
                                Text(names[source] ?: source, Modifier.weight(1f))
                                if (source == p.providerId && p.detailsUrl != null) {
                                    TextButton(onClick = { actions.viewOnSite(p.detailsUrl!!) }) {
                                        Text("View on site")
                                        Icon(Icons.AutoMirrored.Filled.OpenInNew, contentDescription = null, Modifier.padding(start = 4.dp).size(16.dp))
                                    }
                                }
                            }
                        }
                    }
                }

                if (p.infoHash != null || p.magnet != null || p.torrentUrl != null || p.needsResolve) {
                    Column {
                        Row(
                            Modifier.fillMaxWidth().heightIn(min = 48.dp)
                                .clickable(role = Role.Button, onClickLabel = if (technical) "Hide technical information" else "Show technical information") { technical = !technical }
                                .semantics { stateDescription = if (technical) "Expanded" else "Collapsed" },
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            Text("Technical information", Modifier.weight(1f), style = MaterialTheme.typography.titleSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                            Icon(if (technical) Icons.Filled.ExpandLess else Icons.Filled.ExpandMore, contentDescription = null)
                        }
                        if (technical) {
                            Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                                p.infoHash?.let { hash ->
                                    Row(verticalAlignment = Alignment.CenterVertically) {
                                        Column(Modifier.weight(1f)) {
                                            Text("Info hash", style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
                                            SelectionContainer { Text(hash, fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodyMedium) }
                                        }
                                        IconButton(onClick = { actions.copyText("Infohash", hash) }) {
                                            Icon(Icons.Filled.ContentCopy, contentDescription = "Copy info hash")
                                        }
                                    }
                                }
                                Fact("Magnet link", if (result.hasMagnet) (if (p.magnet != null) "Included in the result" else "Built when you open or copy it") else "None")
                                Fact(".torrent file", if (result.hasTorrentFile) "Available" else "None")
                                Fact("Provider", names[p.providerId] ?: p.providerId)
                            }
                        }
                    }
                }
            }
        }
    }
}

/** A label and its value; the label takes the smaller share so large text wraps instead of overflowing. */
@Composable
private fun Fact(label: String, value: String) {
    Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        Text(label, Modifier.weight(0.4f), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
        Text(value, Modifier.weight(0.6f), style = MaterialTheme.typography.bodyMedium)
    }
}

private fun publishedLine(iso: String?): String {
    val instant = Format.parseInstant(iso) ?: return "Publication date not reported"
    val date = DateTimeFormatter.ofLocalizedDate(FormatStyle.MEDIUM).withZone(ZoneId.systemDefault()).format(instant)
    return "Published $date${agoText(iso)?.let { " ($it)" }.orEmpty()}"
}
