# G醫t client (Tauri 2 + Rust)

Shows an OS notification when a followed site of the [G醫t dashboard](https://giiit.goudge.org/) is updated. The follow list
lives **on the device only**; the client reads the public files of the dashboard (`feeds/status.json`, `feeds/<slug>.xml`,
`search.json`). No server, no account. The product, package and binary are called `giiit`; the screen says 「G醫t」.

The specification and the working rules for AI coding agents are in [AGENTS.md](AGENTS.md); this file is the how-to.

```text
g-i-t-client/
├── core/        git_core (Rust): feed parsing, the local state, category rules, one check of all followed sites.
│                No UI, no OS code, so the same code runs on every platform. `cargo test -p git_core` (offline).
├── src-tauri/   the Tauri 2 shell: tray (desktop), timer, notifications, start at login (desktop), commands for the
│                screen; gen/android = the Android project.
├── ui/          the screen (plain HTML/JS, no bundler): 通知 tab (recent updates, categories, sites) and 設定 tab
│                (plan, import of the ☆ list, font size, notification state, about).
├── debian/      source package for a self-hosted apt repository (see "Linux" below)
├── scripts/     make-debian-orig.sh
├── icon.png     source of the app icons (`npm run icons` -> src-tauri/icons, Android and iOS sets included)
├── LICENSE      MPL-2.0
├── TRADEMARK.md the name and logo policy (the licence does not cover them)
├── ChangeLog.md what changed
└── AGENTS.md    the specification
```

## Use (Windows / macOS / Linux)

```powershell
npm install                 # the Tauri CLI
npm run dev                 # runs the app (first build: several minutes)
npm run build               # installer / bundle in target/release/bundle
cargo test -p git_core      # the core's tests (offline)
cargo check -p git-client   # the shell compiles
```

Needs Rust (stable). Windows: the Visual Studio C++ build tools and WebView2 (part of Windows 11). Linux: `libwebkit2gtk-4.1-dev`,
`libgtk-3-dev`, `libayatana-appindicator3-dev`, `librsvg2-dev`, `patchelf`.

**First start.** The app asks which categories you want ("どの分野の更新を受け取りますか？": 種別, 組織, 内容, 領域). Every picked
category is an alternative, whatever its heading (学会 + がん = the learned societies and the cancer sites together). Sites that join a picked category later are followed without any action. You can also pick single sites (the long
list under 「サイトを個別に選ぶ・外す」) or paste the share URL of the ☆ list (設定 → ブラウザの☆から取り込む; dashboard →
「🔗 フォロー設定を共有」). The app then sits in the tray / menu bar and looks for news **after the runs of the pipeline** (a short
window after each of 7:30, 12:30, 16:30 JST, read from `feeds/status.json`), not every few minutes; a site without that file is
checked every 15 minutes.

- The first check of a site only remembers its current entries (no notification); its newest entries are listed under
  **最近の更新**, for the first few sites only.
- More than 3 new entries of one site at once → one notification; more than 5 notifications in one check (a whole category) →
  one summary notification.
- Updates that only say "no substantive change" (markup adjustments) are remembered but neither announced nor listed.
- Desktop notifications cannot be clicked on every OS, so each one is also listed under **最近の更新** (window and tray menu):
  a click opens the diff page. The list is sorted by the time of the update, newest first, and shows only the sites that are
  watched now.
- The state is `state.json` in the app data folder (`GIT_CLIENT_DATA=<dir>` to put it elsewhere). Links are built from the
  configured dashboard URL, not from the host written inside the feed.
- Help is not built in: 設定 → このアプリについて shows the version and opens the pages of the G醫t site (provisional targets).

## Platforms

| | Windows / macOS | Linux | Android | iOS / iPadOS |
|---|---|---|---|---|
| Screen, follow list, feed check, notification text | the same code | the same code | the same code | the same code |
| Checking while the app is closed | tray + timer + start at login: **works** | the same code; GNOME shows no tray icon without an extension (starting the app again shows the window) | a WorkManager job looks after the runs (Kotlin + Rust): written, tried on the emulator; Android decides when it runs | `BGAppRefreshTask`, best effort — **not written yet**; push (APNs/FCM) would be an addition |
| Built and looked at | Windows: yes. macOS: not run | `.deb` and a debug binary built in WSL Debian 13; window looked at under WSLg | debug APK (x86_64) built, run on the emulator, permission dialog and a notification shown | not started (needs a Mac) |
| Store / signing | Windows: SmartScreen warning unless signed; macOS: notarization | `.deb` for download; official Debian archive undecided | Google Play (one-time fee), or an APK | Apple Developer Program (yearly fee) |

**ChromeOS**: use the Android build; it must contain the **x86_64** library (Chromebooks and ChromeBoxes with Intel/AMD CPUs), not
only ARM. `tauri android build` builds all four ABIs by default; do not drop `x86_64`.

watchOS and tvOS are **not** Tauri targets (no web view there).

### Android (built on Windows)

Needs Android Studio (SDK, **NDK**, cmdline-tools), Windows **Developer Mode** (symbolic links), the four Rust targets
(`rustup target add aarch64-linux-android armv7-linux-androideabi i686-linux-android x86_64-linux-android`) and the variables
`ANDROID_HOME`, `NDK_HOME` (the folder under `ndk/`), `JAVA_HOME` (Android Studio's `jbr`).

```powershell
npx tauri android build --apk --debug --target x86_64   # first run ~15 min; leave out --target for all four ABIs
```

### Linux

Build in the Linux file system (not on `/mnt/d`):

```bash
npx tauri build --bundles deb       # giiit_<version>_amd64.deb in target/release/bundle/deb
```

For an own apt repository there is a quilt source package built offline from `cargo vendor` crates:
`scripts/make-debian-orig.sh ..`, unpack the tarball, copy `debian/` into it, `dpkg-buildpackage -us -uc -b`. The maintainer
fields of `debian/control` and `debian/changelog` are `FIXME`, and `debian/copyright` must list the vendored crates before an
upload. The official Debian archive would need every dependency packaged separately: not planned.

Under WSLg the window stays empty with WebKitGTK's DMABUF renderer; the app therefore sets `WEBKIT_DISABLE_DMABUF_RENDERER=1`
itself on Linux (a value you set is kept).

## CI

`.github/workflows/build.yml` (manual) builds unsigned bundles on GitHub's runners. All jobs (core tests, Windows, macOS, Linux `.deb`, Android with the four ABIs, iOS simulator) passed on 2026-10-16; the
results were not installed or run on those systems.

## Free and paid use

| | Free | Trial (first 7 days) | Paid |
| --- | --- | --- | --- |
| Followed sites | 3, one by one | no limit, one by one or by category | no limit, one by one or by category |
| Looking for news | after the first run of the day (a daily digest) | after every run (3 a day) | after every run (3 a day) |

**Trial**: from the first start every function is unlocked for 7 days (`trialStartedMs` in the state file), then the
free plan applies; the picked categories stay in the state but are not used on the free plan. Change the length with the
variable `GIT_TRIAL_DAYS` (at run time, or at build time: `GIT_TRIAL_DAYS=14 npm run build`; `0` = no trial). A clock set back
does not extend it (`clockMs`). Deleting the state file starts a new trial, which is accepted: the follow list lives in the same
file.

No server and no account (`core/src/entitlement.rs`). Two ways to prove a purchase, both checked on the device:

- **Stripe** (direct build): after a payment the operator signs a license key and sends it to the buyer
  (`cargo run -p git_core --example license_tool -- sign ~/.g-i-t-license/private.key <buyer> [days]`).
  The app verifies the Ed25519 signature against `LICENSE_PUBLIC_KEY`. The private key (`~/.g-i-t-license/private.key`)
  must never enter a repository; back it up, because a lost key means the public key in the app has to be replaced.
  `STRIPE_PAYMENT_LINK` in `src-tauri/src/lib.rs` is the Payment Link of the "buy" button (empty = no button).
- **Microsoft Store / App Store / Google Play**: the platform part asks the store's own API (Windows.Services.Store,
  StoreKit 2, Play Billing) and calls the command `set_store_entitlement(source, expiresMs)`; it is trusted for 7 days
  without a new confirmation. **This platform code is not written yet**, and each store needs its own developer account,
  product (subscription) and review. A store build (`--features store-build`) hides the Stripe button and the license-key
  field; do not submit any build to a store before the store's own purchase exists.

Limits of this design: the state file is plain JSON, so someone who edits it can unlock the paid plan; a license key can
be shared. That is the price of "no server".

## Licence

Mozilla Public License 2.0 (see `LICENSE`). It covers the code, not the name "G醫t" / "giiit" or the icon (see `TRADEMARK.md`:
a fork or modified build needs its own name and icon). The paid plan can be changed by anyone who builds the app from source;
that is accepted and not protected by a build check (AGENTS.md §5.5).
