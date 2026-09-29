// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark.ui

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.WindowInsetsSides
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.isImeVisible
import androidx.compose.foundation.layout.only
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.SnackbarDuration
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.SnackbarResult
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.adaptive.navigationsuite.NavigationSuiteScaffold
import androidx.compose.material3.adaptive.navigationsuite.NavigationSuiteType
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.saveable.rememberSaveableStateHolder
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.github.ashxtrem.hashlark.ui.common.CenteredMessage
import io.github.ashxtrem.hashlark.ui.common.LoadingBox
import io.github.ashxtrem.hashlark.ui.library.FavoritesScreen
import io.github.ashxtrem.hashlark.ui.library.HistoryScreen
import io.github.ashxtrem.hashlark.ui.providers.AddProviderDialog
import io.github.ashxtrem.hashlark.ui.providers.ProvidersScreen
import io.github.ashxtrem.hashlark.ui.providers.ProvidersViewModel
import io.github.ashxtrem.hashlark.ui.providers.ReposViewModel
import io.github.ashxtrem.hashlark.ui.providers.TrustDialog
import io.github.ashxtrem.hashlark.ui.search.SearchScreen
import io.github.ashxtrem.hashlark.ui.search.SearchViewModel
import io.github.ashxtrem.hashlark.ui.settings.SettingsScreen
import io.github.ashxtrem.hashlark.ui.settings.SettingsViewModel

/** Everything the root needs; the activity creates these once and they survive rotation and folding. */
class Screens(
    val app: AppViewModel,
    val search: SearchViewModel,
    val library: LibraryViewModel,
    val providers: ProvidersViewModel,
    val repos: ReposViewModel,
    val settings: SettingsViewModel,
)

/**
 * The app's frame: the navigation (bottom bar, rail or permanent drawer, or none
 * in tabletop posture), the current screen, and the dialogs and messages that
 * belong to no single screen.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun HashlarkRoot(screens: Screens, pendingDefinition: PendingDefinition?, onDefinitionHandled: () -> Unit) {
    val app = screens.app
    val shape = rememberWindowShape()
    var destination by rememberSaveable { mutableStateOf(Destination.Search) }
    val snackbar = remember { SnackbarHostState() }
    val searchFocus = remember { FocusRequester() }
    val saveable = rememberSaveableStateHolder()
    val startError by app.startError.collectAsStateWithLifecycle()
    val settings by app.settings.collectAsStateWithLifecycle()
    val update by app.visibleUpdate.collectAsStateWithLifecycle()
    val trustPreview by screens.repos.preview.collectAsStateWithLifecycle()

    val actions = rememberResultActions(app) {
        // A browser check finished: the provider may work now, so search again.
        app.refreshProviders()
        if (screens.search.state.value.shown != null) screens.search.run()
    }

    LaunchedEffect(Unit) {
        app.events.collect { event ->
            when (event) {
                is AppEvent.Navigate -> destination = event.destination
                is AppEvent.RunSearch -> {
                    destination = Destination.Search
                    screens.search.run(event.query)
                }
                is AppEvent.ImportRepo -> {
                    destination = Destination.Providers
                    screens.repos.requestAdd(event.url)
                }
                is AppEvent.ImportDefinition -> Unit // shown by the activity through pendingDefinition
            }
        }
    }
    LaunchedEffect(Unit) {
        app.messages.collect { message ->
            val result = snackbar.showSnackbar(message.text, message.actionLabel, duration = SnackbarDuration.Short)
            if (result == SnackbarResult.ActionPerformed) message.onAction?.invoke()
        }
    }

    BackHandler(enabled = destination != Destination.Search) { destination = Destination.Search }

    val imeVisible = WindowInsets.isImeVisible
    val baseLayout = when {
        shape.isTabletop -> NavigationSuiteType.None
        shape.width == WidthClass.Large -> NavigationSuiteType.NavigationDrawer
        shape.isShort || shape.width >= WidthClass.Medium -> NavigationSuiteType.NavigationRail
        else -> NavigationSuiteType.NavigationBar
    }
    // A bottom bar would sit on top of the keyboard and eat the little room left.
    val layoutType = if (baseLayout == NavigationSuiteType.NavigationBar && imeVisible) NavigationSuiteType.None else baseLayout
    val barShown = layoutType == NavigationSuiteType.NavigationBar
    // The bottom bar looks after the bottom inset itself; every other layout needs it here.
    val sides = WindowInsetsSides.Top + WindowInsetsSides.Horizontal
    val insets = WindowInsets.safeDrawing.only(if (barShown) sides else sides + WindowInsetsSides.Bottom)

    NavigationSuiteScaffold(
        layoutType = layoutType,
        navigationSuiteItems = {
            Destination.entries.forEach { item ->
                item(
                    selected = item == destination,
                    onClick = { destination = item },
                    icon = { Icon(item.icon, contentDescription = null) },
                    label = { Text(item.label) },
                )
            }
        },
    ) {
        Box(Modifier.fillMaxSize()) {
            Column(Modifier.fillMaxSize().windowInsetsPadding(insets)) {
                update?.let { available ->
                    UpdateBanner(
                        version = available.version,
                        onView = { actions.viewOnSite(available.pageUrl) },
                        onDismiss = app::dismissUpdate,
                    )
                }
                Box(Modifier.weight(1f).fillMaxWidth()) {
                    when {
                        startError != null -> CenteredMessage(
                            "Hashlark could not start",
                            body = startError,
                            action = { Button(onClick = app::retryStart) { Text("Try again") } },
                        )
                        settings == null -> LoadingBox(label = "Starting…")
                        else -> saveable.SaveableStateProvider(destination.name) {
                            Screen(destination, shape, screens, actions, searchFocus)
                        }
                    }
                }
            }
            SnackbarHost(
                snackbar,
                Modifier.align(Alignment.BottomCenter).windowInsetsPadding(WindowInsets.safeDrawing.only(WindowInsetsSides.Bottom)).padding(8.dp),
            )
        }
    }

    // ----- dialogs that belong to no screen ---------------------------------------------------------------

    if (settings?.ui?.firstRunDone == false) {
        WelcomeDialog { app.updateSettings { it.copy(ui = it.ui.copy(firstRunDone = true)) } }
    }
    actions.noHandlerMagnet?.let { magnet ->
        NoHandlerDialog(
            onCopy = {
                actions.copyText("Magnet link", magnet)
                app.say("Magnet link copied")
                actions.noHandlerMagnet = null
            },
            onShare = {
                actions.shareText("Magnet link", magnet)
                actions.noHandlerMagnet = null
            },
            onDismiss = { actions.noHandlerMagnet = null },
        )
    }
    trustPreview?.let { TrustDialog(it, onTrust = screens.repos::confirmAdd, onDismiss = screens.repos::cancelAdd) }
    pendingDefinition?.let { pending ->
        AddProviderDialog(
            vm = screens.providers,
            initialYaml = pending.yaml,
            onDismiss = onDefinitionHandled,
            onAdded = { destination = Destination.Providers },
        )
    }
}

/** A provider definition that arrived from another app and awaits confirmation. */
data class PendingDefinition(val yaml: String, val fileName: String?)

@Composable
private fun Screen(
    destination: Destination,
    shape: WindowShape,
    screens: Screens,
    actions: ResultActions,
    searchFocus: FocusRequester,
) {
    val modifier = Modifier.fillMaxSize()
    when (destination) {
        Destination.Search -> SearchScreen(shape, screens.app, screens.search, screens.library, actions, searchFocus, modifier)
        Destination.Favorites -> FavoritesScreen(screens.app, screens.library, actions, modifier)
        Destination.History -> HistoryScreen(screens.library, onRun = { query ->
            screens.app.post(AppEvent.RunSearch(query))
        }, modifier = modifier)
        Destination.Providers -> ProvidersScreen(screens.app, screens.providers, screens.repos, actions, modifier)
        Destination.Settings -> SettingsScreen(screens.app, screens.settings, modifier)
    }
}

@Composable
private fun UpdateBanner(version: String, onView: () -> Unit, onDismiss: () -> Unit) {
    Surface(color = MaterialTheme.colorScheme.primaryContainer, contentColor = MaterialTheme.colorScheme.onPrimaryContainer) {
        Row(
            Modifier.fillMaxWidth().padding(start = 16.dp, end = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            Text("Version $version is available", Modifier.weight(1f))
            TextButton(onClick = onView) { Text("View release") }
            IconButton(onClick = onDismiss) { Icon(Icons.Filled.Close, contentDescription = "Dismiss") }
        }
    }
}

@Composable
private fun WelcomeDialog(onAccept: () -> Unit) {
    AlertDialog(
        onDismissRequest = {},
        title = { Text("Welcome to Hashlark") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text("Hashlark searches many torrent indexers at once and hands what you pick to your torrent client.")
                Text("• It starts with legal sources only, such as the Internet Archive. You can add other providers under Providers.")
                Text("• Hashlark hosts no content and doesn't download anything itself.")
                Text(
                    "• No telemetry. Hashlark only contacts the providers you enable, definition repositories you add, " +
                        "and GitHub to check for updates (you can turn this off).",
                )
                Text(
                    "You are responsible for complying with the law and copyright where you live.",
                    color = MaterialTheme.colorScheme.tertiary,
                )
            }
        },
        confirmButton = { Button(onClick = onAccept) { Text("I understand, get started") } },
    )
}

@Composable
private fun NoHandlerDialog(onCopy: () -> Unit, onShare: () -> Unit, onDismiss: () -> Unit) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("No torrent client found") },
        text = { Text("No app on this device opens magnet links. Install a torrent client, or copy or share the link to use it elsewhere.") },
        confirmButton = { TextButton(onClick = onCopy) { Text("Copy magnet") } },
        dismissButton = {
            Row {
                TextButton(onClick = onShare) { Text("Share") }
                TextButton(onClick = onDismiss) { Text("Close") }
            }
        },
    )
}
