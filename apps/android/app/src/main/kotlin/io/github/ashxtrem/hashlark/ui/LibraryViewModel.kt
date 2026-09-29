// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark.ui

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import io.github.ashxtrem.hashlark.core.Favorite
import io.github.ashxtrem.hashlark.core.HistoryEntry
import io.github.ashxtrem.hashlark.core.MergedResult
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.launch

/** Favourites and search history. */
class LibraryViewModel(private val app: AppViewModel) : ViewModel() {
    private val _favorites = MutableStateFlow<List<Favorite>?>(null)
    val favorites: StateFlow<List<Favorite>?> = _favorites.asStateFlow()

    private val _history = MutableStateFlow<List<HistoryEntry>?>(null)
    val history: StateFlow<List<HistoryEntry>?> = _history.asStateFlow()

    val favoriteIds: StateFlow<Set<String>> = _favorites
        .map { list -> list.orEmpty().map { it.result.id }.toSet() }
        .stateIn(viewModelScope, SharingStarted.Eagerly, emptySet())

    init {
        refreshFavorites()
    }

    fun refreshFavorites() {
        viewModelScope.launch {
            app.withApi { _favorites.value = it.favorites() }
        }
    }

    fun refreshHistory() {
        viewModelScope.launch {
            app.withApi { _history.value = it.history(100) }
        }
    }

    /** Saves the result, or removes it when it is already saved. */
    fun toggleFavorite(result: MergedResult) {
        viewModelScope.launch {
            if (result.id in favoriteIds.value) {
                app.withApi("Could not remove the favourite") { it.removeFavorite(result.id) }
                _favorites.value = _favorites.value.orEmpty().filterNot { it.result.id == result.id }
            } else {
                app.withApi("Could not save the favourite") { it.addFavorite(result.id) }?.let { saved ->
                    _favorites.value = listOf(saved) + _favorites.value.orEmpty()
                }
            }
        }
    }

    fun clearHistory() {
        viewModelScope.launch {
            app.withApi("Could not clear the history") { it.clearHistory() }?.let { _history.value = emptyList() }
        }
    }
}
