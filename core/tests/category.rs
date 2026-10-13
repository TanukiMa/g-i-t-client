//! Choosing sites by category: the facet rule, what the picker shows, what is followed on which plan, and what a whole
//! category does to the notifications. Offline (a fake network).

use git_core::category::{counts, matching};
use git_core::feed::{parse_groups, Group, Site};
use git_core::poller::{poll_once, FetchResult, Fetcher, Notice, MAX_NOTICES};
use git_core::state::State;
use std::cell::RefCell;
use std::collections::HashMap;

fn site(slug: &str, tags: &[&str]) -> Site {
    Site { slug: slug.into(), name: format!("サイト{slug}"), url: "https://orig.example/".into(), tags: tags.iter().map(|t| t.to_string()).collect() }
}

fn group(name: &str, tags: &[&str]) -> Group {
    Group { name: name.into(), tags: tags.iter().map(|t| t.to_string()).collect() }
}

fn strings(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

fn sample() -> (Vec<Site>, Vec<Group>) {
    let sites = vec![
        site("jsa", &["学会", "内科系"]),
        site("jss", &["学会", "外科系"]),
        site("jgca", &["学会", "がん"]),
        site("mhlw", &["行政", "厚生労働省"]),
        site("pmda", &["公的機関", "PMDA", "感染症"]),
        site("solo", &["その他のタグ"]),
    ];
    let groups = vec![group("種別", &["学会", "行政", "公的機関"]), group("領域", &["内科系", "外科系", "がん", "感染症"]), group("組織", &["厚生労働省", "PMDA", "使われていない"])];
    (sites, groups)
}

#[test]
fn the_tags_of_one_heading_are_alternatives_and_the_headings_are_all_required() {
    let (sites, groups) = sample();
    assert!(matching(&sites, &groups, &[]).is_empty(), "nothing picked matches nothing");
    assert_eq!(matching(&sites, &groups, &strings(&["学会"])), ["jsa", "jss", "jgca"]);
    assert_eq!(matching(&sites, &groups, &strings(&["内科系", "外科系"])), ["jsa", "jss"], "same heading: or");
    assert_eq!(matching(&sites, &groups, &strings(&["学会", "がん"])), ["jgca"], "two headings: and");
    assert_eq!(matching(&sites, &groups, &strings(&["学会", "行政"])), ["jsa", "jss", "jgca", "mhlw"]);
    assert_eq!(matching(&sites, &groups, &strings(&["行政", "がん"])), Vec::<String>::new(), "no site is both");
    assert_eq!(matching(&sites, &groups, &strings(&["その他のタグ"])), ["solo"], "a tag of no heading still works");
    assert_eq!(matching(&sites, &groups, &strings(&["学会", "その他のタグ"])), Vec::<String>::new(), "...as one more heading");
    assert!(matching(&sites, &groups, &strings(&["存在しない"])).is_empty());
}

#[test]
fn the_picker_shows_the_headings_in_the_sites_order_with_counts() {
    let (sites, groups) = sample();
    let c = counts(&sites, &groups);
    assert_eq!(c.iter().map(|g| g.name.as_str()).collect::<Vec<_>>(), ["種別", "領域", "組織", "その他"]);
    let kind = &c[0];
    assert_eq!(kind.tags.iter().map(|t| (t.name.as_str(), t.count)).collect::<Vec<_>>(), [("学会", 3), ("行政", 1), ("公的機関", 1)]);
    let org = &c[2];
    assert_eq!(org.tags.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), ["厚生労働省", "PMDA"], "a tag no site carries is not offered");
    assert_eq!(c[3].tags[0].name, "その他のタグ");
    // a site that publishes no groups: everything comes under "その他"
    let c = counts(&sites, &[]);
    assert_eq!(c.len(), 1);
    assert_eq!(c[0].name, "その他");
}

#[test]
fn groups_of_search_json_are_read_and_a_missing_or_broken_list_is_empty() {
    let g = parse_groups(r#"{"v":1,"sites":[],"groups":[["種別",["学会","行政"]],["領域",["内科系"]]],"updates":[]}"#);
    assert_eq!(g, [group("種別", &["学会", "行政"]), group("領域", &["内科系"])]);
    assert!(parse_groups(r#"{"v":1,"sites":[],"updates":[]}"#).is_empty(), "an older site");
    assert!(parse_groups("not json").is_empty());
    assert_eq!(parse_groups(r#"{"groups":[["a",["x"]],"broken",[1,2],["b"]]}"#), [group("a", &["x"])]);
}

fn state_with_rules(trial: bool) -> State {
    let (sites, groups) = sample();
    let mut st = State::default();
    st.sites = sites;
    st.groups = groups;
    st.sites_at = 1_000;
    st.trial_started_ms = if trial { 1_000 } else { 0 };
    st
}

#[test]
fn categories_follow_new_sites_and_can_be_switched_off_one_by_one() {
    let mut st = state_with_rules(true);
    st.tags = strings(&["学会"]);
    st.follow = strings(&["mhlw"]);
    assert_eq!(st.effective_follow(2_000), ["mhlw", "jsa", "jss", "jgca"]);

    st.exclude = strings(&["jss"]);
    assert_eq!(st.effective_follow(2_000), ["mhlw", "jsa", "jgca"], "a switched-off site stays off");

    // the site publishes another learned society: it is followed without any action
    st.sites.push(site("jsnew", &["学会", "内科系"]));
    assert_eq!(st.effective_follow(2_000), ["mhlw", "jsa", "jgca", "jsnew"]);

    // a site that is both followed and matched is not listed twice
    st.follow.push("jsa".into());
    assert_eq!(st.effective_follow(2_000).iter().filter(|s| *s == "jsa").count(), 1);
}

#[test]
fn the_free_plan_follows_only_its_own_list_and_never_uses_categories() {
    let mut st = state_with_rules(false);
    assert!(!st.plan(2_000).unlocked());
    st.tags = strings(&["学会"]);
    st.follow = strings(&["mhlw", "pmda"]);
    assert_eq!(st.effective_follow(2_000), ["mhlw", "pmda"], "the categories are kept but not used");

    // the trial ends: the picks stay in the state, only the free list is looked at
    let mut st = state_with_rules(true);
    st.tags = strings(&["学会"]);
    st.follow = strings(&["mhlw"]);
    assert_eq!(st.effective_follow(2_000).len(), 4);
    let after = 1_000 + 8 * 86_400_000;
    assert_eq!(st.effective_follow(after), ["mhlw"]);
    assert_eq!(st.tags, ["学会"]);
}

#[test]
fn an_old_state_file_without_the_new_fields_still_loads() {
    let st: State = serde_json::from_str(r#"{"v":1,"baseUrl":"https://dash.example/","follow":["a","b"]}"#).unwrap();
    assert_eq!(st.follow, ["a", "b"]);
    assert!(st.tags.is_empty() && st.exclude.is_empty() && st.groups.is_empty() && !st.onboarded);
}

// ---- a whole category and the notifications (poll_once, fake network) ----
struct Net {
    feeds: RefCell<HashMap<String, String>>,
}

impl Fetcher for Net {
    fn get(&self, url: &str, _etag: Option<&str>) -> Result<FetchResult, String> {
        let slug = url.rsplit('/').next().unwrap().trim_end_matches(".xml").to_string();
        match self.feeds.borrow().get(&slug) {
            Some(xml) => Ok(FetchResult { status: 200, text: xml.clone(), etag: String::new() }),
            None => Ok(FetchResult { status: 404, text: String::new(), etag: String::new() }),
        }
    }
}

fn hash(site: usize, n: usize) -> String {
    format!("{site:02x}{n:02x}").repeat(10)
}

fn atom(slug: &str, site: usize, newest: usize) -> String {
    let entries: String = (1..=newest)
        .rev()
        .map(|n| {
            let h = hash(site, n);
            format!(
                r#"<entry><title>サイト{slug}: 追加: お知らせ{n}</title><id>tag:git.example.com,2026:g-i-t-data/{slug}/{h}</id><updated>2026-10-07T0{}:30:00Z</updated><content type="html">&lt;ul&gt;&lt;li&gt;追加: お知らせ{n}&lt;/li&gt;&lt;/ul&gt;</content></entry>"#,
                n % 10
            )
        })
        .collect();
    format!(r#"<?xml version="1.0" encoding="utf-8"?><feed xmlns="http://www.w3.org/2005/Atom"><title>x</title><id>y</id><updated>2026-10-07T00:00:00Z</updated>{entries}</feed>"#)
}

fn category_state(n: usize) -> (State, Net) {
    let mut st = State::default();
    st.base_url = "https://dash.example/".into();
    st.sites = (0..n).map(|i| site(&format!("s{i}"), &["感染症"])).collect();
    st.groups = vec![group("領域", &["感染症"])];
    st.sites_at = 1_000;
    st.trial_started_ms = 1_000;
    st.tags = strings(&["感染症"]);
    let net = Net { feeds: RefCell::new(HashMap::new()) };
    for i in 0..n {
        net.feeds.borrow_mut().insert(format!("s{i}"), atom(&format!("s{i}"), i, 1));
    }
    (st, net)
}

fn look(st: &mut State, net: &Net) -> (git_core::poller::PollResult, Vec<Notice>) {
    let mut notes = vec![];
    let r = poll_once(st, net, &mut |n| notes.push(n.clone()), 2_000);
    (r, notes)
}

#[test]
fn the_first_look_at_a_whole_category_is_silent_and_lists_only_a_few_sites() {
    let (mut st, net) = category_state(12);
    let (r, notes) = look(&mut st, &net);
    assert_eq!(r.first_seen.len(), 12);
    assert!(notes.is_empty(), "no notification for what already existed");
    assert_eq!(st.recent.len(), 5, "one entry each for the first five sites, not a list of 12");
}

#[test]
fn a_few_updates_are_shown_one_by_one_and_many_become_one_notification() {
    let (mut st, net) = category_state(12);
    look(&mut st, &net); // first look: remembered

    for i in 0..MAX_NOTICES {
        net.feeds.borrow_mut().insert(format!("s{i}"), atom(&format!("s{i}"), i, 2));
    }
    let (r, notes) = look(&mut st, &net);
    assert_eq!(notes.len(), MAX_NOTICES, "up to the limit: one each");
    assert_eq!(r.notified, MAX_NOTICES);

    for i in 0..(MAX_NOTICES + 3) {
        net.feeds.borrow_mut().insert(format!("s{i}"), atom(&format!("s{i}"), i, 3));
    }
    let (r, notes) = look(&mut st, &net);
    assert_eq!(notes.len(), 1, "more than the limit: one summary");
    assert_eq!(r.notified, 1);
    let n = &notes[0];
    // sites 0-4 gained one entry (the third), sites 5-7 gained two (the second and the third): 5 + 3 * 2 updates
    assert!(n.body.starts_with("11 件の更新を検知しました"), "{}", n.body);
    assert!(n.body.contains("ほか"), "{}", n.body);
    assert_eq!(n.diff_url, "https://dash.example/", "it opens the dashboard");
    // every update is still listed one by one under 最近の更新
    assert!(st.recent.len() >= MAX_NOTICES + 3);
}

#[test]
fn the_recent_list_is_newest_update_first_whatever_the_order_of_discovery() {
    let (mut st, net) = category_state(2);
    // s0 is looked at first but has the NEWER updates (hours 5, 4, 3); s1 comes second with an older one (hour 1)
    net.feeds.borrow_mut().insert("s0".into(), atom("s0", 0, 5));
    net.feeds.borrow_mut().insert("s1".into(), atom("s1", 1, 1));
    look(&mut st, &net);
    let times: Vec<i64> = st.recent.iter().map(|r| r.updated_ms).collect();
    assert_eq!(times.len(), 4);
    assert!(times.iter().all(|t| *t > 0));
    assert!(times.windows(2).all(|w| w[0] >= w[1]), "not newest first: {times:?}");
    assert_eq!(st.recent[0].slug, "s0");
    assert_eq!(st.recent[3].slug, "s1");

    // a later look finds an update of s1 that is newer than everything: it goes to the top
    net.feeds.borrow_mut().insert("s1".into(), atom("s1", 1, 9));
    look(&mut st, &net);
    assert_eq!(st.recent[0].slug, "s1");
    assert_eq!(st.recent[0].count, 8, "eight new entries of one site become one line");
}

#[test]
fn a_recent_entry_stored_before_the_update_time_was_kept_still_loads_and_sorts_by_when_it_was_found() {
    let st: State = serde_json::from_str(
        r#"{"recent":[{"slug":"a","name":"A","time":"x","text":"t","count":1,"diffUrl":"","historyUrl":"","at":5},{"slug":"b","name":"B","time":"y","text":"t","count":1,"diffUrl":"","historyUrl":"","at":9}]}"#,
    )
    .unwrap();
    assert_eq!(st.recent[0].updated_ms, 0);
    let mut st = st;
    st.sort_recent();
    assert_eq!(st.recent[0].slug, "b");
}

#[test]
fn the_headings_are_read_even_when_the_site_list_is_already_there() {
    // A state that has the sites but no headings (written before they existed, or read from an older dashboard) reads
    // search.json again after an hour; one that has them does not.
    let json = r#"{"v":1,"sites":[["a","A","https://a.example/",["学会"]]],"groups":[["種別",["学会"]]],"updates":[]}"#;
    let net = Net { feeds: RefCell::new(HashMap::new()) };
    net.feeds.borrow_mut().insert("search.json".into(), json.into());
    let mut st = State::default();
    st.base_url = "https://dash.example/".into();
    st.sites = vec![site("a", &["学会"])];
    st.sites_at = 1_000;
    let mut notes = vec![];
    poll_once(&mut st, &net, &mut |n| notes.push(n.clone()), 1_000 + 1_800_000); // half an hour: too early
    assert!(st.groups.is_empty());
    poll_once(&mut st, &net, &mut |n| notes.push(n.clone()), 1_000 + 3_700_000); // more than an hour
    assert_eq!(st.groups, [group("種別", &["学会"])]);
    assert_eq!(st.sites_at, 1_000 + 3_700_000);
    let at = st.sites_at;
    poll_once(&mut st, &net, &mut |n| notes.push(n.clone()), at + 3_700_000); // headings are there: no new request
    assert_eq!(st.sites_at, at);
}

fn atom_texts(slug: &str, site: usize, items: &[(usize, &str)]) -> String {
    let entries: String = items
        .iter()
        .map(|(n, text)| {
            let h = hash(site, *n);
            format!(
                r#"<entry><title>サイト{slug}: {text}</title><id>tag:git.example.com,2026:g-i-t-data/{slug}/{h}</id><updated>2026-10-07T0{}:30:00Z</updated><content type="html">&lt;p&gt;{text}&lt;/p&gt;</content></entry>"#,
                n % 10
            )
        })
        .collect();
    format!(r#"<?xml version="1.0" encoding="utf-8"?><feed xmlns="http://www.w3.org/2005/Atom"><title>x</title><id>y</id><updated>2026-10-07T00:00:00Z</updated>{entries}</feed>"#)
}

#[test]
fn an_update_that_says_nothing_changed_is_remembered_but_is_not_news() {
    const QUIET: &str = "内容に実質的な変更はありません（表示の調整のみ）。";
    let (mut st, net) = category_state(1);
    net.feeds.borrow_mut().insert("s0".into(), atom_texts("s0", 0, &[(3, QUIET), (2, "追加: 本当のお知らせ"), (1, QUIET)]));
    let (r, notes) = look(&mut st, &net);
    assert_eq!(r.first_seen.len(), 1);
    assert!(notes.is_empty());
    assert_eq!(st.recent.len(), 1, "only the real one is listed");
    assert!(st.recent[0].text.contains("本当のお知らせ"), "{}", st.recent[0].text);

    // later: one more quiet update and one real one: only the real one is announced
    net.feeds.borrow_mut().insert("s0".into(), atom_texts("s0", 0, &[(5, "変更: 日程の更新"), (4, QUIET), (3, QUIET), (2, "追加: 本当のお知らせ"), (1, QUIET)]));
    let (r, notes) = look(&mut st, &net);
    assert_eq!(notes.len(), 1);
    assert_eq!(r.notified, 1);
    assert!(notes[0].body.contains("日程の更新"), "{}", notes[0].body);

    // a look at the same feed again: the quiet ones are known and do not come back
    let (_, notes) = look(&mut st, &net);
    assert!(notes.is_empty());

    // only quiet updates: nothing is announced and nothing is listed, but they are remembered
    let before = st.recent.len();
    net.feeds.borrow_mut().insert("s0".into(), atom_texts("s0", 0, &[(7, QUIET), (6, QUIET), (5, "変更: 日程の更新"), (4, QUIET), (3, QUIET), (2, "追加: 本当のお知らせ"), (1, QUIET)]));
    let (_, notes) = look(&mut st, &net);
    assert!(notes.is_empty());
    assert_eq!(st.recent.len(), before);
    assert!(st.seen["s0"].len() >= 7);
}
