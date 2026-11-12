use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

use super::*;
use crate::CreateThreadError;
use crate::model::TagId;
use crate::state::{ForumView, PostPreview};

// Creating a forum post (H-7 to H-9): the official client's one request, its refusals, the
// forum's cooldown and the answer as the user's own new post.

const FORUM: u64 = 300_000_000_000_000_050;
const POSTS: &str = "/api/v9/channels/300000000000000050/threads";
const NEW: u64 = 300_000_000_000_000_077;
const BUG: u64 = 500_000_000_000_000_001;
const IDEA: u64 = 500_000_000_000_000_002;
const STAFF: u64 = 500_000_000_000_000_003;
const ROLE: &str = "600";
const VIEW: u64 = 1 << 10;
const SEND: u64 = 1 << 11;
const REQUIRE_TAG: u64 = 1 << 4;

fn forum() -> ChannelId {
    Snowflake::new(FORUM)
}

fn tags(ids: &[u64]) -> Vec<TagId> {
    ids.iter().copied().map(TagId::new).collect()
}

struct Online {
    account: Account,
    server: MockServer,
    _subscription: Subscription,
    _connection: FakeConnection,
    _fake: FakeGateway,
}

// Online with a forum in the fixture's guild, changed by `edit`.
async fn online_with(edit: impl FnOnce(&mut Value, &mut Value)) -> Online {
    let mut fake = FakeGateway::start().await;
    let server = MockServer::start().await;
    let account = start_with(&fake, &server);
    let subscription = account.store().subscribe();
    account.connect().unwrap();
    let mut connection = fake.accept().await;
    connection.handshake(60_000).await;
    connection
        .send(ready_payload(1, &fake, |ready| {
            let mut forum = json!({
                "id": FORUM.to_string(), "type": 15, "name": "help", "position": 3, "flags": 0,
                "parent_id": "300000000000000001", "permission_overwrites": [],
                "available_tags": [
                    {"id": BUG.to_string(), "name": "Bug", "moderated": false},
                    {"id": IDEA.to_string(), "name": "Idea", "moderated": false},
                    {"id": STAFF.to_string(), "name": "Staff", "moderated": true},
                ],
            });
            edit(ready, &mut forum);
            ready["guilds"][0]["channels"]
                .as_array_mut()
                .unwrap()
                .push(forum);
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
    online_with(|_, _| {}).await
}

// A plain member whose role allows `allow`, in a forum with `slowmode` seconds between posts.
async fn member(allow: u64, slowmode: u32) -> Online {
    online_with(|ready, forum| {
        let guild = &mut ready["guilds"][0];
        guild["properties"]["owner_id"] = "100000000000000099".into();
        guild["roles"][0]["permissions"] = "0".into();
        guild["roles"].as_array_mut().unwrap().push(
            json!({"id": ROLE, "name": "r", "permissions": allow.to_string(), "position": 2}),
        );
        ready["merged_members"][0][0]["roles"] = json!([ROLE]);
        forum["rate_limit_per_user"] = slowmode.into();
    })
    .await
}

// Discord's answer with nested fields: the post and its first message.
fn created(id: u64) -> Value {
    let mut post: Value =
        serde_json::from_str(include_str!("../../../tests/fixtures/thread_create.json")).unwrap();
    post["id"] = id.to_string().into();
    post["parent_id"] = FORUM.to_string().into();
    post["owner_id"] = "100000000000000001".into();
    post["member"]["id"] = id.to_string().into();
    post["applied_tags"] = json!([]);
    post.as_object_mut().unwrap().remove("newly_created");
    let mut first = message(0, id)["d"].clone();
    first.as_object_mut().unwrap().remove("nonce");
    first["channel_id"] = id.to_string().into();
    first["author"]["id"] = "100000000000000001".into();
    first["content"] = "steps to reproduce".into();
    post["message"] = first;
    post
}

fn refusal(status: u16, body: Value) -> ResponseTemplate {
    ResponseTemplate::new(status).set_body_json(body)
}

impl Online {
    async fn answer(&self, response: ResponseTemplate) {
        Mock::given(method("POST"))
            .and(path(POSTS))
            .respond_with(response)
            .mount(&self.server)
            .await;
    }

    async fn creates(&self) {
        self.answer(ResponseTemplate::new(201).set_body_json(created(NEW)))
            .await;
    }

    async fn requests(&self) -> Vec<Request> {
        self.server
            .received_requests()
            .await
            .unwrap()
            .into_iter()
            .filter(|request| request.url.path() == POSTS)
            .collect()
    }

    async fn bodies(&self) -> Vec<Value> {
        self.requests()
            .await
            .iter()
            .map(|request| serde_json::from_slice(&request.body).unwrap())
            .collect()
    }

    async fn post(
        &self,
        name: &str,
        tags: &[TagId],
        content: &str,
    ) -> Result<ChannelId, CreateThreadError> {
        self.account.create_post(forum(), name, tags, content).await
    }

    fn cooldown(&self) -> Option<SystemTime> {
        self.account.store().thread_slowmode(forum())?.until
    }
}

// Module 971389: one request with nested fields, the tags always present, no type, location or
// nonce.
#[tokio::test]
async fn a_post_is_one_request_as_the_official_client_sends_it() {
    let online = owner().await;
    online.creates().await;

    let post = online
        .post(" How do I…? ", &[], "\n steps to reproduce \n")
        .await
        .unwrap();

    assert_eq!(post, Snowflake::new(NEW));
    let requests = online.requests().await;
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method.as_str(), "POST");
    assert_eq!(requests[0].url.query(), Some("use_nested_fields=true"));
    assert_eq!(
        String::from_utf8(requests[0].body.clone()).unwrap(),
        r#"{"name":"How do I…?","auto_archive_duration":4320,"applied_tags":[],"message":{"content":"steps to reproduce"}}"#
    );
}

#[tokio::test]
async fn the_tags_go_in_the_order_chosen_and_the_forums_archive_time() {
    let online = online_with(|_, forum| {
        forum["default_auto_archive_duration"] = 1440.into();
    })
    .await;
    online.creates().await;

    online.post("t", &tags(&[STAFF, BUG]), "m").await.unwrap();

    assert_eq!(
        online.bodies().await,
        [json!({
            "name": "t",
            "auto_archive_duration": 1440,
            "applied_tags": [STAFF.to_string(), BUG.to_string()],
            "message": {"content": "m"},
        })]
    );
}

#[tokio::test]
async fn what_discord_would_refuse_is_refused_before_asking() {
    let online = member(VIEW | SEND, 0).await;
    online.creates().await;
    let long_name = "n".repeat(101);
    let long_message = "m".repeat(online.account.store().message_length_limit() + 1);
    let six = tags(&[BUG, IDEA, BUG, IDEA, BUG, IDEA]);

    let refused = [
        online.post("  ", &[], "m").await,
        online.post(&long_name, &[], "m").await,
        online.post("t", &[], " \n ").await,
        online.post("t", &[], &long_message).await,
        online.post("t", &six, "m").await,
        online.post("t", &tags(&[42]), "m").await,
        online.post("t", &tags(&[STAFF]), "m").await,
        online.account.create_post(general(), "t", &[], "m").await,
        online
            .account
            .create_post(Snowflake::new(1), "t", &[], "m")
            .await,
    ];

    for (index, outcome) in refused.iter().enumerate() {
        assert!(
            matches!(
                outcome,
                Err(CreateThreadError::Request(RequestError::InvalidRequest))
            ),
            "{index}: {outcome:?}"
        );
    }
    assert!(online.requests().await.is_empty());

    online
        .post(&"n".repeat(100), &tags(&[BUG, IDEA]), "m")
        .await
        .unwrap();
}

#[tokio::test]
async fn without_send_messages_nothing_is_asked() {
    let online = member(VIEW, 0).await;
    online.creates().await;

    let created = online.post("t", &[], "m").await;

    assert!(matches!(
        created,
        Err(CreateThreadError::Request(RequestError::InvalidRequest))
    ));
    assert!(online.requests().await.is_empty());
}

#[tokio::test]
async fn a_forum_that_requires_a_tag_refuses_a_post_without_one() {
    let online = online_with(|_, forum| {
        forum["flags"] = REQUIRE_TAG.into();
    })
    .await;
    online.creates().await;

    let untagged = online.post("t", &[], "m").await;
    assert!(matches!(untagged, Err(CreateThreadError::TagRequired)));
    assert!(online.requests().await.is_empty());

    online.post("t", &tags(&[IDEA]), "m").await.unwrap();
}

#[tokio::test]
async fn a_created_post_starts_the_forums_cooldown_and_the_next_waits_for_it() {
    let online = member(VIEW | SEND, 30).await;
    online.creates().await;
    assert_eq!(online.cooldown(), None);
    let before = SystemTime::now();

    online.post("first", &[], "m").await.unwrap();
    let next = online.post("second", &[], "m").await;

    assert!(matches!(next, Err(CreateThreadError::Slowmode)));
    let until = online.cooldown().expect("the cooldown runs");
    assert!(until > before + Duration::from_secs(29), "{until:?}");
    assert!(
        until < SystemTime::now() + Duration::from_secs(31),
        "{until:?}"
    );
    assert_eq!(online.requests().await.len(), 1);
}

#[tokio::test]
async fn discords_slowmode_answer_holds_the_cooldown_without_a_retry() {
    let online = member(VIEW | SEND, 30).await;
    online
        .answer(refusal(
            429,
            json!({"message": "You are being rate limited.", "code": 20016,
                "retry_after": 5.0, "global": false}),
        ))
        .await;
    let before = SystemTime::now();

    let created = online.post("t", &[], "m").await;

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
async fn discords_refusals_say_what_was_refused() {
    let cases = [
        (
            400,
            json!({"code": 160_006, "message": "Maximum number of active threads reached"}),
        ),
        (
            400,
            json!({"code": 200_001, "message": "This title isn't allowed here."}),
        ),
        (
            400,
            json!({"code": 200_000, "message": "This message isn't allowed here."}),
        ),
        (
            400,
            json!({"code": 50035, "message": "Invalid Form Body", "errors":
                {"name": {"_errors": [{"code": "BASE_TYPE_REQUIRED", "message": "This field is required"}]}}}),
        ),
        (
            403,
            json!({"code": 50013, "message": "Missing Permissions"}),
        ),
        (
            429,
            json!({"message": "You are being rate limited.", "retry_after": 60.0, "global": false}),
        ),
    ];
    let mut outcomes = Vec::new();
    for (status, body) in cases {
        let online = member(VIEW | SEND, 30).await;
        online.answer(refusal(status, body)).await;
        outcomes.push(online.post("t", &[], "m").await.unwrap_err());
        assert_eq!(online.cooldown(), None, "a refusal starts no cooldown");
    }

    assert!(matches!(
        outcomes[0],
        CreateThreadError::TooManyThreads {
            announcement: false
        }
    ));
    assert!(
        matches!(&outcomes[1], CreateThreadError::NameBlocked { reason } if reason == "This title isn't allowed here.")
    );
    assert!(
        matches!(&outcomes[2], CreateThreadError::MessageBlocked { reason } if reason == "This message isn't allowed here.")
    );
    assert!(matches!(outcomes[3], CreateThreadError::NameRequired));
    assert!(matches!(
        outcomes[4],
        CreateThreadError::Request(RequestError::Discord { code: 50013, .. })
    ));
    assert!(matches!(
        outcomes[5],
        CreateThreadError::Request(RequestError::RateLimited { .. })
    ));
}

#[tokio::test]
async fn an_answer_that_doesnt_read_as_a_post_may_have_created_it() {
    let online = member(VIEW | SEND, 30).await;
    online
        .answer(ResponseTemplate::new(201).set_body_string("not json"))
        .await;

    let created = online.post("t", &[], "m").await;

    assert!(matches!(created, Err(CreateThreadError::MaybeCreated)));
    assert!(online.cooldown().is_some());
}

// The answer is the user's own new post: in the open forum's list at once, opened, the forum
// acked up to it, and its message is the card's.
#[tokio::test]
async fn the_answer_is_the_users_new_post_with_its_first_message() {
    let online = owner().await;
    online.creates().await;
    online.account.open_forum(Some(forum()));

    let post = online.post("t", &[], "steps to reproduce").await.unwrap();

    let store = online.account.store();
    let listed = store.forum_posts(forum(), &ForumView::default());
    assert!(listed.active.iter().any(|listed| listed.id == post));
    assert_eq!(listed.new_posts, 0);
    assert!(
        store
            .channel(post)
            .is_some_and(|post| { post.thread.as_deref().is_some_and(|info| info.joined()) })
    );
    assert!(store.read_state(post).is_some_and(|read| !read.estimated));
    assert_eq!(
        store.read_state(forum()).and_then(|read| read.acked),
        Some(post.cast())
    );
    assert!(store.new_posts(&[post]).is_empty());
    match store.post_preview(post) {
        Some(PostPreview::Loaded(first)) => assert_eq!(&*first.content, "steps to reproduce"),
        other => panic!("no first message: {other:?}"),
    }
}

// Discord may answer without the message: the post is still kept.
#[tokio::test]
async fn an_answer_without_its_message_is_still_the_post() {
    let online = owner().await;
    let mut bare = created(NEW);
    bare["message"] = json!({"id": "not a message"});
    online
        .answer(ResponseTemplate::new(201).set_body_json(bare))
        .await;
    online.account.open_forum(Some(forum()));

    let post = online.post("t", &[], "m").await.unwrap();

    assert_eq!(online.account.store().post_preview(post), None);
}
