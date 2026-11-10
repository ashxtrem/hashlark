// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark.ui

import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Download
import androidx.compose.material.icons.filled.History
import androidx.compose.material.icons.filled.Hub
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material.icons.filled.Star
import androidx.compose.ui.graphics.vector.ImageVector

/** The top-level screens, shown as a bottom bar, a rail or a drawer depending on the window. */
enum class Destination(val label: String, val icon: ImageVector) {
    Search("Search", Icons.Filled.Search),
    Downloads("Downloads", Icons.Filled.Download),
    Favorites("Favourites", Icons.Filled.Star),
    History("History", Icons.Filled.History),
    Providers("Providers", Icons.Filled.Hub),
    Settings("Settings", Icons.Filled.Settings),
    ;

    companion object {
        /**
         * The items of the navigation. A bottom bar fits five labels, so it leaves out History,
         * which then is a tab of the Favourites screen; a rail or a drawer shows all six.
         */
        fun shownIn(nav: NavKind): List<Destination> = if (nav == NavKind.Bar) entries - History else entries

        /** History lives inside Favourites on a bar layout, so the bar marks Favourites while it is open. */
        fun highlighted(current: Destination, nav: NavKind): Destination =
            if (nav == NavKind.Bar && current == History) Favorites else current

        /** Whether Favourites and History share one screen with a tab row (a bar layout). */
        fun libraryTabs(nav: NavKind): Boolean = nav == NavKind.Bar
    }
}
