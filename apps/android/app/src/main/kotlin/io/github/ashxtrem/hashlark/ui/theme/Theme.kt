// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark.ui.theme

import android.os.Build
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.ColorScheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.dynamicDarkColorScheme
import androidx.compose.material3.dynamicLightColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.remember
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import io.github.ashxtrem.hashlark.core.Theme

/** Status colours Material 3 has no slot for. */
@Immutable
data class StatusColors(
    val ok: Color,
    val okContainer: Color,
    val warn: Color,
    val warnContainer: Color,
)

val LocalStatusColors = staticCompositionLocalOf {
    StatusColors(Color(0xFF13855B), Color(0xFFE3F6EE), Color(0xFFA86400), Color(0xFFFFF3DE))
}

/** The navigation surface (bottom bar, rail, sidebar): deep green in the Hashlark palette, tonal with dynamic colour. */
@Immutable
data class NavColors(
    val container: Color,
    val content: Color,
    val indicator: Color,
    val onIndicator: Color,
)

val LocalNavColors = staticCompositionLocalOf {
    NavColors(Color(0xFF143B32), Color(0xFFDAE9DF), Color(0xFFC9E7A8), Color(0xFF14300A))
}

// The Hashlark palette, from the adaptive UI study (docs/ui-mock): a warm neutral canvas, white surfaces,
// forest-green actions, a mint selection and a lime accent. Text colours were checked for at least
// 4.5:1 against the surfaces they sit on.
private val LightScheme = lightColorScheme(
    primary = Color(0xFF205B46),
    onPrimary = Color(0xFFFFFFFF),
    primaryContainer = Color(0xFFEDF5E9),
    onPrimaryContainer = Color(0xFF143B32),
    secondary = Color(0xFF4F6356),
    onSecondary = Color(0xFFFFFFFF),
    secondaryContainer = Color(0xFFC9E7A8),
    onSecondaryContainer = Color(0xFF14300A),
    tertiary = Color(0xFF8F5300),
    onTertiary = Color(0xFFFFFFFF),
    tertiaryContainer = Color(0xFFFFF0D6),
    onTertiaryContainer = Color(0xFF3A2500),
    background = Color(0xFFF5F6F2),
    onBackground = Color(0xFF202E29),
    surface = Color(0xFFFFFFFF),
    onSurface = Color(0xFF202E29),
    surfaceVariant = Color(0xFFE9EDE5),
    onSurfaceVariant = Color(0xFF5B665F),
    surfaceContainerLowest = Color(0xFFFFFFFF),
    surfaceContainerLow = Color(0xFFFAFBF7),
    surfaceContainer = Color(0xFFF1F3EE),
    surfaceContainerHigh = Color(0xFFEBEEE7),
    surfaceContainerHighest = Color(0xFFE5E9E0),
    outline = Color(0xFF7A857E),
    outlineVariant = Color(0xFFE2E7DF),
    error = Color(0xFFB3261E),
    onError = Color(0xFFFFFFFF),
    errorContainer = Color(0xFFFBE7E4),
    onErrorContainer = Color(0xFF5C0A12),
)

private val DarkScheme = darkColorScheme(
    primary = Color(0xFF9FD3B2),
    onPrimary = Color(0xFF0A2B1E),
    primaryContainer = Color(0xFF213B31),
    onPrimaryContainer = Color(0xFFD5EFC0),
    secondary = Color(0xFFB4CCB9),
    onSecondary = Color(0xFF20352A),
    secondaryContainer = Color(0xFF344B3B),
    onSecondaryContainer = Color(0xFFD5EFC0),
    tertiary = Color(0xFFF0B04A),
    onTertiary = Color(0xFF2E1D00),
    tertiaryContainer = Color(0xFF3A2C12),
    onTertiaryContainer = Color(0xFFFFDEA8),
    background = Color(0xFF101512),
    onBackground = Color(0xFFE2E9E3),
    surface = Color(0xFF171D1A),
    onSurface = Color(0xFFE2E9E3),
    surfaceVariant = Color(0xFF232B27),
    onSurfaceVariant = Color(0xFFA7B2AB),
    surfaceContainerLowest = Color(0xFF0C110E),
    surfaceContainerLow = Color(0xFF141A17),
    surfaceContainer = Color(0xFF1A211E),
    surfaceContainerHigh = Color(0xFF212925),
    surfaceContainerHighest = Color(0xFF29322E),
    outline = Color(0xFF75827A),
    outlineVariant = Color(0xFF2E3833),
    error = Color(0xFFFF8A80),
    onError = Color(0xFF3D0A10),
    errorContainer = Color(0xFF3D1A1F),
    onErrorContainer = Color(0xFFFFDADD),
)

private val LightStatus = StatusColors(Color(0xFF13855B), Color(0xFFE3F6EE), Color(0xFFA86400), Color(0xFFFFF3DE))
private val DarkStatus = StatusColors(Color(0xFF4CC38A), Color(0xFF173327), Color(0xFFF0B04A), Color(0xFF3A2C12))

private val LightNav = NavColors(Color(0xFF143B32), Color(0xFFDAE9DF), Color(0xFFC9E7A8), Color(0xFF14300A))
private val DarkNav = NavColors(Color(0xFF0E1E19), Color(0xFFC3D3C9), Color(0xFF344B3B), Color(0xFFD5EFC0))

/** Whether the app should draw dark: the in-app choice wins over the system's. */
@Composable
fun isDark(theme: Theme): Boolean = when (theme) {
    Theme.Light -> false
    Theme.Dark -> true
    Theme.System -> isSystemInDarkTheme()
}

@Composable
fun HashlarkTheme(
    theme: Theme = Theme.System,
    dynamicColor: Boolean = true,
    content: @Composable () -> Unit,
) {
    val dark = isDark(theme)
    val context = LocalContext.current
    val dynamic = dynamicColor && Build.VERSION.SDK_INT >= Build.VERSION_CODES.S
    val scheme: ColorScheme = when {
        dynamic -> if (dark) dynamicDarkColorScheme(context) else dynamicLightColorScheme(context)
        dark -> DarkScheme
        else -> LightScheme
    }
    // With the wallpaper's colours the navigation follows the scheme instead of the fixed green.
    val nav = remember(scheme, dark, dynamic) {
        if (dynamic) NavColors(scheme.surfaceContainer, scheme.onSurfaceVariant, scheme.secondaryContainer, scheme.onSecondaryContainer) else if (dark) DarkNav else LightNav
    }
    CompositionLocalProvider(LocalStatusColors provides if (dark) DarkStatus else LightStatus, LocalNavColors provides nav) {
        MaterialTheme(colorScheme = scheme, content = content)
    }
}
