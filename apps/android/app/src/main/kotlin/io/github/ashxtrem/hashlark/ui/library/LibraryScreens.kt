// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark.ui.library

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.DeleteSweep
import androidx.compose.material3.Button
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.github.ashxtrem.hashlark.core.HistoryEntry
import io.github.ashxtrem.hashlark.core.SearchQuery
import io.github.ashxtrem.hashlark.core.SortDirection
import io.github.ashxtrem.hashlark.core.SortOrder
import io.github.ashxtrem.hashlark.ui.AppViewModel
import io.github.ashxtrem.hashlark.ui.LibraryViewModel
import io.github.ashxtrem.hashlark.ui.ResultActions
import io.github.ashxtrem.hashlark.ui.common.AdaptiveListDetail
import io.github.ashxtrem.hashlark.ui.common.CenteredMessage
import io.github.ashxtrem.hashlark.ui.common.ConfirmDialog
import io.github.ashxtrem.hashlark.ui.common.LoadingBox
import io.github.ashxtrem.hashlark.ui.common.ScreenTitle
import io.github.ashxtrem.hashlark.ui.search.ResultDetails
import io.github.ashxtrem.hashlark.ui.search.ResultsList
import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.time.format.FormatStyle

/** Saved results: a list with the details of the selected one beside it. */
@Composable
fun FavoritesScreen(
    app: AppViewModel,
    library: LibraryViewModel,
    actions: ResultActions,
    modifier: Modifier = Modifier,
) {
    val favorites by library.favorites.collectAsStateWithLifecycle()
    val names by app.providerNames.collectAsStateWithLifecycle()
    val ids by library.favoriteIds.collectAsStateWithLifecycle()
    var selected by rememberSaveable { mutableStateOf<String?>(null) }
    val listState = rememberLazyListState()
    LaunchedEffect(Unit) { library.refreshFavorites() }

    Column(modifier.fillMaxSize()) {
        ScreenTitle("Favourites")
        val list = favorites
        when {
            list == null -> LoadingBox()
            list.isEmpty() -> CenteredMessage(
                "No favourites yet",
                body = "Tap the star on a result to keep it here, even after the search is gone.",
            )
            else -> AdaptiveListDetail(
                selected = selected,
                onDeselect = { selected = null },
                list = {
                    ResultsList(
                        results = list.map { it.result },
                        selectedId = selected,
                        favoriteIds = ids,
                        order = SortOrder.Date,
                        direction = SortDirection.Descending,
                        actions = actions,
                        onSort = {},
                        onSelect = { selected = it.id },
                        onToggleFavorite = library::toggleFavorite,
                        state = listState,
                    )
                },
                detail = { key, onBack ->
                    val result = list.firstOrNull { it.result.id == key }?.result
                    if (result == null) {
                        CenteredMessage("This favourite was removed")
                    } else {
                        ResultDetails(
                            result = result,
                            favorite = result.id in ids,
                            names = names,
                            actions = actions,
                            onToggleFavorite = { library.toggleFavorite(result) },
                            onBack = onBack,
                        )
                    }
                },
                placeholder = "Select a favourite",
            )
        }
    }
}

/** Past searches; tapping one runs it again. */
@Composable
fun HistoryScreen(
    library: LibraryViewModel,
    onRun: (SearchQuery) -> Unit,
    modifier: Modifier = Modifier,
) {
    val history by library.history.collectAsStateWithLifecycle()
    var selected by rememberSaveable { mutableStateOf<String?>(null) }
    var confirmClear by rememberSaveable { mutableStateOf(false) }
    LaunchedEffect(Unit) { library.refreshHistory() }

    Column(modifier.fillMaxSize()) {
        ScreenTitle("History") {
            if (!history.isNullOrEmpty()) {
                IconButton(onClick = { confirmClear = true }) { Icon(Icons.Filled.DeleteSweep, contentDescription = "Clear history") }
            }
        }
        val entries = history
        when {
            entries == null -> LoadingBox()
            entries.isEmpty() -> CenteredMessage(
                "No searches yet",
                body = "Your searches show up here. They stay on this device, and you can turn history off in Settings.",
            )
            else -> AdaptiveListDetail(
                selected = selected,
                onDeselect = { selected = null },
                list = {
                    LazyColumn {
                        items(entries, key = { it.id }) { entry ->
                            HistoryRow(entry, selected = entry.id.toString() == selected) { selected = entry.id.toString() }
                            HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
                        }
                    }
                },
                detail = { key, onBack ->
                    val entry = entries.firstOrNull { it.id.toString() == key }
                    if (entry == null) CenteredMessage("This search was removed") else HistoryDetail(entry, onBack) { onRun(entry.query) }
                },
                placeholder = "Select a search",
            )
        }
    }
    if (confirmClear) {
        ConfirmDialog(
            title = "Clear history?",
            text = "All saved searches on this device will be deleted.",
            confirmLabel = "Clear",
            destructive = true,
            onConfirm = {
                confirmClear = false
                library.clearHistory()
                selected = null
            },
            onDismiss = { confirmClear = false },
        )
    }
}

private fun title(query: SearchQuery): String =
    query.text.ifBlank { query.imdbId.orEmpty() }.ifBlank { "(empty search)" }

private fun whenText(ts: Long): String =
    DateTimeFormatter.ofLocalizedDateTime(FormatStyle.MEDIUM).withZone(ZoneId.systemDefault()).format(Instant.ofEpochMilli(ts))

@Composable
private fun HistoryRow(entry: HistoryEntry, selected: Boolean, onClick: () -> Unit) {
    Surface(color = if (selected) MaterialTheme.colorScheme.primaryContainer else MaterialTheme.colorScheme.surface) {
        ListItem(
            headlineContent = { Text(title(entry.query)) },
            supportingContent = {
                val cats = entry.query.categories.joinToString(", ") { it.label }
                Text(listOfNotNull(cats.ifEmpty { null }, "${entry.resultCount} results", whenText(entry.ts)).joinToString(" · "))
            },
            modifier = Modifier.clickable(onClick = onClick),
        )
    }
}

@Composable
private fun HistoryDetail(entry: HistoryEntry, onBack: (() -> Unit)?, onRun: () -> Unit) {
    Surface(Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.surface) {
        Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                if (onBack != null) {
                    IconButton(onClick = onBack) { Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back to history") }
                }
                Text(title(entry.query), style = MaterialTheme.typography.titleMedium)
            }
            Text(
                listOfNotNull(
                    entry.query.categories.takeIf { it.isNotEmpty() }?.joinToString(", ") { it.label },
                    "${entry.resultCount} results",
                    "searched ${whenText(entry.ts)}",
                ).joinToString(" · "),
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            Button(onClick = onRun) { Text("Search again") }
        }
    }
}
