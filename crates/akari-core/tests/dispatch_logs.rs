// The only test in its binary: tracing caches callsite interest process-wide, so parallel
// tests racing a thread-local subscriber can hide the logs this test checks.
mod support;

use akari_core::gateway::{DispatchEvent, GatewayEvent, decode};
use serde_json::{Value, json};

use crate::support::gateway::Logs;

fn dispatch(name: &str, data: Value) -> Vec<u8> {
    json!({"op": 0, "s": 7, "t": name, "d": data})
        .to_string()
        .into_bytes()
}

#[test]
fn dispatches_log_their_name_and_guild_only() {
    let logs = Logs::capture();
    let mut message: Value = serde_json::from_str(include_str!("fixtures/message.json"))
        .unwrap_or_else(|err| panic!("fixture is not JSON: {err}"));
    message["guild_id"] = "200000000000000001".into();
    message["content"] = "do-not-log".into();
    let typing = json!({"guild_id": "200000000000000001", "channel_id": "300000000000000002",
        "user_id": "100000000000000002", "timestamp": 1_700_000_000});
    let member_list = json!({"guild_id": "200000000000000001", "id": "everyone",
        "member_count": 3, "online_count": 2, "groups": [],
        "ops": [{"op": "SYNC", "range": [0, 99], "items": [{"member": {"nick": "do-not-log"}}]}]});

    let events = [
        decode(&dispatch("MESSAGE_CREATE", message)),
        decode(&dispatch("TYPING_START", typing)),
        decode(&dispatch("GUILD_MEMBER_LIST_UPDATE", member_list)),
    ];

    assert!(matches!(
        &events[2],
        Ok(GatewayEvent::Dispatch {
            event: DispatchEvent::Other(name),
            ..
        }) if name == "GUILD_MEMBER_LIST_UPDATE"
    ));
    let logs = logs.text();
    for name in ["MESSAGE_CREATE", "TYPING_START", "GUILD_MEMBER_LIST_UPDATE"] {
        assert!(logs.contains(name), "{logs}");
    }
    assert_eq!(
        logs.matches("guild_id=200000000000000001").count(),
        3,
        "{logs}"
    );
    assert!(!logs.contains("do-not-log"), "{logs}");
}
