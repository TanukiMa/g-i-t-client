"use strict";
// The settings / overview screen. It talks to the Rust side only through the commands below (window.__TAURI__).
// Everything that comes from the network (site names, summaries) goes in through textContent: nothing is parsed as HTML.
(function () {
  const invoke = window.__TAURI__.core.invoke;
  const $ = (id) => document.getElementById(id);
  let sites = [], follow = new Set(), freeMax = 3, buyUrl = "";

  const say = (t) => { $("msg").textContent = t; };
  // always Japanese and Japan time, like the dashboard (the device may be set to another language or zone)
  const fmt = (ms) => new Date(ms).toLocaleString("ja-JP", { timeZone: "Asia/Tokyo", month: "numeric", day: "numeric", hour: "2-digit", minute: "2-digit" }) + " JST";
  const when = (ms) => (ms ? fmt(ms) : "まだ確認していません");

  function statusLine(v) {
    $("status").textContent = `フォロー中 ${v.follow.length} サイト ・ サイトの最終実行 ${v.generatedMs ? when(v.generatedMs) : "不明"} ・ 最終確認 ${when(v.lastCheck)}`
      + (v.follow.length ? ` ・ 次回 ${fmt(v.nextCheck)}` : "")
      + (v.lastError ? ` ・ 注意: ${v.lastError}` : "");
    $("scheduleNote").textContent = v.scheduleText;
    $("planText").textContent = v.planText;
    $("buyBtn").hidden = !v.purchaseUrl || v.plan === "pro"; buyUrl = v.purchaseUrl;
    $("licenseRow").hidden = v.storeBuild;
  }

  function renderRecent(list) {
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
      if (only && !follow.has(s.slug)) continue;
      if (q && !(s.name + " " + s.url + " " + s.slug).toLowerCase().includes(q)) continue;
      shown++;
      const li = document.createElement("li");
      const label = document.createElement("label");
      const cb = document.createElement("input");
      cb.type = "checkbox"; cb.checked = follow.has(s.slug);
      cb.addEventListener("change", async () => {
        cb.checked ? follow.add(s.slug) : follow.delete(s.slug);
        const got = await invoke("set_follow", { slugs: [...follow] });
        if (got.length !== follow.size) { follow = new Set(got); cb.checked = follow.has(s.slug); say(`無料プランでは ${freeMax} サイトまでです。`); }
        $("count").textContent = `${follow.size} / ${sites.length} 件をフォロー中`;
      });
      const name = document.createElement("span"); name.textContent = s.name;
      const host = document.createElement("small");
      try { host.textContent = new URL(s.url).hostname.replace(/^www\./, ""); } catch { host.textContent = s.slug; }
      label.append(cb, name, host); li.append(label); list.append(li);
    }
    $("count").textContent = `${follow.size} / ${sites.length} 件をフォロー中（${shown} 件を表示）`;
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

  async function load() {
    const v = await invoke("get_state");
    sites = v.sites; follow = new Set(v.follow); freeMax = v.freeMaxFollows;
    $("login").checked = v.openAtLogin;
    statusLine(v); renderRecent(v.recent); renderSites();
    if (!sites.length) {
      // the first look runs a few seconds after the start: ask again a few times before giving up
      if (loadTries++ < 6) setTimeout(load, 5000);
      else say("サイトの一覧を取得できませんでした。ネットワークを確認してください。");
    }
  }
  let loadTries = 0;

  $("filter").addEventListener("input", renderSites);
  $("only").addEventListener("change", renderSites);
  $("importBtn").addEventListener("click", async () => {
    const r = await invoke("import_follow", { text: $("import").value });
    follow = new Set(r.follow); $("import").value = "";
    say(r.added ? `${r.added} 件を取り込みました。` : "取り込めるサイトが、見つかりませんでした。"); renderSites();
  });
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
  $("buyBtn").addEventListener("click", () => invoke("open_external", { url: buyUrl }));
  $("checkBtn").addEventListener("click", async () => {
    say("確認しています…");
    await invoke("check_now");
    setTimeout(async () => { await load(); say("確認しました。"); }, 4000);
  });
  $("testBtn").addEventListener("click", () => invoke("test_notification"));
  // desktop only: the start-at-login switch has no meaning on a phone
  if (/android|iphone|ipad/i.test(navigator.userAgent)) { $("login").hidden = true; $("loginLabel").hidden = true; }
  load();
})();
