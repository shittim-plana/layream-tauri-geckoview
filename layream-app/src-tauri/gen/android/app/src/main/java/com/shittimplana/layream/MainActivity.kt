package com.shittimplana.layream

import android.content.Intent
import android.os.Bundle
import androidx.activity.enableEdgeToEdge

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    val oauthUri = if (isOAuthIntent(intent)) intent.data else null
    if (oauthUri != null) intent.data = null
    super.onCreate(savedInstanceState)
    if (oauthUri != null) saveOAuthCode(oauthUri)
  }

  override fun onNewIntent(intent: Intent) {
    if (isOAuthIntent(intent)) {
      saveOAuthCode(intent.data!!)
    } else {
      super.onNewIntent(intent)
    }
  }

  private fun isOAuthIntent(intent: Intent?): Boolean {
    return intent?.data?.scheme?.startsWith("com.googleusercontent.apps.") == true
  }

  private fun saveOAuthCode(uri: android.net.Uri) {
    val scheme = uri.scheme ?: return
    val code = uri.getQueryParameter("code") ?: return
    filesDir.resolve("pending_oauth.txt").writeText("$scheme|$code")
  }
}
