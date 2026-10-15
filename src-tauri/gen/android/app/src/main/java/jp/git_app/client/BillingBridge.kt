package jp.git_app.client

import android.app.Activity
import android.webkit.JavascriptInterface
import android.webkit.WebView
import com.android.billingclient.api.AcknowledgePurchaseParams
import com.android.billingclient.api.BillingClient
import com.android.billingclient.api.BillingClientStateListener
import com.android.billingclient.api.BillingFlowParams
import com.android.billingclient.api.BillingResult
import com.android.billingclient.api.PendingPurchasesParams
import com.android.billingclient.api.ProductDetails
import com.android.billingclient.api.Purchase
import com.android.billingclient.api.PurchasesUpdatedListener
import com.android.billingclient.api.QueryProductDetailsParams
import com.android.billingclient.api.QueryPurchasesParams
import org.json.JSONArray
import org.json.JSONObject

/**
 * Google Play Billing for the store build: one subscription [PRODUCT_ID] with the base plans "monthly" and "yearly"
 * (set up in the Play Console). Exposed to the page as `window.GiiitBilling`; the answer comes back through
 * `window.giiitBillingChanged(json)`: `{ok, entitled, offers:[{id, price}], error}`.
 *
 * There is no server (AGENTS.md principle 1), so nothing here can read the expiry time from the Play Developer API. What
 * the library gives is the list of the subscriptions that are active NOW (`queryPurchasesAsync`), so "entitled" means "a
 * purchased subscription is in that list"; the page hands the result to the core with a short expiry that the next
 * confirmation renews. A purchase is acknowledged here (Google refunds an unacknowledged one after 3 days).
 */
class BillingBridge(private val activity: Activity, private val webView: WebView) {
    companion object {
        const val PRODUCT_ID = "giiit_pro"
    }

    private var details: ProductDetails? = null
    private var offers = JSONArray()

    private val listener = PurchasesUpdatedListener { result, purchases ->
        if (result.responseCode == BillingClient.BillingResponseCode.OK && purchases != null) {
            handle(purchases)
        } else if (result.responseCode != BillingClient.BillingResponseCode.USER_CANCELED) {
            push(false, false, result.debugMessage.ifEmpty { "code " + result.responseCode })
        }
    }

    private val client: BillingClient = BillingClient.newBuilder(activity.applicationContext)
        .setListener(listener)
        .enablePendingPurchases(PendingPurchasesParams.newBuilder().enableOneTimeProducts().build())
        .enableAutoServiceReconnection()
        .build()

    /** Connect (if needed), read the offers and the active subscriptions, and report both. */
    @JavascriptInterface
    fun refresh() {
        connected { ok ->
            if (!ok) { push(false, false, "Google Play に接続できません"); return@connected }
            loadOffers()
            client.queryPurchasesAsync(QueryPurchasesParams.newBuilder().setProductType(BillingClient.ProductType.SUBS).build()) { r, list ->
                if (r.responseCode == BillingClient.BillingResponseCode.OK) handle(list) else push(false, false, r.debugMessage)
            }
        }
    }

    /** Opens Google Play's purchase sheet for the base plan ("monthly" or "yearly"). */
    @JavascriptInterface
    fun buy(basePlanId: String) {
        val d = details ?: return
        val offer = d.subscriptionOfferDetails?.firstOrNull { it.basePlanId == basePlanId } ?: return
        val params = BillingFlowParams.newBuilder().setProductDetailsParamsList(
            listOf(BillingFlowParams.ProductDetailsParams.newBuilder().setProductDetails(d).setOfferToken(offer.offerToken).build())
        ).build()
        activity.runOnUiThread { client.launchBillingFlow(activity, params) }
    }

    /** The subscription page of Google Play (to cancel or change it). */
    @JavascriptInterface
    fun manage() {
        val uri = android.net.Uri.parse("https://play.google.com/store/account/subscriptions?sku=$PRODUCT_ID&package=${activity.packageName}")
        activity.runOnUiThread { try { activity.startActivity(android.content.Intent(android.content.Intent.ACTION_VIEW, uri)) } catch (_: Exception) {} }
    }

    private fun connected(done: (Boolean) -> Unit) {
        if (client.isReady) { done(true); return }
        client.startConnection(object : BillingClientStateListener {
            override fun onBillingSetupFinished(r: BillingResult) { done(r.responseCode == BillingClient.BillingResponseCode.OK) }
            override fun onBillingServiceDisconnected() {}
        })
    }

    private fun loadOffers() {
        val p = QueryProductDetailsParams.newBuilder().setProductList(
            listOf(QueryProductDetailsParams.Product.newBuilder().setProductId(PRODUCT_ID).setProductType(BillingClient.ProductType.SUBS).build())
        ).build()
        client.queryProductDetailsAsync(p) { r, res ->
            if (r.responseCode != BillingClient.BillingResponseCode.OK) return@queryProductDetailsAsync
            details = res.productDetailsList.firstOrNull()
            val arr = JSONArray()
            for (o in details?.subscriptionOfferDetails.orEmpty()) {
                if (o.offerId != null) continue // base plans only (no promotional offers yet)
                arr.put(JSONObject().put("id", o.basePlanId).put("price", o.pricingPhases.pricingPhaseList.firstOrNull()?.formattedPrice ?: ""))
            }
            offers = arr
        }
    }

    private fun handle(list: List<Purchase>?) {
        val mine = list.orEmpty().filter { PRODUCT_ID in it.products && it.purchaseState == Purchase.PurchaseState.PURCHASED }
        for (p in mine) {
            if (!p.isAcknowledged) {
                client.acknowledgePurchase(AcknowledgePurchaseParams.newBuilder().setPurchaseToken(p.purchaseToken).build()) {}
            }
        }
        push(true, mine.isNotEmpty(), "")
    }

    private fun push(ok: Boolean, entitled: Boolean, error: String) {
        val js = JSONObject().put("ok", ok).put("entitled", entitled).put("offers", offers).put("error", error).toString()
        webView.post { webView.evaluateJavascript("window.giiitBillingChanged && window.giiitBillingChanged($js)", null) }
    }
}
