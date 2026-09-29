// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark.ui.search

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Checkbox
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilterChip
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp
import io.github.ashxtrem.hashlark.core.Category
import io.github.ashxtrem.hashlark.core.ProviderView
import io.github.ashxtrem.hashlark.core.SortDirection
import io.github.ashxtrem.hashlark.core.SortOrder

/** The label of a sort choice, with the wording the desktop app uses. */
fun sortLabel(order: SortOrder, direction: SortDirection): String {
    val ascending = direction == SortDirection.Ascending
    return when (order) {
        SortOrder.Relevance -> "Best match"
        SortOrder.Title -> if (ascending) "Title A–Z" else "Title Z–A"
        SortOrder.Seeders -> if (ascending) "Fewest seeders" else "Most seeders"
        SortOrder.Peers -> if (ascending) "Fewest peers" else "Most peers"
        SortOrder.Size -> if (ascending) "Smallest" else "Largest"
        SortOrder.Date -> if (ascending) "Oldest" else "Newest"
    }
}

/** "All" plus one chip per category. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun CategoryChips(
    selected: Set<Category>,
    onToggle: (Category) -> Unit,
    onClear: () -> Unit,
    modifier: Modifier = Modifier,
    /** Wrap onto several lines (a panel) or stay on one line (inside a scrolling row). */
    wrap: Boolean = true,
) {
    val chips: @Composable () -> Unit = {
        FilterChip(selected = selected.isEmpty(), onClick = onClear, label = { Text("All") })
        Category.entries.forEach { category ->
            FilterChip(
                selected = category in selected,
                onClick = { onToggle(category) },
                label = { Text(category.label) },
            )
        }
    }
    if (wrap) {
        FlowRow(modifier, horizontalArrangement = Arrangement.spacedBy(8.dp)) { chips() }
    } else {
        Row(modifier, horizontalArrangement = Arrangement.spacedBy(8.dp)) { chips() }
    }
}

/**
 * Sort order and provider selection. Shown as a side panel on wide windows and
 * in a bottom sheet on narrow ones.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun FilterContent(
    order: SortOrder,
    direction: SortDirection,
    onPickSort: (SortOrder) -> Unit,
    providers: List<ProviderView>,
    selectedProviders: Set<String>?,
    onProvidersChange: (Set<String>?) -> Unit,
    modifier: Modifier = Modifier,
    categories: (@Composable () -> Unit)? = null,
) {
    val enabled = providers.filter { it.enabled && it.error == null }
    Column(modifier.verticalScroll(rememberScrollState()).padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        if (categories != null) {
            Text("Categories", style = MaterialTheme.typography.titleSmall)
            categories()
        }
        Text("Sort by", style = MaterialTheme.typography.titleSmall, modifier = Modifier.padding(top = 8.dp))
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            SortOrder.entries.forEach { sort ->
                FilterChip(
                    selected = sort == order,
                    onClick = { onPickSort(sort) },
                    label = { Text(sortLabel(sort, if (sort == order) direction else SortDirection.Descending)) },
                )
            }
        }
        Text("Providers", style = MaterialTheme.typography.titleSmall, modifier = Modifier.padding(top = 8.dp))
        if (enabled.isEmpty()) {
            Text("No provider is enabled.", color = MaterialTheme.colorScheme.onSurfaceVariant)
        } else {
            CheckRow("All enabled providers", checked = selectedProviders == null) { onProvidersChange(null) }
            enabled.forEach { provider ->
                val checked = selectedProviders?.contains(provider.id) ?: true
                CheckRow(provider.name, checked) {
                    val current = selectedProviders ?: enabled.map { it.id }.toSet()
                    val next = if (checked) current - provider.id else current + provider.id
                    // Every provider ticked is the same as "all", and none ticked would search nothing.
                    onProvidersChange(if (next.size == enabled.size || next.isEmpty()) null else next)
                }
            }
        }
    }
}

@Composable
private fun CheckRow(label: String, checked: Boolean, onToggle: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().toggleable(value = checked, role = Role.Checkbox, onValueChange = { onToggle() }),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Checkbox(checked = checked, onCheckedChange = null)
        Text(label, Modifier.padding(start = 12.dp, top = 8.dp, bottom = 8.dp))
    }
}

/** The filters in a bottom sheet, for narrow windows. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun FilterSheet(onDismiss: () -> Unit, content: @Composable () -> Unit) {
    ModalBottomSheet(onDismissRequest = onDismiss, sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)) {
        content()
    }
}
