package jp.git_app.client

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.ProcessLifecycleOwner
import androidx.work.Worker
import androidx.work.WorkerParameters
import org.json.JSONObject

/**
 * One look at the sites, run by WorkManager. While the app is on the screen its own timer thread looks, so the job does
 * nothing then (only books the next one). Otherwise the Rust core looks, saves its state, and hands back what to show.
 */
class CheckWorker(context: Context, params: WorkerParameters) : Worker(context, params) {
    override fun doWork(): Result {
        val dataDir = applicationContext.dataDir.absolutePath
        var nextMs = System.currentTimeMillis() + RETRY_MS
        try {
            if (appIsOnScreen()) {
                nextMs = BackgroundCheck.nativeNext(dataDir)
            } else {
                val out = JSONObject(BackgroundCheck.nativeCheck(dataDir))
                val notices = out.optJSONArray("notices")
                if (notices != null) {
                    for (i in 0 until notices.length()) show(notices.getJSONObject(i))
                }
                nextMs = out.optLong("nextMs", nextMs)
            }
        } catch (t: Throwable) {
            // never let a failed look stop the chain: try again later
        }
        BackgroundCheck.schedule(applicationContext, nextMs - System.currentTimeMillis())
        return Result.success()
    }

    private fun appIsOnScreen(): Boolean =
        ProcessLifecycleOwner.get().lifecycle.currentState.isAtLeast(Lifecycle.State.STARTED)

    private fun show(n: JSONObject) {
        val ctx = applicationContext
        if (!NotificationManagerCompat.from(ctx).areNotificationsEnabled()) return
        val url = n.optString("url")
        val lower = url.lowercase()
        // only web addresses are ever handed to the browser, like everywhere else in the app
        val tap = if (lower.startsWith("https://") || lower.startsWith("http://")) {
            val intent = Intent(Intent.ACTION_VIEW, Uri.parse(url)).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            PendingIntent.getActivity(ctx, url.hashCode(), intent, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
        } else null

        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            val channel = NotificationChannel(CHANNEL, "更新の通知", NotificationManager.IMPORTANCE_DEFAULT)
            ctx.getSystemService(NotificationManager::class.java).createNotificationChannel(channel)
        }
        val body = n.optString("body")
        val notification = NotificationCompat.Builder(ctx, CHANNEL)
            .setSmallIcon(ctx.applicationInfo.icon)
            .setContentTitle(n.optString("title"))
            .setContentText(body.lineSequence().joinToString(" "))
            .setStyle(NotificationCompat.BigTextStyle().bigText(body))
            .setContentIntent(tap)
            .setAutoCancel(true)
            .build()
        try {
            NotificationManagerCompat.from(ctx).notify((url + body).hashCode(), notification)
        } catch (e: SecurityException) {
            // the permission was taken away meanwhile
        }
    }

    companion object {
        private const val CHANNEL = "updates"
        private const val RETRY_MS = 15 * 60_000L
    }
}
