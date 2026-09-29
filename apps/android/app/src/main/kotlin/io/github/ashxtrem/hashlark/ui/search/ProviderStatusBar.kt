// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark.ui.search

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.filled.Error
import androidx.compose.material.icons.filled.PauseCircle
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material3.AssistChip
import androidx.compose.material3.AssistChipDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import io.github.ashxtrem.hashlark.core.ErrorKind
import io.github.ashxtrem.hashlark.core.Format
import io.github.ashxtrem.hashlark.core.ProviderProgress
import io.github.ashxtrem.hashlark.ui.theme.LocalStatusColors

/** What the providers have done so far, as counts; one place so the label, the tests and the screen reader agree. */
data class ProviderSummary(val total: Int, val running: Int, val answered: Int, val failed: Int, val stopped: Int) {
    val searching: Boolean get() = running > 0

    /** Failures other than "the user stopped the search". */
    val realFailures: Int get() = failed - stopped

    /** Providers that finished with an answer are counted; ones that failed or were stopped are not called successful. */
    val label: String
        get() = when {
            running > 0 -> "Searching · $answered of $total answered"
            stopped > 0 -> "Stopped · $answered of $total answered"
            failed > 0 -> "$answered of $total answered · $failed failed"
            total == 1 -> "Provider answered"
            else -> "All $total providers answered"
        }

    companion object {
        fun of(providers: Map<String, ProviderProgress>): ProviderSummary = ProviderSummary(
            total = providers.size,
            running = providers.count { it.value is ProviderProgress.Running },
            answered = providers.count { it.value is ProviderProgress.Finished },
            failed = providers.count { it.value is ProviderProgress.Failed },
            stopped = providers.count { (it.value as? ProviderProgress.Failed)?.wasCancelled == true },
        )
    }
}

/**
 * How the providers are doing, in one chip so the results get the room: "Searching · 2 of 4 answered",
 * "All 4 providers answered", or "3 of 4 answered · 1 failed" in the error colours. A provider
 * counts as answered only when it delivered a result set; a failed, timed-out or stopped request
 * never counts. Tapping the chip opens the full list, one line per provider, with the way forward
 * for a provider that needs a browser check.
 */
@Composable
fun ProviderStatusBar(
    providers: Map<String, ProviderProgress>,
    names: Map<String, String>,
    onChallenge: (String) -> Unit,
    onRetry: () -> Unit,
    modifier: Modifier = Modifier,
) {
    if (providers.isEmpty()) return
    val status = LocalStatusColors.current
    var open by remember { mutableStateOf(false) }
    val summary = ProviderSummary.of(providers)
    val problem = summary.failed > 0 && !summary.searching

    Box(modifier) {
        AssistChip(
            onClick = { open = true },
            label = { Text(summary.label) },
            leadingIcon = {
                when {
                    summary.searching -> CircularProgressIndicator(Modifier.size(18.dp), strokeWidth = 2.dp)
                    summary.stopped > 0 -> Icon(Icons.Filled.PauseCircle, contentDescription = null, modifier = Modifier.size(18.dp))
                    summary.failed > 0 -> Icon(Icons.Filled.Error, contentDescription = null, tint = MaterialTheme.colorScheme.error, modifier = Modifier.size(18.dp))
                    else -> Icon(Icons.Filled.CheckCircle, contentDescription = null, tint = status.ok, modifier = Modifier.size(18.dp))
                }
            },
            colors = if (problem && summary.stopped == 0) {
                AssistChipDefaults.assistChipColors(containerColor = MaterialTheme.colorScheme.errorContainer, labelColor = MaterialTheme.colorScheme.onErrorContainer)
            } else {
                AssistChipDefaults.assistChipColors()
            },
            modifier = Modifier.semantics { contentDescription = "Provider status: ${summary.label}. Opens details." },
        )
        DropdownMenu(expanded = open, onDismissRequest = { open = false }) {
            providers.forEach { (id, progress) ->
                val name = names[id] ?: id
                when (progress) {
                    ProviderProgress.Running -> DropdownMenuItem(
                        text = { Text("$name: searching…") },
                        leadingIcon = { CircularProgressIndicator(Modifier.size(18.dp), strokeWidth = 2.dp) },
                        onClick = {},
                    )
                    is ProviderProgress.Finished -> DropdownMenuItem(
                        text = {
                            val results = if (progress.count == 1) "1 result" else "${progress.count} results"
                            Text("$name: $results in ${Format.duration(progress.latencyMs)}")
                        },
                        leadingIcon = { Icon(Icons.Filled.CheckCircle, contentDescription = "Answered", tint = status.ok, modifier = Modifier.size(18.dp)) },
                        onClick = {},
                    )
                    is ProviderProgress.Failed -> {
                        val needsCheck = progress.kind == ErrorKind.ChallengeRequired
                        DropdownMenuItem(
                            text = {
                                Text(
                                    when {
                                        progress.wasCancelled -> "$name: stopped before it answered"
                                        needsCheck -> "$name: needs a browser check. Open site to continue"
                                        else -> "$name: ${progress.kind.label}"
                                    },
                                )
                            },
                            leadingIcon = {
                                if (progress.wasCancelled) {
                                    Icon(Icons.Filled.PauseCircle, contentDescription = "Stopped", modifier = Modifier.size(18.dp))
                                } else {
                                    Icon(Icons.Filled.Error, contentDescription = "Failed", tint = MaterialTheme.colorScheme.error, modifier = Modifier.size(18.dp))
                                }
                            },
                            onClick = {
                                open = false
                                if (needsCheck) onChallenge(id)
                            },
                        )
                    }
                }
            }
            if (!summary.searching && summary.failed > 0) {
                HorizontalDivider()
                DropdownMenuItem(
                    text = { Text("Search again") },
                    leadingIcon = { Icon(Icons.Filled.Refresh, contentDescription = null, modifier = Modifier.size(18.dp)) },
                    onClick = {
                        open = false
                        onRetry()
                    },
                )
            }
        }
    }
}
