use akari_core::gateway::{DecodeError, DispatchEvent, GatewayEvent, Hello, decode};

#[track_caller]
fn decode_ok(json: &str) -> GatewayEvent {
    decode(json.as_bytes()).unwrap_or_else(|err| panic!("failed to decode: {err}"))
}

#[test]
fn hello_decodes() {
    assert_eq!(
        decode_ok(include_str!("fixtures/hello.json")),
        GatewayEvent::Hello(Hello {
            heartbeat_interval: 41_250,
            trace: vec![r#"["gateway-prd-us-east1-c-6w69",{"micros":0.0}]"#.to_owned()],
        })
    );
}

#[test]
fn control_opcodes_decode() {
    assert_eq!(
        decode_ok(r#"{"op": 1, "d": null, "s": null, "t": null}"#),
        GatewayEvent::Heartbeat
    );
    assert_eq!(
        decode_ok(r#"{"op": 7, "d": null, "s": null, "t": null}"#),
        GatewayEvent::Reconnect
    );
    assert_eq!(
        decode_ok(r#"{"op": 11, "d": null, "s": null, "t": null}"#),
        GatewayEvent::HeartbeatAck
    );
    assert_eq!(decode_ok(r#"{"op": 11}"#), GatewayEvent::HeartbeatAck);
}

#[test]
fn invalid_session_carries_resumability() {
    assert_eq!(
        decode_ok(r#"{"op": 9, "d": true, "s": null, "t": null}"#),
        GatewayEvent::InvalidSession { resumable: true }
    );
    assert_eq!(
        decode_ok(r#"{"op": 9, "d": false, "s": null, "t": null}"#),
        GatewayEvent::InvalidSession { resumable: false }
    );
    assert_eq!(
        decode_ok(r#"{"op": 9, "d": null, "s": null, "t": null}"#),
        GatewayEvent::InvalidSession { resumable: false }
    );
}

#[test]
fn unknown_opcodes_and_events_are_not_errors() {
    assert_eq!(
        decode_ok(r#"{"op": 99, "d": {"x": 1}, "s": null, "t": null}"#),
        GatewayEvent::Unknown { op: 99 }
    );
    assert_eq!(
        decode_ok(r#"{"op": 0, "d": {"x": 1}, "s": 42, "t": "SOMETHING_NEW"}"#),
        GatewayEvent::Dispatch {
            seq: 42,
            event: DispatchEvent::Other("SOMETHING_NEW".to_owned()),
        }
    );
}

#[test]
fn malformed_payloads_are_errors() {
    assert!(matches!(
        decode(br#"{"op": 0, "d": {}, "t": "READY"}"#),
        Err(DecodeError::MissingField { op: 0, field: "s" })
    ));
    assert!(matches!(
        decode(br#"{"op": 10, "d": null, "s": null, "t": null}"#),
        Err(DecodeError::MissingField { op: 10, field: "d" })
    ));
    assert!(matches!(decode(b"not json"), Err(DecodeError::Json(_))));
    assert!(matches!(
        decode(br#"{"op": 10, "d": {"heartbeat_interval": "soon"}}"#),
        Err(DecodeError::Json(_))
    ));
}
