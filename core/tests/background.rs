//! The background job (what Android's WorkManager runs): state file in, notifications and next time out.

use git_core::background;
use git_core::feed::Site;
use git_core::poller::{FetchResult, Fetcher};
use git_core::state::{self, State};
use std::cell::RefCell;
use std::collections::HashMap;

struct Net {
    files: RefCell<HashMap<String, String>>,
}

impl Net {
    fn put(&self, path: &str, body: &str) {
        self.files.borrow_mut().insert(path.into(), body.into());
    }
}

impl Fetcher for Net {
    fn get(&self, url: &str, _etag: Option<&str>) -> Result<FetchResult, String> {
        let path = url.strip_prefix("https://dash.example/").unwrap_or(url);
        match self.files.borrow().get(path) {
            Some(b) => Ok(FetchResult { status: 200, text: b.clone(), etag: String::new() }),
            None => Ok(FetchResult { status: 404, text: String::new(), etag: String::new() }),
        }
    }
}

const NOW: i64 = 1_791_000_000_000; // any time works: the job does not depend on the window

fn status_json(head: &str) -> String {
    format!(r#"{{"v":1,"generated":"2026-10-13T03:41:00Z","tz":"Asia/Tokyo","utcOffsetMin":540,"runs":["07:30","12:30","16:30"],"delayMin":8,"windowMin":25,"sites":{{"a":"{head}"}}}}"#)
}

fn hash(n: usize) -> String {
    format!("{n:x}").repeat(40).chars().take(40).collect()
}

fn atom(newest: usize) -> String {
    let entries: String = (1..=newest)
        .rev()
        .map(|n| {
            let h = hash(n);
            format!(
                r#"<entry><title>サイトa: 追加: お知らせ{n}</title><id>tag:x.example,2026:g-i-t-data/a/{h}</id><updated>2026-10-07T0{n}:30:00Z</updated><content type="html">&lt;ul&gt;&lt;li&gt;追加: お知らせ{n}&lt;/li&gt;&lt;/ul&gt;</content></entry>"#
            )
        })
        .collect();
    format!(r#"<?xml version="1.0" encoding="utf-8"?><feed xmlns="http://www.w3.org/2005/Atom"><title>x</title><id>y</id><updated>2026-10-07T00:00:00Z</updated>{entries}</feed>"#)
}

fn head(n: usize) -> String {
    hash(n)[..7].to_string()
}

fn write_state(path: &std::path::Path, follow: &[&str]) {
    let mut st = State::default();
    st.base_url = "https://dash.example/".into();
    st.follow = follow.iter().map(|s| s.to_string()).collect();
    st.sites = vec![Site { slug: "a".into(), name: "サイトa".into(), url: "https://orig.example/".into(), tags: vec![] }];
    st.sites_at = NOW; // the names are fresh: search.json is not asked for
    st.trial_started_ms = NOW - 1000;
    state::save(path, &st).unwrap();
}

#[test]
fn the_job_remembers_first_and_announces_a_new_update_once() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.json");
    write_state(&path, &["a"]);
    let net = Net { files: RefCell::new(HashMap::new()) };
    net.put("feeds/status.json", &status_json(&head(1)));
    net.put(&format!("feeds/a.xml?h={}", head(1)), &atom(1));

    let o = background::run(&path, &net, NOW);
    assert!(o.notices.is_empty(), "the first look only remembers");
    assert!(o.error.is_empty(), "{}", o.error);
    assert!(o.next_ms > NOW, "the next look is in the future");
    let saved = state::load(&path);
    assert!(saved.last_check > 0 && saved.seen.contains_key("a"), "the look is saved");

    // a new update appears
    net.put("feeds/status.json", &status_json(&head(2)));
    net.put(&format!("feeds/a.xml?h={}", head(2)), &atom(2));
    let o = background::run(&path, &net, NOW + 60_000);
    assert_eq!(o.notices.len(), 1);
    assert!(o.notices[0].body.contains("お知らせ2"), "{}", o.notices[0].body);
    assert!(o.notices[0].diff_url.starts_with("https://dash.example/sites/a/diff_"));

    // looked at again: not announced a second time
    let o = background::run(&path, &net, NOW + 120_000);
    assert!(o.notices.is_empty());
}

#[test]
fn nothing_followed_means_no_network_no_file_written_and_a_soon_retry() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.json");
    let net = Net { files: RefCell::new(HashMap::new()) };
    let o = background::run(&path, &net, NOW);
    assert!(o.notices.is_empty() && o.error.is_empty());
    assert_eq!(o.next_ms, NOW + 30 * 60_000);
    assert!(!path.exists(), "a job must not create a state (and start the trial) for an app that was never set up");
    assert_eq!(background::next(&path, NOW), NOW + 30 * 60_000);

    // a state exists but nothing is picked yet
    write_state(&path, &[]);
    let o = background::run(&path, &net, NOW);
    assert_eq!(o.next_ms, NOW + 30 * 60_000);
}

#[test]
fn the_next_time_follows_the_schedule_once_something_is_followed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.json");
    write_state(&path, &["a"]);
    assert!(background::next(&path, NOW) >= NOW, "never in the past");
}

#[test]
fn two_holders_of_the_lock_take_turns() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.json");
    let first = state::FileLock::acquire(&path).unwrap();
    let p2 = path.clone();
    let t = std::thread::spawn(move || {
        let _second = state::FileLock::acquire(&p2).unwrap(); // waits for the first
        std::time::Instant::now()
    });
    std::thread::sleep(std::time::Duration::from_millis(200));
    let released = std::time::Instant::now();
    drop(first);
    let got = t.join().unwrap();
    assert!(got >= released, "the second holder got the lock before the first one let go");
}

#[test]
fn the_lock_can_be_tried_without_waiting() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.json");
    let held = state::FileLock::try_acquire(&path).expect("free at first");
    assert!(state::FileLock::try_acquire(&path).is_none(), "taken: nobody waits");
    drop(held);
    assert!(state::FileLock::try_acquire(&path).is_some(), "free again");
}
