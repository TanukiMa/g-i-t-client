//! One look at the sites without the app's window or timer thread: what the background job of a phone does (Android:
//! WorkManager, see src-tauri/gen/android). It reads the state file, looks at the sites like the checking thread does, saves,
//! and hands back the notifications and the time of the next look. The platform shows the notifications itself. No OS code.

use crate::poller::{next_check_ms, poll_scheduled, Fetcher, Notice};
use crate::state::{self, FileLock};
use std::path::Path;

/// Nothing is followed: look again soon, so that sites picked in the meantime are not waited for long (no network is used).
const IDLE_RETRY_MS: i64 = 30 * 60_000;

#[derive(Debug)]
pub struct Outcome {
    /// What to show now (already limited: a summary when there are many).
    pub notices: Vec<Notice>,
    /// When the next look is due (Unix ms); never in the past.
    pub next_ms: i64,
    /// The errors of this look ("" = none).
    pub error: String,
}

/// Looks once. `path` is the state file. Takes the lock of the state file for the whole look, so that the app (which takes
/// the same lock) and this job never work on the state at the same time.
pub fn run(path: &Path, f: &dyn Fetcher, now_ms: i64) -> Outcome {
    let _lock = match FileLock::acquire(path) {
        Ok(l) => l,
        Err(e) => return Outcome { notices: vec![], next_ms: now_ms + IDLE_RETRY_MS, error: format!("state lock: {e}") },
    };
    // A job must not create a state (and start the trial) for an app that was never set up.
    if !path.exists() {
        return Outcome { notices: vec![], next_ms: now_ms + IDLE_RETRY_MS, error: String::new() };
    }
    let mut state = state::load(path);
    if state.effective_follow(now_ms).is_empty() {
        return Outcome { notices: vec![], next_ms: now_ms + IDLE_RETRY_MS, error: String::new() };
    }
    let mut notices: Vec<Notice> = vec![];
    poll_scheduled(&mut state, f, &mut |n| notices.push(n.clone()), now_ms);
    let mut error = state.last_error.clone();
    if let Err(e) = state::save(path, &state) {
        error = format!("{error} / save: {e}");
    }
    Outcome { notices, next_ms: next_check_ms(&state, now_ms), error }
}

/// When the next look is due, without looking (to book the first job when the app starts).
pub fn next(path: &Path, now_ms: i64) -> i64 {
    if !path.exists() {
        return now_ms + IDLE_RETRY_MS;
    }
    let state = state::load(path);
    if state.effective_follow(now_ms).is_empty() {
        now_ms + IDLE_RETRY_MS
    } else {
        next_check_ms(&state, now_ms)
    }
}
