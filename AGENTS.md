# AGENTS.md: G醫t client (`g-i-t-client`)

Instructions for AI coding agents (Claude Code, Antigravity, ...) working in this repository. The server side (pipeline,
dashboard, feeds) is specified in `g-i-t-app/AGENTS.md`; this file covers only the client. `README.md` is the user-facing
how-to; this file is the specification. Where the two disagree, the code wins, then this file.

Legend used below: **[done]** implemented and tested, **[partly]** implemented but not verified end to end, **[todo]** not
written. Never describe a [todo] item as working.

---

## 1. Purpose and non-negotiable principles

A small desktop/mobile app (Tauri 2 + Rust) that shows an OS notification when a **followed** site is updated.

1. **No server, no account.** The follow list, the state and the plan live on the device in one JSON file. Do not add a
   backend, a user database, telemetry or sign-in without an explicit decision by the owner.
2. **The client is a reader of public files.** It reads `feeds/status.json`, `feeds/<slug>.xml` and `search.json` of the
   dashboard (default base URL `https://giiit.goudge.org/`). It never writes to the dashboard.
3. **Network text is untrusted.** Site names, summaries and feed text reach the screen only through `textContent`, never
   `innerHTML`. Only http(s) URLs are handed to the OS browser. The CSP in `src-tauri/tauri.conf.json` stays strict
   (`default-src 'self'`, no inline script or style).
4. **Core has no UI and no OS code.** Everything that is the same on every platform lives in `core/` (`git_core`). Platform
   shells hand in a `Fetcher` and a notification callback.
5. **Pure functions of the clock.** Anything that depends on time takes `now_ms` as an argument (see `schedule.rs`,
   `entitlement.rs`, `State::plan`), so it can be tested without waiting.
6. **Do not reinvent the wheel; use existing Rust crates to the full.** Before writing code for something general (parsing,
   crypto, time, HTTP, paths, locking, notifications, autostart, store billing, ...), look for a maintained crate (or a
   Tauri plugin) and use it. Hand-written code is for what is specific to G醫t (the schedule rule, the plan rule, the
   feed/state logic). Checks before adding a crate:
   - maintained (recent release, not archived), widely used, a licence compatible with ours (see §10), and it builds on
     every target we want (Windows, macOS, Linux, Android, iOS) or can be switched off with `cfg` for the others;
   - small dependency tree (`cargo tree -p git_core`); the core must stay light and `unsafe`-free of our own;
   - never write your own cryptography, signature, ZIP/XML/JSON/URL parser or date arithmetic: the core already uses
     `ed25519-dalek`, `base64`, `serde_json`, `roxmltree`, `chrono`, `ureq`; the shell uses the Tauri 2 plugins
     (`notification`, `opener`, `autostart`, `single-instance`). Prefer extending those over a second crate for the same
     job. Run `cargo audit` (and `cargo deny` if configured) when dependencies change; commit `Cargo.lock`.
   - when no crate fits, say so in the change and keep the hand-written part small and tested.

---

## 1a. Targets and distribution (decision record)

This section says what the app is built **for**. Entries marked **[decided]** were stated by the owner; entries marked
**[undecided]** are a working assumption only: do not start store-specific work on them, ask first (see §10).

| Target | In scope? | Distribution channel | Billing | Status |
| --- | --- | --- | --- | --- |
| Windows 10/11 (desktop) | **[decided]** yes; it is the development and test platform | installer from the build; Microsoft Store **[undecided]** | Stripe license key (direct build); Microsoft Store subscription **[undecided]** | built and run |
| macOS (desktop) | **[decided]** yes, same code as Windows | direct download (notarized); Mac App Store **[undecided]** | Stripe license key (direct build); App Store IAP for the store build | not run |
| Android | **[decided]** wanted; the checking while closed is not written | Google Play **[undecided]**, or an APK | Google Play Billing for the store build | **[partly]** debug APK for x86_64 builds on Windows (`src-tauri/gen/android` exists), not run on a device or the emulator; background check missing |
| iOS / iPadOS | **[decided]** wanted; no background check without push | App Store **[undecided]** | App Store IAP only (see §6) | screen shared, push missing |
| watchOS, tvOS | no: not Tauri targets | – | – | – |
| Linux (desktop) | **[decided]** wanted | Debian package (`.deb`, Tauri bundle) as a direct download, and a source package (`debian/`) for a **self-hosted apt repository**; **submission to the official Debian archive is undecided** (see below); Flathub **[undecided]** | Stripe license key (direct build only: no store on Linux sells subscriptions) | **[partly]** both packages build in WSL Debian 13 (`giiit_0.1.0_amd64.deb` from Tauri; `giiit_0.1.0-1_amd64.deb` from `debian/`); the binary was started under WSLg and its window renders correctly (first-start screen, chips) **only with `WEBKIT_DISABLE_DMABUF_RENDERER=1`** (without it the window stays empty), so the app sets that variable itself on Linux; the `.deb` packages have not been installed, and notifications, tray and autostart have not been tried on Linux; GNOME shows no tray icon without an AppIndicator extension, so the window must not be the only way back in |
| ChromeOS | **[decided]** wanted (owner's requirement: it must run on x86_64 Chromebooks and ChromeBoxes, not only on ARM) | Google Play (the Android build), or the Linux `.deb` in the ChromeOS Linux container **[undecided]** | Google Play Billing (store build); Stripe license key for the `.deb` | not built, not tested |

Linux packaging rules:
- **Names are ASCII.** Product, package and binary are `giiit` (`productName` / `mainBinaryName` in `tauri.conf.json`);
  the text "G醫t" stays in the window title, tray tooltip, notifications and the `.desktop` display name. A Debian package
  name with multibyte characters (the earlier `g醫t`) is invalid. Do not change `identifier` (`jp.git-app.client`): it
  decides where `state.json` lives. Consequences: on Windows the toast name now follows the shortcut name `giiit` (a
  `tauri.windows.conf.json` can set `productName` back to "G醫t" if the owner wants), and an autostart entry made under
  the old name stays behind (turn "ログイン時に起動" off and on again).
- **Two ways to build the Linux package.** (1) `npx tauri build --bundles deb` (build in the Linux file system, not on
  `/mnt/d`; WSL Debian works): the quick `.deb` for GitHub Releases. Fix its `Maintainer` (`bundle.publisher`) before
  publishing. (2) `debian/` + `scripts/make-debian-orig.sh`: a quilt source package built offline from `cargo vendor`
  crates, for an own apt repository: `scripts/make-debian-orig.sh ..`, unpack the tarball, copy `debian/` into it,
  `dpkg-buildpackage -us -uc -b`. `debian/control` and `debian/changelog` hold `FIXME` maintainer fields and
  `debian/copyright` must list the vendored crates' licences before a first upload.
- **Official Debian archive: undecided.** It needs every Rust dependency packaged separately in Debian (no vendoring, no
  network at build time), a sponsoring Debian Developer, an ITP bug and a full `debian/copyright`. Tauri 2 has hundreds of
  crates, many not in Debian, so this is a large piece of work. Do not start it without the owner.

ChromeOS rule: the Android build **must contain the x86_64 library** (and arm64-v8a), not arm only. Tauri builds
`aarch64`, `armv7`, `i686` and `x86_64` Android targets; do not drop `x86_64` from the build or from the Play Console
device catalog. Check on a real x86_64 Chromebook/ChromeBox (or the ChromeOS emulator image) before saying it works.
The Linux `.deb` must also be built for both `amd64` and `arm64`.

Rules that follow from the table:
- **Two build kinds.** A *direct build* (Windows installer, macOS outside the store) carries the Stripe button and the
  license-key field. A *store build* (any store) shows only that store's own purchase (§6). One code base, one switch.
- The owner wants the purchase to be possible in **Mac App Store, App Store, Google Play, Microsoft Store and Stripe**;
  the first release order is **[undecided]** (working assumption: Windows direct build first, then the Microsoft Store).
- A new platform or store is added only after the owner confirms it here; update this table in the same change.

---

## 2. Layout

```text
g-i-t-client/
├── core/                 git_core (Rust library)
│   ├── src/feed.rs         Atom / search.json / status.json parsing, share-URL parsing (?follow=a,b,c)
│   ├── src/state.rs        State (one JSON file, atomic save), migration, plan helpers
│   ├── src/schedule.rs     when to look: windows after the pipeline runs
│   ├── src/poller.rs       poll_once / poll_scheduled / next_check_ms (ETag, 304, first-seen, burst)
│   ├── src/entitlement.rs  plans, trial, store result, Ed25519 license keys
│   ├── examples/license_tool.rs   operator tool: keygen / sign (never ship, never commit its output keys)
│   └── tests/              core.rs, scheduled.rs, entitlement.rs (offline, fake network)
├── src-tauri/            Tauri shell: tray, timer thread, notifications, autostart, commands for the screen
├── debian/               source-package files for a self-hosted apt repository (quilt, vendored crates); see §1a
├── scripts/make-debian-orig.sh   makes ../giiit_<version>.orig.tar.xz (source + `cargo vendor`)
├── ui/                   plain HTML/JS/CSS, no bundler (index.html, app.js, style.css)
├── icon.png              source of the icons (`npm run icons`)
├── .github/workflows/build.yml   manual, unsigned bundles (Windows/macOS/Android/iOS simulator) — [partly]: not run on GitHub yet; the Windows build, the Android debug APK and the Linux packages were built locally
├── README.md             user-facing documentation
├── ChangeLog.md          what changed, by area (update it with every change that a user or a developer would notice)
├── TRADEMARK.md          name and logo policy (draft; the owner fills the FIXME lines)
├── LICENSE               MPL-2.0
└── AGENTS.md             this file
```

---

## 3. Commands

```powershell
cargo test -p git_core          # core tests, offline (currently 5 + 12 + 14 + 4 + 13 = 48, 0 doc)
cargo check -p git-client       # the Tauri shell compiles (first run takes minutes; run it in the background)
npm install; npm run dev        # run the app
npm run build                   # installer in target/release/bundle
GIT_TRIAL_DAYS=14 npm run build # build with another trial length
# Android (Windows): needs Developer Mode (symlinks), ANDROID_HOME, NDK_HOME (SDK ndk folder), JAVA_HOME (Android Studio's jbr)
npx tauri android build --apk --debug --target x86_64   # first run ~15 min; omit --target for all four ABIs (keep x86_64: ChromeOS)
cargo run -p git_core --example license_tool -- keygen | sign <private-key-file> <subscriber> [days]
```

Before saying a change is done: `cargo test -p git_core` and `cargo check -p git-client` both pass. UI changes cannot be
verified by these two; say so instead of claiming they work.

`GIT_CLIENT_DATA=<dir>` moves `state.json` (use it for tests and for a second instance).

---

## 4. Behaviour of the core

### 4.1 When the app looks **[done]**
- The pipeline runs at fixed times. The app reads `feeds/status.json` (ETag/304) in a window that begins `delayMin`
  minutes after each run and lasts `windowMin` minutes, every `step` (3) minutes, and stops as soon as `generated` is not
  before the start of that run. Outside the windows it does nothing.
- Only feeds of followed sites whose head hash changed are read, at `feeds/<slug>.xml?h=<hash>`.
- At start, after a missed window, on "check now" and when the follow list changes it looks at once. A random 0–45 s is
  added so that apps do not ask in the same second.
- A site without `status.json` (404) falls back to every followed feed every `interval_min` minutes (default 15, 5–240).
  `interval_min` is not shown on the screen any more; it is a fallback only.

### 4.2 What is announced **[done]**
- A site seen for the first time is only remembered (no flood of old updates: **no OS notification**), but its newest
  `FIRST_LOOK_RECENT` (3) entries are listed under 「最近の更新」 so that a just-followed site does not look empty.
  A site with no real update yet gets an empty `seen` entry, so its first update is announced.
- An update is remembered as `<slug>/<commit>` (`Entry::key()`), not as the whole entry id (the id carries the host name
  and changed with the domain move). Old states are converted on load.
- More than `BURST` (3) new entries of one site at once become one notification.
- More than `MAX_NOTICES` (5) notifications in ONE check (several sites, typically a whole category) become one summary
  notification ("11 件の更新を検知しました（A、B、C ほか）") that opens the dashboard; every update is still listed
  one by one under 「最近の更新」.
- 「最近の更新」 is sorted by the time of the update, newest first (`Recent::updated_ms`, `State::sort_recent`), not by
  the time it was found; at most `RECENT_LIMIT` (50). The screen shows only the entries of the sites that are looked at
  now (a category that was switched off keeps its history in the state, it is back when the category is picked again).
- Of the sites seen for the first time in one check only the first `FIRST_LOOK_SITES` (5) get their newest entries
  listed, so that picking a whole category does not bury the list.
- Links are built from the configured base URL, never from the host inside a feed.
- One failing site does not stop the others. A network error leaves the state unchanged.

### 4.3 State file **[done]**
`state.json` in the app data folder, camelCase keys, atomic save. `State::load` runs `migrate()`:
the old default base URL (`https://tanukima.github.io/g-i-t-data/`) becomes the new one, and the links stored in
"最近の更新" that still start with the old address are rewritten to the current base URL.
New fields must have `#[serde(default)]` behaviour (the struct has it) so that old files still load.

### 4.4 Choosing sites by category **[done in core, shell and screen; looked at in the emulator only in part]**
- The dashboard publishes `tags` per site and, since `g-i-t-app` commit `3deaa04`, `groups` (`tag_groups` of
  config.yaml: headings and their tags, in order) in `search.json`. The client reads both (`parse_sites`, `parse_groups`).
  A dashboard without `groups` still works: all tags come under one heading "その他", sorted.
- **Rule (`category.rs`, `matching`)**: the picked tags of one heading are alternatives ("or"), the headings are all
  required ("and"); picked tags of no heading form one more heading; nothing picked matches nothing. Example: 学会 +
  (がん or 感染症).
- **State**: `tags` (picked categories), `exclude` (sites of a picked category that the user switched off), `follow`
  (sites added one by one), `groups`, `onboarded`. `State::effective_follow` = `follow` + (sites matching `tags` −
  `exclude`) on the paid plan and in the trial; on the free plan only the first 3 of `follow` (categories are kept but
  not used). A site that joins a picked category later is followed without any action.
- **Plan split (owner's decision)**: free = up to 3 sites chosen one by one; trial and paid = categories and no limit.
- **Commands**: `set_tags` (refused on the free plan), `toggle_site(slug, on)` (a site brought in by a category is
  switched off through `exclude`, others go into / out of `follow`; free limit applies), `finish_setup`.
- **Screen**: first start (nothing followed, not `onboarded`, `groups` known) shows 「どの分野の更新を受け取りますか？」;
  the 通知 tab has 「分野で選ぶ」 (chips per heading with the number of sites, "選んだ分野の N サイトが対象です") and the
  long site list folded under 「サイトを個別に選ぶ・外す」.
- `search.json` is read again after an hour when the list has no headings yet (`GROUPS_RETRY_MS`); `get_state` copies
  `groups` together with `sites`.

---

## 5. Plans, trial and licenses (specification)

### 5.1 Plans **[done]**

| | Free | Trial (first 7 days) | Paid |
| --- | --- | --- | --- |
| Followed sites | 3 (`FREE_MAX_FOLLOWS`), one by one | no limit, one by one or by category (§4.4) | no limit, one by one or by category |
| Looking for news | only the first run of the day (a daily digest) | every run (3 a day) | every run (3 a day) |

- `State::plan(now)` returns `Plan::Pro` (paid) > `Plan::Trial` > `Plan::Free`. `Plan::unlocked()` is true for Pro and
  Trial and is what the limits test; `is_pro()` means "paid" only.
- Free: `State::effective_follow` looks at the first 3 followed sites only; the list itself is never shortened by a
  downgrade. `State::effective_schedule` keeps only the first run of the day. Updates of later runs are found by the next
  morning's look (head hash differs), so nothing is lost, only delayed.
- Additions beyond the limit are refused by the shell (`limit_follow`); removing is always allowed. The screen says
  「無料プランでは N サイトまでです。」.

### 5.2 Trial **[done]**
- `trialStartedMs` is set at the first start (`State::tick`); the trial lasts `entitlement::trial_days()` days:
  environment variable `GIT_TRIAL_DAYS` at run time, else the build-time value of the same name, else
  `DEFAULT_TRIAL_DAYS` = 7. `0` = no trial. Clamped to 0–3650.
- `clockMs` is the latest time ever seen; the trial uses `max(now, clockMs)`, so a clock set back does not extend it.
- Deleting `state.json` restarts the trial. **Accepted by the owner**: the follow list is in the same file, so it costs
  the user their settings. Do not add OS-level markers (registry, keychain) unless the owner asks.
- Existing installations start their trial at the first start of the version that has it.

### 5.3 Two ways to prove a purchase, both on the device **[done: core / todo: store code]**
1. **Stripe → license key [done].** The operator signs `GIT1.<payload>.<signature>` (Ed25519; payload `{v:1, sub, exp}`,
   `exp` Unix ms, 0 = no expiry) with `license_tool sign`. The app verifies it against `LICENSE_PUBLIC_KEY`
   (`core/src/entitlement.rs`) every time the plan is computed; the key text is kept in `state.license`. The command
   `set_license(key)` rejects keys that do not verify. Issuing the key after a Stripe payment is **manual** — there is no
   webhook (that would need a server; ask the owner before adding one).
2. **Store subscription → `set_store_entitlement(source, expiresMs)` [todo for every store].** The platform part asks the
   store's own API (Microsoft Store: Windows.Services.Store; App Store: StoreKit 2; Google Play: Play Billing) and calls
   the command. The result is trusted until `expiresMs` and for at most `STORE_GRACE_MS` = 7 days after the last
   confirmation (offline use). `expiresMs = 0` clears it. Only the command exists; **no store SDK code is written**, and
   each store needs a developer account, a subscription product and a review.

### 5.4 Keys and secrets **[important]**
- The operator's **private key** is `~/.g-i-t-license/private.key` (outside any repository). Never print it, never
  commit it, never paste it into a chat. If it is lost, `LICENSE_PUBLIC_KEY` must be replaced and the app re-released;
  keys signed with the old key stop working.
- `LICENSE_PUBLIC_KEY` in the repository is the public half of that key and is not secret.
- `STRIPE_PAYMENT_LINK` (`src-tauri/src/lib.rs`) is empty. While it is empty the "購入する(Stripe)" button is hidden.
  It is a public Stripe Payment Link, not a secret key.

### 5.5 Known limits of the design (state them, do not hide them)
- `state.json` is plain JSON: whoever edits `store` or `trialStartedMs` unlocks the paid plan. A license key can be
  shared. This is the price of "no server".
- Store results are trusted as handed in by the shell; there is no server-side receipt validation.
- **The source is public (MPL-2.0), so anyone can build a copy without the limits.** `FREE_MAX_FOLLOWS`, the schedule rule
  and the key check are ordinary code. A build-time gate (a passphrase in an environment variable whose hash must match a
  hash in the source) was considered and **rejected as protection**: the hash and the check are in the same source, so the
  check is deleted or the hash replaced in one line, and the limits can be edited directly without touching the gate.
  What the owner can rely on instead: the official signed/notarized builds and the stores' own distribution, the name
  and icon (trademark, not covered by the licence), and the fact that everything the app reads is public anyway.
  Do not add a build gate and do not describe the limits as "protected".

### 5.6 Testers and developers
- Preferred: a signed, time-limited license key (`license_tool sign ... <days>`); it goes through the same code as a
  customer's key, so it also tests the release build.
- Store builds: use the stores' own test facilities (Apple Sandbox/TestFlight, Google Play license testers, Microsoft
  Store flights).
- **Never** add a hidden switch, a hard-coded key, a debug menu or an environment variable that unlocks the paid plan in a
  release build. A debug-only unlock must be behind `cfg!(debug_assertions)` or a Cargo feature that release builds do not
  enable, and the owner must have asked for it. [todo, not requested yet]

---

## 6. Store and platform policy decisions

- **Apple (iOS, and App Store builds on macOS):** digital features unlocked in the app must use In-App Purchase
  (Guideline 3.1.1). License keys as an unlock mechanism are not allowed, and in-app links to an outside payment are
  restricted (the rules differ by region: US, EU, Japan have exceptions that change over time — check the current
  guidelines before relying on one). Guideline 3.1.3(b) allows using a subscription bought elsewhere only if the same
  thing can also be bought with IAP.
- **Therefore a store build must hide the license-key field and the Stripe button, and offer only the store's own
  purchase. [partly]** The switch exists: Cargo feature `store-build` of `src-tauri` (`cargo check -p git-client
  --features store-build`; for a bundle `npm run tauri build -- --features store-build`). With it, `View.storeBuild` is
  true, `purchaseUrl` is empty, the screen hides the license row, and `set_license` refuses. Compiles in both modes; the
  screen itself has not been looked at. The store's own purchase button and billing code are still **[todo]**, so do not
  submit this app to any store yet. The direct-download build (no feature) keeps Stripe + license key.
- Google Play and Microsoft Store have their own billing rules (Microsoft is the most permissive for non-game apps).
  Check the current policy before shipping either.
- **Trial without a payment method** (the app's own 7-day trial) is allowed on every store; it is not a store free trial.

---

## 7. Tauri shell and screen

### 7.1 Shell (`src-tauri/src/lib.rs`)
- Desktop **[done]**: tray (check now, recent updates, settings, dashboard, start at login, quit), checking thread,
  autostart (`tauri-plugin-autostart`), single instance, the window only hides on close.
- Notifications through `tauri-plugin-notification` **[done]**. On Windows the notification shows the app name correctly
  **only when the app is installed** (the plugin sets the AUMID only outside `target\debug` / `target\release`). Running
  `target\release\git-client.exe` directly shows "PowerShell". This is the plugin's behaviour, not a bug of ours; test
  notification names with the installed build.
- Every notification is also stored under "最近の更新" (desktop notifications are not clickable everywhere); a click
  opens the diff page.
- Commands the screen may call: `get_state`, `set_follow`, `import_follow`, `set_options` (only `open_at_login` is used
  by the screen; `base_url` / `interval_min` remain as optional arguments), `set_license`, `set_store_entitlement`,
  `check_now`, `test_notification`, `open_external`. `get_state` returns the view including `plan` (`free` / `trial` /
  `pro`), `planText`, `freeMaxFollows`, `purchaseUrl`.
- Commands are async or short; the main thread never waits for the network.

### 7.2 Screen (`ui/`)
- **Two screens, switched by tabs** (`ui/index.html`, `showView` in `app.js`):
  - **通知**: recent updates, 分野で選ぶ (§4.4), the sites one by one (folded; filter, "通知の対象のみ"), 今すぐ確認, the schedule text.
  - **設定**: プラン (plan line, license-key field, Stripe buy button; hidden in a store build), ブラウザの☆から取り込む
    (import of a shared ☆ list; kept on every platform, capped by the free plan through `limit_follow`), 表示と起動
    (font size, start at login: saved as soon as it is changed, there is no save button), 動作の確認 (test notification),
    **このアプリについて** (version, OS and CPU, direct or store build from `View.about`; buttons for 使い方 / G醫t について /
    プライバシー that open `<base URL>guide.html`, `about.html`, `privacy.html` in the browser; licence MPL-2.0).
  - **Link targets are provisional (仮)**: `data-page` of the three buttons in `ui/index.html` (`guide.html`, `about.html`,
    `privacy.html` of the dashboard); the owner has not decided the final pages.
  - **Help is not built into the app**: it lives on the G醫t site. Those pages describe the dashboard; a page for the
    app itself would be a change in `g-i-t-app` (ask the owner). A store listing needs the privacy URL anyway.
  - Dates and times on the screen are always Japanese and JST (`fmt` in `app.js`), whatever the device language is.
- `[hidden]` must stay `display: none !important` in `style.css`: `.row` / `.grid` set `display` and would otherwise
  show elements that the script hides (the license row in a store build).
- The screens were looked at on the Android emulator (x86_64) in part; the desktop windows have not been looked at by eye since the tabs were added.
- **Settings that are deliberately not on the screen:** the dashboard URL and the check interval. The schedule comes from
  `status.json`; the base URL is fixed to the default (a different URL can still be set through `state.json` or the
  `set_options` command for development).
- **Font size:** Small 14 px / Normal 16 px (default) / Large 19 px / Huge 23 px, stored in `localStorage`
  (`git-client:size`), applied as `data-size` on `<html>` (`--fs` in `style.css`).
- UI text is Japanese. Keep strings short and factual.

---

## 8. Platform status

| | Windows | macOS | Android | iOS / iPadOS |
| --- | --- | --- | --- | --- |
| Screen, follow list, check, notifications | [done] (built and run) | [partly] (same code, not run) | [partly] (same code) | [partly] (same code) |
| Checking while the app is closed | [done] tray + timer + autostart | [partly] same code | [partly] WorkManager job (Kotlin + JNI + `git_core::background`), see §8b | [todo] no reliable background run; needs push (APNs/FCM) |
| Store billing | [todo] Microsoft Store | [todo] App Store (StoreKit 2) | [todo] Google Play Billing | [todo] StoreKit 2 |
| Signing / distribution | unsigned (SmartScreen warns) | needs notarization | needs signing | needs the Apple Developer Program |

watchOS and tvOS are not Tauri targets.

### 8a. Mobile background checking (decision record; Android is written, see §8b; iOS is **[todo]**)

Decided by the owner:
- **iOS: `BGAppRefreshTask` first, no server.** It is best effort: the OS decides when it runs, and it does not run at all
  when the user force-quits the app, turns "Appのバックグラウンド更新" off, or never opens the app. Tell the user so; never
  promise punctual delivery.
- **Scheduling reuses the core.** The platform part asks the core for the next time (`next_check_ms` /
  `effective_schedule`: free = first run of the day, paid/trial = 3 runs) and submits a `BGAppRefreshTaskRequest` with
  `earliestBeginDate` = run time + `delayMin` (not a fixed 7:45/12:45/16:45: the windows come from `status.json`). When the
  task runs and `status.json` says the run has not finished (`generated` too old), resubmit a request a few minutes later;
  at the end of every task call `setTaskCompleted(success:)` and submit the next request. A task has about 30 s: read
  `status.json`, then only the feeds that changed. Notify with a local notification from inside the task.
  `BGAppRefreshTask` has no "requires network" flag (only `BGProcessingTask` has): try the request and reschedule on failure.
- **Background refresh cannot be switched on by the app** (iOS allows only the user to do it). We rely on the OS default
  (on for a new app when the system setting is on) and **detect and guide**: show the state of
  `UIApplication.backgroundRefreshStatus` (`available` / `denied` / `restricted`), of Low Power Mode
  (`ProcessInfo.isLowPowerModeEnabled`) and of the notification permission; when something is off, say that notifications
  will be late or missing, with a button that opens the app's page in Settings (`UIApplication.openSettingsURLString`).
- **Notification permission:** ask with `requestAuthorization` at the first start, after a one-line explanation on the
  screen (not as a bare system dialog); if it is refused, show the guidance above.
- **Android** is the same idea with WorkManager (one-time jobs, chained; the OS decides when they run): same scheduling
  rule, same "detect and guide" screen (notification permission on Android 13+, battery restrictions: see §8b).
- Code form: Swift/Kotlin is a Tauri mobile plugin that calls the Rust core; do not write a bridge by hand when a crate
  (e.g. UniFFI) or Tauri's plugin mechanism does it (principle 6). The screen shows a "通知の状態" block only on mobile.
- `src-tauri/gen/android` exists; `src-tauri/gen/apple` does not (`tauri ios init`); iOS needs a Mac with Xcode. Do not claim any of this works until it has run on a real device (the simulator does not
  fire background tasks; use the debugger command `_simulateLaunchForTaskWithIdentifier`).

**Policy confirmed by the owner:** iOS starts with `BGAppRefreshTask`; it is booked at the start of the `status.json`
window; lateness is by design (state it in the app and the store text); push is an **optional later addition**, not a
prerequisite for the first iOS release.

Push, if it is added (FCM topics `am-<slug>` for the first run, `pm-<slug>` for later runs, subscribed by the client; limits
enforced on the device only, no user database) needs a change in `g-i-t-app`, an Apple Developer account and a note in
the privacy text (Google receives the device token). Ask the owner before starting.

An idea for phone notifications without a user database: one FCM topic per followed site (`site-<slug>`), the device
subscribes, and the pipeline publishes one message per updated site. Not written; needs a server-side change in
`g-i-t-app` and an owner decision.

### 8b. Android background job **[partly]**

Written and looked at on the emulator (x86_64): a forced job ran with the app in the background and with **no app process**
(after `am crash`; the process was started for the job), looked, saved, and showed the notification, and tapping it opened the
diff page in the browser. **Not tried on a real phone, not tried over days, not tried under Doze.**
- **Core** (`core/src/background.rs`, tested offline): `run(state_file, fetcher, now)` takes the lock of the state file, loads
  the state, looks like the timer thread (`poll_scheduled`), saves, and returns `{notices, next_ms, error}`; `next()` returns
  only the next time. Nothing followed, or no state file yet: no network, nothing written (a job must not create a state and
  start the trial for an app that was never set up), next try in 30 minutes.
- **Lock** (`state::FileLock`, `<state>.lock`, Rust 1.89 `File::lock`): the app's `check()` holds it for a whole look, the job
  holds it for a whole look. The app's other saves use `try_acquire` (a command must not wait for the network) and first
  take over what the job saved (`absorb_disk`: `merge_poll` of the disk state if its `last_check` is newer), so an older
  `seen` is never saved over a newer one (the same update would be announced twice).
- **Rust entry points** (`src-tauri/src/lib.rs`, `mod android_job`, crate `jni`): `Java_jp_git_1app_client_BackgroundCheck_nativeCheck`
  (returns JSON `{"notices":[{"title","body","url"}],"nextMs","error"}`) and `..._nativeNext`; panics are caught. The tap target
  is the diff page of one update, the history page of several, the dashboard for the summary notification.
- **Kotlin** (`src-tauri/gen/android/app/src/main/java/jp/git_app/client/`): `BackgroundCheck` (loads `git_client_lib`, books
  `OneTimeWorkRequest` "giiit-check" with `NetworkType.CONNECTED`, `ExistingWorkPolicy.REPLACE`, delay = next time + 0–45 s,
  at least 1 minute), `CheckWorker` (does nothing but book the next job while the app is on the screen, because the app's own
  thread looks then; otherwise `nativeCheck`, shows the notifications on channel `updates` with a tap to the browser, http(s)
  only, then books the next job; a failed look books a retry in 15 minutes), `MainActivity` (books a job when it is created and
  when it is left). The data folder is `Context.dataDir` (`state.json` lies directly in it).
- **Gradle**: `androidx.work:work-runtime`; **ProGuard/R8** (`proguard-rules.pro`): keep `BackgroundCheck` (JNI),
  the `@JavascriptInterface` methods of `DeviceBridge`, and the constructor of `WorkDatabase_Impl` (without it the **release** build
  died at start: `NoSuchMethodException: androidx.work.impl.WorkDatabase_Impl.<init>`; the debug build is not minified and never showed it).
  **Release build** (`npx tauri android build --apk`, unsigned; all four ABIs `arm64-v8a`, `armeabi-v7a`, `x86`, `x86_64` are in it,
  28.9 MB; the debug APK is 162-334 MB): to try it, `zipalign` + `apksigner` with the debug key, then install. Started on the emulator,
  the forced job ran and booked the next one. A release build cannot be inspected through DevTools or `run-as`, so the JNI look itself
  (with a followed site) was not tried in the minified build.
- **Detect and guide** (`DeviceBridge.kt`, exposed to the page as `window.GiiitAndroid` by `MainActivity.onWebViewCreate`; read
  only): `status()` returns `{notifications, ignoringBatteryOptimizations, backgroundRestricted, standbyBucket}`; 設定 → 動作の確認
  shows 「端末の状態」 only when the bridge exists: a warning when background use is restricted (the job cannot run) or the app
  is in the "rare"/"restricted" standby bucket, and a hint when the app is subject to battery optimization; two buttons open
  the system's pages (the app's settings, the battery optimization list) through `openAppSettings()` / `openBatterySettings()`.
  The state is read again when the user comes back to the app. **The app never asks for the battery exemption itself**
  (`ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS` is restricted by Google Play): the user changes it on the system page. The
  bridge is the one place where the page talks to native code without `invoke`; it only reads the phone's state and opens
  system pages, and the WebView shows only the app's own files (strict CSP). Looked at on the emulator through DevTools; the
  buttons were not pressed there and have not been tried on a real phone.
- **Known limits**: Android decides when a job runs (Doze, battery saver, app standby); a force-stopped app runs nothing until
  it is opened again; the job does not ask for the notification permission (the app does, after the first-start screen) and
  shows nothing without it (the look is still made, the updates are listed in the app); a job started by the system with no
  app process pays for starting the process (slow on the emulator).
- **To test by hand** (emulator): `adb shell cmd jobscheduler run -f -n androidx.work.systemjobscheduler jp.git_app.client <id>`
  with the id from `adb shell dumpsys jobscheduler | grep CheckWorker` (it changes after every run). To make an update look
  new, remove its key from `seen[<slug>]` and its entry from `heads` in `state.json` (`run-as`; bump `lastCheck` by 1 so that
  the running app takes it over). Do not use `am force-stop` (it removes the jobs) or `am kill` (WorkManager takes it for a force stop, books its jobs again and that
  cancels the forced run); to test without an app process use `am crash jp.git_app.client` first. On Git Bash set `MSYS_NO_PATHCONV=1`.

---

## 9. Acceptance checks (what "done" means for the plan feature)

Core, automated (`cargo test -p git_core`):
- A signed license is accepted; a forged, expired, swapped-payload or garbage key is rejected; `exp = 0` never expires.
- A store result counts until it ends, survives 6 days offline, and stops after 8 days without confirmation.
- Free: 5 followed sites → 3 are looked at, 1 run a day; Trial/Paid: all, 3 runs a day.
- Trial: unlocked for `GIT_TRIAL_DAYS`, then free; a clock set back does not extend it; a subscription wins over the trial;
  `0` days = no trial.

By hand on a build (not automated — say so when you did not do it):
- Free plan, 3 sites followed: ticking a 4th shows 「無料プランでは 3 サイトまでです。」 and the box unticks.
- Pasting a valid key under プラン shows 有料プラン; a garbage key shows 「有効にできませんでした: …」.
- The plan line changes between お試し期間中 / 無料プラン / 有料プラン as the state changes.
- Changing the font size changes the whole screen and survives a restart.
- A "最近の更新" entry that was stored with the old address opens the new address.

---

## 10. Open decisions (ask the owner; do not guess)

1. ~~Prices~~ **Decided (owner, 2026-10-15): 1,000 yen per month or 10,000 yen per year, both excluding tax.** Product IDs per store are still open.
   **Channels decided:** first release on **Google Play**; Windows: GitHub + a winget manifest, later Microsoft Store; macOS: GitHub +
   a Homebrew tap, later Mac App Store; Debian: an apt line (repository) on GitHub; iOS: App Store. Store builds (Google Play,
   Microsoft Store, Mac App Store, App Store) are bought through that store's subscription; every other build (winget, Homebrew,
   `.deb`) through Stripe + license key. Not started: store billing code, winget manifest, Homebrew tap, apt repository.
2. Whether the limits stay at 3 sites / 1 look a day, or other features also become paid.
3. What happens at a refund or cancellation beyond "the store result expires" (today: nothing special).
4. How many devices one Stripe license key may be used on (today: unlimited; no way to count without a server).
5. Order of platforms for the stores (Windows first is the working assumption).
6. Whether to add the debug-only unlock (§5.6) and the `store-build` switch (§6).
7. Whether to automate Stripe key delivery (needs a small server / webhook, which breaks principle 1).

**Decided: licence of this repository = MPL-2.0** (`LICENSE`, `license = "MPL-2.0"` in the Cargo.toml files). Reasons:
the app depends on many crates (mostly MIT / Apache-2.0 / BSD), file-level copyleft keeps changes to our files open
without infecting the dependencies, and it can be shipped through stores (Firefox for iOS is MPL-2.0 on the App Store).
GPL/AGPL were rejected: store terms conflict with them and AGPL's network clause means nothing for a client without a
server. Consequences for agents:
- Only add dependencies whose licence is compatible with MPL-2.0 (MIT, Apache-2.0, BSD, ISC, MPL-2.0, Zlib, Unicode;
  **not** GPL/AGPL/LGPL-static without asking). Check with `cargo deny check licenses` or `cargo license`.
- The licence covers the code, not the name "G醫t" / "giiit" or the icon: see `TRADEMARK.md` (draft). A fork or a modified build
  must not use the name or the icon; do not put the name or the icon into a new store listing for anyone else.
- The operator private key and `.env` are never part of the repository, whatever the licence.
- If the owner later accepts outside contributions, ask about a CLA before merging them.

---

## 11. Working rules for agents

- Keep the core free of OS and UI code; put platform code in `src-tauri/` or native modules.
- When you add a state field, give it a default and a test that an old `state.json` still loads.
- Do not change the meaning of an existing JSON key in `state.json` without a `migrate()` step.
- Do not edit `Cargo.lock` by hand; commit it when dependencies change.
- Long builds (`cargo check -p git-client`, `npm run build`) take minutes: run them in the background and wait for the
  result instead of guessing.
- Write tests next to the behaviour (`core/tests/`); a plan or schedule rule without a test is not finished.
- Keep `README.md` in step with the code (paths, commands, platform table, plan table); it was rewritten on 2026-10-14. The
  workflow is `.github/workflows/build.yml`. Fix drift when you touch a section.
- Keep `ChangeLog.md` in step: add a line under "Unreleased" for every change that a user or a developer would notice (what was
  added, changed or fixed, and what is still not done).
- Commit messages: short imperative English or Japanese, one topic per commit. Do not commit `target/`, `node_modules/`,
  keys or `.env`.
