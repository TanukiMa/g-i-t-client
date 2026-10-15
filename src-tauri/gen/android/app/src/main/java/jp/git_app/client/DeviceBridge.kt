package jp.git_app.client

import android.app.Activity
import android.app.ActivityManager
import android.app.usage.UsageStatsManager
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.PowerManager
import android.provider.Settings
import android.webkit.JavascriptInterface
import androidx.core.app.NotificationManagerCompat
import org.json.JSONObject

/**
 * What the settings screen needs to tell the user why a phone may look late, and two buttons that open the system's own
 * settings pages. Exposed to the page as `window.GiiitAndroid` (see MainActivity). Read-only: it changes nothing itself;
 * the user changes the setting on the system page. The WebView shows only the app's own files (strict CSP), so no foreign
 * page can reach it.
 */
class DeviceBridge(private val activity: Activity) {
    /**
     * `{"notifications":bool, "ignoringBatteryOptimizations":bool, "backgroundRestricted":bool, "standbyBucket":int}`
     * standbyBucket: 10 active, 20 working set, 30 frequent, 40 rare, 45 restricted (0 = not known).
     */
    @JavascriptInterface
    fun status(): String {
        val ctx: Context = activity.applicationContext
        val out = JSONObject()
        out.put("notifications", NotificationManagerCompat.from(ctx).areNotificationsEnabled())
        val power = ctx.getSystemService(Context.POWER_SERVICE) as PowerManager
        out.put("ignoringBatteryOptimizations", power.isIgnoringBatteryOptimizations(ctx.packageName))
        var restricted = false
        var bucket = 0
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            restricted = (ctx.getSystemService(Context.ACTIVITY_SERVICE) as ActivityManager).isBackgroundRestricted
            bucket = (ctx.getSystemService(Context.USAGE_STATS_SERVICE) as UsageStatsManager).appStandbyBucket
        }
        out.put("backgroundRestricted", restricted)
        out.put("standbyBucket", bucket)
        return out.toString()
    }

    /** The system's page of this app (notifications, battery, background data). */
    @JavascriptInterface
    fun openAppSettings() {
        val intent = Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.parse("package:" + activity.packageName))
        start(intent)
    }

    /** The system's list of apps with their battery optimization. */
    @JavascriptInterface
    fun openBatterySettings() {
        start(Intent(Settings.ACTION_IGNORE_BATTERY_OPTIMIZATION_SETTINGS))
    }

    private fun start(intent: Intent) {
        activity.runOnUiThread {
            try {
                activity.startActivity(intent)
            } catch (e: Exception) {
                // a phone without that settings page: nothing to open
            }
        }
    }
}
