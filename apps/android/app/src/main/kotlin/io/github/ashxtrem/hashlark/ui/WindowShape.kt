// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark.ui

import androidx.compose.material3.adaptive.currentWindowAdaptiveInfo
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.remember
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalWindowInfo
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp

enum class WidthClass { Compact, Medium, Expanded, Large }

enum class HeightClass { Compact, Medium, Expanded }

/** The physical arrangement of a foldable's window. */
enum class Posture {
    /** Flat, or a device that does not fold. */
    Flat,

    /** Half-open with a horizontal hinge: two halves stacked. */
    Tabletop,

    /** Half-open with a vertical hinge: two halves side by side. */
    Book,
}

/**
 * Everything layouts decide on: the size of this window (not the device or
 * the screen: split screen and pop-up windows count) and the fold posture.
 * Layouts never look at the device model, so what works for the Galaxy Z Fold7
 * also works for other foldables, tablets and desktop windowing.
 */
@Immutable
data class WindowShape(
    val widthDp: Dp,
    val heightDp: Dp,
    val posture: Posture = Posture.Flat,
    /** Vertical position of the hinge in the window, as a fraction of its height (tabletop only). */
    val hingeFraction: Float = 0.5f,
    /** Where a vertical hinge starts and ends across the window's width, as fractions (book posture only). */
    val hingeStartFraction: Float = 0.5f,
    val hingeEndFraction: Float = 0.5f,
) {
    val width: WidthClass = classifyWidth(widthDp)
    val height: HeightClass = classifyHeight(heightDp)

    val isTabletop: Boolean get() = posture == Posture.Tabletop
    val isBook: Boolean get() = posture == Posture.Book

    /** Wide enough for a list and its details side by side. */
    val hasTwoPanes: Boolean get() = width >= WidthClass.Medium || isBook

    /**
     * Wide enough to add a filter panel next to the list and details. A results table
     * needs about 500 dp, so this starts at the Large class (1200 dp), not at Expanded:
     * the inner screen of a foldable gets two roomy panes instead of three cramped ones.
     */
    val hasThreePanes: Boolean get() = width >= WidthClass.Large && !isBook && !isTabletop

    /** Little vertical room, e.g. the Fold7 cover screen in landscape. */
    val isShort: Boolean get() = height == HeightClass.Compact

    companion object {
        fun classifyWidth(width: Dp): WidthClass = when {
            width < 600.dp -> WidthClass.Compact
            width < 840.dp -> WidthClass.Medium
            width < 1200.dp -> WidthClass.Expanded
            else -> WidthClass.Large
        }

        fun classifyHeight(height: Dp): HeightClass = when {
            height < 480.dp -> HeightClass.Compact
            height < 900.dp -> HeightClass.Medium
            else -> HeightClass.Expanded
        }
    }
}

/** Two halves of at least 300 dp each, or the window is not split at the hinge. */
private const val MIN_SPLIT_DP = 600

/** The shape of the current window; recomposes when it is resized, folded or rotated. */
@Composable
fun rememberWindowShape(): WindowShape {
    val size = LocalWindowInfo.current.containerSize
    val density = LocalDensity.current
    val posture = currentWindowAdaptiveInfo().windowPosture
    val hinge = posture.hingeList.firstOrNull { it.isSeparating || !it.isFlat }
    val widthDp = with(density) { size.width.toDp() }
    val heightDp = with(density) { size.height.toDp() }
    // A fold only splits the window into two panes when each half is big enough to use.
    // (A window that is much smaller than the unfolded screen, like the cover screen,
    // must never be split, even if the system still reports the hinge.)
    val tabletop = posture.isTabletop && heightDp >= MIN_SPLIT_DP.dp
    val book = !posture.isTabletop && hinge != null && hinge.isVertical && !hinge.isFlat && widthDp >= MIN_SPLIT_DP.dp
    val hingeTop = hinge?.bounds?.top
    val hingeLeft = hinge?.bounds?.left
    val hingeRight = hinge?.bounds?.right
    return remember(size, density.density, tabletop, book, hingeTop, hingeLeft, hingeRight) {
        WindowShape(
            widthDp = widthDp,
            heightDp = heightDp,
            posture = when {
                tabletop -> Posture.Tabletop
                book -> Posture.Book
                else -> Posture.Flat
            },
            hingeFraction = if (tabletop && hingeTop != null && size.height > 0) {
                (hingeTop / size.height).coerceIn(0.2f, 0.8f)
            } else {
                0.5f
            },
            hingeStartFraction = if (book && hingeLeft != null && size.width > 0) (hingeLeft / size.width).coerceIn(0.3f, 0.7f) else 0.5f,
            hingeEndFraction = if (book && hingeRight != null && size.width > 0) (hingeRight / size.width).coerceIn(0.3f, 0.7f) else 0.5f,
        )
    }
}
