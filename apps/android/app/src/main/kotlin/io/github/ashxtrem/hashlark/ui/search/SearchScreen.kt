// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark.ui.search

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.VerticalDivider
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.isCtrlPressed
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.github.ashxtrem.hashlark.core.MergedResult
import io.github.ashxtrem.hashlark.core.ResultSorter
import io.github.ashxtrem.hashlark.ui.AppEvent
import io.github.ashxtrem.hashlark.ui.AppViewModel
import io.github.ashxtrem.hashlark.ui.Destination
import io.github.ashxtrem.hashlark.ui.LibraryViewModel
import io.github.ashxtrem.hashlark.ui.PanePlan
import io.github.ashxtrem.hashlark.ui.ResultActions
import io.github.ashxtrem.hashlark.ui.WidthClass
import io.github.ashxtrem.hashlark.ui.WindowShape
import io.github.ashxtrem.hashlark.ui.common.FullScreenDetailEffect

/**
 * The search screen. The search controls always sit above the results workspace; what the
 * workspace shows depends on the room [WindowShape.planPanes] finds, not on the nominal window width:
 *
 * - results only, as wide as the window allows, until a result is opened (no empty details pane);
 * - a result opened and only one pane fits: full-screen details that replace the controls and the
 *   main navigation; Back returns to the same place in the list;
 * - a result opened and both panes fit: results beside the details, the selected row marked, and a
 *   close button on the details that gives the width back to the results;
 * - tabletop posture: results (or details) above the hinge, the controls and keyboard below it.
 *
 * The Fold7's inner display (about 750 x 832 dp, or 832 x 750 rotated) lands in the second case.
 */
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
    var filtersWereOpen by remember { mutableStateOf(false) }
    val filterFocus = remember { FocusRequester() }
    var restoreFocusId by remember { mutableStateOf<String?>(null) }

    // Sorted only when the results or the order change, not on every keystroke in the search field.
    val results = remember(state.progress.results, state.order, state.direction) {
        ResultSorter.sort(state.progress.results.values, state.order, state.direction)
    }
    val selected: MergedResult? = state.selectedId?.let { state.progress.results[it] }
    // Whether the user is looking at a result's details. Resizing or folding never changes this, so a
    // detail screen the user closed does not come back, and one they are reading does not vanish.
    val detailVisible = state.detailOpen && selected != null
    val plan = shape.planPanes()
    val fullScreenDetail = detailVisible && !shape.isTabletop && plan is PanePlan.Single
    // Results alone next to a hinge that has a gap: keep them on the list side of it.
    val resultsOnlyPlan: PanePlan.Single? = when {
        plan is PanePlan.Single -> plan
        plan is PanePlan.Split && plan.hingeGap > 0.dp -> PanePlan.Single(endInset = (shape.contentWidth - plan.listWidth).coerceAtLeast(0.dp))
        else -> null
    }

    FullScreenDetailEffect(fullScreenDetail)
    val closeDetail = {
        restoreFocusId = state.selectedId
        search.closeDetail()
    }
    // Back closes the details, whether they replace the list, sit beside it or fill the upper half.
    BackHandler(enabled = detailVisible) { closeDetail() }

    // After the filters close, focus returns to the button that opened them.
    LaunchedEffect(showFilters) {
        if (showFilters) {
            filtersWereOpen = true
        } else if (filtersWereOpen) {
            filtersWereOpen = false
            runCatching { filterFocus.requestFocus() }
        }
    }

    // With little height (a phone in landscape) the category strip slides away while scrolling down
    // and comes back on scrolling up or while typing. The search field and filter button stay.
    var fieldFocused by remember { mutableStateOf(false) }
    val stripVisible by remember(shape.isShort) {
        derivedStateOf { !shape.isShort || fieldFocused || !listState.canScrollBackward || listState.lastScrolledBackward }
    }

    val details: @Composable (asPane: Boolean, onBack: () -> Unit) -> Unit = { asPane, onBack ->
        if (selected != null) {
            ResultDetails(
                result = selected,
                favorite = selected.id in favoriteIds,
                names = names,
                actions = actions,
                onToggleFavorite = { library.toggleFavorite(selected) },
                onBack = onBack,
                asPane = asPane,
            )
        }
    }
    val controls: @Composable () -> Unit = {
        SearchControls(
            state = state,
            search = search,
            shape = shape,
            searchFocus = searchFocus,
            filterFocus = filterFocus,
            onOpenFilters = { showFilters = true },
            stripVisible = stripVisible,
            onFieldFocus = { fieldFocused = it },
        )
    }
    val resultsPane: @Composable (Modifier) -> Unit = { paneModifier ->
        ResultsPane(
            state = state,
            results = results,
            names = names,
            favoriteIds = favoriteIds,
            enabledProviders = enabledCount,
            search = search,
            actions = actions,
            listState = listState,
            onSelect = { result -> search.select(result.id) },
            onToggleFavorite = library::toggleFavorite,
            onOpenProviders = { app.post(AppEvent.Navigate(Destination.Providers)) },
            restoreFocusId = restoreFocusId,
            onFocusRestored = { restoreFocusId = null },
            bottomPadding = if (shape.isTabletop) 8.dp else 24.dp,
            modifier = paneModifier,
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
                top = { if (detailVisible) details(false, closeDetail) else resultsPane(Modifier) },
                bottom = controls,
            )

            fullScreenDetail -> Box(Modifier.fillMaxSize().hingeSafe(plan as PanePlan.Single)) { details(false, closeDetail) }

            plan is PanePlan.Split && detailVisible && plan.hingeGap > 0.dp -> {
                // A hinge with a gap: the controls belong to the list side, nothing sits under the hinge.
                Row(Modifier.fillMaxSize()) {
                    Column(Modifier.width(plan.listWidth).fillMaxHeight()) {
                        controls()
                        resultsPane(Modifier.weight(1f))
                    }
                    Spacer(Modifier.width(plan.hingeGap))
                    Box(Modifier.weight(1f).fillMaxHeight()) { details(true, closeDetail) }
                }
            }

            else -> Column(Modifier.fillMaxSize().hingeSafe(resultsOnlyPlan)) {
                controls()
                Row(Modifier.weight(1f).fillMaxWidth()) {
                    if (plan is PanePlan.Split && detailVisible) {
                        // Below the shared controls: the results, a divider, the selected result's details.
                        Box(Modifier.width(plan.listWidth).fillMaxHeight()) { resultsPane(Modifier) }
                        VerticalDivider()
                        Box(Modifier.weight(1f).fillMaxHeight()) { details(true, closeDetail) }
                    } else {
                        // No result open, or only one pane fits: the results take the whole width.
                        Box(Modifier.weight(1f).fillMaxHeight()) { resultsPane(Modifier) }
                    }
                }
            }
        }
    }

    if (showFilters) {
        val initial = FilterDraft(state.categories, state.providerIds, state.order, state.direction)
        val dismiss = { showFilters = false }
        val apply: (FilterDraft) -> Unit = { draft ->
            search.applyFilters(draft.categories, draft.providerIds, draft.order, draft.direction)
            showFilters = false
        }
        val roomy = shape.width >= WidthClass.Medium || shape.isShort
        if (roomy) {
            FilterDialog(initial, providers.orEmpty(), onApply = apply, onDismiss = dismiss)
        } else {
            FilterSheet(initial, providers.orEmpty(), onApply = apply, onDismiss = dismiss)
        }
    }
}

/** Keeps the content off one side of a hinge that has a gap; no padding on a flat display. */
private fun Modifier.hingeSafe(plan: PanePlan.Single?): Modifier =
    if (plan == null || (plan.startInset <= 0.dp && plan.endInset <= 0.dp)) this else this.padding(start = plan.startInset, end = plan.endInset)

/** Results (or details) above the hinge, controls below it. */
@Composable
private fun TabletopLayout(
    shape: WindowShape,
    top: @Composable () -> Unit,
    bottom: @Composable () -> Unit,
) {
    Column(Modifier.fillMaxSize()) {
        Box(Modifier.weight(shape.hingeFraction).fillMaxWidth()) { top() }
        HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
        Box(Modifier.weight(1f - shape.hingeFraction).fillMaxWidth().imePadding()) { bottom() }
    }
}
