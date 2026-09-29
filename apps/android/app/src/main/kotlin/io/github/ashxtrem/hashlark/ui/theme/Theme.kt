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

// The Hashlark palette: the same tokens as the desktop app (apps/desktop/src/app.css).
private val LightScheme = lightColorScheme(
    primary = Color(0xFF5B4EF0),
    onPrimary = Color(0xFFFFFFFF),
    primaryContainer = Color(0xFFECEBFE),
    onPrimaryContainer = Color(0xFF2A1FA8),
    secondary = Color(0xFF6B6B80),
    onSecondary = Color(0xFFFFFFFF),
    secondaryContainer = Color(0xFFE6E5F5),
    onSecondaryContainer = Color(0xFF1B1B29),
    tertiary = Color(0xFFA86400),
    onTertiary = Color(0xFFFFFFFF),
    tertiaryContainer = Color(0xFFFFF3DE),
    onTertiaryContainer = Color(0xFF3A2500),
    background = Color(0xFFF7F7FB),
    onBackground = Color(0xFF1B1B29),
    surface = Color(0xFFFFFFFF),
    onSurface = Color(0xFF1B1B29),
    surfaceVariant = Color(0xFFF0F0F7),
    onSurfaceVariant = Color(0xFF6B6B80),
    surfaceContainerLowest = Color(0xFFFFFFFF),
    surfaceContainerLow = Color(0xFFF9F9FD),
    surfaceContainer = Color(0xFFF3F3F9),
    surfaceContainerHigh = Color(0xFFEEEEF6),
    surfaceContainerHighest = Color(0xFFE8E8F1),
    outline = Color(0xFF8A8AA0),
    outlineVariant = Color(0xFFE2E2EC),
    error = Color(0xFFC62F3B),
    onError = Color(0xFFFFFFFF),
    errorContainer = Color(0xFFFDE8EA),
    onErrorContainer = Color(0xFF5C0A12),
)

private val DarkScheme = darkColorScheme(
    primary = Color(0xFF8B82FF),
    onPrimary = Color(0xFF14121F),
    primaryContainer = Color(0xFF2A2748),
    onPrimaryContainer = Color(0xFFDDD9FF),
    secondary = Color(0xFF9A9AB0),
    onSecondary = Color(0xFF14121F),
    secondaryContainer = Color(0xFF2E2E42),
    onSecondaryContainer = Color(0xFFECECF4),
    tertiary = Color(0xFFF0B04A),
    onTertiary = Color(0xFF2E1D00),
    tertiaryContainer = Color(0xFF3A2C12),
    onTertiaryContainer = Color(0xFFFFDEA8),
    background = Color(0xFF111118),
    onBackground = Color(0xFFECECF4),
    surface = Color(0xFF1A1A24),
    onSurface = Color(0xFFECECF4),
    surfaceVariant = Color(0xFF232331),
    onSurfaceVariant = Color(0xFF9A9AB0),
    surfaceContainerLowest = Color(0xFF0E0E14),
    surfaceContainerLow = Color(0xFF16161F),
    surfaceContainer = Color(0xFF1A1A24),
    surfaceContainerHigh = Color(0xFF212130),
    surfaceContainerHighest = Color(0xFF2A2A3B),
    outline = Color(0xFF6E6E86),
    outlineVariant = Color(0xFF2E2E3E),
    error = Color(0xFFFF6B77),
    onError = Color(0xFF3D0A10),
    errorContainer = Color(0xFF3D1A1F),
    onErrorContainer = Color(0xFFFFDADD),
)

private val LightStatus = StatusColors(Color(0xFF13855B), Color(0xFFE3F6EE), Color(0xFFA86400), Color(0xFFFFF3DE))
private val DarkStatus = StatusColors(Color(0xFF4CC38A), Color(0xFF173327), Color(0xFFF0B04A), Color(0xFF3A2C12))

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
    val scheme: ColorScheme = when {
        dynamicColor && Build.VERSION.SDK_INT >= Build.VERSION_CODES.S ->
            if (dark) dynamicDarkColorScheme(context) else dynamicLightColorScheme(context)
        dark -> DarkScheme
        else -> LightScheme
    }
    CompositionLocalProvider(LocalStatusColors provides if (dark) DarkStatus else LightStatus) {
        MaterialTheme(colorScheme = scheme, content = content)
    }
}
