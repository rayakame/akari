// The only test in its binary: tracing caches callsite interest process-wide, so parallel
// tests racing a thread-local subscriber can hide the logs this test checks.
use std::io;
use std::sync::{Arc, Mutex};

use akari_core::gateway::{DispatchEvent, GatewayEvent, decode};
use serde_json::Value;

#[derive(Clone, Default)]
struct Logs(Arc<Mutex<Vec<u8>>>);

impl io::Write for Logs {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0
            .lock()
            .unwrap_or_else(|err| panic!("{err}"))
            .extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn skipped_entries_never_log_their_values() {
    let logs = Logs::default();
    let writer = logs.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_ansi(false)
        .with_writer(move || writer.clone())
        .finish();
    tracing::subscriber::set_global_default(subscriber).unwrap_or_else(|err| panic!("{err}"));

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

    let logs = String::from_utf8(logs.0.lock().unwrap_or_else(|err| panic!("{err}")).clone())
        .unwrap_or_else(|err| panic!("{err}"));
    assert!(logs.contains("100000000000000002"), "{logs}");
    assert!(logs.contains("200000000000000001"), "{logs}");
    assert_eq!(logs.matches("line 1").count(), 2, "{logs}");
    assert!(!logs.contains("do-not-log"), "{logs}");
}
