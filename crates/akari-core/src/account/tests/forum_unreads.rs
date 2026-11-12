use wiremock::MockServer;

use super::*;
use crate::gateway::FORUM_UNREADS_LIMIT;

// Op 28 (H-5): the open forum's active posts that are unread, opened and acked are asked for
// their new-message counts, each once a connection, at most 180 a frame.

const FORUM: u64 = 300_000_000_000_000_050;
const FIRST: u64 = 300_000_000_000_000_060;
const SECOND: u64 = 300_000_000_000_000_070;

fn forum() -> ChannelId {
    Snowflake::new(FORUM)
}

// Unread past the ack the READY carries for it: its newest message is its ID + 5, the ack + 2.
fn post(id: u64) -> Value {
    json!({
        "id": id.to_string(), "type": 11, "name": "a question", "guild_id": G1,
        "parent_id": FORUM.to_string(), "owner_id": "100000000000000002",
        "last_message_id": (id + 5).to_string(), "message_count": 8, "member_count": 1,
        "rate_limit_per_user": 0, "flags": 0, "applied_tags": [],
        "thread_metadata": {
            "archived": false, "auto_archive_duration": 1440,
            "archive_timestamp": "2024-03-02T08:30:00.250000+00:00", "locked": false
        }
    })
}

fn with_forum(ready: &mut Value, posts: &[u64], acked: &[u64]) {
    let guild = &mut ready["guilds"][0];
    guild["channels"].as_array_mut().unwrap().push(json!({
        "id": FORUM.to_string(), "type": 15, "name": "help", "position": 3, "flags": 0,
        "parent_id": "300000000000000001", "permission_overwrites": [],
    }));
    let threads = guild["threads"].as_array_mut().unwrap();
    threads.extend(posts.iter().copied().map(post));
    let entries = ready["read_state"]["entries"].as_array_mut().unwrap();
    entries.extend(
        acked
            .iter()
            .map(|id| json!({"id": id.to_string(), "last_message_id": (id + 2).to_string()})),
    );
}

struct Online {
    account: Account,
    subscription: Subscription,
    connection: FakeConnection,
    fake: FakeGateway,
    _server: MockServer,
}

async fn online_with(posts: &[u64], acked: &[u64]) -> Online {
    let mut fake = FakeGateway::start().await;
    let server = MockServer::start().await;
    let account = start_with(&fake, &server);
    let subscription = account.store().subscribe();
    account.connect().unwrap();
    let mut connection = fake.accept().await;
    connection.handshake(60_000).await;
    connection
        .send(ready_payload(1, &fake, |ready| {
            with_forum(ready, posts, acked);
        }))
        .await;
    events_until(&subscription, "Online").await;
    Online {
        account,
        subscription,
        connection,
        fake,
        _server: server,
    }
}

impl Online {
    // The next op 28's posts with their acks, other commands skipped.
    async fn asked(&mut self) -> Vec<(u64, u64)> {
        loop {
            let command = next_command(&mut self.connection)
                .await
                .expect("no op 28 arrived");
            if command["op"] != 28 {
                continue;
            }
            assert_eq!(command["d"]["guild_id"], G1);
            assert_eq!(command["d"]["channel_id"], FORUM.to_string());
            let id = |value: &Value| value.as_str().unwrap().parse::<u64>().unwrap();
            return command["d"]["threads"]
                .as_array()
                .unwrap()
                .iter()
                .map(|thread| (id(&thread["thread_id"]), id(&thread["ack_message_id"])))
                .collect();
        }
    }

    async fn counts_changed(&self) {
        let changed = format!("{:?}", StoreEvent::PostCountsChanged { forum_id: forum() });
        events_until(&self.subscription, &changed).await;
    }
}

#[tokio::test]
async fn opening_a_forum_asks_each_unread_opened_post_once() {
    let mut online = online_with(&[FIRST], &[FIRST, SECOND]).await;

    online.account.open_forum(Some(forum()));
    assert_eq!(online.asked().await, [(FIRST, FIRST + 2)]);

    online.account.open_forum(Some(forum()));
    online.account.refresh_forum(forum());
    // A sync brings a second post: only that one is asked, so the first was asked once.
    online
        .connection
        .send(json!({"op": 0, "s": 2, "t": "THREAD_LIST_SYNC", "d": {
            "guild_id": G1, "threads": [post(FIRST), post(SECOND)], "members": [],
        }}))
        .await;
    assert_eq!(online.asked().await, [(SECOND, SECOND + 2)]);
}

#[tokio::test]
async fn the_answer_shows_as_the_posts_count() {
    let mut online = online_with(&[FIRST], &[FIRST]).await;
    online.account.open_forum(Some(forum()));
    online.asked().await;

    online
        .connection
        .send(json!({"op": 0, "s": 2, "t": "FORUM_UNREADS", "d": {
            "channel_id": FORUM.to_string(),
            "threads": [{"thread_id": FIRST.to_string(), "missing": false, "count": 3}],
        }}))
        .await;
    online.counts_changed().await;

    let counts = online
        .account
        .store()
        .post_new_counts(&[Snowflake::new(FIRST)]);
    assert_eq!(counts.len(), 1);
    assert_eq!(counts[0].post.get(), FIRST);
    assert_eq!(counts[0].count, Some(3));
}

#[tokio::test]
async fn a_new_connection_asks_the_open_forums_posts_again() {
    let mut online = online_with(&[FIRST], &[FIRST]).await;
    online.account.open_forum(Some(forum()));
    assert_eq!(online.asked().await, [(FIRST, FIRST + 2)]);

    let again = ready_payload(2, &online.fake, |ready| {
        with_forum(ready, &[FIRST], &[FIRST]);
    });
    online.connection.send(again).await;

    assert_eq!(online.asked().await, [(FIRST, FIRST + 2)]);
}

#[tokio::test]
async fn a_frame_asks_180_posts_and_the_next_refresh_the_rest() {
    let posts: Vec<u64> = (0..200).map(|index| FIRST + index * 10).collect();
    let mut online = online_with(&posts, &posts).await;

    online.account.open_forum(Some(forum()));
    let first = online.asked().await;
    online.account.refresh_forum(forum());
    let rest = online.asked().await;

    assert_eq!(first.len(), FORUM_UNREADS_LIMIT);
    assert_eq!(rest.len(), posts.len() - FORUM_UNREADS_LIMIT);
    assert!(rest.iter().all(|post| !first.contains(post)));
}

// Nothing can be asked while the connection is down, so nothing counts as asked: the resumed
// session asks.
#[tokio::test]
async fn a_forum_opened_while_reconnecting_is_asked_once_resumed() {
    let mut online = online_with(&[FIRST], &[FIRST]).await;
    online.connection.close(4000).await;
    events_until(&online.subscription, "Connecting").await;

    online.account.open_forum(Some(forum()));
    online.connection = online.fake.accept().await;
    assert_eq!(online.connection.handshake(60_000).await["op"], 6);
    online.connection.dispatch(2, "RESUMED").await;

    assert_eq!(online.asked().await, [(FIRST, FIRST + 2)]);
}

// Not online, nothing is marked as asked, whatever a send would answer: the posts are still
// to ask the moment the forum was opened.
#[tokio::test]
async fn a_forum_opened_while_not_online_marks_nothing_asked() {
    let mut online = online_with(&[FIRST], &[FIRST]).await;
    online.connection.close(4000).await;
    events_until(&online.subscription, "Connecting").await;
    let post = (Snowflake::new(FIRST), Snowflake::new(FIRST + 2));

    online.account.open_forum(Some(forum()));
    let store = online.account.store();
    let waiting = store.next_forum_unreads().map(|ask| ask.threads);
    assert_eq!(waiting, Some(vec![post]), "still to ask");
    store.forget_forum_unreads(&[post]);

    online.connection = online.fake.accept().await;
    assert_eq!(online.connection.handshake(60_000).await["op"], 6);
    online.connection.dispatch(2, "RESUMED").await;
    assert_eq!(online.asked().await, [(FIRST, FIRST + 2)]);
}

// The store still says online when the gateway refuses the frame: the posts aren't left as
// asked, and the resumed session asks them.
#[tokio::test]
async fn a_frame_the_gateway_refuses_is_asked_again() {
    let mut online = online_with(&[FIRST], &[FIRST]).await;
    let gateway = &online.account.shared.gateway;

    gateway.disconnect();
    assert!(matches!(
        online.account.store().connection(),
        ConnectionState::Online
    ));
    online.account.open_forum(Some(forum()));
    online.connection.client_close_code().await;
    gateway.connect().unwrap();

    online.connection = online.fake.accept().await;
    assert_eq!(online.connection.handshake(60_000).await["op"], 6);
    online.connection.dispatch(2, "RESUMED").await;
    assert_eq!(online.asked().await, [(FIRST, FIRST + 2)]);
}
