// The only test in its binary: tracing caches callsite interest process-wide, so parallel
// tests racing a thread-local subscriber can hide the logs this test checks.
mod support;

use akari_core::gateway::{DispatchEvent, GatewayEvent, decode};
use serde_json::Value;

use crate::support::gateway::Logs;

#[test]
fn skipped_entries_never_log_their_values() {
    let logs = Logs::capture();

    let mut ready: Value = serde_json::from_str(include_str!("fixtures/ready.json"))
        .unwrap_or_else(|err| panic!("fixture is not JSON: {err}"));
    ready["d"]["users"][0]["accent_color"] = "do-not-log".into();
    ready["d"]["guilds"][0]["properties"]["afk_timeout"] = "do-not-log".into();
    let event = decode(ready.to_string().as_bytes()).unwrap_or_else(|err| panic!("{err}"));
    assert!(matches!(
        event,
        GatewayEvent::Dispatch {
            event: DispatchEvent::Ready(_),
            ..
        }
    ));

    let logs = logs.text();
    assert!(logs.contains("100000000000000002"), "{logs}");
    assert!(logs.contains("200000000000000001"), "{logs}");
    assert_eq!(logs.matches("line 1").count(), 2, "{logs}");
    assert!(!logs.contains("do-not-log"), "{logs}");
}
