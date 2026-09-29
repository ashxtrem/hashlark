// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark.ui.search

import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.expandVertically
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Clear
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.filled.Stop
import androidx.compose.material.icons.filled.Tune
import androidx.compose.material3.Badge
import androidx.compose.material3.BadgedBox
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.VerticalDivider
import androidx.compose.runtime.Composable
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.isCtrlPressed
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.github.ashxtrem.hashlark.R
import io.github.ashxtrem.hashlark.core.Format
import io.github.ashxtrem.hashlark.core.MergedResult
import io.github.ashxtrem.hashlark.ui.AppViewModel
import io.github.ashxtrem.hashlark.ui.LibraryViewModel
import io.github.ashxtrem.hashlark.ui.ResultActions
import io.github.ashxtrem.hashlark.ui.WindowShape
import io.github.ashxtrem.hashlark.ui.common.CenteredMessage
import io.github.ashxtrem.hashlark.ui.common.TwoPane

/**
 * The search screen, laid out for the shape of its window:
 *
 * - narrow: the search field, chips and results; a result opens full-screen;
 * - medium: results next to the details of the selected one;
 * - expanded: filters, results table and details;
 * - book posture: results left of the hinge, details right of it;
 * - tabletop posture: results (or details) above the hinge, the search field,
 *   chips and keyboard below it.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SearchScreen(
    shape: WindowShape,
    app: AppViewModel,
    search: SearchViewModel,
    library: LibraryViewModel,
    actions: ResultActions,
    searchFocus: FocusRequester,
    modifier: Modifier = Modifier,
) {
    val state by search.state.collectAsStateWithLifecycle()
    val names by app.providerNames.collectAsStateWithLifecycle()
    val providers by app.providers.collectAsStateWithLifecycle()
    val favoriteIds by library.favoriteIds.collectAsStateWithLifecycle()
    val enabledCount by app.enabledProviderCount.collectAsStateWithLifecycle()
    val listState = rememberLazyListState()
    var showFilters by rememberSaveable { mutableStateOf(false) }
    val selected: MergedResult? = state.selectedId?.let { state.progress.results[it] }

    val select: (MergedResult) -> Unit = { result -> search.select(result.id) }
    // In a single pane the back button (or gesture) closes the details and shows the list again.
    BackHandler(enabled = !shape.hasTwoPanes && !shape.isTabletop && state.detailOpen && selected != null) { search.closeDetail() }
    // In tabletop posture the details take the upper half; back returns to the results.
    BackHandler(enabled = shape.isTabletop && selected != null) { search.select(null) }

    val filterContent: @Composable (Modifier, Boolean) -> Unit = { contentModifier, withCategories ->
        FilterContent(
            order = state.order,
            direction = state.direction,
            onPickSort = search::pickSort,
            providers = providers.orEmpty(),
            selectedProviders = state.providerIds,
            onProvidersChange = search::setProviders,
            modifier = contentModifier,
            categories = if (withCategories) {
                { CategoryChips(state.categories, search::toggleCategory, search::clearCategories) }
            } else {
                null
            },
        )
    }

    val results: @Composable (Modifier, Boolean) -> Unit = { resultsModifier, showControls ->
        SearchPane(
            shape = shape,
            state = state,
            names = names,
            favoriteIds = favoriteIds,
            enabledProviders = enabledCount,
            search = search,
            actions = actions,
            listState = listState,
            searchFocus = searchFocus,
            showControls = showControls,
            categoriesInline = !shape.hasThreePanes,
            onOpenFilters = { showFilters = true },
            onSelect = select,
            onToggleFavorite = library::toggleFavorite,
            modifier = resultsModifier,
        )
    }

    Box(
        modifier.onPreviewKeyEvent { event ->
            // Ctrl+F focuses the search field, like the "/" key does on the desktop.
            if (event.type == KeyEventType.KeyDown && event.isCtrlPressed && event.key == Key.F) {
                searchFocus.requestFocus()
                true
            } else {
                false
            }
        },
    ) {
        when {
            shape.isTabletop -> TabletopLayout(
                shape = shape,
                top = {
                    if (selected != null) {
                        ResultDetails(
                            result = selected,
                            favorite = selected.id in favoriteIds,
                            names = names,
                            actions = actions,
                            onToggleFavorite = { library.toggleFavorite(selected) },
                            onBack = { search.select(null) },
                        )
                    } else {
                        SearchResultsOnly(state, names, favoriteIds, enabledCount, search, actions, listState, select, library::toggleFavorite, shape)
                    }
                },
                bottom = { SearchControls(state, search, searchFocus, categoriesInline = true, onOpenFilters = { showFilters = true }, names = names, actions = actions) },
            )

            else -> Row(Modifier.fillMaxSize()) {
                if (shape.hasThreePanes) {
                    Surface(Modifier.width(280.dp).fillMaxHeight(), color = MaterialTheme.colorScheme.surfaceContainerLow) {
                        filterContent(Modifier.fillMaxSize(), true)
                    }
                    VerticalDivider()
                }
                val detailPane: @Composable () -> Unit = {
                    if (selected != null) {
                        ResultDetails(
                            result = selected,
                            favorite = selected.id in favoriteIds,
                            names = names,
                            actions = actions,
                            onToggleFavorite = { library.toggleFavorite(selected) },
                            onBack = if (shape.hasTwoPanes) null else search::closeDetail,
                        )
                    } else {
                        DetailsPlaceholder()
                    }
                }
                when {
                    shape.hasTwoPanes -> TwoPane(
                        shape,
                        list = { results(Modifier.fillMaxSize(), true) },
                        detail = detailPane,
                        // Next to the filter panel the results get most of the room, enough for the table.
                        listWidthOverride = if (shape.hasThreePanes) ((shape.widthDp - 280.dp) * 0.62f) else null,
                    )
                    state.detailOpen && selected != null -> detailPane()
                    else -> results(Modifier.fillMaxSize(), true)
                }
            }
        }
    }

    if (showFilters && !shape.hasThreePanes) {
        FilterSheet(onDismiss = { showFilters = false }) {
            filterContent(Modifier, false)
        }
    }
}

@Composable
private fun DetailsPlaceholder() {
    Surface(Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.surfaceContainerLow) {
        CenteredMessage("Select a result", body = "Its details show up here.")
    }
}

/** Results above the hinge, controls below it. */
@Composable
private fun TabletopLayout(
    shape: WindowShape,
    top: @Composable () -> Unit,
    bottom: @Composable () -> Unit,
) {
    Column(Modifier.fillMaxSize()) {
        Box(Modifier.weight(shape.hingeFraction).fillMaxWidth()) { top() }
        VerticalDividerSpacer()
        Box(Modifier.weight(1f - shape.hingeFraction).fillMaxWidth().imePadding()) { bottom() }
    }
}

@Composable
private fun VerticalDividerSpacer() {
    androidx.compose.material3.HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
}

/** Controls, status and results for one column (the list pane). */
@Composable
private fun SearchPane(
    shape: WindowShape,
    state: SearchUiState,
    names: Map<String, String>,
    favoriteIds: Set<String>,
    enabledProviders: Int,
    search: SearchViewModel,
    actions: ResultActions,
    listState: LazyListState,
    searchFocus: FocusRequester,
    showControls: Boolean,
    categoriesInline: Boolean,
    onOpenFilters: () -> Unit,
    onSelect: (MergedResult) -> Unit,
    onToggleFavorite: (MergedResult) -> Unit,
    modifier: Modifier = Modifier,
) {
    var fieldFocused by remember { mutableStateOf(false) }
    // With little height (the cover screen in landscape) the controls slide away while
    // scrolling down and come back on scrolling up or while typing.
    val controlsVisible by remember(shape.isShort) {
        derivedStateOf {
            !shape.isShort || fieldFocused || !listState.canScrollBackward || listState.lastScrolledBackward
        }
    }
    Column(modifier) {
        AnimatedVisibility(visible = controlsVisible, enter = expandVertically(), exit = shrinkVertically()) {
            SearchControls(
                state, search, searchFocus, categoriesInline, onOpenFilters, names, actions,
                onFieldFocus = { fieldFocused = it },
            )
        }
        SearchResultsOnly(state, names, favoriteIds, enabledProviders, search, actions, listState, onSelect, onToggleFavorite, shape)
    }
}

/** The search field, category chips, filter button and provider status. */
@OptIn(androidx.compose.foundation.layout.ExperimentalLayoutApi::class)
@Composable
private fun SearchControls(
    state: SearchUiState,
    search: SearchViewModel,
    focus: FocusRequester,
    categoriesInline: Boolean,
    onOpenFilters: () -> Unit,
    names: Map<String, String>,
    actions: ResultActions,
    onFieldFocus: (Boolean) -> Unit = {},
) {
    val keyboard = LocalSoftwareKeyboardController.current
    val submit = {
        keyboard?.hide()
        search.run()
    }
    Column(Modifier.padding(horizontal = 16.dp, vertical = 8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            OutlinedTextField(
                value = if (state.imdbId != null && state.text.isBlank()) state.imdbId else state.text,
                onValueChange = search::setText,
                modifier = Modifier
                    .weight(1f)
                    .focusRequester(focus)
                    .onFocusChanged { onFieldFocus(it.isFocused) }
                    // "/" would type a slash; only Enter is special here.
                    .onPreviewKeyEvent { event ->
                        if (event.type == KeyEventType.KeyDown && event.key == Key.Enter) {
                            submit()
                            true
                        } else {
                            false
                        }
                    },
                singleLine = true,
                placeholder = { Text("Search torrents") },
                leadingIcon = { Icon(Icons.Filled.Search, contentDescription = null) },
                trailingIcon = {
                    if (state.text.isNotEmpty() || state.imdbId != null) {
                        IconButton(onClick = { search.setText("") }) { Icon(Icons.Filled.Clear, contentDescription = "Clear") }
                    }
                },
                keyboardOptions = KeyboardOptions(imeAction = ImeAction.Search),
                keyboardActions = KeyboardActions(onSearch = { submit() }),
                shape = MaterialTheme.shapes.extraLarge,
            )
            if (state.running) {
                OutlinedButton(onClick = search::cancel) {
                    Icon(Icons.Filled.Stop, contentDescription = null, Modifier.size(18.dp))
                    Text("Stop", Modifier.padding(start = 6.dp))
                }
            } else {
                Button(onClick = submit, enabled = state.canSearch) { Text("Search") }
            }
        }
        if (categoriesInline) {
            Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                val customFilters = state.providerIds != null
                IconButton(onClick = onOpenFilters) {
                    BadgedBox(badge = { if (customFilters) Badge() }) {
                        Icon(Icons.Filled.Tune, contentDescription = "Filters and sorting: ${sortLabel(state.order, state.direction)}")
                    }
                }
                CategoryChips(state.categories, search::toggleCategory, search::clearCategories, wrap = false)
            }
        }
    }
}

/** Results with their state: progress, errors, empty and first-use screens. */
@Composable
private fun SearchResultsOnly(
    state: SearchUiState,
    names: Map<String, String>,
    favoriteIds: Set<String>,
    enabledProviders: Int,
    search: SearchViewModel,
    actions: ResultActions,
    listState: LazyListState,
    onSelect: (MergedResult) -> Unit,
    onToggleFavorite: (MergedResult) -> Unit,
    shape: WindowShape,
) {
    val sorted = state.sorted
    Column(Modifier.fillMaxSize()) {
        if (state.running) LinearProgressIndicator(Modifier.fillMaxWidth())
        state.error?.let { message ->
            Card(
                Modifier.padding(16.dp).fillMaxWidth(),
                colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.errorContainer),
            ) { Text(message, Modifier.padding(16.dp), color = MaterialTheme.colorScheme.onErrorContainer) }
        }
        // One line: the result count on the left, the providers' status (a small chip that
        // opens the details) on the right, instead of a chip per provider.
        ResultsSummary(state, sorted.size, names, actions)
        when {
            sorted.isNotEmpty() -> {
                ResultsList(
                    results = sorted,
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
                    contentPadding = PaddingValues(bottom = if (shape.isTabletop) 8.dp else 24.dp),
                    footer = if (state.progress.done && !state.running) {
                        {
                            Box(Modifier.fillMaxWidth().padding(16.dp), contentAlignment = Alignment.Center) {
                                OutlinedButton(onClick = search::loadMore) { Text("More results (page ${state.page + 1})") }
                            }
                        }
                    } else {
                        null
                    },
                )
            }
            state.running -> CenteredMessage(
                "Searching ${state.progress.providers.size.coerceAtLeast(1)} provider${if (state.progress.providers.size == 1) "" else "s"}…",
            )
            !state.idle -> {
                if (state.progress.providers.isEmpty()) {
                    CenteredMessage("No provider can handle this search.", body = "Enable providers or try other categories.")
                } else {
                    CenteredMessage("No results for “${state.shown?.text.orEmpty().ifBlank { state.shown?.imdbId.orEmpty() }}”.", body = "Try fewer or different words, or other categories.")
                }
            }
            else -> Box(Modifier.fillMaxSize().padding(24.dp), contentAlignment = Alignment.Center) {
                Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(12.dp)) {
                    Icon(
                        painterResource(R.drawable.ic_launcher_monochrome),
                        contentDescription = null,
                        modifier = Modifier.size(96.dp),
                        tint = MaterialTheme.colorScheme.primary,
                    )
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
}

@Composable
private fun ResultsSummary(state: SearchUiState, count: Int, names: Map<String, String>, actions: ResultActions) {
    if (state.progress.providers.isEmpty() && count == 0) return
    val subject = state.shown?.text.orEmpty().ifBlank { state.shown?.imdbId.orEmpty() }
    val tail = when {
        state.running -> " · still searching…"
        state.progress.durationMs != null -> " in ${Format.duration(state.progress.durationMs)}"
        else -> ""
    }
    Row(
        Modifier.fillMaxWidth().padding(start = 16.dp, end = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            if (count > 0) "$count results for $subject$tail" else "",
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            maxLines = 1,
            overflow = androidx.compose.ui.text.style.TextOverflow.Ellipsis,
            modifier = Modifier.weight(1f),
        )
        ProviderStatusBar(state.progress.providers, names, onChallenge = actions::challenge)
    }
}
