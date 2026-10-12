# G醫t client (Tauri 2 + Rust)

Shows an OS notification when a **followed** site is updated. The follow list lives **on the device only**; the client reads the
public Atom feeds (`feeds/<slug>.xml`) and `search.json` of the dashboard. No server, no account.

```text
client/
├── core/        git_core (Rust): feed parsing, the local state, one check of all followed sites. No UI, no OS code,
│                so the same code runs on Windows, macOS, Android and iOS. `cargo test -p git_core` (12 tests, offline).
├── src-tauri/   the Tauri 2 shell: tray (desktop), timer, notifications, start at login (desktop), commands for the screen.
├── ui/          the screen (plain HTML/JS, no bundler): recent updates, site picker, import of the ☆ list, settings.
└── icon.png     source of the app icons (`npm run icons` -> src-tauri/icons, Android and iOS sets included)
```

## Use (Windows / macOS)

```powershell
cd client
npm install                 # the Tauri CLI
npm run dev                 # runs the app (first build: several minutes)
npm run build               # installer / bundle in target/release/bundle
cargo test -p git_core      # the core's tests
```
Needs Rust (stable), and on Windows the Visual Studio C++ build tools and WebView2 (already part of Windows 11).
On first start the settings window opens: choose the sites (or paste the share URL of the ☆ list: dashboard →
「🔗 フォロー設定を共有」). The app then sits in the tray / menu bar and looks for news **after the runs of the pipeline** (a short window after each of 7:30, 12:30, 16:30 JST, read from `feeds/status.json`), not every few minutes; a site without that file is checked every 15 minutes (5–240).

- The first check of a site only remembers its current entries: no flood of old updates.
- More than 3 new entries of one site at once → one summary notification.
- Desktop notifications cannot be clicked on every OS, so each one is also listed under **最近の更新** (window and tray menu):
  a click opens the diff page.
- The state is `state.json` in the app data folder (`GIT_CLIENT_DATA=<dir>` to put it elsewhere). Links are built from the
  configured dashboard URL, not from the host written inside the feed.

## One code, four platforms: what is shared and what is not

| | Windows / macOS | Android | iOS / iPadOS |
|---|---|---|---|
| Screen, follow list, feed check, notification text | the same code | the same code | the same code |
| Checking while the app is closed | tray + timer + start at login: **works** | the OS limits it: a periodic job (WorkManager, ≥ 15 min) or a foreground service is needed — **native Kotlin code, not written yet** | no reliable background run; **push (APNs/FCM) is needed** — not written yet |
| Store / signing | Windows: SmartScreen warning unless signed; macOS: notarization (Apple Developer Program) for distribution | Google Play (one-time fee), or an APK | Apple Developer Program (yearly fee), TestFlight |

watchOS and tvOS are **not** Tauri targets (no web view there): a Watch / TV app would be native SwiftUI, sharing only `git_core`
through a Swift binding.

An idea for reliable phone notifications without a user database: the device subscribes to one FCM topic per followed site
(`site-<slug>`), and the pipeline publishes one message per updated site. The server never learns who follows what.

## CI

`.github/workflows/client.yml` (manual) builds unsigned bundles on GitHub's runners. **Untested**: the Windows build and the core
tests were run locally; the macOS, Android and iOS-simulator jobs were written from the Tauri documentation and have not run yet.

## Free and paid use

| | Free | Paid |
| --- | --- | --- |
| Followed sites | 3 | no limit |
| Looking for news | after the first run of the day (a daily digest) | after every run (3 a day) |

**Trial**: from the first start every function is unlocked for 7 days (`trialStartedMs` in the state file), then the
free plan applies. Change the length with the variable `GIT_TRIAL_DAYS` (at run time, or at build time:
`GIT_TRIAL_DAYS=14 npm run build`; `0` = no trial). A clock set back does not extend it (`clockMs`). Deleting the state
file starts a new trial, which is accepted: the follow list lives in the same file.

No server and no account (`core/src/entitlement.rs`). Two ways to prove a purchase, both checked on the device:

- **Stripe**: after a payment the operator signs a license key and sends it to the buyer
  (`cargo run -p git_core --example license_tool -- sign ~/.g-i-t-license/private.key <buyer> [days]`).
  The app verifies the Ed25519 signature against `LICENSE_PUBLIC_KEY`. The private key (`~/.g-i-t-license/private.key`)
  must never enter a repository; back it up, because a lost key means the public key in the app has to be replaced.
  `STRIPE_PAYMENT_LINK` in `src-tauri/src/lib.rs` is the Payment Link of the "buy" button (empty = no button).
- **Microsoft Store / App Store / Google Play**: the platform part asks the store's own API (Windows.Services.Store,
  StoreKit 2, Play Billing) and calls the command `set_store_entitlement(source, expiresMs)`; it is trusted for 7 days
  without a new confirmation. **This platform code is not written yet**, and each store needs its own developer account,
  product (subscription) and review.

Limits of this design: the state file is plain JSON, so someone who edits it can unlock the paid plan; a license key can
be shared. That is the price of "no server".
