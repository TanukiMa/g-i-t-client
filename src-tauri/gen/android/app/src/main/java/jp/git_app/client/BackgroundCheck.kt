package jp.git_app.client

import android.content.Context
import androidx.work.Constraints
import androidx.work.ExistingWorkPolicy
import androidx.work.NetworkType
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.WorkManager
import java.util.concurrent.TimeUnit
import kotlin.random.Random

/**
 * The look at the sites while the app is closed. The work is done by the Rust core (`git_core::background`): this class
 * only books the next job (WorkManager) and calls the two entry points of `src-tauri/src/lib.rs`.
 *
 * Best effort: Android decides when a job runs (Doze, battery saver, a force-stopped app does not run at all). The jobs
 * are chained: every job books the next one for the time that the Rust side names (after the next run of the pipeline).
 */
object BackgroundCheck {
    init {
        System.loadLibrary("git_client_lib")
    }

    /** `{"notices":[{"title","body","url"}],"nextMs":..., "error":"..."}` */
    @JvmStatic external fun nativeCheck(dataDir: String): String

    /** When the next look is due (Unix ms), without looking. */
    @JvmStatic external fun nativeNext(dataDir: String): Long

    private const val WORK_NAME = "giiit-check"
    private const val MIN_DELAY_MS = 60_000L

    /** Books the next job `delayMs` from now (replaces a job that is already waiting). */
    fun schedule(context: Context, delayMs: Long) {
        // 0-45 s more, like the checking thread: thousands of apps must not ask in the same second
        val delay = maxOf(delayMs, MIN_DELAY_MS) + Random.nextLong(45_000)
        val request = OneTimeWorkRequestBuilder<CheckWorker>()
            .setInitialDelay(delay, TimeUnit.MILLISECONDS)
            .setConstraints(Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build())
            .build()
        WorkManager.getInstance(context).enqueueUniqueWork(WORK_NAME, ExistingWorkPolicy.REPLACE, request)
    }

    /** Books the next job for the time that the state file says (used when the app starts and when it is left). */
    fun scheduleFromState(context: Context) {
        val next = try {
            nativeNext(context.dataDir.absolutePath)
        } catch (t: Throwable) {
            System.currentTimeMillis() + 30 * 60_000L
        }
        schedule(context, next - System.currentTimeMillis())
    }
}
