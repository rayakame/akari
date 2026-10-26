use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

use super::*;
use crate::state::MediaCover;

const DM: u64 = 300_000_000_000_000_010;
const SCAN: &str = "/api/v9/channels/300000000000000010/explicit-media";
const STRANGER: u64 = 100_000_000_000_000_002;

fn dm_message(seq: u64, id: u64, scanned: bool) -> Value {
    let mut message: Value =
        serde_json::from_str(include_str!("../../../tests/fixtures/message_create.json")).unwrap();
    message["id"] = id.to_string().into();
    message["channel_id"] = DM.to_string().into();
    message["author"]["id"] = STRANGER.to_string().into();
    let fields = message.as_object_mut().unwrap();
    fields.remove("guild_id");
    fields.remove("member");
    let mut image = json!({
        "id": format!("6000000000000{id:05}"), "filename": "photo.png", "size": 1000,
        "url": "https://cdn.discordapp.com/attachments/1/2/photo.png",
        "proxy_url": "https://media.discordapp.net/attachments/1/2/photo.png",
        "width": 640, "height": 480,
    });
    if scanned {
        image["content_scan_version"] = 4.into();
    }
    message["attachments"] = json!([image]);
    json!({"op": 0, "s": seq, "t": "MESSAGE_CREATE", "d": message})
}

fn cover(account: &Account, id: u64) -> Option<MediaCover> {
    let store = account.store();
    let message = store
        .message(Snowflake::new(DM), Snowflake::new(id))
        .expect("the message");
    store.shown_attachments(&message)[0].cover
}

async fn scans(server: &MockServer) -> Vec<Request> {
    server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .filter(|request| request.url.path() == SCAN)
        .collect()
}

fn asked(request: &Request) -> Vec<String> {
    let body: Value = serde_json::from_slice(&request.body).unwrap();
    serde_json::from_value(body["message_ids"].clone()).unwrap()
}

// A stranger's unscanned image in a viewed DM is asked for once, PATCHed with its ID, and stops
// waiting after the wait (test timing) whether Discord answers or not.
#[tokio::test]
async fn unscanned_media_is_asked_for_once_and_stops_waiting() {
    let mut fake = FakeGateway::start().await;
    let server = MockServer::start().await;
    Mock::given(method("PATCH"))
        .and(path(SCAN))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;
    let account = start_with(&fake, &server);
    let subscription = account.store().subscribe();
    let mut connection = online(&mut fake, &account).await;
    events_until(&subscription, "Online").await;
    account.view_channel(Snowflake::new(DM));

    connection.send(dm_message(2, 10, false)).await;
    connection.send(dm_message(3, 11, true)).await;
    events_until(&subscription, "MessageInserted(11)").await;
    assert_eq!(cover(&account, 10), Some(MediaCover::Pending));

    events_until(&subscription, "MessageUpdated(10)").await;
    assert_eq!(cover(&account, 10), None);
    let requests = scans(&server).await;
    assert_eq!(requests.len(), 1, "once");
    assert_eq!(requests[0].method.as_str(), "PATCH");
    assert_eq!(asked(&requests[0]), ["10"]);
    assert!(requests[0].headers.get("authorization").is_some());
}

// The docs' limit: 1–75 message IDs per request; a page of 80 needs two.
#[tokio::test]
async fn a_page_of_messages_goes_in_chunks_of_75() {
    let mut fake = FakeGateway::start().await;
    let server = MockServer::start().await;
    Mock::given(method("PATCH"))
        .and(path(SCAN))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;
    let page: Vec<Value> = (0..80)
        .rev()
        .map(|index| dm_message(0, 100 + index, false)["d"].clone())
        .collect();
    Mock::given(method("GET"))
        .and(path("/api/v9/channels/300000000000000010/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_json(page))
        .mount(&server)
        .await;
    let account = start_with(&fake, &server);
    let subscription = account.store().subscribe();
    let _connection = online(&mut fake, &account).await;
    events_until(&subscription, "Online").await;

    account
        .load_messages(Snowflake::new(DM), MessageLoad::Latest { limit: 80 })
        .await
        .unwrap();
    events_until(&subscription, "MessageUpdated(179)").await;

    let mut sizes: Vec<usize> = scans(&server)
        .await
        .iter()
        .map(|request| asked(request).len())
        .collect();
    sizes.sort_unstable();
    assert_eq!(sizes, [5, 75]);
}

// A failed request still ends the wait: the media shows.
#[tokio::test]
async fn a_failed_scan_still_ends_the_wait() {
    let mut fake = FakeGateway::start().await;
    let server = MockServer::start().await;
    Mock::given(method("PATCH"))
        .and(path(SCAN))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;
    let account = start_with(&fake, &server);
    let subscription = account.store().subscribe();
    let mut connection = online(&mut fake, &account).await;
    events_until(&subscription, "Online").await;
    account.view_channel(Snowflake::new(DM));

    connection.send(dm_message(2, 10, false)).await;

    events_until(&subscription, "MessageUpdated(10)").await;
    assert_eq!(cover(&account, 10), None);
}
