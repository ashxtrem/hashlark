// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark.ui

import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.History
import androidx.compose.material.icons.filled.Hub
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material.icons.filled.Star
import androidx.compose.ui.graphics.vector.ImageVector

/** The top-level screens, shown as a bottom bar, a rail or a drawer depending on the window. */
enum class Destination(val label: String, val icon: ImageVector) {
    Search("Search", Icons.Filled.Search),
    Favorites("Favourites", Icons.Filled.Star),
    History("History", Icons.Filled.History),
    Providers("Providers", Icons.Filled.Hub),
    Settings("Settings", Icons.Filled.Settings),
}
