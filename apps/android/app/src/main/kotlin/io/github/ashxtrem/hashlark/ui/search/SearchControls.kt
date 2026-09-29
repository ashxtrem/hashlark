// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark.ui.search

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.expandVertically
import androidx.compose.animation.shrinkVertically
import androidx.compose.foundation.ScrollState
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Clear
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.filled.Stop
import androidx.compose.material.icons.filled.Tune
import androidx.compose.material3.Button
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilledIconButton
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.FilledTonalIconButton
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.BlendMode
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.CompositingStrategy
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.unit.dp
import io.github.ashxtrem.hashlark.ui.WindowShape

/**
 * The search field with its submit or stop button, the filter button and the category strip. They sit
 * above the results workspace, so the field keeps the full width whatever the panes below do.
 *
 * On short windows (a phone in landscape) the filter button moves next to the field and only the
 * category strip slides away while the results scroll down, so the filters stay reachable.
 */
@Composable
fun SearchControls(
    state: SearchUiState,
    search: SearchViewModel,
    shape: WindowShape,
    searchFocus: FocusRequester,
    filterFocus: FocusRequester,
    onOpenFilters: () -> Unit,
    stripVisible: Boolean,
    modifier: Modifier = Modifier,
    onFieldFocus: (Boolean) -> Unit = {},
) {
    val keyboard = LocalSoftwareKeyboardController.current
    val submit = {
        keyboard?.hide()
        search.run()
    }
    val activeFilters = FilterDraft(state.categories, state.providerIds).activeCount
    val short = shape.isShort
    Column(
        modifier.padding(horizontal = 16.dp, vertical = if (short) 4.dp else 8.dp),
        verticalArrangement = Arrangement.spacedBy(if (short) 4.dp else 8.dp),
    ) {
        BoxWithConstraints(Modifier.fillMaxWidth()) {
            // A labelled button when there is room; a 48 dp icon button when the field needs the width.
            val roomy = maxWidth >= 480.dp
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                OutlinedTextField(
                    value = if (state.imdbId != null && state.text.isBlank()) state.imdbId else state.text,
                    onValueChange = search::setText,
                    modifier = Modifier
                        .weight(1f)
                        .focusRequester(searchFocus)
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
                    label = { Text("Search torrents") },
                    leadingIcon = if (roomy) {
                        { Icon(Icons.Filled.Search, contentDescription = null) }
                    } else {
                        null
                    },
                    trailingIcon = {
                        if (state.text.isNotEmpty() || state.imdbId != null) {
                            IconButton(onClick = { search.setText("") }) { Icon(Icons.Filled.Clear, contentDescription = "Clear search") }
                        }
                    },
                    keyboardOptions = KeyboardOptions(imeAction = ImeAction.Search),
                    keyboardActions = KeyboardActions(onSearch = { submit() }),
                    shape = MaterialTheme.shapes.extraLarge,
                )
                if (short) FilterButton(activeFilters, onOpenFilters, filterFocus, labelled = false)
                when {
                    state.running && roomy -> OutlinedButton(onClick = search::cancel, modifier = Modifier.heightIn(min = 48.dp)) {
                        Icon(Icons.Filled.Stop, contentDescription = null, Modifier.size(18.dp))
                        Text("Stop", Modifier.padding(start = 6.dp))
                    }
                    state.running -> FilledTonalIconButton(onClick = search::cancel, modifier = Modifier.size(48.dp)) {
                        Icon(Icons.Filled.Stop, contentDescription = "Stop search")
                    }
                    roomy -> Button(onClick = submit, enabled = state.canSearch, modifier = Modifier.heightIn(min = 48.dp)) { Text("Search") }
                    else -> FilledIconButton(onClick = submit, enabled = state.canSearch, modifier = Modifier.size(48.dp)) {
                        Icon(Icons.Filled.Search, contentDescription = "Search")
                    }
                }
            }
        }
        AnimatedVisibility(visible = stripVisible, enter = expandVertically(), exit = shrinkVertically()) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                if (!short) FilterButton(activeFilters, onOpenFilters, filterFocus, labelled = true)
                CategoryStrip(state, search, Modifier.weight(1f))
            }
        }
    }
}

/** Always outside the scrolling strip, so it is never scrolled out of reach. */
@Composable
private fun FilterButton(activeFilters: Int, onClick: () -> Unit, focus: FocusRequester, labelled: Boolean) {
    val description = if (activeFilters == 0) "Filters and sorting" else "Filters and sorting, $activeFilters active"
    if (labelled) {
        FilledTonalButton(
            onClick = onClick,
            modifier = Modifier.heightIn(min = 48.dp).focusRequester(focus).semantics { contentDescription = description },
        ) {
            Icon(Icons.Filled.Tune, contentDescription = null, Modifier.size(18.dp))
            Text(if (activeFilters == 0) "Filters" else "Filters · $activeFilters", Modifier.padding(start = 6.dp))
        }
    } else {
        FilledTonalIconButton(onClick = onClick, modifier = Modifier.size(48.dp).focusRequester(focus)) {
            Icon(Icons.Filled.Tune, contentDescription = description)
        }
    }
}

/** A horizontally scrolling row of category chips whose faded edges show that there is more beyond them. */
@Composable
private fun CategoryStrip(state: SearchUiState, search: SearchViewModel, modifier: Modifier = Modifier) {
    val scroll = rememberScrollState()
    CategoryChips(
        selected = state.categories,
        onToggle = search::toggleCategory,
        onClear = search::clearCategories,
        wrap = false,
        modifier = modifier.fadingEdges(scroll).horizontalScroll(scroll),
    )
}

/** Fades the start or end of a scrolling row while there is more content beyond that edge. */
private fun Modifier.fadingEdges(scroll: ScrollState, width: androidx.compose.ui.unit.Dp = 28.dp): Modifier = this
    .graphicsLayer { compositingStrategy = CompositingStrategy.Offscreen }
    .drawWithContent {
        drawContent()
        val fade = width.toPx()
        if (scroll.canScrollBackward) {
            drawRect(
                Brush.horizontalGradient(listOf(Color.Transparent, Color.Black), startX = 0f, endX = fade),
                size = Size(fade, size.height),
                blendMode = BlendMode.DstIn,
            )
        }
        if (scroll.canScrollForward) {
            drawRect(
                Brush.horizontalGradient(listOf(Color.Black, Color.Transparent), startX = size.width - fade, endX = size.width),
                topLeft = Offset(size.width - fade, 0f),
                size = Size(fade, size.height),
                blendMode = BlendMode.DstIn,
            )
        }
    }
