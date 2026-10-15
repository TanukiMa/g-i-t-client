# ChangeLog

All notable changes of the G醫t client (`giiit`). Format after [Keep a Changelog](https://keepachangelog.com/); versions follow
`src-tauri/Cargo.toml`. The version is still `0.1.0`: nothing has been released yet, so everything below is **Unreleased**.
Dates are the days the work was done. Changes of the server side are in `g-i-t-app` and only named here where the client depends
on them.

## [Unreleased] (0.1.0), 2026-10-12 to 2026-10-15

### Added

**Plans, trial and licenses** (2026-10-12)
- `core/src/entitlement.rs`: the plans Free / Trial / Pro. Free follows 3 sites one by one and looks at the first run of the day
  only; Trial and Pro have no limit and look after every run.
- Trial: the first 7 days from the first start; length from `GIT_TRIAL_DAYS` (run time or build time, `0` = no trial, 0–3650).
  `clockMs` keeps the latest time seen, so a clock set back does not extend it.
- License keys `GIT1.<payload>.<signature>` (Ed25519, payload `{v, sub, exp}`) checked on the device against
  `LICENSE_PUBLIC_KEY`; the operator tool `core/examples/license_tool.rs` (`keygen`, `sign`).
- Store subscriptions through the command `set_store_entitlement(source, expiresMs)`, trusted for 7 days without a new
  confirmation. **Only the command exists; no store SDK code is written.**
- `State`: `license`, `store`, `trialStartedMs`, `clockMs`; `plan()`, `tick()`, `effective_follow()`, `effective_schedule()`.
- Shell: commands `set_license`, `set_store_entitlement`; `View.plan`, `planText`, `freeMaxFollows`, `purchaseUrl`;
  `STRIPE_PAYMENT_LINK` (empty: no button); free limit through `limit_follow`.
- Cargo feature `store-build` (`src-tauri`): hides the Stripe button and the license-key field and makes `set_license` refuse.

**Choosing sites by category** (2026-10-13)
- `core/src/category.rs`: the tags of one heading are alternatives, the headings are all required (a faceted rule); picked tags
  of no heading form one more heading; nothing picked matches nothing; `counts()` for the picker.
- `feed.rs`: `Group`, `parse_groups` (the `groups` of `search.json`).
- `State`: `tags`, `exclude`, `groups`, `onboarded`; `effective_follow()` adds the sites of the picked categories (Trial and
  Pro only), so a site that joins a category later is followed automatically.
- Shell commands `set_tags`, `toggle_site`, `finish_setup`; `View.active`, `groups`, `tags`, `ruleCount`, `unlocked`,
  `onboarded`.
- Screen: the first-start question 「どの分野の更新を受け取りますか？」, 「分野で選ぶ」 on the 通知 tab, the long site list folded
  under 「サイトを個別に選ぶ・外す」.

**Notifications and the list of updates** (2026-10-13)
- More than 5 notifications in one check become one summary notification (`MAX_NOTICES`, `deliver()`).
- A site seen for the first time still gets no notification, but its newest 3 entries are listed under 「最近の更新」, for the
  first 5 sites of a check only.
- 「最近の更新」 is sorted by the time of the update, newest first (`Recent.updatedMs`, `Notice.updated_ms`,
  `State::sort_recent`); the screen shows only the sites that are watched now.
- Updates that only say "no substantive change" (`NO_CHANGE_PREFIX`) are remembered but neither announced nor listed.
- The notification permission is asked after the first-start screen (`plugin:notification|request_permission`); 設定 shows the
  state and a button to ask again, and says where to switch it on after a refusal.

**Screen** (2026-10-12 to 2026-10-13)
- Font size Small / Normal / Large / Huge (14 / 16 / 19 / 23 px, kept in `localStorage`).
- Two tabs, 通知 and 設定; 設定: プラン, ブラウザの☆から取り込む, 表示と起動 (start at login is saved when it is changed),
  動作の確認, このアプリについて (version, OS and CPU, direct or store build; buttons to `guide.html`, `about.html`,
  `privacy.html` of the dashboard, **provisional targets**; licence).
- The top line shows 「サイトの最終実行」 (`generated` of `status.json`) next to the last check and the next one.

**Packaging and platforms** (2026-10-13)
- Product, package and binary renamed to ASCII `giiit` (`productName`, `mainBinaryName`); the screen still says 「G醫t」;
  `identifier` is unchanged (`jp.git-app.client`), so `state.json` stays where it was.
- `LICENSE` (MPL-2.0) and `license = "MPL-2.0"` in the Cargo.toml files.
- Linux: `.deb` through `tauri build --bundles deb`; `debian/` (quilt source package for an own apt repository, vendored crates,
  `FIXME` maintainer fields) and `scripts/make-debian-orig.sh`.
- Android: `src-tauri/gen/android` (`tauri android init`); the manifest does not require a touch screen (ChromeOS, ChromeBox);
  debug APK for x86_64 built on Windows and run on the emulator.
- `TRADEMARK.md` (draft: names and icon are not covered by the licence), this `ChangeLog.md`.

**Android background job** (2026-10-15)
- `core/src/background.rs` (`run`, `next`): one look without the window or the timer thread, for a phone's background job.
- `state::FileLock` (`<state>.lock`): the app and the job never look and save at the same time; the app's saves take over what
  the job saved first (`absorb_disk`), so the same update is not announced twice.
- Rust entry points `nativeCheck` / `nativeNext` (crate `jni`, Android only) and Kotlin `BackgroundCheck`, `CheckWorker`
  (WorkManager, chained one-time jobs after the runs of the pipeline, notifications on channel `updates`, tap opens the page
  in the browser), `MainActivity` books the job; Gradle `androidx.work:work-runtime`, a ProGuard keep rule.

- Android: 設定 → 動作の確認 shows 「端末の状態」 (notification permission, battery optimization, background restriction, standby
  bucket) with buttons that open the system's settings pages (`DeviceBridge.kt`, `window.GiiitAndroid`); read only.

**Tests**: `core/tests/entitlement.rs` (4), `core/tests/category.rs` (12), `core/tests/background.rs` (5); `scheduled.rs` and `core.rs` updated. `cargo test -p git_core`:
5 + 12 + 14 + 4 + 13 = 48, offline.

**Documentation**: `AGENTS.md` for this repository (principles, targets and distribution, behaviour, plans, store rules, mobile
background checking, open decisions, working rules); `README.md` rewritten to the current state.

### Changed

- The dashboard URL and the check interval are no longer on the screen (the schedule comes from `status.json`; the interval is
  only the fallback for a site without it).
- `State::migrate` rewrites the links of stored 「最近の更新」 entries that still start with the old address
  (`https://tanukima.github.io/g-i-t-data/`) to the current base URL.
- `merge_poll` copies `groups` as well; `forget_site_data` clears them.
- The check thread and the tray use the sites that are really looked at (`effective_follow`), not the follow list alone.
- On the first start the settings window opens only when nothing is followed, no category is picked and the screen was never
  answered.
- Dates and times on the screen are Japanese and JST whatever the device language is.
- Link colour and scrollbars follow the light or dark scheme (`color-scheme`, `--link`).
- Linux: the app sets `WEBKIT_DISABLE_DMABUF_RENDERER=1` itself (a value you set is kept).
- Schedule text before the first check no longer claims that the site publishes no schedule.

### Fixed

- Links under 「最近の更新」 that still pointed at `tanukima.github.io/g-i-t-data` (see `migrate` above).
- `[hidden]` did not hide elements that had `display: flex` (the license row of a store build); now `display: none !important`.
- The list of sites was empty on the first start until the first check; the screen asks again a few times.
- A just-followed site showed nothing under 「最近の更新」 (its entries were remembered silently, and not listed).
- The order of 「最近の更新」 followed the order of discovery, not the time of the update.
- The first-start screen closed as soon as one category was picked.
- The category headings were never loaded after a fresh start: `get_state` copied only the site names, and later checks did not
  read `search.json` again. Now they are copied, and a list without headings is read again after an hour.
- The picker of the first-start screen and the 通知 tab showed the same tags under one heading "その他" when the dashboard did not
  publish `groups` yet (a consequence of the server side, now published).
- An empty window on Linux under WSLg (WebKitGTK's DMABUF renderer).
- `debian/rules`: `dh_clean` deleted the `Cargo.toml.orig` files of the vendored crates (`dh_clean -X Cargo.toml.orig`);
  `make-debian-orig.sh` read the version with a trailing carriage return from a CRLF `Cargo.toml`.
- Debian package name `g醫t` (invalid) — now `giiit`.

### Security and decisions

- The operator's private key lives outside the repository (`~/.g-i-t-license/private.key`) and is never printed or committed.
  An accidental print during the first `keygen` run was discarded and the key regenerated.
- **No build gate.** A passphrase / hash check at build time was considered and rejected as protection: the check is in the same
  public source and the limits are plain code. See `AGENTS.md` §5.5; the name and the icon are protected by `TRADEMARK.md`,
  not by the build.
- MPL-2.0 chosen (not GPL/AGPL: store terms; AGPL's network clause means nothing for a client without a server).

### Fixed (release build)

- Android release (R8) build crashed at start (`WorkDatabase_Impl` constructor removed); keep rules added for it and for `DeviceBridge`.

### Related changes in other repositories

- `g-i-t-app` (commit `3deaa04`): `search.json` gets `groups` (the `tag_groups` of `config.yaml`), so that the client can show
  the headings. It is additive; older clients ignore it. Needs a run or `remake-dashboard` to reach the site.

### Not done (do not describe as working)

- Android release build: unsigned APK with all four ABIs (28.9 MB) builds, starts and runs the job on the emulator after the keep
  rules were added; a real signature, the Play bundle (`.aab`) and the JNI look with a followed site in the minified build were not tried.
- Store billing for Microsoft Store, App Store, Google Play; the store's own purchase button.
- Android: the background job was tried on the emulator only, with the app in the background and with no app process (not on a
  real phone, not over days, not under Doze); the
  buttons of the device state were not pressed there. iOS: `BGAppRefreshTask`; push (APNs/FCM). macOS and iOS were not built.
- The `.deb` packages were not installed; notifications, tray and autostart were not tried on Linux. ChromeOS was not tried.
- `build.yml` was not run on GitHub's runners.
- Final targets of the help links under このアプリについて; a help page for the app itself.
- Maintainer, copyright and trademark owner fields (`FIXME`) in `debian/` and `TRADEMARK.md`.
