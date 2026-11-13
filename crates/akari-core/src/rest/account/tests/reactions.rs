use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use wiremock::matchers::path_regex;

use super::*;
use crate::state::ReactionEmoji;

const MESSAGE: u64 = 400_000_000_000_000_003;
const BASE: &str = "/api/v9/channels/300000000000000002/messages/400000000000000003/reactions";
const HEART: &str = "❤\u{fe0f}";
const HEART_PATH: &str = "%E2%9D%A4%EF%B8%8F";

fn target() -> MessageId {
    Snowflake::new(MESSAGE)
}

fn unicode(name: &str) -> ReactionEmoji {
    ReactionEmoji {
        id: None,
        name: Some(name.into()),
        animated: false,
    }
}

fn custom(id: u64, name: Option<&str>, animated: bool) -> ReactionEmoji {
    ReactionEmoji {
        id: Some(Snowflake::new(id)),
        name: name.map(Into::into),
        animated,
    }
}

async fn answering(status: u16) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(path_regex("/reactions"))
        .respond_with(ResponseTemplate::new(status))
        .mount(&server)
        .await;
    server
}

// Method, path and query as they went over the wire.
async fn lines(server: &MockServer) -> Vec<String> {
    requests(server)
        .await
        .iter()
        .map(|request| {
            let query = request
                .url
                .query()
                .map(|query| format!("?{query}"))
                .unwrap_or_default();
            format!("{} {}{query}", request.method, request.url.path())
        })
        .collect()
}

#[test]
fn an_emoji_is_written_into_the_path_as_the_official_client_sends_it() {
    let cases = [
        (unicode(HEART), HEART_PATH),
        (unicode("❤"), "%E2%9D%A4"),
        (unicode("👍🏽"), "%F0%9F%91%8D%F0%9F%8F%BD"),
        (
            unicode("👨\u{200d}👩\u{200d}👧"),
            "%F0%9F%91%A8%E2%80%8D%F0%9F%91%A9%E2%80%8D%F0%9F%91%A7",
        ),
        (unicode("🇩🇪"), "%F0%9F%87%A9%F0%9F%87%AA"),
        (unicode("#\u{fe0f}\u{20e3}"), "%23%EF%B8%8F%E2%83%A3"),
        (custom(7, Some("blob_wave"), false), "blob_wave:7"),
        (custom(8, Some("party"), true), "party:8"),
        (custom(9, None, false), "null:9"),
    ];

    for (emoji, expected) in cases {
        assert_eq!(reaction_segment(&emoji), expected, "{emoji:?}");
    }
}

#[tokio::test]
async fn adding_a_reaction_puts_the_own_path_with_location_and_type() {
    let server = answering(204).await;
    let rest = rest(&server).await;

    rest.add_reaction(
        channel(),
        target(),
        &unicode(HEART),
        "Message%20Inline%20Button",
    )
    .await
    .unwrap();
    rest.add_reaction(
        channel(),
        target(),
        &custom(7, Some("wave"), true),
        "Message",
    )
    .await
    .unwrap();

    assert_eq!(
        lines(&server).await,
        [
            format!("PUT {BASE}/{HEART_PATH}/@me?location=Message%20Inline%20Button&type=0"),
            format!("PUT {BASE}/wave:7/@me?location=Message&type=0"),
        ]
    );
    assert!(requests(&server).await[0].body.is_empty(), "no body");
}

#[tokio::test]
async fn removing_a_reaction_names_its_type_before_the_user() {
    let server = answering(204).await;
    let rest = rest(&server).await;
    let mira = Snowflake::new(100_000_000_000_000_002);

    rest.remove_reaction(channel(), target(), &unicode(HEART), false, None, "Message")
        .await
        .unwrap();
    rest.remove_reaction(channel(), target(), &unicode(HEART), true, None, "Message")
        .await
        .unwrap();
    rest.remove_reaction(
        channel(),
        target(),
        &custom(7, Some("wave"), false),
        false,
        Some(mira),
        "Message",
    )
    .await
    .unwrap();
    rest.remove_reaction(
        channel(),
        target(),
        &unicode("👍"),
        true,
        Some(mira),
        "Message",
    )
    .await
    .unwrap();

    assert_eq!(
        lines(&server).await,
        [
            format!("DELETE {BASE}/{HEART_PATH}/0/@me?location=Message&burst=false"),
            format!("DELETE {BASE}/{HEART_PATH}/1/@me?location=Message&burst=true"),
            format!("DELETE {BASE}/wave:7/0/100000000000000002?location=Message&burst=false"),
            format!("DELETE {BASE}/%F0%9F%91%8D/1/100000000000000002?location=Message&burst=true"),
        ]
    );
}

#[tokio::test]
async fn removing_all_reactions_or_an_emojis_has_no_query() {
    let server = answering(204).await;
    let rest = rest(&server).await;

    rest.remove_reactions(channel(), target(), None)
        .await
        .unwrap();
    rest.remove_reactions(channel(), target(), Some(&unicode(HEART)))
        .await
        .unwrap();
    rest.remove_reactions(channel(), target(), Some(&custom(7, Some("wave"), false)))
        .await
        .unwrap();

    assert_eq!(
        lines(&server).await,
        [
            format!("DELETE {BASE}"),
            format!("DELETE {BASE}/{HEART_PATH}"),
            format!("DELETE {BASE}/wave:7"),
        ]
    );
}

fn user(id: u64, name: &str) -> Value {
    json!({"id": id.to_string(), "username": name, "discriminator": "0", "avatar": null})
}

#[tokio::test]
async fn reactors_are_asked_by_page_and_type() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path_regex("/reactions/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            user(100_000_000_000_000_002, "mira"),
            {"id": "broken"},
            user(100_000_000_000_000_003, "tom"),
        ])))
        .mount(&server)
        .await;
    let rest = rest(&server).await;

    let first = rest
        .reactors(channel(), target(), &unicode(HEART), false, None, 100)
        .await
        .unwrap();
    rest.reactors(
        channel(),
        target(),
        &custom(7, Some("wave"), false),
        true,
        Some(Snowflake::new(100_000_000_000_000_003)),
        3,
    )
    .await
    .unwrap();

    assert_eq!(
        lines(&server).await,
        [
            format!("GET {BASE}/{HEART_PATH}?limit=100&type=0"),
            format!("GET {BASE}/wave:7?limit=3&after=100000000000000003&type=1"),
        ]
    );
    let names: Vec<&str> = first
        .users
        .iter()
        .map(|user| user.username.as_str())
        .collect();
    assert_eq!(
        names,
        ["mira", "tom"],
        "a user that doesn't decode is skipped"
    );
    assert_eq!(first.received, 3, "but counts toward the page");
}

#[tokio::test]
async fn a_reaction_is_asked_once_more_after_a_429s_wait_and_never_a_third_time() {
    let server = MockServer::start().await;
    Mock::given(path_regex("/reactions"))
        .respond_with(rate_limited(Some(0.02)))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(path_regex("/reactions"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;
    let limited = MockServer::start().await;
    Mock::given(path_regex("/reactions"))
        .respond_with(rate_limited(Some(0.02)))
        .mount(&limited)
        .await;

    let recovered = rest(&server)
        .await
        .add_reaction(channel(), target(), &unicode(HEART), "Message")
        .await;
    let failed = rest(&limited)
        .await
        .add_reaction(channel(), target(), &unicode(HEART), "Message")
        .await;

    assert!(recovered.is_ok(), "{recovered:?}");
    assert_eq!(requests(&server).await.len(), 2);
    assert!(
        matches!(
            failed,
            Err(RequestError::RateLimited {
                retry_after: Some(_)
            })
        ),
        "{failed:?}"
    );
    assert_eq!(requests(&limited).await.len(), 2);
}

#[tokio::test]
async fn a_reaction_is_asked_once_more_after_a_502_or_504() {
    for status in [502, 504] {
        let server = MockServer::start().await;
        Mock::given(path_regex("/reactions"))
            .respond_with(ResponseTemplate::new(status))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(path_regex("/reactions"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;
        let failing = answering(status).await;

        let recovered = rest(&server)
            .await
            .remove_reaction(channel(), target(), &unicode(HEART), false, None, "Message")
            .await;
        let failed = rest(&failing)
            .await
            .remove_reactions(channel(), target(), None)
            .await;

        assert!(recovered.is_ok(), "{status}: {recovered:?}");
        assert!(
            matches!(failed, Err(RequestError::ServerError { .. })),
            "{status}: {failed:?}"
        );
        assert_eq!(requests(&failing).await.len(), 2, "{status}");
    }
}

#[tokio::test]
async fn a_refusal_is_not_asked_again() {
    for status in [400, 403, 404, 500] {
        let server = MockServer::start().await;
        Mock::given(path_regex("/reactions"))
            .respond_with(
                ResponseTemplate::new(status)
                    .set_body_json(json!({"message": "Missing Permissions", "code": 50013})),
            )
            .mount(&server)
            .await;

        let refused = rest(&server)
            .await
            .add_reaction(channel(), target(), &unicode(HEART), "Message")
            .await;

        assert!(refused.is_err(), "{status}");
        assert_eq!(requests(&server).await.len(), 1, "{status}");
    }
}

// Hangs up on the first `failures` connections, then answers 204.
async fn flaky(failures: usize) -> (String, Arc<std::sync::atomic::AtomicUsize>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let asked = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counter = asked.clone();
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            let mut head = vec![0; 8192];
            if stream.read(&mut head).await.unwrap_or(0) == 0 {
                continue;
            }
            let seen = counter.fetch_add(1, Ordering::SeqCst);
            if seen >= failures {
                let _ = stream
                    .write_all(b"HTTP/1.1 204 No Content\r\nconnection: close\r\n\r\n")
                    .await;
            }
        }
    });
    (format!("http://{address}/api/v9/"), asked)
}

fn rest_at(api: String) -> AccountRest {
    AccountRest::new(
        client_with("ws://127.0.0.1:9/".to_owned(), api),
        Token::new(TOKEN.to_owned()),
        Duration::from_millis(10),
    )
}

#[tokio::test]
async fn a_reaction_is_asked_once_more_after_a_connection_that_broke() {
    let (api, asked) = flaky(1).await;
    let (dead, dead_asked) = flaky(usize::MAX).await;

    let recovered = rest_at(api)
        .add_reaction(channel(), target(), &unicode(HEART), "Message")
        .await;
    let failed = rest_at(dead)
        .add_reaction(channel(), target(), &unicode(HEART), "Message")
        .await;

    assert!(recovered.is_ok(), "{recovered:?}");
    assert_eq!(asked.load(Ordering::SeqCst), 2);
    assert!(
        matches!(failed, Err(RequestError::Network(_))),
        "{failed:?}"
    );
    assert_eq!(dead_asked.load(Ordering::SeqCst), 2, "never a third time");
}

#[tokio::test]
async fn a_message_load_is_not_asked_again_after_a_connection_that_broke() {
    let (api, asked) = flaky(1).await;

    let failed = rest_at(api)
        .list_messages(channel(), Query::Latest, 3)
        .await;

    assert!(
        matches!(failed, Err(RequestError::Network(_))),
        "{failed:?}"
    );
    assert_eq!(asked.load(Ordering::SeqCst), 1);
}
