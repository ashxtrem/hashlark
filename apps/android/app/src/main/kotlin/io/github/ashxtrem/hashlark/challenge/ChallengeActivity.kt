// SPDX-License-Identifier: GPL-3.0-or-later

package io.github.ashxtrem.hashlark.challenge

import android.annotation.SuppressLint
import android.app.Activity
import android.content.Context
import android.content.Intent
import android.graphics.Bitmap
import android.os.Bundle
import android.webkit.CookieManager
import android.webkit.WebResourceRequest
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.activity.ComponentActivity
import androidx.activity.compose.BackHandler
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import io.github.ashxtrem.hashlark.core.Session
import io.github.ashxtrem.hashlark.core.SessionCookie
import io.github.ashxtrem.hashlark.ffi.userAgent
import io.github.ashxtrem.hashlark.hashlark
import io.github.ashxtrem.hashlark.ui.AppViewModel
import io.github.ashxtrem.hashlark.ui.theme.HashlarkTheme
import kotlinx.coroutines.launch

/**
 * The browser check. Some sites show a "checking your browser" page that a
 * person has to pass; Hashlark does not try to solve these. The user passes
 * it here, in a `WebView` that sends the same user agent as the core, then
 * the site's cookies are handed to that provider (and only that provider).
 */
class ChallengeActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        val providerId = intent.getStringExtra(EXTRA_PROVIDER_ID)
        val name = intent.getStringExtra(EXTRA_NAME).orEmpty()
        val url = intent.getStringExtra(EXTRA_URL)
        if (providerId == null || url == null || !url.startsWith("http")) {
            finish()
            return
        }
        setContent {
            HashlarkTheme {
                ChallengeScreen(
                    providerId = providerId,
                    name = name,
                    url = url,
                    onCancel = { finish() },
                    onDone = {
                        setResult(Activity.RESULT_OK)
                        finish()
                    },
                )
            }
        }
    }

    companion object {
        private const val EXTRA_PROVIDER_ID = "provider_id"
        private const val EXTRA_NAME = "name"
        private const val EXTRA_URL = "url"

        fun intent(context: Context, providerId: String, name: String, url: String): Intent =
            Intent(context, ChallengeActivity::class.java)
                .putExtra(EXTRA_PROVIDER_ID, providerId)
                .putExtra(EXTRA_NAME, name)
                .putExtra(EXTRA_URL, url)

        /** Parses a `Cookie` header value (`a=1; b=2`) into cookies. */
        fun parseCookies(header: String?): List<SessionCookie> =
            header.orEmpty().split(';')
                .map { it.trim() }
                .filter { it.contains('=') }
                .map { SessionCookie(it.substringBefore('='), it.substringAfter('=')) }
                .filter { it.name.isNotEmpty() }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@SuppressLint("SetJavaScriptEnabled")
@Composable
private fun ChallengeScreen(
    providerId: String,
    name: String,
    url: String,
    onCancel: () -> Unit,
    onDone: () -> Unit,
) {
    val context = androidx.compose.ui.platform.LocalContext.current
    val scope = rememberCoroutineScope()
    var webView by remember { mutableStateOf<WebView?>(null) }
    var progress by remember { mutableStateOf(0) }
    var saving by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }

    BackHandler(enabled = webView?.canGoBack() == true) { webView?.goBack() }

    Scaffold(
        topBar = {
            TopAppBar(
                title = {
                    Column {
                        Text("Browser check", style = MaterialTheme.typography.titleMedium)
                        Text(name, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                },
                navigationIcon = {
                    IconButton(onClick = onCancel) {
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Cancel")
                    }
                },
                actions = {
                    if (saving) {
                        CircularProgressIndicator(Modifier.padding(end = 16.dp), strokeWidth = 2.dp)
                    } else {
                        TextButton(onClick = {
                            saving = true
                            scope.launch {
                                val current = webView?.url ?: url
                                val cookies = ChallengeActivity.parseCookies(CookieManager.getInstance().getCookie(current))
                                if (cookies.isEmpty()) {
                                    error = "The site did not set any cookies yet. Finish the check first."
                                    saving = false
                                    return@launch
                                }
                                val app = context.hashlark
                                try {
                                    app.runtime.api().setSession(
                                        providerId,
                                        Session(url = current, cookies = cookies, userAgent = userAgent()),
                                    )
                                    onDone()
                                } catch (e: Exception) {
                                    error = AppViewModel.describe(e)
                                    saving = false
                                }
                            }
                        }) { Text("Done") }
                    }
                },
            )
        },
    ) { padding ->
        Column(Modifier.fillMaxSize().padding(padding)) {
            Text(
                error ?: "Complete the check on the site. When its normal page appears, tap Done. " +
                    "Hashlark keeps the site's cookies for this provider only, until they expire.",
                style = MaterialTheme.typography.bodySmall,
                color = if (error != null) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp),
            )
            if (progress in 1..99) {
                LinearProgressIndicator(progress = { progress / 100f }, modifier = Modifier.fillMaxWidth())
            }
            Row(Modifier.fillMaxSize(), horizontalArrangement = Arrangement.Center, verticalAlignment = Alignment.Top) {
                AndroidView(
                    modifier = Modifier.fillMaxSize(),
                    factory = { ctx ->
                        WebView(ctx).apply {
                            settings.javaScriptEnabled = true
                            settings.domStorageEnabled = true
                            // Challenge cookies only work with the agent that earned them.
                            settings.userAgentString = userAgent()
                            CookieManager.getInstance().setAcceptCookie(true)
                            CookieManager.getInstance().setAcceptThirdPartyCookies(this, true)
                            webViewClient = object : WebViewClient() {
                                override fun onPageStarted(view: WebView?, url: String?, favicon: Bitmap?) {
                                    error = null
                                }

                                override fun shouldOverrideUrlLoading(view: WebView?, request: WebResourceRequest?): Boolean =
                                    request?.url?.scheme?.startsWith("http") != true
                            }
                            webChromeClient = object : android.webkit.WebChromeClient() {
                                override fun onProgressChanged(view: WebView?, newProgress: Int) {
                                    progress = newProgress
                                }
                            }
                            loadUrl(url)
                            webView = this
                        }
                    },
                    onRelease = { it.destroy() },
                )
            }
        }
    }
}
