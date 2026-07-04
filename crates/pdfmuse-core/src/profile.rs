//! Stage profiler for investigating hotspots (PER-228), gated on the
//! `PDFMUSE_PROFILE` env var. It only reads the clock to print stage timings to
//! stderr — it never touches parsed output or determinism.
//!
//! The clock (`Instant::now`) is read **only when enabled**, so this stays off the
//! WASM path (where a monotonic clock isn't available) as long as the env var is
//! unset — which it always is in the browser.

use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

/// Accumulates per-substage time across (possibly parallel) page workers, so the
/// layout breakdown can be summed and printed once instead of per page.
static ACC: Mutex<BTreeMap<&'static str, u128>> = Mutex::new(BTreeMap::new());

/// Is stage profiling turned on for this run? The env var is read once and cached,
/// so per-page calls on the hot path cost only an atomic load.
pub(crate) fn enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("PDFMUSE_PROFILE").is_some())
}

/// Add `t`'s elapsed nanos to substage `key` (only when profiling).
pub(crate) fn accum(on: bool, key: &'static str, t: &Option<Instant>) {
    if let Some(t) = t.filter(|_| on) {
        *ACC.lock().unwrap().entry(key).or_insert(0) += t.elapsed().as_nanos();
    }
}

/// Print and clear the accumulated substage totals.
pub(crate) fn dump(on: bool) {
    if on {
        let mut m = ACC.lock().unwrap();
        for (k, v) in m.iter() {
            eprintln!("[prof]   {k:44} {:>9.2} ms", *v as f64 / 1e6);
        }
        m.clear();
    }
}

/// Start a timer iff profiling is on (returns `None` otherwise — no clock read).
pub(crate) fn start(on: bool) -> Option<Instant> {
    on.then(Instant::now)
}

/// Print the elapsed time for `stage` if the timer is live.
pub(crate) fn log(t: &Option<Instant>, stage: &str) {
    if let Some(t) = t {
        eprintln!("[prof] {stage:46} {:>9.2} ms", t.elapsed().as_secs_f64() * 1000.0);
    }
}
