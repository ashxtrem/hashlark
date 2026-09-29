// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark.ui.common

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.VerticalDivider
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import io.github.ashxtrem.hashlark.ui.PanePlan
import io.github.ashxtrem.hashlark.ui.rememberWindowShape

/**
 * Tells the app frame that a screen is showing a full-screen detail, so it hides the main
 * navigation (Back returns to the list). Screens report it with [FullScreenDetailEffect].
 */
@Stable
class DetailChrome {
    var fullScreen by mutableStateOf(false)
}

val LocalDetailChrome = compositionLocalOf<DetailChrome?> { null }

/** While [active] is true (and this call is composed), the main navigation is hidden. */
@Composable
fun FullScreenDetailEffect(active: Boolean) {
    val chrome = LocalDetailChrome.current
    DisposableEffect(chrome, active) {
        if (active) chrome?.fullScreen = true
        onDispose { if (active) chrome?.fullScreen = false }
    }
}

/**
 * A list with a details pane, for every screen that has one (providers,
 * settings, history, favourites). The caller owns [selected]; [onDeselect] is
 * called when the user goes back to the list. When the window cannot fit both
 * panes (see [io.github.ashxtrem.hashlark.ui.WindowShape.planPanes]) the detail
 * replaces the list and Back (also the predictive-back gesture) returns to it;
 * otherwise both are shown, and next to a vertical hinge they sit on
 * either side of it.
 *
 * Which layout applies is decided by the window alone, from its size right now,
 * so folding and unfolding always land on the right one.
 */
@Composable
fun AdaptiveListDetail(
    selected: String?,
    onDeselect: () -> Unit,
    list: @Composable () -> Unit,
    detail: @Composable (key: String, onBack: (() -> Unit)?) -> Unit,
    modifier: Modifier = Modifier,
    placeholder: String = "Select an item",
) {
    val shape = rememberWindowShape()
    val plan = shape.planPanes()
    if (plan is PanePlan.Split) {
        TwoPane(
            plan = plan,
            modifier = modifier,
            list = list,
            detail = {
                if (selected != null) detail(selected, null) else DetailPlaceholder(placeholder)
            },
        )
    } else {
        BackHandler(enabled = selected != null, onBack = onDeselect)
        FullScreenDetailEffect(selected != null)
        Box(modifier) {
            if (selected != null) detail(selected, onDeselect) else list()
        }
    }
}

/** The list on the left and the details on the right, split at the hinge when there is one. */
@Composable
fun TwoPane(
    plan: PanePlan.Split,
    list: @Composable () -> Unit,
    detail: @Composable () -> Unit,
    modifier: Modifier = Modifier,
) {
    Row(modifier.fillMaxSize()) {
        Box(Modifier.width(plan.listWidth).fillMaxHeight()) { list() }
        if (plan.hingeGap > 0.dp) Spacer(Modifier.width(plan.hingeGap)) else VerticalDivider()
        Box(Modifier.weight(1f).fillMaxHeight()) { detail() }
    }
}

@Composable
fun DetailPlaceholder(text: String, modifier: Modifier = Modifier) {
    Surface(modifier.fillMaxSize(), color = MaterialTheme.colorScheme.surfaceContainerLow) {
        Row(Modifier.fillMaxSize(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.Center) {
            Text(text, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
    }
}

/** A slim title row used instead of a full app bar, to leave room on short windows. */
@Composable
fun ScreenTitle(title: String, modifier: Modifier = Modifier, actions: @Composable () -> Unit = {}) {
    Row(
        modifier.fillMaxWidth().padding(start = 16.dp, end = 8.dp, top = 8.dp, bottom = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(title, style = MaterialTheme.typography.titleLarge, modifier = Modifier.weight(1f))
        actions()
    }
}
