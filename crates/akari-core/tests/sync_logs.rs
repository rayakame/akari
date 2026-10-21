// The only test in its binary: tracing caches callsite interest process-wide, so parallel
// tests racing a thread-local subscriber can hide the logs this test checks.
mod support;

use akari_core::gateway::decode;
use serde_json::{Value, json};

use crate::support::gateway::Logs;

// What a live check reads to see Discord send `most_recent_messages`: counts only.
#[test]
fn a_thread_list_sync_logs_its_counts_only() {
    let logs = Logs::capture();
    let mut sync: Value = serde_json::from_str(include_str!("fixtures/thread_list_sync.json"))
        .unwrap_or_else(|err| panic!("fixture is not JSON: {err}"));
    sync["threads"][0]["name"] = "do-not-log".into();
    sync["most_recent_messages"][0]["content"] = "do-not-log".into();

    let decoded = decode(
        json!({"op": 0, "s": 7, "t": "THREAD_LIST_SYNC", "d": sync})
            .to_string()
            .as_bytes(),
    );

    assert!(decoded.is_ok());
    let logs = logs.text();
    for field in ["channels=1", "threads=1", "members=1", "latest_messages=1"] {
        assert!(logs.contains(field), "{logs}");
    }
    assert!(!logs.contains("do-not-log"), "{logs}");
}
