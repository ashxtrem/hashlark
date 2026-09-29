// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark.ui.settings

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import io.github.ashxtrem.hashlark.core.Settings
import io.github.ashxtrem.hashlark.ui.AppViewModel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

/**
 * Settings are edited as a draft and applied together, like on the desktop:
 * nothing changes until the user saves, and the core validates the whole
 * document (a bad proxy address is refused with a message, not half-applied).
 */
class SettingsViewModel(private val app: AppViewModel) : ViewModel() {
    private val _draft = MutableStateFlow<Settings?>(null)
    val draft: StateFlow<Settings?> = _draft.asStateFlow()

    private val _saving = MutableStateFlow(false)
    val saving: StateFlow<Boolean> = _saving.asStateFlow()

    val dirty: StateFlow<Boolean> = combine(_draft, app.settings) { draft, saved -> draft != null && draft != saved }
        .stateIn(viewModelScope, SharingStarted.Eagerly, false)

    init {
        viewModelScope.launch {
            // Follow the saved settings until the user starts editing.
            app.settings.collect { saved ->
                if (saved != null && (_draft.value == null || !dirty.value)) _draft.value = saved
            }
        }
    }

    fun edit(transform: (Settings) -> Settings) = _draft.update { it?.let(transform) }

    fun discard() {
        _draft.value = app.settings.value
    }

    fun save() {
        val draft = _draft.value ?: return
        _saving.value = true
        viewModelScope.launch {
            try {
                val saved = app.saveSettings(draft)
                if (saved != null) {
                    _draft.value = saved
                    app.say("Settings saved")
                }
            } finally {
                _saving.value = false
            }
        }
    }
}
