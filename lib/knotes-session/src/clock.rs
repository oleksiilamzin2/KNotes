//! The time the cards are counted and queued by (ADR 0037).

use std::sync::OnceLock;

/// Now, in seconds since 1970: the Mac's clock, or the time `KNOTES_PROBE_NOW` gives in its
/// place for the whole run. The app's tests give it: a card graded in one launch is «due in
/// ten minutes» in the next, and with the Mac's own clock a run that slept between the two (the
/// lid closed) found it due and failed. The session's counts and the window's queue read the
/// same clock, so they agree.
pub fn now() -> i64 {
    static GIVEN: OnceLock<Option<i64>> = OnceLock::new();
    GIVEN.get_or_init(|| std::env::var("KNOTES_PROBE_NOW").ok()?.parse().ok()).unwrap_or_else(|| std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64))
}
