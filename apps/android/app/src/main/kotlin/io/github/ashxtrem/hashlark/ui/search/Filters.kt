// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark.ui.search

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.Checkbox
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilterChip
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.minimumInteractiveComponentSize
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.paneTitle
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
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

/**
 * What the filter sheet edits. It is a draft: nothing reaches the search until Apply, so
 * dismissing the sheet never applies half-made changes.
 */
data class FilterDraft(
    val categories: Set<Category> = emptySet(),
    /** `null` searches every enabled provider. */
    val providerIds: Set<String>? = null,
    val order: SortOrder = SortOrder.Date,
    val direction: SortDirection = SortDirection.Descending,
) {
    /** A sort chip: another order starts descending, the current one flips its direction. */
    fun pickSort(sort: SortOrder): FilterDraft = if (sort == order) {
        copy(direction = if (direction == SortDirection.Descending) SortDirection.Ascending else SortDirection.Descending)
    } else {
        copy(order = sort, direction = SortDirection.Descending)
    }

    /** The categories and providers a search uses, as opposed to how results are sorted. */
    val activeCount: Int get() = categories.size + if (providerIds != null) 1 else 0

    companion object {
        /** What Reset returns to: every category, every enabled provider, newest first. */
        val Default = FilterDraft()
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
        Row(modifier, horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) { chips() }
    }
}

/**
 * Categories, sort order and provider selection, with Reset, Cancel and Apply. Shown in a bottom
 * sheet on compact windows and in a dialog on wider ones; only Apply changes the search.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun FilterPanel(
    initial: FilterDraft,
    providers: List<ProviderView>,
    onApply: (FilterDraft) -> Unit,
    onCancel: () -> Unit,
    modifier: Modifier = Modifier,
) {
    var draft by remember { mutableStateOf(initial) }
    val enabled = providers.filter { it.enabled && it.error == null }
    Column(modifier) {
        Column(
            Modifier.weight(1f, fill = false).verticalScroll(rememberScrollState()).padding(horizontal = 16.dp, vertical = 8.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Text("Filters and sorting", style = MaterialTheme.typography.titleLarge, modifier = Modifier.semantics { paneTitle = "Filters and sorting" })
            Text("Categories", style = MaterialTheme.typography.titleSmall, modifier = Modifier.padding(top = 8.dp))
            CategoryChips(
                selected = draft.categories,
                onToggle = { c -> draft = draft.copy(categories = if (c in draft.categories) draft.categories - c else draft.categories + c) },
                onClear = { draft = draft.copy(categories = emptySet()) },
            )
            Text("Sort by", style = MaterialTheme.typography.titleSmall, modifier = Modifier.padding(top = 8.dp))
            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                SortOrder.entries.forEach { sort ->
                    FilterChip(
                        selected = sort == draft.order,
                        onClick = { draft = draft.pickSort(sort) },
                        label = { Text(sortLabel(sort, if (sort == draft.order) draft.direction else SortDirection.Descending)) },
                    )
                }
            }
            Text("Tap the chosen sort again to reverse it.", style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            Text("Providers", style = MaterialTheme.typography.titleSmall, modifier = Modifier.padding(top = 8.dp))
            if (enabled.isEmpty()) {
                Text("No provider is enabled. Turn some on under Providers.", color = MaterialTheme.colorScheme.onSurfaceVariant)
            } else {
                CheckRow("All enabled providers", checked = draft.providerIds == null) { draft = draft.copy(providerIds = null) }
                enabled.forEach { provider ->
                    val checked = draft.providerIds?.contains(provider.id) ?: true
                    CheckRow(provider.name, checked) {
                        val current = draft.providerIds ?: enabled.map { it.id }.toSet()
                        val next = if (checked) current - provider.id else current + provider.id
                        // Every provider ticked is the same as "all", and none ticked would search nothing.
                        draft = draft.copy(providerIds = if (next.size == enabled.size || next.isEmpty()) null else next)
                    }
                }
            }
        }
        HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
        Row(Modifier.fillMaxWidth().padding(horizontal = 8.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
            TextButton(onClick = { draft = FilterDraft.Default }, enabled = draft != FilterDraft.Default) { Text("Reset") }
            Spacer(Modifier.weight(1f))
            TextButton(onClick = onCancel) { Text("Cancel") }
            Button(onClick = { onApply(draft) }, modifier = Modifier.padding(start = 8.dp, end = 8.dp)) { Text("Apply") }
        }
    }
}

@Composable
private fun CheckRow(label: String, checked: Boolean, onToggle: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().minimumInteractiveComponentSize().toggleable(value = checked, role = Role.Checkbox, onValueChange = { onToggle() }),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Checkbox(checked = checked, onCheckedChange = null)
        Text(label, Modifier.padding(start = 12.dp))
    }
}

/** The filters in a bottom sheet, for narrow windows. Swiping it away, its scrim and Back all discard the draft. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun FilterSheet(
    initial: FilterDraft,
    providers: List<ProviderView>,
    onApply: (FilterDraft) -> Unit,
    onDismiss: () -> Unit,
) {
    ModalBottomSheet(onDismissRequest = onDismiss, sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)) {
        FilterPanel(initial, providers, onApply = onApply, onCancel = onDismiss)
    }
}

/** The filters in a dialog, for windows wide enough that a sheet would stretch across them. */
@Composable
fun FilterDialog(
    initial: FilterDraft,
    providers: List<ProviderView>,
    onApply: (FilterDraft) -> Unit,
    onDismiss: () -> Unit,
) {
    Dialog(onDismissRequest = onDismiss, properties = DialogProperties(usePlatformDefaultWidth = false)) {
        Surface(
            modifier = Modifier.padding(24.dp).widthIn(max = 560.dp).heightIn(max = 640.dp),
            shape = MaterialTheme.shapes.extraLarge,
            tonalElevation = 6.dp,
        ) {
            Box { FilterPanel(initial, providers, onApply = onApply, onCancel = onDismiss, modifier = Modifier.padding(top = 16.dp)) }
        }
    }
}
