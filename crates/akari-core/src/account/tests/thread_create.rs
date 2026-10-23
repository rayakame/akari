use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

use super::*;
use crate::{CreateThreadError, CreateThreadLocation};

const SOURCE: u64 = 400_000_000_000_000_005;
const FROM_SOURCE: &str = "/api/v9/channels/300000000000000002/messages/400000000000000005/threads";
const THREADS: &str = "/api/v9/channels/300000000000000002/threads";
const NEWS: u64 = 300_000_000_000_000_004;
const NEWS_THREADS: &str = "/api/v9/channels/300000000000000004/threads";
const NEW: u64 = 300_000_000_000_000_077;
const ROLE: &str = "600";
// View, send (in threads too) and read history, both thread kinds.
const MEMBER: u64 = (1 << 10) | (1 << 11) | (1 << 16) | (1 << 35) | (1 << 36) | (1 << 38);

struct Online {
    account: Account,
    server: MockServer,
    _subscription: Subscription,
    _connection: FakeConnection,
    _fake: FakeGateway,
}

async fn online_with(edit: impl FnOnce(&mut Value)) -> Online {
    let mut fake = FakeGateway::start().await;
    let server = MockServer::start().await;
    let account = start_with(&fake, &server);
    let subscription = account.store().subscribe();
    account.connect().unwrap();
    let mut connection = fake.accept().await;
    connection.handshake(60_000).await;
    connection
        .send(ready_payload(1, &fake, |ready| {
            ready["guilds"][0]["channels"].as_array_mut().unwrap().push(
                json!({"id": NEWS.to_string(), "type": 5, "name": "news", "position": 2,
                    "parent_id": "300000000000000001", "permission_overwrites": []}),
            );
            edit(ready);
        }))
        .await;
    events_until(&subscription, "Online").await;
    Online {
        account,
        server,
        _subscription: subscription,
        _connection: connection,
        _fake: fake,
    }
}

async fn owner() -> Online {
    online_with(|_| {}).await
}

// A plain member whose role allows `allow`, in a #general with `slowmode` seconds.
async fn member(allow: u64, slowmode: u32) -> Online {
    online_with(|ready| {
        let guild = &mut ready["guilds"][0];
        guild["properties"]["owner_id"] = "100000000000000099".into();
        guild["roles"][0]["permissions"] = "0".into();
        guild["roles"].as_array_mut().unwrap().push(
            json!({"id": ROLE, "name": "r", "permissions": allow.to_string(), "position": 2}),
        );
        guild["channels"][1]["rate_limit_per_user"] = slowmode.into();
        ready["merged_members"][0][0]["roles"] = json!([ROLE]);
    })
    .await
}

fn created(id: u64, parent: u64, kind: u8) -> Value {
    let mut thread: Value =
        serde_json::from_str(include_str!("../../../tests/fixtures/thread_create.json")).unwrap();
    thread["id"] = id.to_string().into();
    thread["parent_id"] = parent.to_string().into();
    thread["type"] = kind.into();
    thread["owner_id"] = "100000000000000001".into();
    thread["member"]["id"] = id.to_string().into();
    thread.as_object_mut().unwrap().remove("newly_created");
    thread
}

fn refusal(status: u16, body: Value) -> ResponseTemplate {
    ResponseTemplate::new(status).set_body_json(body)
}

impl Online {
    async fn answer(&self, route: &str, response: ResponseTemplate) {
        Mock::given(method("POST"))
            .and(path(route))
            .respond_with(response)
            .mount(&self.server)
            .await;
    }

    async fn with_source(&self, message: Value) {
        mock_page(&self.server, ("limit", "50"), Value::Array(vec![message])).await;
        self.account
            .load_messages(general(), MessageLoad::Latest { limit: 50 })
            .await
            .unwrap();
    }

    async fn requests(&self) -> Vec<Request> {
        self.server
            .received_requests()
            .await
            .unwrap()
            .into_iter()
            .filter(|request| request.method.as_str() == "POST")
            .collect()
    }

    async fn bodies(&self) -> Vec<String> {
        self.requests()
            .await
            .iter()
            .map(|request| String::from_utf8(request.body.clone()).unwrap())
            .collect()
    }

    async fn create(
        &self,
        parent: u64,
        message: Option<u64>,
        name: &str,
        private: bool,
        location: CreateThreadLocation,
    ) -> Result<ChannelId, CreateThreadError> {
        self.account
            .create_thread(
                Snowflake::new(parent),
                message.map(Snowflake::new),
                name,
                private,
                location,
            )
            .await
    }

    fn cooldown(&self) -> Option<SystemTime> {
        self.account.store().thread_slowmode(general())?.until
    }
}

fn source(flags: u64) -> Value {
    let mut message = message(0, SOURCE)["d"].clone();
    message.as_object_mut().unwrap().remove("nonce");
    message["flags"] = flags.into();
    message
}

#[tokio::test]
async fn a_thread_from_a_message_posts_the_official_body() {
    let online = owner().await;
    online.with_source(source(0)).await;
    online
        .answer(
            FROM_SOURCE,
            ResponseTemplate::new(201).set_body_json(created(SOURCE, GENERAL, 11)),
        )
        .await;

    let thread = online
        .create(
            GENERAL,
            Some(SOURCE),
            " das ist ein satz ",
            false,
            CreateThreadLocation::Message,
        )
        .await
        .unwrap();

    assert_eq!(thread, Snowflake::new(SOURCE));
    assert_eq!(
        online.bodies().await,
        [
            r#"{"name":"das ist ein satz","type":11,"auto_archive_duration":4320,"location":"Message"}"#
        ]
    );
}

#[tokio::test]
async fn a_private_thread_posts_type_12_to_the_channels_threads() {
    let online = owner().await;
    online
        .answer(
            THREADS,
            ResponseTemplate::new(201).set_body_json(created(NEW, GENERAL, 12)),
        )
        .await;

    online
        .create(
            GENERAL,
            None,
            "f4",
            true,
            CreateThreadLocation::ThreadBrowserToolbar,
        )
        .await
        .unwrap();

    assert_eq!(
        online.bodies().await,
        [
            r#"{"name":"f4","type":12,"auto_archive_duration":4320,"location":"Thread Browser Toolbar"}"#
        ]
    );
}

#[tokio::test]
async fn each_location_has_the_official_words() {
    let online = owner().await;
    online
        .answer(
            THREADS,
            ResponseTemplate::new(201).set_body_json(created(NEW, GENERAL, 11)),
        )
        .await;

    for location in [
        CreateThreadLocation::ThreadBrowserEmptyState,
        CreateThreadLocation::KeyboardShortcut,
    ] {
        online
            .create(GENERAL, None, "t", false, location)
            .await
            .unwrap();
    }

    assert_eq!(
        online.bodies().await,
        [
            r#"{"name":"t","type":11,"auto_archive_duration":4320,"location":"Thread Browser Empty State"}"#,
            r#"{"name":"t","type":11,"auto_archive_duration":4320,"location":"Keyboard Shortcut"}"#,
        ]
    );
}

#[tokio::test]
async fn an_announcement_channels_threads_are_type_10_and_never_private() {
    let online = owner().await;
    online
        .answer(
            NEWS_THREADS,
            ResponseTemplate::new(201).set_body_json(created(NEW, NEWS, 10)),
        )
        .await;

    online
        .create(
            NEWS,
            None,
            "news",
            false,
            CreateThreadLocation::KeyboardShortcut,
        )
        .await
        .unwrap();
    let private = online
        .create(
            NEWS,
            None,
            "news",
            true,
            CreateThreadLocation::KeyboardShortcut,
        )
        .await;

    assert!(matches!(
        private,
        Err(CreateThreadError::Request(RequestError::InvalidRequest))
    ));
    assert_eq!(
        online.bodies().await,
        [
            r#"{"name":"news","type":10,"auto_archive_duration":4320,"location":"Keyboard Shortcut"}"#
        ]
    );
}

#[tokio::test]
async fn the_parents_default_archive_duration_is_sent() {
    let online = online_with(|ready| {
        ready["guilds"][0]["channels"][1]["default_auto_archive_duration"] = 1440.into();
    })
    .await;
    online
        .answer(
            THREADS,
            ResponseTemplate::new(201).set_body_json(created(NEW, GENERAL, 11)),
        )
        .await;

    online
        .create(
            GENERAL,
            None,
            "t",
            false,
            CreateThreadLocation::KeyboardShortcut,
        )
        .await
        .unwrap();

    assert!(online.bodies().await[0].contains(r#""auto_archive_duration":1440"#));
}

#[tokio::test]
async fn the_answer_puts_the_thread_in_the_store_joined() {
    let online = owner().await;
    online
        .answer(
            THREADS,
            ResponseTemplate::new(201).set_body_json(created(NEW, GENERAL, 11)),
        )
        .await;

    let thread = online
        .create(
            GENERAL,
            None,
            "t",
            false,
            CreateThreadLocation::KeyboardShortcut,
        )
        .await
        .unwrap();

    let stored = online.account.store().channel(thread).expect("kept");
    assert_eq!(stored.parent_id, Some(general()));
    assert!(stored.thread.as_deref().unwrap().joined());
}

#[tokio::test]
async fn what_discord_would_refuse_is_refused_before_asking() {
    let online = owner().await;
    online.with_source(source(1 << 5)).await;
    let refused = |result: Result<ChannelId, CreateThreadError>| {
        matches!(
            result,
            Err(CreateThreadError::Request(RequestError::InvalidRequest))
        )
    };
    let location = CreateThreadLocation::Message;

    assert!(refused(
        online.create(GENERAL, None, "  \n", false, location).await
    ));
    assert!(refused(
        online
            .create(GENERAL, None, &"x".repeat(101), false, location)
            .await
    ));
    // Already has a thread; a thread from a message is never private.
    assert!(refused(
        online
            .create(GENERAL, Some(SOURCE), "t", false, location)
            .await
    ));
    assert!(refused(
        online
            .create(GENERAL, Some(SOURCE), "t", true, location)
            .await
    ));
    assert!(refused(
        online.create(VOICE, None, "t", false, location).await
    ));
    assert!(online.requests().await.is_empty());
}

#[tokio::test]
async fn a_hundred_code_points_are_a_valid_name() {
    let online = owner().await;
    online
        .answer(
            THREADS,
            ResponseTemplate::new(201).set_body_json(created(NEW, GENERAL, 11)),
        )
        .await;

    let name = "é".repeat(100);
    online
        .create(
            GENERAL,
            None,
            &name,
            false,
            CreateThreadLocation::KeyboardShortcut,
        )
        .await
        .unwrap();

    assert_eq!(online.requests().await.len(), 1);
}

#[tokio::test]
async fn without_the_permission_nothing_is_asked() {
    let online = member((1 << 10) | (1 << 11) | (1 << 16), 0).await;

    let public = online
        .create(
            GENERAL,
            None,
            "t",
            false,
            CreateThreadLocation::KeyboardShortcut,
        )
        .await;
    let private = online
        .create(
            GENERAL,
            None,
            "t",
            true,
            CreateThreadLocation::KeyboardShortcut,
        )
        .await;

    assert!(matches!(
        public,
        Err(CreateThreadError::Request(RequestError::InvalidRequest))
    ));
    assert!(matches!(
        private,
        Err(CreateThreadError::Request(RequestError::InvalidRequest))
    ));
    assert!(online.requests().await.is_empty());
}

#[tokio::test]
async fn a_created_thread_starts_the_cooldown_and_the_next_waits_for_it() {
    let online = member(MEMBER, 30).await;
    online
        .answer(
            THREADS,
            ResponseTemplate::new(201).set_body_json(created(NEW, GENERAL, 11)),
        )
        .await;
    assert_eq!(online.cooldown(), None);

    online
        .create(
            GENERAL,
            None,
            "first",
            false,
            CreateThreadLocation::KeyboardShortcut,
        )
        .await
        .unwrap();
    let next = online
        .create(
            GENERAL,
            None,
            "second",
            false,
            CreateThreadLocation::KeyboardShortcut,
        )
        .await;

    assert!(matches!(next, Err(CreateThreadError::Slowmode)));
    assert!(online.cooldown().is_some());
    assert_eq!(online.requests().await.len(), 1);
}

#[tokio::test]
async fn discords_slowmode_answer_holds_the_cooldown_without_a_retry() {
    let online = member(MEMBER, 30).await;
    online
        .answer(
            THREADS,
            refusal(
                429,
                json!({"message": "You are being rate limited.", "code": 20016,
                    "retry_after": 5.0, "global": false}),
            ),
        )
        .await;
    let before = SystemTime::now();

    let created = online
        .create(
            GENERAL,
            None,
            "t",
            false,
            CreateThreadLocation::KeyboardShortcut,
        )
        .await;

    assert!(matches!(created, Err(CreateThreadError::Slowmode)));
    assert_eq!(online.requests().await.len(), 1);
    let until = online.cooldown().expect("held");
    assert!(until > before + Duration::from_secs(4), "{until:?}");
    assert!(
        until < SystemTime::now() + Duration::from_secs(6),
        "{until:?}"
    );
}

#[tokio::test]
async fn another_rate_limit_is_a_request_error() {
    let online = owner().await;
    online
        .answer(
            THREADS,
            refusal(429, json!({"message": "You are being rate limited.", "retry_after": 60.0, "global": false})),
        )
        .await;

    let created = online
        .create(
            GENERAL,
            None,
            "t",
            false,
            CreateThreadLocation::KeyboardShortcut,
        )
        .await;

    assert!(matches!(
        created,
        Err(CreateThreadError::Request(RequestError::RateLimited { .. }))
    ));
    assert_eq!(online.cooldown(), None);
}

#[tokio::test]
async fn discords_refusals_say_what_was_refused() {
    let cases = [
        (
            400,
            json!({"code": 160_006, "message": "Maximum number of active threads reached"}),
        ),
        (
            400,
            json!({"code": 160_007, "message": "Maximum number of active announcement threads reached"}),
        ),
        (
            400,
            json!({"code": 200_001, "message": "This name isn't allowed here."}),
        ),
        (
            400,
            json!({"code": 50035, "message": "Invalid Form Body", "errors":
                {"name": {"_errors": [{"code": "BASE_TYPE_REQUIRED", "message": "This field is required"}]}}}),
        ),
        (
            400,
            json!({"code": 200_000, "message": "Blocked by AutoMod."}),
        ),
        (
            403,
            json!({"code": 50013, "message": "Missing Permissions"}),
        ),
    ];
    let mut outcomes = Vec::new();
    for (status, body) in cases {
        let online = owner().await;
        online.answer(THREADS, refusal(status, body)).await;
        outcomes.push(
            online
                .create(
                    GENERAL,
                    None,
                    "t",
                    false,
                    CreateThreadLocation::KeyboardShortcut,
                )
                .await
                .unwrap_err(),
        );
    }

    assert!(matches!(
        outcomes[0],
        CreateThreadError::TooManyThreads {
            announcement: false
        }
    ));
    assert!(matches!(
        outcomes[1],
        CreateThreadError::TooManyThreads { announcement: true }
    ));
    assert!(
        matches!(&outcomes[2], CreateThreadError::NameBlocked { reason } if reason == "This name isn't allowed here.")
    );
    assert!(matches!(outcomes[3], CreateThreadError::NameRequired));
    // The official client shows nothing for 200000.
    assert!(
        matches!(&outcomes[4], CreateThreadError::NameBlocked { reason } if reason == "Blocked by AutoMod.")
    );
    assert!(matches!(
        outcomes[5],
        CreateThreadError::Request(RequestError::Discord { code: 50013, .. })
    ));
}

#[tokio::test]
async fn a_captcha_is_a_request_error() {
    let online = owner().await;
    online
        .answer(
            THREADS,
            refusal(
                400,
                json!({"captcha_key": ["captcha-required"], "captcha_service": "hcaptcha",
                "captcha_sitekey": "site"}),
            ),
        )
        .await;

    let created = online
        .create(
            GENERAL,
            None,
            "t",
            false,
            CreateThreadLocation::KeyboardShortcut,
        )
        .await;

    assert!(matches!(
        created,
        Err(CreateThreadError::Request(RequestError::CaptchaRequired(_)))
    ));
}

#[tokio::test]
async fn a_refusal_starts_no_cooldown() {
    let online = member(MEMBER, 30).await;
    online
        .answer(
            THREADS,
            refusal(
                403,
                json!({"code": 50013, "message": "Missing Permissions"}),
            ),
        )
        .await;

    let _ = online
        .create(
            GENERAL,
            None,
            "t",
            false,
            CreateThreadLocation::KeyboardShortcut,
        )
        .await;

    assert_eq!(online.cooldown(), None);
}

// A message the window let go of meanwhile (a jump, a trim) is Discord's to judge.
#[tokio::test]
async fn a_message_no_longer_loaded_is_asked_about() {
    let online = owner().await;
    let route = "/api/v9/channels/300000000000000002/messages/400000000000000006/threads";
    online
        .answer(
            route,
            ResponseTemplate::new(201).set_body_json(created(SOURCE + 1, GENERAL, 11)),
        )
        .await;

    let thread = online
        .create(
            GENERAL,
            Some(SOURCE + 1),
            "t",
            false,
            CreateThreadLocation::Message,
        )
        .await
        .unwrap();

    assert_eq!(thread, Snowflake::new(SOURCE + 1));
}

#[tokio::test]
async fn a_server_error_isnt_retried_so_nothing_is_created_twice() {
    let online = member(MEMBER, 30).await;
    online.answer(THREADS, ResponseTemplate::new(502)).await;

    let created = online
        .create(
            GENERAL,
            None,
            "t",
            false,
            CreateThreadLocation::KeyboardShortcut,
        )
        .await;

    // It may have been created behind the error: the cooldown starts as if it was.
    assert!(matches!(created, Err(CreateThreadError::MaybeCreated)));
    assert_eq!(online.requests().await.len(), 1);
    assert!(online.cooldown().is_some());
}

#[tokio::test]
async fn an_answer_that_doesnt_read_as_a_thread_may_have_created_it() {
    let mut text_channel = created(NEW, GENERAL, 11);
    text_channel["type"] = 0.into();
    for answer in [
        ResponseTemplate::new(201).set_body_string("not json"),
        ResponseTemplate::new(201).set_body_json(text_channel),
    ] {
        let online = member(MEMBER, 30).await;
        online.answer(THREADS, answer).await;

        let created = online
            .create(
                GENERAL,
                None,
                "t",
                false,
                CreateThreadLocation::KeyboardShortcut,
            )
            .await;

        assert!(matches!(created, Err(CreateThreadError::MaybeCreated)));
        assert!(online.cooldown().is_some());
    }
}

#[test]
fn only_failures_after_the_request_left_may_have_created_it() {
    use crate::error::{TransportError, TransportErrorKind};
    let network = |kind| RequestError::Network(TransportError::new(kind, "test"));

    assert!(super::super::maybe_created(&RequestError::ServerError {
        status: 500
    }));
    assert!(super::super::maybe_created(&network(
        TransportErrorKind::Timeout
    )));
    assert!(super::super::maybe_created(&network(
        TransportErrorKind::Protocol
    )));
    assert!(!super::super::maybe_created(&network(
        TransportErrorKind::Connect
    )));
    assert!(!super::super::maybe_created(&network(
        TransportErrorKind::Tls
    )));
    assert!(!super::super::maybe_created(&RequestError::RateLimited {
        retry_after: None
    }));
    assert!(!super::super::maybe_created(&RequestError::Discord {
        status: 403,
        code: 50013,
        message: String::new()
    }));
}
