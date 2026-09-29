// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark.ui.search

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.layout.Box
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.ArrowDownward
import androidx.compose.material.icons.filled.ArrowUpward
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import io.github.ashxtrem.hashlark.core.MergedResult
import io.github.ashxtrem.hashlark.core.SortDirection
import io.github.ashxtrem.hashlark.core.SortOrder
import io.github.ashxtrem.hashlark.ui.ResultActions

/** Test tag of the results list. */
const val RESULTS_TAG = "results"

/**
 * The results, as cards on narrow widths and as a sortable table on wide
 * ones. The density follows the width this list gets, so the same code serves
 * a phone, the list pane next to a details pane, and a full-width tablet.
 */
@Composable
fun ResultsList(
    results: List<MergedResult>,
    selectedId: String?,
    favoriteIds: Set<String>,
    order: SortOrder,
    direction: SortDirection,
    actions: ResultActions,
    onSort: (SortOrder) -> Unit,
    onSelect: (MergedResult) -> Unit,
    onToggleFavorite: (MergedResult) -> Unit,
    modifier: Modifier = Modifier,
    state: LazyListState,
    contentPadding: PaddingValues = PaddingValues(),
    footer: @Composable (() -> Unit)? = null,
) {
    BoxWithConstraints(modifier) {
        val density = RowDensity.forWidth(maxWidth.value)
        LazyColumn(state = state, contentPadding = contentPadding, modifier = Modifier.testTag(RESULTS_TAG)) {
            if (density == RowDensity.Wide) {
                item(key = "header") { TableHeader(order, direction, onSort) }
            }
            items(results, key = { it.id }) { result ->
                ResultRow(
                    result = result,
                    density = density,
                    selected = result.id == selectedId,
                    favorite = result.id in favoriteIds,
                    actions = actions,
                    onClick = { onSelect(result) },
                    onToggleFavorite = { onToggleFavorite(result) },
                )
                HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
            }
            if (footer != null) item(key = "footer") { footer() }
        }
    }
}

@Composable
private fun TableHeader(order: SortOrder, direction: SortDirection, onSort: (SortOrder) -> Unit) {
    Surface(color = MaterialTheme.colorScheme.surfaceContainerHigh) {
        Row(Modifier.fillMaxWidth().padding(start = 16.dp, end = 4.dp), verticalAlignment = Alignment.CenterVertically) {
            HeaderCell("Title", SortOrder.Title, order, direction, onSort, Modifier.weight(1f), TextAlign.Start)
            HeaderCell("Size", SortOrder.Size, order, direction, onSort, Modifier.width(TableColumns.size))
            HeaderCell("Seeds", SortOrder.Seeders, order, direction, onSort, Modifier.width(TableColumns.seeders))
            HeaderCell("Peers", SortOrder.Peers, order, direction, onSort, Modifier.width(TableColumns.peers))
            HeaderCell("Age", SortOrder.Date, order, direction, onSort, Modifier.width(TableColumns.age))
            Text(
                "Sources",
                style = MaterialTheme.typography.labelMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                textAlign = TextAlign.End,
                modifier = Modifier.width(TableColumns.sources),
            )
            Box(Modifier.width(TableColumns.actions))
        }
    }
}

@Composable
private fun HeaderCell(
    label: String,
    column: SortOrder,
    order: SortOrder,
    direction: SortDirection,
    onSort: (SortOrder) -> Unit,
    modifier: Modifier,
    align: TextAlign = TextAlign.End,
) {
    val active = column == order
    Row(
        modifier.clickable { onSort(column) }.padding(vertical = 10.dp),
        horizontalArrangement = if (align == TextAlign.Start) androidx.compose.foundation.layout.Arrangement.Start else androidx.compose.foundation.layout.Arrangement.End,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (active) {
            Icon(
                if (direction == SortDirection.Descending) Icons.Filled.ArrowDownward else Icons.Filled.ArrowUpward,
                contentDescription = if (direction == SortDirection.Descending) "Sorted descending" else "Sorted ascending",
                modifier = Modifier.width(14.dp),
                tint = MaterialTheme.colorScheme.primary,
            )
        }
        Text(
            label,
            style = MaterialTheme.typography.labelMedium,
            color = if (active) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}
