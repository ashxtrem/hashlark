// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark.ui.search

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.filled.OpenInNew
import androidx.compose.material.icons.filled.ContentCopy
import androidx.compose.material.icons.filled.Download
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
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import io.github.ashxtrem.hashlark.core.Format
import io.github.ashxtrem.hashlark.core.MergedResult
import io.github.ashxtrem.hashlark.ui.ResultActions
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.time.format.FormatStyle

/** Everything known about one result, with the actions that apply to it. */
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
) {
    val p = result.primary
    Surface(modifier.fillMaxSize(), color = MaterialTheme.colorScheme.surface) {
        Column(Modifier.verticalScroll(rememberScrollState()).padding(16.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            Row(verticalAlignment = Alignment.Top) {
                if (onBack != null) {
                    IconButton(onClick = onBack) { Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back to results") }
                }
                Text(p.title, style = MaterialTheme.typography.titleMedium, modifier = Modifier.weight(1f).padding(top = if (onBack != null) 12.dp else 0.dp))
            }

            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Button(onClick = { actions.open(result) }) {
                    Icon(Icons.Filled.Download, contentDescription = null)
                    Text("Open in client", Modifier.padding(start = 8.dp))
                }
                OutlinedButton(onClick = { actions.copyMagnet(result) }, enabled = result.hasMagnet) {
                    Icon(Icons.Filled.ContentCopy, contentDescription = null)
                    Text("Copy magnet", Modifier.padding(start = 8.dp))
                }
                OutlinedButton(onClick = { actions.saveTorrent(result) }, enabled = result.hasTorrentFile) {
                    Icon(Icons.Filled.FileDownload, contentDescription = null)
                    Text(".torrent", Modifier.padding(start = 8.dp))
                }
                OutlinedButton(onClick = onToggleFavorite) {
                    Icon(if (favorite) Icons.Filled.Star else Icons.Outlined.StarBorder, contentDescription = null)
                    Text(if (favorite) "Saved" else "Save", Modifier.padding(start = 8.dp))
                }
                OutlinedButton(onClick = { actions.share(result) }, enabled = result.hasMagnet) {
                    Icon(Icons.Filled.Share, contentDescription = null)
                    Text("Share", Modifier.padding(start = 8.dp))
                }
            }

            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Fact("Size", Format.bytes(p.sizeBytes))
                Fact("Seeds / peers", "${Format.count(result.seeders)} / ${Format.count(result.leechers)}")
                Fact("Published", published(p.published))
                Fact("Category", p.category?.label ?: Format.UNKNOWN)
                p.infoHash?.let { hash ->
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Text("Infohash", Modifier.width(112.dp), color = MaterialTheme.colorScheme.onSurfaceVariant)
                        Text(hash, Modifier.weight(1f), fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodySmall)
                        IconButton(onClick = {
                            actions.copyText("Infohash", hash)
                        }) { Icon(Icons.Filled.ContentCopy, contentDescription = "Copy infohash") }
                    }
                }
            }

            Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
                Text("Found on", style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.onSurfaceVariant)
                result.sources.forEach { source ->
                    Surface(shape = MaterialTheme.shapes.medium, color = MaterialTheme.colorScheme.surfaceContainerHigh) {
                        Row(Modifier.fillMaxWidth().padding(start = 12.dp, end = 4.dp), verticalAlignment = Alignment.CenterVertically) {
                            Text(names[source] ?: source, Modifier.weight(1f))
                            if (source == p.providerId && p.detailsUrl != null) {
                                TextButton(onClick = { actions.viewOnSite(p.detailsUrl!!) }) {
                                    Text("View on site")
                                    Icon(Icons.AutoMirrored.Filled.OpenInNew, contentDescription = null, Modifier.padding(start = 4.dp).width(16.dp))
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun Fact(label: String, value: String) {
    Row {
        Text(label, Modifier.width(112.dp), color = MaterialTheme.colorScheme.onSurfaceVariant)
        Text(value, Modifier.weight(1f))
    }
}

private fun published(iso: String?): String {
    val instant = Format.parseInstant(iso) ?: return Format.UNKNOWN
    return DateTimeFormatter.ofLocalizedDateTime(FormatStyle.MEDIUM).withZone(ZoneId.systemDefault()).format(instant)
}
