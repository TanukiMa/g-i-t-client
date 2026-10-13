//! One check of all followed sites. No globals and no OS code: the fetcher and the notification callback are passed in.

use crate::feed::{parse_atom, parse_groups, parse_sites, parse_status, Entry};
use crate::schedule::Schedule;
use crate::state::{Recent, State, SEEN_LIMIT};
use chrono::{DateTime, FixedOffset};
use std::time::Duration;

/// More new entries than this at once -> one summary notification instead of many.
pub const BURST: usize = 3;
/// More notifications than this in ONE check (several sites) -> one summary notification.
pub const MAX_NOTICES: usize = 5;
/// Sites seen for the first time whose newest entries are listed under "最近の更新" in one check: with a whole category
/// followed at once, listing every site would only bury the list.
const FIRST_LOOK_SITES: usize = 5;
const SITES_MAX_AGE_MS: i64 = 7 * 24 * 3600 * 1000;
const GROUPS_RETRY_MS: i64 = 3600 * 1000;

pub struct FetchResult {
    pub status: u16,
    pub text: String,
    pub etag: String,
}

pub trait Fetcher {
    fn get(&self, url: &str, etag: Option<&str>) -> Result<FetchResult, String>;
}

/// What the platform shows. `diff_url` / `history_url` are built from the configured base URL, not from the feed's host.
#[derive(Debug, Clone)]
pub struct Notice {
    pub slug: String,
    pub title: String,
    pub body: String,
    pub count: usize,
    pub diff_url: String,
    pub history_url: String,
    /// Unix milliseconds of the update (the newest one, for a summary).
    pub updated_ms: i64,
}

#[derive(Debug, Default)]
pub struct PollResult {
    pub checked: usize,
    pub notified: usize,
    pub errors: Vec<(String, String)>,
    pub first_seen: Vec<String>,
    /// How many of those had their newest entries listed (limited by FIRST_LOOK_SITES).
    pub first_listed: usize,
    /// True when status.json was read (false: every feed was read, the old way).
    pub via_status: bool,
    /// Followed sites whose newest update had changed (their feeds were read).
    pub status_changed: usize,
}

/// The real network: ureq, a time limit, ETag.
pub struct HttpFetcher {
    agent: ureq::Agent,
}

impl HttpFetcher {
    pub fn new() -> Self {
        HttpFetcher { agent: ureq::AgentBuilder::new().timeout(Duration::from_secs(20)).user_agent("g-i-t-client").build() }
    }
}

impl Default for HttpFetcher {
    fn default() -> Self {
        Self::new()
    }
}

impl Fetcher for HttpFetcher {
    fn get(&self, url: &str, etag: Option<&str>) -> Result<FetchResult, String> {
        let mut req = self.agent.get(url);
        if let Some(e) = etag {
            req = req.set("If-None-Match", e);
        }
        match req.call() {
            Ok(res) => {
                let etag = res.header("etag").unwrap_or("").to_string();
                let status = res.status();
                Ok(FetchResult { status, text: res.into_string().map_err(|e| e.to_string())?, etag })
            }
            Err(ureq::Error::Status(code, res)) => Ok(FetchResult { status: code, text: String::new(), etag: res.header("etag").unwrap_or("").to_string() }),
            Err(e) => Err(e.to_string()),
        }
    }
}

/// "2026-10-07T03:30:00Z" -> "2026-10-07 12:30 JST"
pub fn jst(iso: &str) -> String {
    match DateTime::parse_from_rfc3339(iso) {
        Ok(d) => d.with_timezone(&FixedOffset::east_opt(9 * 3600).unwrap()).format("%Y-%m-%d %H:%M JST").to_string(),
        Err(_) => String::new(),
    }
}

fn iso_ms(iso: &str) -> i64 {
    DateTime::parse_from_rfc3339(iso).map(|d| d.timestamp_millis()).unwrap_or(0)
}

fn diff_url(base: &str, e: &Entry) -> String {
    format!("{base}sites/{}/diff_{}.html", e.slug, &e.hash[..7])
}

fn history_url(base: &str, slug: &str) -> String {
    format!("{base}sites/{slug}/history.html")
}

/// The site names (search.json) are refreshed when missing or older than a week, or when a site is not in them
/// (`unknown`: a followed site, or a site that status.json lists, that the names do not contain yet).
pub fn refresh_sites(state: &mut State, f: &dyn Fetcher, now_ms: i64) -> Result<bool, String> {
    refresh_sites_if(state, f, now_ms, false)
}

fn refresh_sites_if(state: &mut State, f: &dyn Fetcher, now_ms: i64, unknown: bool) -> Result<bool, String> {
    let age = now_ms - state.sites_at;
    // A list without category headings is read again after an hour: the site may have started to publish them (a state
    // from before this field, or an older dashboard). One 250 KB file an hour, and only while the app is looking anyway.
    let groups_missing = state.groups.is_empty() && age >= GROUPS_RETRY_MS;
    if !unknown && !state.sites.is_empty() && age < SITES_MAX_AGE_MS && !groups_missing {
        return Ok(false);
    }
    let r = f.get(&format!("{}search.json", state.base_url), None)?;
    if r.status != 200 {
        return Err(format!("search.json: HTTP {}", r.status));
    }
    state.sites = parse_sites(&r.text)?;
    state.groups = parse_groups(&r.text);
    state.sites_at = now_ms;
    Ok(true)
}

/// The fixed text the pipeline stores when a page changed only in its markup ("内容に実質的な変更はありません（表示の調整のみ）。"):
/// such an update is not news, so it is remembered but neither announced nor listed.
pub const NO_CHANGE_PREFIX: &str = "内容に実質的な変更はありません";

/// True for an entry that carries only that fixed text.
pub fn is_no_change(name: &str, e: &Entry) -> bool {
    headline_of(name, e).trim_start().starts_with(NO_CHANGE_PREFIX)
}

/// How many of the newest entries of a site seen for the first time are listed (silently) under "最近の更新".
const FIRST_LOOK_RECENT: usize = 3;

/// The one-line text of an entry: its title without the site name, or its body text when nothing is left.
fn headline_of(name: &str, e: &Entry) -> String {
    let headline = e.title.strip_prefix(name).map(|t| t.trim_start_matches([':', '：', ' ']).to_string()).unwrap_or_else(|| e.title.clone());
    if headline.is_empty() { e.text.clone() } else { headline }
}

/// What to do with the entries of one site's feed: remember them (first time) or announce the new ones.
///  - a site seen for the first time is only remembered (no flood of old updates; its newest entries are listed, silently),
///  - ids already known are never announced again,
///  - more than BURST new ones at once -> one summary notification.
fn handle_entries(state: &mut State, slug: &str, entries: Vec<Entry>, out: &mut Vec<Notice>, now_ms: i64, result: &mut PollResult) {
    let name = state.sites.iter().find(|s| s.slug == slug).map(|s| s.name.clone()).unwrap_or_else(|| slug.to_string());
    let Some(seen) = state.seen.get(slug).cloned() else {
        state.seen.insert(slug.to_string(), entries.iter().take(SEEN_LIMIT).map(|e| e.key()).collect());
        result.first_seen.push(slug.to_string());
        // No notification, but the newest entries are listed under "最近の更新": otherwise a site that was just
        // followed would show nothing although the dashboard shows its latest updates.
        if result.first_listed >= FIRST_LOOK_SITES || entries.iter().all(|e| is_no_change(&name, e)) {
            return;
        }
        result.first_listed += 1;
        let base = state.base_url.clone();
        for e in entries.iter().filter(|e| !is_no_change(&name, e)).take(FIRST_LOOK_RECENT).collect::<Vec<_>>().into_iter().rev() {
            state.recent.insert(0, Recent {
                slug: slug.to_string(),
                name: name.clone(),
                time: jst(&e.updated),
                text: headline_of(&name, e),
                count: 1,
                diff_url: diff_url(&base, e),
                history_url: history_url(&base, slug),
                at: now_ms,
                updated_ms: iso_ms(&e.updated),
            });
        }
        state.sort_recent();
        return;
    };
    let all_fresh: Vec<&Entry> = entries.iter().filter(|e| !seen.contains(&e.key())).rev().collect(); // oldest first
    if all_fresh.is_empty() {
        return;
    }
    let fresh: Vec<&Entry> = all_fresh.iter().copied().filter(|e| !is_no_change(&name, e)).collect(); // what is news
    let base = state.base_url.clone();
    let mut notices: Vec<Notice> = Vec::new();
    if fresh.len() > BURST {
        let last = fresh[fresh.len() - 1];
        notices.push(Notice {
            slug: slug.to_string(),
            title: name.clone(),
            body: format!("{} 件の更新を検知しました（最新: {}）", fresh.len(), jst(&last.updated)),
            count: fresh.len(),
            diff_url: diff_url(&base, last),
            history_url: history_url(&base, slug),
            updated_ms: iso_ms(&last.updated),
        });
    } else {
        for e in &fresh {
            let text = headline_of(&name, e);
            notices.push(Notice {
                slug: slug.to_string(),
                title: name.clone(),
                body: format!("{}\n{}", jst(&e.updated), text),
                count: 1,
                diff_url: diff_url(&base, e),
                history_url: history_url(&base, slug),
                updated_ms: iso_ms(&e.updated),
            });
        }
    }
    for n in &notices {
        out.push(n.clone());
        state.recent.insert(
            0,
            Recent {
                slug: n.slug.clone(),
                name: n.title.clone(),
                time: n.body.lines().next().unwrap_or("").to_string(),
                text: n.body.lines().skip(1).collect::<Vec<_>>().join(" "),
                count: n.count,
                diff_url: n.diff_url.clone(),
                history_url: n.history_url.clone(),
                at: now_ms,
                updated_ms: n.updated_ms,
            },
        );
    }
    state.sort_recent();
    let mut all = seen;
    all.extend(all_fresh.iter().map(|e| e.key())); // the quiet ones too: they must not come back as news later
    let skip = all.len().saturating_sub(SEEN_LIMIT);
    state.seen.insert(slug.to_string(), all.into_iter().skip(skip).collect());
}

/// Hands the notifications of one check to the platform. Up to `MAX_NOTICES` are shown one by one; more than that (a whole
/// category was followed) become ONE notification that says how many updates there are and for which sites.
fn deliver(base: &str, notices: Vec<Notice>, notify: &mut dyn FnMut(&Notice), result: &mut PollResult) {
    if notices.len() > MAX_NOTICES {
        let total: usize = notices.iter().map(|n| n.count).sum();
        let mut names: Vec<&str> = vec![];
        for n in &notices {
            if !names.contains(&n.title.as_str()) {
                names.push(&n.title);
            }
        }
        let shown = names.iter().take(3).copied().collect::<Vec<_>>().join("、");
        let more = if names.len() > 3 { " ほか" } else { "" };
        notify(&Notice {
            slug: String::new(),
            title: "G醫t".into(),
            body: format!("{total} 件の更新を検知しました（{shown}{more}）"),
            count: total,
            diff_url: base.to_string(),
            history_url: base.to_string(),
            updated_ms: notices.iter().map(|n| n.updated_ms).max().unwrap_or(0),
        });
        result.notified += 1;
    } else {
        for n in &notices {
            notify(n);
            result.notified += 1;
        }
    }
}

fn finish(state: &mut State, result: &PollResult, now_ms: i64) {
    state.last_check = now_ms;
    state.tick(now_ms);
    state.last_error = result.errors.iter().map(|(s, m)| format!("{s}: {m}")).collect::<Vec<_>>().join(" / ").chars().take(300).collect();
}

/// The old way: every followed site's feed, one request each (a 304 costs almost nothing). Used when the site does not
/// publish feeds/status.json. One failing site does not stop the others. `state` is changed in place; the caller saves it.
pub fn poll_once(state: &mut State, f: &dyn Fetcher, notify: &mut dyn FnMut(&Notice), now_ms: i64) -> PollResult {
    let mut result = PollResult::default();
    if let Err(e) = refresh_sites(state, f, now_ms) {
        result.errors.push(("(sites)".into(), e));
    }
    let follow = state.effective_follow(now_ms);
    let mut notices: Vec<Notice> = Vec::new();
    for slug in &follow {
        let r = match f.get(&format!("{}feeds/{slug}.xml", state.base_url), state.etags.get(slug).map(String::as_str)) {
            Ok(r) => r,
            Err(e) => {
                result.errors.push((slug.clone(), e));
                continue;
            }
        };
        result.checked += 1;
        match r.status {
            304 => continue,
            404 => {
                result.errors.push((slug.clone(), "フィードがありません（サイトが、まだありません）".into()));
                continue;
            }
            200 => {}
            code => {
                result.errors.push((slug.clone(), format!("HTTP {code}")));
                continue;
            }
        }
        let entries: Vec<Entry> = match parse_atom(&r.text) {
            Ok(v) => v.into_iter().filter(|e| &e.slug == slug).collect(),
            Err(e) => {
                result.errors.push((slug.clone(), e));
                continue;
            }
        };
        if !r.etag.is_empty() {
            state.etags.insert(slug.clone(), r.etag.clone());
        }
        handle_entries(state, slug, entries, &mut notices, now_ms, &mut result);
    }
    let base = state.base_url.clone();
    deliver(&base, notices, notify, &mut result);
    finish(state, &result, now_ms);
    result
}

/// The way after the pipeline has been told to publish feeds/status.json: ONE small file says which sites have a new
/// newest update, and only the feeds of followed sites that changed are read (at an address with `?h=<hash>`, so that no
/// cache can hand out the old feed). When the site has no status.json (404) it falls back to `poll_once`.
/// A network error ends the check without touching anything (the next look is made by the schedule).
pub fn poll_scheduled(state: &mut State, f: &dyn Fetcher, notify: &mut dyn FnMut(&Notice), now_ms: i64) -> PollResult {
    let mut result = PollResult::default();
    let url = format!("{}feeds/status.json", state.base_url);
    let etag = (!state.status_etag.is_empty() && state.status_mode).then(|| state.status_etag.clone());
    let r = match f.get(&url, etag.as_deref()) {
        Ok(r) => r,
        Err(e) => {
            result.errors.push(("(status)".into(), e));
            finish_error_only(state, &result);
            return result;
        }
    };
    result.checked += 1;
    match r.status {
        200 => match parse_status(&r.text) {
            Ok(s) => {
                state.schedule = Schedule { runs: s.runs, delay_min: s.delay_min, window_min: s.window_min, offset_min: s.offset_min, ..state.schedule.clone() };
                state.generated_ms = s.generated_ms;
                state.status_sites = s.sites;
                state.status_etag = r.etag;
                state.status_mode = true;
            }
            Err(e) => {
                result.errors.push(("(status)".into(), e));
                finish_error_only(state, &result);
                return result;
            }
        },
        304 if state.status_mode => {}
        404 => {
            state.status_mode = false; // an older site: look at every feed, on the interval
            return poll_once(state, f, notify, now_ms);
        }
        code => {
            result.errors.push(("(status)".into(), format!("HTTP {code}")));
            finish_error_only(state, &result);
            return result;
        }
    }
    result.via_status = true;

    // Names of the sites: also when a site appeared that they do not know yet.
    let unknown = state.status_sites.keys().any(|slug| !state.sites.iter().any(|s| &s.slug == slug));
    if let Err(e) = refresh_sites_if(state, f, now_ms, unknown) {
        result.errors.push(("(sites)".into(), e));
    }

    let follow = state.effective_follow(now_ms);
    let mut notices: Vec<Notice> = Vec::new();
    for slug in &follow {
        let Some(head) = state.status_sites.get(slug).cloned() else {
            // no real update yet: remember that the site is known, so that its first update is announced
            if !state.seen.contains_key(slug) {
                state.seen.insert(slug.clone(), vec![]);
                result.first_seen.push(slug.clone());
            }
            continue;
        };
        if state.heads.get(slug) == Some(&head) {
            continue; // nothing new: no request at all
        }
        let r = match f.get(&format!("{}feeds/{slug}.xml?h={head}", state.base_url), None) {
            Ok(r) => r,
            Err(e) => {
                result.errors.push((slug.clone(), e));
                continue;
            }
        };
        result.checked += 1;
        if r.status != 200 {
            result.errors.push((slug.clone(), format!("HTTP {}", r.status)));
            continue;
        }
        let entries: Vec<Entry> = match parse_atom(&r.text) {
            Ok(v) => v.into_iter().filter(|e| &e.slug == slug).collect(),
            Err(e) => {
                result.errors.push((slug.clone(), e));
                continue;
            }
        };
        if !entries.iter().any(|e| e.hash.starts_with(&head)) {
            result.errors.push((slug.clone(), "フィードがまだ古い（次の確認でもう一度読みます）".into()));
            continue; // the head is not remembered, so the next look reads the feed again
        }
        handle_entries(state, slug, entries, &mut notices, now_ms, &mut result);
        state.heads.insert(slug.clone(), head);
        result.status_changed += 1;
    }
    let base = state.base_url.clone();
    deliver(&base, notices, notify, &mut result);
    finish(state, &result, now_ms);
    result
}

/// A failed look at status.json: only the message is kept (`last_check` stays, so that "last check" means a check that worked).
fn finish_error_only(state: &mut State, result: &PollResult) {
    state.last_error = result.errors.iter().map(|(s, m)| format!("{s}: {m}")).collect::<Vec<_>>().join(" / ").chars().take(300).collect();
}

/// When the next look is due (Unix ms): after the runs (see schedule.rs) while the site publishes status.json,
/// otherwise `interval_min` after the last check. Never in the past.
pub fn next_check_ms(state: &State, now_ms: i64) -> i64 {
    if state.last_check == 0 {
        return now_ms;
    }
    let interval_ms = state.interval_min.clamp(5, 240) as i64 * 60_000;
    let at = if !state.status_mode {
        state.last_check + interval_ms
    } else if state.last_check < state.effective_schedule(now_ms).latest_window_start(now_ms) {
        now_ms // a window began after the last look (the app was closed or the computer asleep): catch up now
    } else {
        state.effective_schedule(now_ms).next_check(now_ms, state.generated_ms)
    };
    at.max(now_ms)
}
