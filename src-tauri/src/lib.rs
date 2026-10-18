//! G醫t client (Tauri 2). A thin shell around `git_core`: it keeps the state on this device, checks the feeds of the
//! followed sites on a timer, shows the OS notifications and offers the settings window. Nothing is sent anywhere.
//!
//! Desktop (Windows / macOS): tray icon, looking for news after the runs of the pipeline (not every few minutes: see
//! git_core::schedule), start at login. Mobile: the same core and screen; what happens
//! while the app is closed is decided by the OS (see client/README.md).

use git_core::category::{counts, GroupCount};
use git_core::feed::{parse_follow_input, Site};
use git_core::entitlement::{verify_license, Plan, Source, StoreEntitlement, FREE_MAX_FOLLOWS};
use git_core::poller::{next_check_ms, poll_scheduled, refresh_sites, HttpFetcher, Notice};
use git_core::state::{self, Recent, State};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_opener::OpenerExt;

/// Everything the commands and the checking thread share.
struct Shared {
    state: Mutex<State>,
    file: PathBuf,
    wake: Mutex<Sender<()>>,
}

type SharedState = Arc<Shared>;

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// What the background job of a phone looked at and remembered since the app last saved is taken over first, so that the app
/// never saves an older `seen` over it (the same update would be announced twice).
fn absorb_disk(shared: &Shared, state: &mut State) {
    if shared.file.exists() {
        let disk = state::load(&shared.file);
        if disk.last_check > state.last_check {
            state.merge_poll(disk);
        }
    }
}

/// Saves the state. Takes the lock of the state file if it is free; if a check holds it (a look can take seconds, and a
/// command must not wait for the network) the state is written without it: that check merges and saves again when it ends.
fn save(shared: &Shared, state: &mut State) {
    let lock = state::FileLock::try_acquire(&shared.file);
    if lock.is_some() {
        absorb_disk(shared, state);
    }
    let _ = state::save(&shared.file, state);
}

/// Only http(s) addresses are ever handed to the OS browser.
fn open_url(app: &AppHandle, url: &str) {
    let l = url.to_lowercase();
    if l.starts_with("https://") || l.starts_with("http://") {
        let _ = app.opener().open_url(url, None::<&str>);
    }
}

fn show_notice(app: &AppHandle, n: &Notice) {
    let _ = app.notification().builder().title(&n.title).body(&n.body).show();
}

/// One check: works on a copy of the state (the network can be slow), then merges what it found.
fn check(app: &AppHandle, shared: &Shared) {
    // The lock is held for the whole look: the background job of a phone takes the same one, so the two never work on the
    // state at the same time (see git_core::background).
    let _lock = state::FileLock::acquire(&shared.file).ok();
    let mut polled = {
        let mut s = shared.state.lock().unwrap();
        absorb_disk(shared, &mut s);
        s.clone()
    };
    let fetcher = HttpFetcher::new();
    let _ = poll_scheduled(&mut polled, &fetcher, &mut |n| show_notice(app, n), now_ms());
    let snapshot = {
        let mut s = shared.state.lock().unwrap();
        s.merge_poll(polled);
        let _ = state::save(&shared.file, &s); // the lock is already ours
        s.clone()
    };
    #[cfg(desktop)]
    desktop::refresh_tray(app, &snapshot);
    let _ = &snapshot;
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct View {
    base_url: String,
    interval_min: u32,
    follow: Vec<String>,
    sites: Vec<Site>,
    recent: Vec<Recent>,
    open_at_login: bool,
    last_check: i64,
    last_error: String,
    /// When the next look is due (Unix ms).
    next_check: i64,
    /// True: the site publishes status.json and the app looks after the runs; false: every `interval_min` minutes.
    status_mode: bool,
    schedule_text: String,
    /// When the site's last run finished (`generated` of status.json, Unix ms); 0 = not known (no status.json yet).
    generated_ms: i64,
    /// "free" or "pro"; `plan_text` is the line shown in the settings; `free_max_follows` the limit of the free plan.
    plan: &'static str,
    plan_text: String,
    free_max_follows: usize,
    /// The sites that are really looked at (followed ones, plus the picked categories on the paid plan and in the trial).
    active: Vec<String>,
    /// The category picker: headings with their tags and how many sites carry each.
    groups: Vec<GroupCount>,
    /// The categories that are picked.
    tags: Vec<String>,
    /// How many sites the picked categories bring in (the ones switched off not counted).
    rule_count: usize,
    /// True on the paid plan and in the trial: categories can be used.
    unlocked: bool,
    /// False until the first-start category screen has been answered; the screen shows it when nothing is followed yet.
    onboarded: bool,
    purchase_url: &'static str,
    /// "0.1.0 ・ Windows x86_64 ・ 直接配布": what a user tells when something goes wrong (shown under 設定).
    about: String,
    /// True in a store build (Cargo feature `store-build`): no Stripe button, no license-key field; only the store sells.
    store_build: bool,
}

/// Where the "buy" button of the Stripe route leads (a Stripe Payment Link). Empty: no button.
const STRIPE_PAYMENT_LINK: &str = "";

/// A store build shows only the store's own purchase (App Store rules forbid license keys and outside payment links).
const STORE_BUILD: bool = cfg!(feature = "store-build");

fn plan_text(s: &State, now: i64) -> String {
    match s.plan(now) {
        Plan::Pro { source, expires_ms } => {
            let until = if expires_ms == 0 { "期限なし".to_string() } else { format!("あと約 {} 日", ((expires_ms - now).max(0) + 86_399_999) / 86_400_000) };
            format!("有料プラン（{}・{}）：フォロー数の制限なし、1日3回の定時確認", source.label(), until)
        }
        Plan::Trial { ends_ms } => format!(
            "お試し期間中：あと約 {} 日は制限なし。その後は無料プラン（{} サイトまで・1日1回）になります",
            ((ends_ms - now).max(0) + 86_399_999) / 86_400_000,
            FREE_MAX_FOLLOWS
        ),
        Plan::Free => format!("無料プラン：フォローは {} サイトまで、確認は1日1回（朝の更新分）", FREE_MAX_FOLLOWS),
    }
}

/// Additions are allowed only up to the limit of the plan; removing is always allowed.
fn limit_follow(old: &[String], wanted: Vec<String>, pro: bool) -> Vec<String> {
    if pro {
        return wanted;
    }
    let mut out: Vec<String> = wanted.iter().filter(|w| old.contains(w)).cloned().collect();
    for w in wanted.into_iter().filter(|w| !old.contains(w)) {
        if out.len() < FREE_MAX_FOLLOWS {
            out.push(w);
        }
    }
    out
}

fn schedule_text(s: &State) -> String {
    if s.status_mode {
        let c = &s.effective_schedule(now_ms());
        format!("定時: {}（日本時間）の約 {} 分後から {} 分間、{} 分おきに確認します。それ以外の時間は確認しません。", c.runs_text(), c.delay_min, c.window_min, c.step_min)
    } else if s.last_check == 0 {
        "最初の確認のあとに、確認の予定が表示されます。".to_string() // nothing has been read yet: do not claim that the site has no schedule
    } else {
        format!("このサイトは定時の情報を出していないので、{} 分おきに確認します。", s.interval_min)
    }
}

fn view(app: &AppHandle, s: &State) -> View {
    #[cfg(desktop)]
    let open_at_login = {
        use tauri_plugin_autostart::ManagerExt;
        app.autolaunch().is_enabled().unwrap_or(false)
    };
    #[cfg(not(desktop))]
    let open_at_login = false;
    let _ = app;
    View {
        base_url: s.base_url.clone(),
        interval_min: s.interval_min,
        follow: s.follow.clone(),
        sites: s.sites.clone(),
        recent: s.recent.clone(),
        open_at_login,
        last_check: s.last_check,
        last_error: s.last_error.clone(),
        next_check: next_check_ms(s, now_ms()),
        status_mode: s.status_mode,
        schedule_text: schedule_text(s),
        generated_ms: s.generated_ms,
        plan: match s.plan(now_ms()) { Plan::Pro { .. } => "pro", Plan::Trial { .. } => "trial", Plan::Free => "free" },
        plan_text: plan_text(s, now_ms()),
        free_max_follows: FREE_MAX_FOLLOWS,
        active: s.effective_follow(now_ms()),
        groups: counts(&s.sites, &s.groups),
        tags: s.tags.clone(),
        rule_count: s.rule_matches().iter().filter(|x| !s.exclude.contains(x)).count(),
        unlocked: s.plan(now_ms()).unlocked(),
        onboarded: s.onboarded,
        about: format!(
            "{} ・ {} {} ・ {}",
            app.package_info().version,
            std::env::consts::OS,
            std::env::consts::ARCH,
            if STORE_BUILD { "ストア版" } else { "直接配布" }
        ),
        purchase_url: if STORE_BUILD { "" } else { STRIPE_PAYMENT_LINK },
        store_build: STORE_BUILD,
    }
}

// ---- the commands the settings window may call (async: the main thread must never wait for the network) ----

#[tauri::command]
async fn get_state(app: AppHandle, shared: tauri::State<'_, SharedState>) -> Result<View, String> {
    let shared = shared.inner().clone();
    let need_names = shared.state.lock().unwrap().sites.is_empty();
    if need_names {
        let sh = shared.clone();
        let _ = tauri::async_runtime::spawn_blocking(move || {
            let mut copy = sh.state.lock().unwrap().clone();
            if refresh_sites(&mut copy, &HttpFetcher::new(), now_ms()).is_ok() {
                let mut s = sh.state.lock().unwrap();
                s.sites = copy.sites;
                s.groups = copy.groups;
                s.sites_at = copy.sites_at;
                save(&sh, &mut s);
            }
        })
        .await;
    }
    let s = shared.state.lock().unwrap().clone();
    Ok(view(&app, &s))
}

#[tauri::command]
fn set_follow(app: AppHandle, shared: tauri::State<'_, SharedState>, slugs: Vec<String>) -> Vec<String> {
    let follow = parse_follow_input(&slugs.join(","));
    let mut s = shared.state.lock().unwrap();
    let follow = limit_follow(&s.follow, follow, s.plan(now_ms()).unlocked());
    s.follow = follow.clone();
    save(&shared, &mut s);
    #[cfg(desktop)]
    desktop::refresh_tray(&app, &s);
    let _ = &app;
    let _ = shared.wake.lock().unwrap().send(()); // remember the newly followed sites now, not at the next window
    follow
}

/// Picks the categories whose sites are followed (and every site that joins them later). Only the paid plan and the trial.
#[tauri::command]
fn set_tags(app: AppHandle, shared: tauri::State<'_, SharedState>, tags: Vec<String>) -> Result<(), String> {
    let mut s = shared.state.lock().unwrap();
    if !tags.is_empty() && !s.plan(now_ms()).unlocked() {
        return Err("分野での選択は、お試し期間中と有料プランで使えます。".into());
    }
    let mut picked: Vec<String> = Vec::new();
    for t in tags {
        if s.sites.iter().any(|x| x.tags.contains(&t)) && !picked.contains(&t) {
            picked.push(t);
        }
    }
    s.tags = picked;
    let matches = s.rule_matches();
    s.exclude.retain(|x| matches.contains(x)); // switching a site off only means something while a category brings it in
    save(&shared, &mut s);
    #[cfg(desktop)]
    desktop::refresh_tray(&app, &s);
    let _ = &app;
    let _ = shared.wake.lock().unwrap().send(()); // look at the new sites now: they are only remembered (no flood)
    Ok(())
}

/// Switches one site on or off. A site that a picked category brings in is switched off through `exclude`; any other site
/// goes into or out of the followed list (the free plan's limit applies to that list). Returns the sites now looked at.
#[tauri::command]
fn toggle_site(app: AppHandle, shared: tauri::State<'_, SharedState>, slug: String, on: bool) -> Vec<String> {
    let slug = parse_follow_input(&slug).into_iter().next().unwrap_or_default();
    let mut s = shared.state.lock().unwrap();
    if !slug.is_empty() {
        let unlocked = s.plan(now_ms()).unlocked();
        let by_category = unlocked && s.rule_matches().contains(&slug);
        if on {
            s.exclude.retain(|x| x != &slug);
            if !by_category && !s.follow.contains(&slug) {
                let mut wanted = s.follow.clone();
                wanted.push(slug.clone());
                let old = s.follow.clone();
                s.follow = limit_follow(&old, wanted, unlocked);
            }
        } else {
            s.follow.retain(|x| x != &slug);
            if by_category && !s.exclude.contains(&slug) {
                s.exclude.push(slug.clone());
            }
        }
        save(&shared, &mut s);
        #[cfg(desktop)]
        desktop::refresh_tray(&app, &s);
        let _ = shared.wake.lock().unwrap().send(());
    }
    let _ = &app;
    s.effective_follow(now_ms())
}

/// The first-start category screen has been answered (or skipped).
#[tauri::command]
fn finish_setup(shared: tauri::State<'_, SharedState>) {
    let mut s = shared.state.lock().unwrap();
    s.onboarded = true;
    save(&shared, &mut s);
}

#[derive(Serialize)]
struct Imported {
    added: usize,
    follow: Vec<String>,
}

#[tauri::command]
fn import_follow(app: AppHandle, shared: tauri::State<'_, SharedState>, text: String) -> Imported {
    let add = parse_follow_input(&text);
    let mut s = shared.state.lock().unwrap();
    let before = s.follow.len();
    let mut wanted = s.follow.clone();
    for a in &add {
        if !wanted.contains(a) {
            wanted.push(a.clone());
        }
    }
    let old = s.follow.clone();
    s.follow = limit_follow(&old, wanted, s.plan(now_ms()).unlocked());
    let add: Vec<String> = s.follow[before..].to_vec();
    save(&shared, &mut s);
    #[cfg(desktop)]
    desktop::refresh_tray(&app, &s);
    let _ = &app;
    let _ = shared.wake.lock().unwrap().send(());
    Imported { added: add.len(), follow: s.follow.clone() }
}

#[tauri::command]
fn set_options(app: AppHandle, shared: tauri::State<'_, SharedState>, base_url: Option<String>, interval_min: Option<u32>, open_at_login: Option<bool>) -> Result<(), String> {
    let mut s = shared.state.lock().unwrap();
    if let Some(u) = base_url {
        let next = state::normalize_base(&u)?;
        if next != s.base_url {
            s.base_url = next; // another site: start fresh
            s.forget_site_data();
        }
    }
    if let Some(m) = interval_min {
        s.interval_min = m.clamp(5, 240);
    }
    #[cfg(desktop)]
    if let Some(on) = open_at_login {
        use tauri_plugin_autostart::ManagerExt;
        let _ = if on { app.autolaunch().enable() } else { app.autolaunch().disable() };
    }
    let _ = (&app, open_at_login);
    save(&shared, &mut s);
    let _ = shared.wake.lock().unwrap().send(()); // re-read the interval
    Ok(())
}

/// The Stripe route: the signed license key the buyer received. An empty text removes it.
#[tauri::command]
fn set_license(app: AppHandle, shared: tauri::State<'_, SharedState>, key: String) -> Result<(), String> {
    if STORE_BUILD {
        return Err("このビルドではライセンスキーは使えません。".into());
    }
    let key = key.trim().to_string();
    if !key.is_empty() {
        verify_license(&key, now_ms())?;
    }
    let mut s = shared.state.lock().unwrap();
    s.license = key;
    save(&shared, &mut s);
    #[cfg(desktop)]
    desktop::refresh_tray(&app, &s);
    let _ = &app;
    let _ = shared.wake.lock().unwrap().send(());
    Ok(())
}

/// The store route: called by the platform part (Microsoft Store / App Store / Google Play) after the store itself has
/// confirmed the subscription. `expires_ms` = 0 clears it. The store, not this app, decides whether it is valid.
#[tauri::command]
fn set_store_entitlement(shared: tauri::State<'_, SharedState>, source: Source, expires_ms: i64) {
    let mut s = shared.state.lock().unwrap();
    s.store = (expires_ms > 0).then(|| StoreEntitlement { source, expires_ms, checked_ms: now_ms() });
    save(&shared, &mut s);
    let _ = shared.wake.lock().unwrap().send(());
}

#[tauri::command]
fn check_now(shared: tauri::State<'_, SharedState>) {
    let _ = shared.wake.lock().unwrap().send(());
}

#[tauri::command]
fn test_notification(app: AppHandle) {
    let _ = app.notification().builder().title("G醫t").body("通知のテストです。").show();
}

#[tauri::command]
fn open_external(app: AppHandle, url: String) {
    open_url(&app, &url);
}

/// Waits until `target_ms` on the wall clock (so a computer that slept wakes up late, not never) or until somebody wakes
/// the thread ("check now", a changed follow list or option). True: it was woken.
fn wait_until(rx: &std::sync::mpsc::Receiver<()>, target_ms: i64) -> bool {
    loop {
        let left = target_ms - now_ms();
        if left <= 0 {
            return false;
        }
        match rx.recv_timeout(Duration::from_millis(left.min(30_000) as u64)) {
            Ok(()) => {
                while rx.try_recv().is_ok() {} // several wake-ups at once (many sites ticked) are one check
                return true;
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                std::thread::sleep(Duration::from_secs(30));
            }
        }
    }
}

/// 0 to 45 seconds, different on every computer and every time: thousands of apps must not ask in the same second.
fn jitter_ms() -> i64 {
    let n = (now_ms() as u64) ^ (std::process::id() as u64).wrapping_mul(2_654_435_761);
    (n.wrapping_mul(6_364_136_223_846_793_005) >> 40) as i64 % 45_000
}

/// The checking thread. It looks for news after the runs of the pipeline (git_core::schedule): in a short window after
/// each run, every few minutes until the run is seen to be finished. Right after the start, after a wake-up (check now,
/// a changed follow list) and when a window was missed (the app was closed, the computer asleep) it looks at once.
fn spawn_checker(app: AppHandle, shared: SharedState, rx: std::sync::mpsc::Receiver<()>) {
    std::thread::spawn(move || {
        let mut target = now_ms() + 10_000; // the first look shortly after the start
        loop {
            let manual = wait_until(&rx, target);
            let follows = !shared.state.lock().unwrap().effective_follow(now_ms()).is_empty();
            if follows || manual {
                check(&app, &shared);
            }
            let at = {
                let s = shared.state.lock().unwrap();
                if s.effective_follow(now_ms()).is_empty() { now_ms() + 10 * 60_000 } else { next_check_ms(&s, now_ms()) }
            };
            target = at + jitter_ms();
        }
    });
}

#[cfg(desktop)]
mod desktop {
    use super::*;
    use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
    use tauri_plugin_autostart::ManagerExt;

    pub fn show_window(app: &AppHandle) {
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.show();
            let _ = w.unminimize();
            let _ = w.set_focus();
        }
    }

    fn build_menu(app: &AppHandle, s: &State) -> tauri::Result<Menu<tauri::Wry>> {
        let when = if s.last_check == 0 { "まだ確認していません".to_string() } else { format!("最終確認: {}", format_local(s.last_check)) };
        let active = s.effective_follow(now_ms());
        let next = if active.is_empty() { "次回の確認: （サイトを選んでください）".to_string() } else { format!("次回の確認: {}", format_local(next_check_ms(s, now_ms()))) };
        let mut recent_items: Vec<MenuItem<tauri::Wry>> = Vec::new();
        for (i, r) in s.recent.iter().take(8).enumerate() {
            let label = format!("{}  {}", r.name, r.time);
            recent_items.push(MenuItem::with_id(app, format!("open:{i}"), label, true, None::<&str>)?);
        }
        let recent_refs: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> = recent_items.iter().map(|i| i as &dyn tauri::menu::IsMenuItem<tauri::Wry>).collect();
        let recent = Submenu::with_items(app, "最近の更新（クリックで差分を開く）", !recent_items.is_empty(), &recent_refs)?;
        let login = CheckMenuItem::with_id(app, "login", "ログイン時に起動", true, app.autolaunch().is_enabled().unwrap_or(false), None::<&str>)?;
        Menu::with_items(
            app,
            &[
                &MenuItem::with_id(app, "info", format!("フォロー中: {} サイト", active.len()), false, None::<&str>)?,
                &MenuItem::with_id(app, "when", when, false, None::<&str>)?,
                &MenuItem::with_id(app, "next", next, false, None::<&str>)?,
                &PredefinedMenuItem::separator(app)?,
                &MenuItem::with_id(app, "check", "今すぐ確認", true, None::<&str>)?,
                &recent,
                &MenuItem::with_id(app, "settings", "設定・サイトの選択…", true, None::<&str>)?,
                &MenuItem::with_id(app, "dashboard", "ダッシュボードを開く", true, None::<&str>)?,
                &PredefinedMenuItem::separator(app)?,
                &login,
                &PredefinedMenuItem::separator(app)?,
                &MenuItem::with_id(app, "quit", "終了", true, None::<&str>)?,
            ],
        )
    }

    fn format_local(ms: i64) -> String {
        // "MM-DD HH:MM" in JST: the same clock as everywhere else in G醫t
        let secs = ms / 1000 + 9 * 3600;
        let days = secs.div_euclid(86400);
        let rem = secs.rem_euclid(86400);
        let (y, m, d) = civil_from_days(days);
        format!("{y:04}-{m:02}-{d:02} {:02}:{:02} JST", rem / 3600, (rem % 3600) / 60)
    }

    // days since 1970-01-01 -> (year, month, day), proleptic Gregorian (Howard Hinnant's algorithm)
    fn civil_from_days(z: i64) -> (i64, i64, i64) {
        let z = z + 719468;
        let era = z.div_euclid(146097);
        let doe = z.rem_euclid(146097);
        let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
        let y = yoe + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        (if m <= 2 { y + 1 } else { y }, m, d)
    }

    pub fn refresh_tray(app: &AppHandle, s: &State) {
        if let Some(tray) = app.tray_by_id("main") {
            if let Ok(menu) = build_menu(app, s) {
                let _ = tray.set_menu(Some(menu));
            }
            let _ = tray.set_tooltip(Some(format!("G醫t（{} サイトをフォロー中）", s.effective_follow(now_ms()).len())));
        }
    }

    pub fn setup(app: &AppHandle, shared: &SharedState) -> tauri::Result<()> {
        let menu = build_menu(app, &shared.state.lock().unwrap())?;
        let shared_for_menu = shared.clone();
        TrayIconBuilder::with_id("main")
            .icon(app.default_window_icon().cloned().expect("window icon"))
            .tooltip("G醫t")
            .menu(&menu)
            .show_menu_on_left_click(false)
            .on_tray_icon_event(|tray, event| {
                if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                    show_window(tray.app_handle());
                }
            })
            .on_menu_event(move |app, event| {
                let id = event.id().as_ref().to_string();
                match id.as_str() {
                    "check" => {
                        let _ = shared_for_menu.wake.lock().unwrap().send(());
                    }
                    "settings" => show_window(app),
                    "dashboard" => {
                        let url = shared_for_menu.state.lock().unwrap().base_url.clone();
                        open_url(app, &url);
                    }
                    "login" => {
                        let al = app.autolaunch();
                        let _ = if al.is_enabled().unwrap_or(false) { al.disable() } else { al.enable() };
                        let s = shared_for_menu.state.lock().unwrap().clone();
                        refresh_tray(app, &s);
                    }
                    "quit" => app.exit(0),
                    other if other.starts_with("open:") => {
                        if let Ok(i) = other[5..].parse::<usize>() {
                            let url = shared_for_menu.state.lock().unwrap().recent.get(i).map(|r| if r.count > 1 { r.history_url.clone() } else { r.diff_url.clone() });
                            if let Some(url) = url {
                                open_url(app, &url);
                            }
                        }
                    }
                    _ => {}
                }
            })
            .build(app)?;
        Ok(())
    }
}

/// Android: what Kotlin's `BackgroundCheck` calls (the job of WorkManager, which runs without the window and without the
/// timer thread). The work is `git_core::background`; this only moves strings and numbers across.
#[cfg(target_os = "android")]
mod android_job {
    use super::now_ms;
    use git_core::background;
    use git_core::poller::{HttpFetcher, Notice};
    use jni::objects::{JClass, JString};
    use jni::sys::{jlong, jstring};
    use jni::JNIEnv;
    use std::panic::{catch_unwind, AssertUnwindSafe};
    use std::path::PathBuf;

    fn state_file(env: &mut JNIEnv, dir: &JString) -> Option<PathBuf> {
        let dir: String = env.get_string(dir).ok()?.into();
        Some(PathBuf::from(dir).join("state.json"))
    }

    /// What is opened when the notification is tapped: the diff page of one update, the history of several.
    fn target(n: &Notice) -> &str {
        if n.count > 1 || n.slug.is_empty() { &n.history_url } else { &n.diff_url }
    }

    /// `{"notices":[{"title","body","url"}],"nextMs":..., "error":"..."}`
    #[no_mangle]
    pub extern "system" fn Java_jp_git_1app_client_BackgroundCheck_nativeCheck<'l>(mut env: JNIEnv<'l>, _class: JClass<'l>, dir: JString<'l>) -> jstring {
        let json = catch_unwind(AssertUnwindSafe(|| match state_file(&mut env, &dir) {
            Some(path) => {
                let o = background::run(&path, &HttpFetcher::new(), now_ms());
                let notices: Vec<serde_json::Value> =
                    o.notices.iter().map(|n| serde_json::json!({ "title": n.title, "body": n.body, "url": target(n) })).collect();
                serde_json::json!({ "notices": notices, "nextMs": o.next_ms, "error": o.error }).to_string()
            }
            None => serde_json::json!({ "notices": [], "nextMs": now_ms() + 30 * 60_000, "error": "no data directory" }).to_string(),
        }))
        .unwrap_or_else(|_| serde_json::json!({ "notices": [], "nextMs": now_ms() + 30 * 60_000, "error": "panic" }).to_string());
        env.new_string(json).map(|s| s.into_raw()).unwrap_or(std::ptr::null_mut())
    }

    /// When the next look is due (Unix ms), without looking.
    #[no_mangle]
    pub extern "system" fn Java_jp_git_1app_client_BackgroundCheck_nativeNext<'l>(mut env: JNIEnv<'l>, _class: JClass<'l>, dir: JString<'l>) -> jlong {
        catch_unwind(AssertUnwindSafe(|| match state_file(&mut env, &dir) {
            Some(path) => background::next(&path, now_ms()),
            None => now_ms() + 30 * 60_000,
        }))
        .unwrap_or_else(|_| now_ms() + 30 * 60_000)
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Linux: WebKitGTK's DMABUF renderer shows an empty window on some graphics stacks (seen under WSLg; known for NVIDIA).
    // This screen is plain HTML, so the plain renderer costs nothing. A value set by the user is kept.
    #[cfg(target_os = "linux")]
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }
    let mut builder = tauri::Builder::default();

    #[cfg(desktop)]
    {
        builder = builder
            .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| desktop::show_window(app)))
            .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, None::<Vec<&'static str>>));
    }

    builder
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let dir = std::env::var("GIT_CLIENT_DATA").map(PathBuf::from).unwrap_or_else(|_| app.path().app_data_dir().expect("app data dir"));
            let file = dir.join("state.json");
            let (tx, rx) = channel::<()>();
            let shared: SharedState = Arc::new(Shared { state: Mutex::new(state::load(&file)), file, wake: Mutex::new(tx) });
            app.manage(shared.clone());
            spawn_checker(app.handle().clone(), shared.clone(), rx);
            #[cfg(desktop)]
            {
                desktop::setup(app.handle(), &shared)?;
                if { let s = shared.state.lock().unwrap(); !s.onboarded && s.follow.is_empty() && s.tags.is_empty() } {
                    desktop::show_window(app.handle()); // first start: choose the sites
                }
            }
            #[cfg(mobile)]
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // The window only hides: the app stays in the tray / keeps checking
            #[cfg(desktop)]
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
            let _ = (window, event);
        })
        .invoke_handler(tauri::generate_handler![get_state, set_follow, set_tags, toggle_site, finish_setup, import_follow, set_options, set_license, set_store_entitlement, check_now, test_notification, open_external])
        .run(tauri::generate_context!())
        .expect("error while running G醫t client");
}
