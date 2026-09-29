// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark.ui.search

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.automirrored.filled.Sort
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.github.ashxtrem.hashlark.R
import io.github.ashxtrem.hashlark.core.Format
import io.github.ashxtrem.hashlark.core.MergedResult
import io.github.ashxtrem.hashlark.core.SortDirection
import io.github.ashxtrem.hashlark.core.SortOrder
import io.github.ashxtrem.hashlark.ui.ResultActions
import io.github.ashxtrem.hashlark.ui.common.CenteredMessage

/**
 * What the results area shows for one search state: the first-use screen, progress, the streaming or
 * finished list (with its sort control and provider status), and the empty and failure screens, each
 * with a way forward.
 */
@Composable
fun ResultsPane(
    state: SearchUiState,
    results: List<MergedResult>,
    names: Map<String, String>,
    favoriteIds: Set<String>,
    enabledProviders: Int,
    search: SearchViewModel,
    actions: ResultActions,
    listState: LazyListState,
    onSelect: (MergedResult) -> Unit,
    onToggleFavorite: (MergedResult) -> Unit,
    onOpenProviders: () -> Unit,
    restoreFocusId: String?,
    onFocusRestored: () -> Unit,
    bottomPadding: androidx.compose.ui.unit.Dp,
    modifier: Modifier = Modifier,
) {
    val summary = ProviderSummary.of(state.progress.providers)
    Column(modifier.fillMaxSize()) {
        if (state.running) {
            LinearProgressIndicator(Modifier.fillMaxWidth().semantics { contentDescription = "Searching" })
        }
        state.error?.let { message ->
            Card(
                Modifier.padding(16.dp).fillMaxWidth(),
                colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.errorContainer),
            ) {
                Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text(message, color = MaterialTheme.colorScheme.onErrorContainer)
                    OutlinedButton(onClick = search::run, enabled = state.canSearch) { Text("Try again") }
                }
            }
        }
        if (!state.idle) ResultsHeader(state, results.size, names, actions, search)
        when {
            results.isNotEmpty() -> ResultsList(
                results = results,
                selectedId = state.selectedId,
                favoriteIds = favoriteIds,
                order = state.order,
                direction = state.direction,
                actions = actions,
                onSort = search::toggleSort,
                onSelect = onSelect,
                onToggleFavorite = onToggleFavorite,
                modifier = Modifier.weight(1f),
                state = listState,
                contentPadding = PaddingValues(bottom = bottomPadding),
                providerNames = names,
                restoreFocusId = restoreFocusId,
                onFocusRestored = onFocusRestored,
                footer = if (state.progress.done && !state.running && !state.cancelled) {
                    {
                        Box(Modifier.fillMaxWidth().padding(16.dp), contentAlignment = Alignment.Center) {
                            OutlinedButton(onClick = search::loadMore, modifier = Modifier.heightIn(min = 48.dp)) { Text("More results (page ${state.page + 1})") }
                        }
                    }
                } else {
                    null
                },
            )
            state.running -> CenteredMessage(
                "Searching ${state.progress.providers.size.coerceAtLeast(1)} provider${if (state.progress.providers.size == 1) "" else "s"}…",
                body = "Results appear as each provider answers. You can stop the search at any time.",
            )
            state.idle -> FirstUse(enabledProviders, onOpenProviders)
            state.error != null -> Unit // the card above says what went wrong and offers to try again
            state.cancelled -> CenteredMessage(
                "Search stopped",
                body = "No results had arrived yet.",
                action = { Button(onClick = search::run, enabled = state.canSearch) { Text("Search again") } },
            )
            state.progress.providers.isEmpty() || enabledProviders == 0 -> CenteredMessage(
                "No provider can handle this search",
                body = if (enabledProviders == 0) "No provider is enabled. Turn some on to search." else "Enable more providers, or try other categories.",
                action = { Button(onClick = onOpenProviders) { Text("Open Providers") } },
            )
            summary.failed == summary.total -> CenteredMessage(
                if (summary.total == 1) "The provider could not answer" else "None of the ${summary.total} providers could answer",
                body = "Open the provider status above to see why. Check your connection, then try again.",
                action = { Button(onClick = search::run, enabled = state.canSearch) { Text("Try again") } },
            )
            summary.failed > 0 -> CenteredMessage(
                "No results, and ${summary.failed} of ${summary.total} providers failed",
                body = "The ones that answered had nothing for “${subject(state)}”. Open the provider status above for the failures.",
                action = { Button(onClick = search::run, enabled = state.canSearch) { Text("Search again") } },
            )
            else -> CenteredMessage(
                "No results for “${subject(state)}”",
                body = "Try fewer or different words, or other categories.",
                action = if (state.categories.isNotEmpty() || state.providerIds != null) {
                    {
                        OutlinedButton(onClick = {
                            search.resetFilters()
                            search.run()
                        }) { Text("Clear filters and search again") }
                    }
                } else {
                    null
                },
            )
        }
    }
}

private fun subject(state: SearchUiState): String = state.shown?.text.orEmpty().ifBlank { state.shown?.imdbId.orEmpty() }

@Composable
private fun FirstUse(enabledProviders: Int, onOpenProviders: () -> Unit) {
    Box(Modifier.fillMaxSize().padding(24.dp), contentAlignment = Alignment.Center) {
        Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Icon(
                painterResource(R.drawable.ic_launcher_monochrome),
                contentDescription = null,
                modifier = Modifier.size(96.dp),
                tint = MaterialTheme.colorScheme.primary,
            )
            if (enabledProviders == 0) {
                Text("No providers are enabled", style = MaterialTheme.typography.titleMedium, textAlign = TextAlign.Center)
                Text(
                    "Turn on at least one provider to search.",
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    textAlign = TextAlign.Center,
                )
                Button(onClick = onOpenProviders) { Text("Open Providers") }
            } else {
                Text("Search every provider at once", style = MaterialTheme.typography.titleMedium, textAlign = TextAlign.Center)
                Text(
                    "Results stream in as each provider answers, with duplicates merged. " +
                        "You're searching $enabledProviders provider${if (enabledProviders == 1) "" else "s"}.",
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    textAlign = TextAlign.Center,
                )
            }
        }
    }
}

/**
 * The result count and what it is for, then the provider status and the sort control. Both controls
 * are always visible; the status wraps onto its own line on narrow windows instead of being cut off.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun ResultsHeader(state: SearchUiState, count: Int, names: Map<String, String>, actions: ResultActions, search: SearchViewModel) {
    val subject = subject(state)
    val tail = when {
        state.running -> " · still searching…"
        state.cancelled -> " · search stopped"
        state.progress.durationMs != null -> " in ${Format.duration(state.progress.durationMs)}"
        else -> ""
    }
    val shown = state.shown
    val filtersChanged = shown != null && !state.running &&
        (shown.categories.toSet() != state.categories || shown.providers?.toSet() != state.providerIds)
    Column(Modifier.fillMaxWidth().padding(start = 16.dp, end = 8.dp, top = 4.dp)) {
        if (count > 0) {
            Text(
                if (count == 1) "1 result for “$subject”$tail" else "$count results for “$subject”$tail",
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
                // Announced once when the search settles, not for every streamed batch.
                modifier = if (state.running) Modifier else Modifier.semantics { liveRegion = LiveRegionMode.Polite },
            )
        }
        FlowRow(
            Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalArrangement = Arrangement.Center,
        ) {
            ProviderStatusBar(state.progress.providers, names, onChallenge = actions::challenge, onRetry = search::run)
            if (count > 1) SortControl(state, search)
        }
        if (filtersChanged) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(
                    "Filters changed since this search.",
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.weight(1f),
                )
                TextButton(onClick = search::run, enabled = state.canSearch) { Text("Search again") }
            }
        }
    }
}

/** The current sort order, visible in the results header; choosing the current one again reverses it. */
@Composable
private fun SortControl(state: SearchUiState, search: SearchViewModel) {
    var open by remember { mutableStateOf(false) }
    Box {
        TextButton(onClick = { open = true }, modifier = Modifier.heightIn(min = 48.dp)) {
            Icon(Icons.AutoMirrored.Filled.Sort, contentDescription = null, Modifier.size(18.dp))
            Text("Sort: ${sortLabel(state.order, state.direction)}", Modifier.padding(start = 6.dp))
        }
        DropdownMenu(expanded = open, onDismissRequest = { open = false }) {
            SortOrder.entries.forEach { sort ->
                val current = sort == state.order
                DropdownMenuItem(
                    text = { Text(sortLabel(sort, if (current) state.direction else SortDirection.Descending)) },
                    trailingIcon = if (current) {
                        { Icon(Icons.Filled.Check, contentDescription = "Current sort. Choose again to reverse.") }
                    } else {
                        null
                    },
                    onClick = {
                        open = false
                        if (current) search.toggleSort(sort) else search.pickSort(sort)
                    },
                )
            }
        }
    }
}
