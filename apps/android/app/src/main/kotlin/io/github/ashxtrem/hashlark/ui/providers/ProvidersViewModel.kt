// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark.ui.providers

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import io.github.ashxtrem.hashlark.core.DefinitionCheck
import io.github.ashxtrem.hashlark.core.ProviderPatch
import io.github.ashxtrem.hashlark.core.ProviderView
import io.github.ashxtrem.hashlark.core.RepoPreview
import io.github.ashxtrem.hashlark.core.RepoView
import io.github.ashxtrem.hashlark.core.SyncReport
import io.github.ashxtrem.hashlark.core.TestReport
import io.github.ashxtrem.hashlark.ui.AppViewModel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

/** Provider management: enabling, settings, tests, adding and removing. */
class ProvidersViewModel(private val app: AppViewModel) : ViewModel() {
    private val _testing = MutableStateFlow<Set<String>>(emptySet())

    /** Providers whose test is running. */
    val testing: StateFlow<Set<String>> = _testing.asStateFlow()

    private val _reports = MutableStateFlow<Map<String, TestReport>>(emptyMap())

    /** The last test result per provider. */
    val reports: StateFlow<Map<String, TestReport>> = _reports.asStateFlow()

    private val _saving = MutableStateFlow(false)
    val saving: StateFlow<Boolean> = _saving.asStateFlow()

    fun setEnabled(provider: ProviderView, enabled: Boolean) {
        // Show the change at once; the core's answer replaces it.
        app.putProvider(provider.copy(enabled = enabled))
        viewModelScope.launch {
            val updated = app.withApi("Could not change ${provider.name}") { it.updateProvider(provider.id, ProviderPatch(enabled = enabled)) }
            if (updated != null) app.putProvider(updated) else app.putProvider(provider)
        }
    }

    /** Applies [patch]; returns the updated provider, or `null` after showing the error. */
    suspend fun update(provider: ProviderView, patch: ProviderPatch, done: String? = null): ProviderView? {
        _saving.value = true
        try {
            val updated = app.withApi("Could not save") { it.updateProvider(provider.id, patch) }
            if (updated != null) {
                app.putProvider(updated)
                done?.let(app::say)
            }
            return updated
        } finally {
            _saving.value = false
        }
    }

    fun test(provider: ProviderView) {
        if (provider.id in _testing.value) return
        _testing.update { it + provider.id }
        viewModelScope.launch {
            try {
                app.withApi("Test failed") { it.testProvider(provider.id) }?.let { report ->
                    _reports.update { it + (provider.id to report) }
                }
                app.refreshProviders()
            } finally {
                _testing.update { it - provider.id }
            }
        }
    }

    fun remove(provider: ProviderView) {
        viewModelScope.launch {
            val done = app.withApi("Could not remove ${provider.name}") { it.removeProvider(provider.id) }
            if (done != null) {
                app.removeProviderLocally(provider.id)
                app.say("${provider.name} removed")
            }
        }
    }

    suspend fun addTorznab(name: String, url: String, apiKey: String?): ProviderView? {
        _saving.value = true
        try {
            val added = app.withApi("Could not add the provider") { it.addTorznab(name.trim(), url.trim(), apiKey) }
            if (added != null) {
                app.putProvider(added)
                app.say("${added.name} added")
            }
            return added
        } finally {
            _saving.value = false
        }
    }

    suspend fun checkDefinition(yaml: String): DefinitionCheck? = app.withApi { it.checkDefinition(yaml) }

    suspend fun addDefinition(yaml: String): ProviderView? {
        _saving.value = true
        try {
            val added = app.withApi { it.addDefinition(yaml) }
            if (added != null) {
                app.putProvider(added)
                app.say("${added.name} added")
            }
            return added
        } finally {
            _saving.value = false
        }
    }
}

/** Definition repositories, with the trust-on-first-use check. */
class ReposViewModel(private val app: AppViewModel) : ViewModel() {
    private val _repos = MutableStateFlow<List<RepoView>?>(null)
    val repos: StateFlow<List<RepoView>?> = _repos.asStateFlow()

    private val _busy = MutableStateFlow<Set<String>>(emptySet())

    /** Repository ids being synced; `""` while a repository is being added or previewed. */
    val busy: StateFlow<Set<String>> = _busy.asStateFlow()

    private val _preview = MutableStateFlow<RepoPreview?>(null)

    /** The repository the user is being asked to trust. */
    val preview: StateFlow<RepoPreview?> = _preview.asStateFlow()

    init {
        refresh()
    }

    fun refresh() {
        viewModelScope.launch {
            app.withApi { _repos.value = it.repos() }
        }
    }

    /** Fetches the repository's signed index and asks the user to confirm its key. */
    fun requestAdd(url: String) {
        _busy.update { it + "" }
        viewModelScope.launch {
            try {
                app.withApi("Could not read the repository") { it.previewRepo(url.trim()) }?.let { preview ->
                    if (preview.exists) app.say("This repository is already added") else _preview.value = preview
                }
            } finally {
                _busy.update { it - "" }
            }
        }
    }

    fun cancelAdd() {
        _preview.value = null
    }

    /** The user trusts the shown key: add the repository and install its definitions (disabled). */
    fun confirmAdd() {
        val preview = _preview.value ?: return
        _preview.value = null
        _busy.update { it + "" }
        viewModelScope.launch {
            try {
                app.withApi("Could not add the repository") { it.addRepo(preview.url) }?.let { added ->
                    app.say("Added ${added.repo.name ?: "repository"}: ${summary(added.sync)}")
                    added.sync.errors.forEach(app::say)
                }
                refresh()
                app.refreshProviders()
            } finally {
                _busy.update { it - "" }
            }
        }
    }

    fun sync(repo: RepoView) {
        _busy.update { it + repo.id }
        viewModelScope.launch {
            try {
                app.withApi("Sync failed") { it.syncRepo(repo.id) }?.let { report ->
                    app.say("${repo.name ?: repo.id}: ${summary(report)}")
                }
                refresh()
                app.refreshProviders()
            } finally {
                _busy.update { it - repo.id }
            }
        }
    }

    fun remove(repo: RepoView) {
        viewModelScope.launch {
            if (app.withApi("Could not remove the repository") { it.removeRepo(repo.id) } != null) {
                app.say("Repository removed")
                refresh()
                app.refreshProviders()
            }
        }
    }

    companion object {
        fun summary(report: SyncReport): String {
            val parts = buildList {
                if (report.added.isNotEmpty()) add("${report.added.size} new (disabled until you enable them)")
                if (report.updated.isNotEmpty()) add("${report.updated.size} updated")
                if (report.removed.isNotEmpty()) add("${report.removed.size} removed")
                if (isEmpty()) add("no changes")
                if (report.errors.isNotEmpty()) add("${report.errors.size} skipped")
            }
            return parts.joinToString(", ")
        }
    }
}
