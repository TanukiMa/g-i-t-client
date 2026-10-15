package jp.git_app.client

import android.os.Bundle
import android.webkit.WebView
import androidx.activity.enableEdgeToEdge

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    bookBackgroundCheck()
  }

  // The settings screen reads the phone's notification and battery state through this (see DeviceBridge).
  override fun onWebViewCreate(webView: WebView) {
    webView.addJavascriptInterface(DeviceBridge(this), "GiiitAndroid")
  }

  // When the app is left, the follow list may have changed: book the next look for the time that the state now says.
  override fun onStop() {
    super.onStop()
    bookBackgroundCheck()
  }

  private fun bookBackgroundCheck() {
    val context = applicationContext
    Thread { BackgroundCheck.scheduleFromState(context) }.start()
  }
}
