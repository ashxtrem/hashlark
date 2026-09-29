// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark.ui.search

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import io.github.ashxtrem.hashlark.core.Category
import io.github.ashxtrem.hashlark.core.HashlarkRuntime
import io.github.ashxtrem.hashlark.core.MergedResult
import io.github.ashxtrem.hashlark.core.ResultSorter
import io.github.ashxtrem.hashlark.core.SearchProgress
import io.github.ashxtrem.hashlark.core.SearchQuery
import io.github.ashxtrem.hashlark.core.SortDirection
import io.github.ashxtrem.hashlark.core.SortOrder
import io.github.ashxtrem.hashlark.ui.AppViewModel
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

/** Everything the search screen shows and edits. */
data class SearchUiState(
    val text: String = "",
    val imdbId: String? = null,
    val categories: Set<Category> = emptySet(),
    /** `null` searches every enabled provider. */
    val providerIds: Set<String>? = null,
    val order: SortOrder = SortOrder.Date,
    val direction: SortDirection = SortDirection.Descending,
    /** The query whose results are shown. */
    val shown: SearchQuery? = null,
    val progress: SearchProgress = SearchProgress(),
    val running: Boolean = false,
    val error: String? = null,
    /** The user stopped the search; the results shown are what had arrived by then. */
    val cancelled: Boolean = false,
    val selectedId: String? = null,
    /** Narrow windows show the selected result's details instead of the list. */
    val detailOpen: Boolean = false,
    val page: Int = 1,
) {
    val sorted: List<MergedResult> by lazy { ResultSorter.sort(progress.results.values, order, direction) }

    val canSearch: Boolean get() = text.isNotBlank() || !imdbId.isNullOrBlank()

    val idle: Boolean get() = shown == null

    fun toQuery(page: Int = 1) = SearchQuery(
        text = text.trim(),
        categories = categories.toList(),
        providers = providerIds?.toList(),
        imdbId = imdbId,
        page = page,
        sort = order,
    )
}

/**
 * The running search. It is activity-scoped, so the query, filters, results
 * and selection survive rotating and folding the device; the search itself
 * keeps streaming results while the window changes shape.
 */
class SearchViewModel(private val runtime: HashlarkRuntime) : ViewModel() {
    private val _state = MutableStateFlow(SearchUiState())
    val state: StateFlow<SearchUiState> = _state.asStateFlow()

    private var job: Job? = null

    fun setText(text: String) = _state.update { it.copy(text = text, imdbId = null) }

    fun toggleCategory(category: Category) = _state.update {
        it.copy(categories = if (category in it.categories) it.categories - category else it.categories + category)
    }

    fun clearCategories() = _state.update { it.copy(categories = emptySet()) }

    fun setProviders(ids: Set<String>?) = _state.update { it.copy(providerIds = ids) }

    /** The filter sheet's Apply: categories, providers and sort change together, and nothing is searched yet. */
    fun applyFilters(categories: Set<Category>, providerIds: Set<String>?, order: SortOrder, direction: SortDirection) =
        _state.update { it.copy(categories = categories, providerIds = providerIds, order = order, direction = direction) }

    /** Back to searching every enabled provider in every category. The sort order is a view setting and stays. */
    fun resetFilters() = _state.update { it.copy(categories = emptySet(), providerIds = null) }

    /** Header tap: descending the first time, ascending the second. */
    fun toggleSort(order: SortOrder) = _state.update {
        if (it.order == order) {
            it.copy(direction = if (it.direction == SortDirection.Descending) SortDirection.Ascending else SortDirection.Descending)
        } else {
            it.copy(order = order, direction = SortDirection.Descending)
        }
    }

    /** Menu choice: the chosen order, starting at descending. */
    fun pickSort(order: SortOrder) = _state.update { it.copy(order = order, direction = SortDirection.Descending) }

    fun select(id: String?) = _state.update { it.copy(selectedId = id, detailOpen = id != null) }

    /** Back from the details to the list; the result stays highlighted. */
    fun closeDetail() = _state.update { it.copy(detailOpen = false) }

    /** Loads a query from outside (history, share sheet, shortcut) and runs it. */
    fun run(query: SearchQuery) {
        _state.update {
            it.copy(
                text = query.text,
                imdbId = query.imdbId,
                categories = query.categories.toSet(),
                providerIds = query.providers?.toSet(),
                order = if (query.sort == SortOrder.Relevance) it.order else query.sort,
            )
        }
        run()
    }

    /** Runs the search as edited on screen. */
    fun run() {
        val state = _state.value
        if (!state.canSearch) return
        start(state.toQuery(), append = false)
    }

    /** Fetches the next page and adds it to the results already shown. */
    fun loadMore() {
        val state = _state.value
        val shown = state.shown ?: return
        if (state.running) return
        start(shown.copy(page = shown.page + 1), append = true)
    }

    fun cancel() {
        job?.cancel()
        job = null
        _state.update { it.copy(running = false, cancelled = true, progress = it.progress.cancelled()) }
    }

    private fun start(query: SearchQuery, append: Boolean) {
        job?.cancel()
        _state.update {
            it.copy(
                shown = query,
                page = query.page,
                running = true,
                error = null,
                cancelled = false,
                progress = if (append) it.progress.copy(done = false, providers = emptyMap()) else SearchProgress(),
                selectedId = if (append) it.selectedId else null,
                detailOpen = append && it.detailOpen,
            )
        }
        job = viewModelScope.launch {
            try {
                runtime.api().search(query).collect { event ->
                    _state.update { it.copy(progress = it.progress.reduce(event)) }
                }
                _state.update { it.copy(running = false) }
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                _state.update { it.copy(running = false, error = AppViewModel.describe(e)) }
            }
        }
    }

    override fun onCleared() {
        job?.cancel()
    }
}
