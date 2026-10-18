"use strict";
// The settings / overview screen. It talks to the Rust side only through the commands below (window.__TAURI__).
// Everything that comes from the network (site names, summaries) goes in through textContent: nothing is parsed as HTML.
(function () {
  const invoke = window.__TAURI__.core.invoke;
  const $ = (id) => document.getElementById(id);
  let sites = [], active = new Set(), tags = [], freeMax = 3, buyUrl = "", baseUrl = "", firstLoad = true;

  const say = (t) => { $("msg").textContent = t; };
  // always Japanese and Japan time, like the dashboard (the device may be set to another language or zone)
  const fmt = (ms) => new Date(ms).toLocaleString("ja-JP", { timeZone: "Asia/Tokyo", month: "numeric", day: "numeric", hour: "2-digit", minute: "2-digit" }) + " JST";
  const when = (ms) => (ms ? fmt(ms) : "まだ確認していません");

  // Google Play subscription (Android store build only): the bridge is BillingBridge.kt. The store decides; the page only hands
  // its answer to the core. The expiry is short (3 days) and renewed at every confirmation, because there is no server to read
  // the real end date from.
  const play = window.GiiitBilling;
  let storeBuild = false, playOffers = [], playError = "", lastPlan = "free";
  function showPlay(plan) {
    lastPlan = plan;
    const on = !!play && storeBuild;
    $("playBox").hidden = !on;
    if (!on) return;
    const paid = plan === "pro";
    for (const [id, btn, label] of [["monthly", "buyMonthly", "月額"], ["yearly", "buyYearly", "年額"]]) {
      const o = playOffers.find((x) => x.id === id);
      $(btn).hidden = paid || !o;
      if (o) $(btn).textContent = `${label} ${o.price}で購入（Google Play）`;
    }
    $("manageBtn").hidden = !paid;
    $("playNote").textContent = playError ? `Google Play: ${playError}` : (paid ? "Google Play の購読が有効です。" : (playOffers.length ? "購入すると、無料プランの制限がなくなります。税別の価格です。" : ""));
  }
  window.giiitBillingChanged = (r) => {
    playOffers = r.offers || [];
    playError = r.ok ? "" : (r.error || "");
    // only a successful answer may change the plan: a failed one says nothing about the subscription
    if (r.ok) invoke("set_store_entitlement", { source: "google-play", expiresMs: r.entitled ? Date.now() + 3 * 86400000 : 0 }).then(load);
    else showPlay(lastPlan);
  };
  if (play) {
    $("buyMonthly").addEventListener("click", () => play.buy("monthly"));
    $("buyYearly").addEventListener("click", () => play.buy("yearly"));
    $("manageBtn").addEventListener("click", () => play.manage());
    document.addEventListener("visibilitychange", () => { if (!document.hidden && storeBuild) play.refresh(); });
  }

  function statusLine(v) {
    $("status").textContent = `通知の対象 ${v.active.length} サイト ・ サイトの最終実行 ${v.generatedMs ? when(v.generatedMs) : "不明"} ・ 最終確認 ${when(v.lastCheck)}`
      + (v.active.length ? ` ・ 次回 ${fmt(v.nextCheck)}` : "")
      + (v.lastError ? ` ・ 注意: ${v.lastError}` : "");
    $("scheduleNote").textContent = v.scheduleText;
    $("planText").textContent = v.planText;
    $("buyBtn").hidden = !v.purchaseUrl || v.plan === "pro"; buyUrl = v.purchaseUrl;
    $("licenseRow").hidden = v.storeBuild;
    storeBuild = v.storeBuild; showPlay(v.plan);
    $("about").textContent = v.about; baseUrl = v.baseUrl;
  }

  let recentAll = [];
  // Only the updates of the sites that are looked at now: what was announced for a category that was switched off stays
  // in the state (it is back when the category is picked again) but is not listed.
  function renderRecent(all) {
    recentAll = all;
    // an entry that says "nothing changed" (from an older version of the app) is not shown either
    const list = all.filter((r) => active.has(r.slug) && !r.text.startsWith("内容に実質的な変更はありません")).sort((a, b) => (b.updatedMs || b.at) - (a.updatedMs || a.at)); // newest update first
    const ul = $("recent");
    ul.textContent = "";
    $("recentEmpty").hidden = list.length > 0;
    for (const r of list) {
      const li = document.createElement("li");
      const a = document.createElement("a");
      a.href = "#";
      a.textContent = `${r.name}  ${r.time}`;
      a.addEventListener("click", (e) => { e.preventDefault(); invoke("open_external", { url: r.count > 1 ? r.historyUrl : r.diffUrl }); });
      const small = document.createElement("small");
      small.textContent = "  " + r.text;
      li.append(a, small);
      ul.append(li);
    }
  }

  function renderSites() {
    const q = $("filter").value.trim().toLowerCase();
    const only = $("only").checked;
    const list = $("sites");
    list.textContent = "";
    let shown = 0;
    for (const s of sites) {
      if (only && !active.has(s.slug)) continue;
      if (q && !(s.name + " " + s.url + " " + s.slug).toLowerCase().includes(q)) continue;
      shown++;
      const li = document.createElement("li");
      const label = document.createElement("label");
      const cb = document.createElement("input");
      cb.type = "checkbox"; cb.checked = active.has(s.slug);
      cb.addEventListener("change", async () => {
        const wanted = cb.checked;
        active = new Set(await invoke("toggle_site", { slug: s.slug, on: wanted }));
        if (active.has(s.slug) !== wanted) { cb.checked = active.has(s.slug); say(`無料プランでは ${freeMax} サイトまでです。`); }
        softRefresh(); // the picker and the status line show the new count; the long list keeps its scroll position
      });
      const name = document.createElement("span"); name.textContent = s.name;
      const host = document.createElement("small");
      try { host.textContent = new URL(s.url).hostname.replace(/^www\./, ""); } catch { host.textContent = s.slug; }
      label.append(cb, name, host); li.append(label); list.append(li);
    }
    $("count").textContent = `（${active.size} / ${sites.length} 件が対象・${shown} 件を表示）`;
  }

  // The category picker: one row of buttons per heading. The same code draws the first-start screen and the 通知 tab.
  function renderChips(box, v) {
    box.textContent = "";
    for (const g of v.groups) {
      const h = document.createElement("h3"); h.textContent = g.name;
      const row = document.createElement("div"); row.className = "chips";
      for (const t of g.tags) {
        const b = document.createElement("button");
        b.type = "button"; b.className = "chip";
        b.setAttribute("aria-pressed", String(v.tags.includes(t.name)));
        const picked = v.tags.includes(t.name);
        b.textContent = `${t.name} ${t.count}`;
        // 0 = nothing is left next to the picks of the other headings (it would not add a site): shown dimmed, still pressable
        b.classList.toggle("empty", t.count === 0 && !picked);
        b.addEventListener("click", () => pickTag(t.name));
        row.append(b);
      }
      box.append(h, row);
    }
  }
  async function pickTag(name) {
    const next = tags.includes(name) ? tags.filter((x) => x !== name) : [...tags, name];
    try { await invoke("set_tags", { tags: next }); say(""); } catch (e) { say(String(e)); return; }
    if (!tags.length && next.length) $("siteDetails").open = false; // the long list is no longer needed once categories are used
    await load();
  }
  function chipTexts(v) {
    const n = !v.tags.length ? "分野を選ぶと、対象のサイト数がここに出ます。"
      : v.ruleCount === 0 ? "選んだ分野の組み合わせに、合うサイトがありません。違う見出しの分野は「すべて」に当てはまるサイトだけが対象になります（例: 「職能・業界団体」と「がん」）。どちらかを外してください。"
      : `選んだ分野の ${v.ruleCount} サイトが対象です。`;
    $("chipCount").textContent = n; $("setupCount").textContent = n;
    $("chipNote").textContent = v.plan === "pro" ? "分野を選ぶと、その分野のサイトの更新が通知されます。新しく加わったサイトも自動で対象になります。見出しの中は「どれか」、見出し同士は「すべて」に当てはまるサイトです。"
      : v.plan === "trial" ? "お試し期間中は、分野で選べます。終わったあとも続けるには、有料プランが要ります（無料プランは 3 サイトまで個別に選べます）。"
      : "分野での選択は、有料プランで使えます。無料プランは、サイトを 3 つまで個別に選べます。";
    for (const b of document.querySelectorAll(".chip")) b.disabled = !v.unlocked;
  }

  // The permission to show notifications (Android 13+ and iOS ask the user; on a PC it is already there).
  async function notifyGranted() {
    try { return (await invoke("plugin:notification|is_permission_granted")) === true; } catch { return false; }
  }
  async function showNotifyState() {
    const ok = await notifyGranted();
    $("notifyState").textContent = ok ? "通知: 許可されています。"
      : "通知: 許可されていません。許可しないと、更新があっても通知が出ません（「最近の更新」には並びます）。";
    $("notifyBtn").hidden = ok;
    return ok;
  }
  // Asks once (the system dialog); when it was refused before, the system does not ask again: say where to switch it on.
  async function askNotify() {
    if (await notifyGranted()) return true;
    let r = "";
    try { r = await invoke("plugin:notification|request_permission"); } catch { r = "error"; }
    const ok = await showNotifyState();
    if (!ok) say("通知が許可されていません。端末の設定 →「アプリ」→ G醫t →「通知」で許可してください。");
    return ok;
  }

  // Android only: the app's own bridge (DeviceBridge.kt) tells what keeps a phone from looking on time. It changes nothing: the
  // buttons open the system's own settings pages.
  const bridge = window.GiiitAndroid;
  function showDeviceState() {
    if (!bridge) return;
    $("deviceBox").hidden = false;
    let st;
    try { st = JSON.parse(bridge.status()); } catch { return; }
    const lines = [];
    if (st.backgroundRestricted) lines.push("バックグラウンドの動作が制限されています。このままでは、アプリを閉じている間の確認ができません。「アプリの設定」で、電池の項目を「制限なし」か「最適化」にしてください。");
    else if (st.standbyBucket >= 45) lines.push("このアプリは「制限」の状態で、確認の回数がごく少なくなっています。アプリを開くと、改善します。");
    else if (st.standbyBucket >= 40) lines.push("このアプリはあまり使われていないため、確認の回数が制限されています。時々アプリを開くと、改善します。");
    lines.push(st.ignoringBatteryOptimizations
      ? "電池の最適化の対象外です（確認は、決まった時刻に近く行われます）。"
      : "電池の最適化の対象です（標準の設定）。通知が遅れる場合は、「電池の設定」で、このアプリを「最適化しない」にすると改善することがあります。");
    $("deviceState").textContent = lines.join(" ");
  }
  if (bridge) {
    $("appSettingsBtn").addEventListener("click", () => bridge.openAppSettings());
    $("batterySettingsBtn").addEventListener("click", () => bridge.openBatterySettings());
    // the user comes back from the system's settings page: show the new state
    document.addEventListener("visibilitychange", () => { if (!document.hidden) { showDeviceState(); showNotifyState(); } });
  }

  function applySize(v) {
    if (!["small", "normal", "large", "huge"].includes(v)) v = "normal";
    document.documentElement.dataset.size = v; $("fontSize").value = v;
  }
  let savedSize = "normal";
  try { savedSize = localStorage.getItem("git-client:size") || "normal"; } catch {}
  applySize(savedSize);
  $("fontSize").addEventListener("change", () => {
    applySize($("fontSize").value);
    try { localStorage.setItem("git-client:size", $("fontSize").value); } catch {}
  });

  async function softRefresh() {
    const v = await invoke("get_state");
    active = new Set(v.active); tags = v.tags;
    renderChips($("chips"), v); chipTexts(v); statusLine(v); renderRecent(v.recent);
    $("count").textContent = `（${active.size} / ${sites.length} 件が対象）`;
  }

  async function load() {
    const v = await invoke("get_state");
    sites = v.sites; active = new Set(v.active); tags = v.tags; freeMax = v.freeMaxFollows;
    $("login").checked = v.openAtLogin;
    // the first start: choose categories before anything else (nothing is followed yet and the screen was never answered)
    // decided once, at the first load: picking a category must not close the screen before 「この分野で始める」 is pressed
    if (setupOpen === null && v.groups.length > 0) setupOpen = !v.onboarded && !v.follow.length && !v.tags.length;
    const setup = setupOpen === true;
    $("setup").hidden = !setup; $("appBody").hidden = setup;
    renderChips($("setupChips"), v); renderChips($("chips"), v); chipTexts(v);
    if (firstLoad && sites.length) { firstLoad = false; $("siteDetails").open = !v.tags.length; } // the long list stays closed once categories are used
    statusLine(v); renderRecent(v.recent); renderSites();
    if (!sites.length) {
      // the first look runs a few seconds after the start: ask again a few times before giving up
      if (loadTries++ < 6) setTimeout(load, 5000);
      else say("サイトの一覧を取得できませんでした。ネットワークを確認してください。");
    }
  }
  let loadTries = 0, setupOpen = null;

  $("filter").addEventListener("input", renderSites);
  $("only").addEventListener("change", renderSites);
  $("importBtn").addEventListener("click", async () => {
    const r = await invoke("import_follow", { text: $("import").value });
    $("import").value = "";
    say(r.added ? `${r.added} 件を取り込みました。` : "取り込めるサイトが、見つかりませんでした。"); await load();
  });
  const finishSetup = async () => { await invoke("finish_setup"); setupOpen = false; await load(); await askNotify(); };
  $("setupDone").addEventListener("click", finishSetup);
  $("setupSkip").addEventListener("click", finishSetup);
  // the start-at-login switch is saved as soon as it is changed (there is no save button)
  $("login").addEventListener("change", async () => {
    try {
      await invoke("set_options", { openAtLogin: $("login").checked });
      say("保存しました。"); await load();
    } catch (e) { say("保存できませんでした: " + e); }
  });
  // two screens: 通知 (recent updates, sites) and 設定 (plan, import, display, start at login)
  function showView(name) {
    for (const t of document.querySelectorAll(".tab")) t.setAttribute("aria-selected", String(t.dataset.view === name));
    $("view-main").hidden = name !== "main"; $("view-settings").hidden = name !== "settings";
    say("");
  }
  for (const t of document.querySelectorAll(".tab")) t.addEventListener("click", () => showView(t.dataset.view));
  $("licenseBtn").addEventListener("click", async () => {
    try { await invoke("set_license", { key: $("licenseKey").value }); $("licenseKey").value = ""; say("ライセンスキーを確認しました。"); await load(); }
    catch (e) { say("有効にできませんでした: " + e); }
  });
  // the help lives on the G醫t site: the buttons open its pages in the browser (the address comes from the app's own state)
  for (const b of document.querySelectorAll("[data-page]")) b.addEventListener("click", () => invoke("open_external", { url: baseUrl + b.dataset.page }));
  $("buyBtn").addEventListener("click", () => invoke("open_external", { url: buyUrl }));
  $("checkBtn").addEventListener("click", async () => {
    say("確認しています…");
    await invoke("check_now");
    setTimeout(async () => { await load(); say("確認しました。"); }, 4000);
  });
  $("notifyBtn").addEventListener("click", askNotify);
  $("testBtn").addEventListener("click", async () => { if (await askNotify()) invoke("test_notification"); });
  // desktop only: the start-at-login switch has no meaning on a phone
  if (/android|iphone|ipad/i.test(navigator.userAgent)) { $("login").hidden = true; $("loginLabel").hidden = true; }
  load().then(() => { if (play && storeBuild) play.refresh(); });
  showNotifyState();
  showDeviceState();
})();
