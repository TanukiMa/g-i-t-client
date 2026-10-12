//! Free and paid use: license keys, store results, and what the plan changes.

use git_core::entitlement::*;
use git_core::state::State;

const DAY: i64 = 86_400_000;
const SECRET: [u8; 32] = [7; 32];

fn public() -> [u8; 32] {
    ed25519_dalek::SigningKey::from_bytes(&SECRET).verifying_key().to_bytes()
}

#[test]
fn a_signed_license_is_accepted_and_a_changed_one_is_not() {
    let token = sign_license(&SECRET, "order-1", 100 * DAY);
    let l = verify_license_with(&public(), &token, 10 * DAY).unwrap();
    assert_eq!((l.sub.as_str(), l.expires_ms), ("order-1", 100 * DAY));
    assert!(verify_license_with(&public(), &token, 101 * DAY).unwrap_err().contains("期限"));
    let forged = sign_license(&[8; 32], "order-1", 0);
    assert!(verify_license_with(&public(), &forged, 0).is_err(), "signed with another key");
    let mut parts: Vec<String> = token.split('.').map(String::from).collect();
    parts[1] = sign_license(&SECRET, "other", 0).split('.').nth(1).unwrap().to_string();
    assert!(verify_license_with(&public(), &parts.join("."), 0).is_err(), "payload swapped");
    assert!(verify_license_with(&public(), "garbage", 0).is_err());
    assert!(verify_license_with(&public(), &sign_license(&SECRET, "forever", 0), 1000 * DAY).is_ok(), "0 = no expiry");
}

#[test]
fn a_store_subscription_counts_until_it_ends_or_is_not_confirmed_for_a_week() {
    let now = 50 * DAY;
    let mk = |expires, checked| Some(StoreEntitlement { source: Source::GooglePlay, expires_ms: expires, checked_ms: checked });
    assert!(plan("", &mk(now + DAY, now), now).is_pro());
    assert!(!plan("", &mk(now - 1, now - 2), now).is_pro(), "ended");
    assert!(plan("", &mk(now + 30 * DAY, now - 6 * DAY), now).is_pro(), "offline for 6 days");
    assert!(!plan("", &mk(now + 30 * DAY, now - 8 * DAY), now).is_pro(), "not confirmed for 8 days");
    assert!(!plan("", &None, now).is_pro());
    assert!(!plan("GIT1.x.y", &None, now).is_pro(), "a license that does not verify is ignored");
}

#[test]
fn the_free_plan_follows_three_sites_and_looks_after_the_first_run_only() {
    let now = 50 * DAY;
    let mut st = State::default();
    st.follow = ["a", "b", "c", "d", "e"].iter().map(|s| s.to_string()).collect();
    assert_eq!(st.effective_follow(now), ["a", "b", "c"]);
    assert_eq!(st.effective_schedule(now).runs.len(), 1);
    assert_eq!(st.follow.len(), 5, "the list itself is kept");

    st.store = Some(StoreEntitlement { source: Source::AppStore, expires_ms: now + DAY, checked_ms: now });
    assert_eq!(st.effective_follow(now).len(), 5);
    assert_eq!(st.effective_schedule(now).runs.len(), 3);
}

#[test]
fn the_trial_unlocks_everything_for_the_set_days_and_a_set_back_clock_does_not_extend_it() {
    use git_core::entitlement::Plan;
    std::env::set_var("GIT_TRIAL_DAYS", "7");
    let t0 = 100 * DAY;
    let mut st = State::default();
    st.follow = ["a", "b", "c", "d", "e"].iter().map(|s| s.to_string()).collect();
    assert_eq!(st.plan(t0), Plan::Free, "before the first start nothing is running");
    st.tick(t0);
    assert_eq!(st.trial_started_ms, t0);
    assert!(matches!(st.plan(t0 + 6 * DAY), Plan::Trial { .. }));
    assert_eq!(st.effective_follow(t0 + 6 * DAY).len(), 5);
    assert_eq!(st.effective_schedule(t0 + 6 * DAY).runs.len(), 3);
    assert_eq!(st.plan(t0 + 8 * DAY), Plan::Free);
    assert_eq!(st.effective_follow(t0 + 8 * DAY).len(), 3);
    st.tick(t0 + 8 * DAY);
    assert_eq!(st.plan(t0 + DAY), Plan::Free, "the clock was set back: the trial stays over");
    st.store = Some(StoreEntitlement { source: Source::AppStore, expires_ms: t0 + 40 * DAY, checked_ms: t0 + 8 * DAY });
    assert!(st.plan(t0 + 9 * DAY).is_pro(), "a subscription wins");
    std::env::set_var("GIT_TRIAL_DAYS", "0");
    let mut st2 = State::default();
    st2.tick(t0);
    assert_eq!(st2.plan(t0), Plan::Free, "0 days = no trial");
    std::env::remove_var("GIT_TRIAL_DAYS");
}
