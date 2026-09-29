// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark.ui.search

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.filled.Error
import androidx.compose.material3.AssistChip
import androidx.compose.material3.AssistChipDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
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

/**
 * How the providers are doing, in one small chip so the results get the room:
 * "Searching 2 of 4", "4 providers", or "3 of 4 providers" in the error colour
 * when some failed. Tapping it (or pressing Enter on it) opens the full list, one
 * line per provider, with the way forward for a provider that needs a browser check.
 */
@Composable
fun ProviderStatusBar(
    providers: Map<String, ProviderProgress>,
    names: Map<String, String>,
    onChallenge: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    if (providers.isEmpty()) return
    val status = LocalStatusColors.current
    var open by remember { mutableStateOf(false) }

    val total = providers.size
    val running = providers.count { it.value is ProviderProgress.Running }
    val failed = providers.count { it.value is ProviderProgress.Failed }
    val label = when {
        running > 0 -> "Searching ${total - running} of $total"
        failed > 0 -> "${total - failed} of $total providers"
        else -> if (total == 1) "1 provider" else "$total providers"
    }

    Box(modifier) {
        AssistChip(
            onClick = { open = true },
            label = { Text(label) },
            leadingIcon = {
                when {
                    running > 0 -> CircularProgressIndicator(Modifier.size(16.dp), strokeWidth = 2.dp)
                    failed > 0 -> Icon(Icons.Filled.Error, contentDescription = null, tint = MaterialTheme.colorScheme.error, modifier = Modifier.size(16.dp))
                    else -> Icon(Icons.Filled.CheckCircle, contentDescription = null, tint = status.ok, modifier = Modifier.size(16.dp))
                }
            },
            colors = if (failed > 0 && running == 0) {
                AssistChipDefaults.assistChipColors(containerColor = MaterialTheme.colorScheme.errorContainer)
            } else {
                AssistChipDefaults.assistChipColors()
            },
            modifier = Modifier.semantics { contentDescription = "Provider status: $label. Tap for details." },
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
                        leadingIcon = { Icon(Icons.Filled.CheckCircle, contentDescription = "Done", tint = status.ok, modifier = Modifier.size(18.dp)) },
                        onClick = {},
                    )
                    is ProviderProgress.Failed -> {
                        val needsCheck = progress.kind == ErrorKind.ChallengeRequired
                        DropdownMenuItem(
                            text = { Text(if (needsCheck) "$name: needs a browser check. Open site to continue" else "$name: ${progress.kind.label}") },
                            leadingIcon = { Icon(Icons.Filled.Error, contentDescription = "Failed", tint = MaterialTheme.colorScheme.error, modifier = Modifier.size(18.dp)) },
                            onClick = {
                                open = false
                                if (needsCheck) onChallenge(id)
                            },
                        )
                    }
                }
            }
        }
    }
}
