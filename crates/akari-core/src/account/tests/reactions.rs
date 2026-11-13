use wiremock::matchers::{method, path_regex};
use wiremock::{Mock, MockServer, Request as Received, Respond, ResponseTemplate};

use super::*;
use crate::state::ReactionEmoji;
use crate::{ReactionError, ReactionKind, ReactionLocation};

const TARGET: u64 = 400_000_000_000_000_003;
const MIRA: u64 = 100_000_000_000_000_002;
const TOM: u64 = 100_000_000_000_000_003;
const DM: u64 = 300_000_000_000_000_010;
const THREAD: u64 = 300_000_000_000_000_020;
const STARTER_ROW: u64 = 400_000_000_000_000_031;
const HEART: &str = "❤\u{fe0f}";
const HEART_PATH: &str = "%E2%9D%A4%EF%B8%8F";
const UP: &str = "👍";
const BASE: &str = "/api/v9/channels/300000000000000002/messages/400000000000000003/reactions";
const REACTIONS: &str = "/reactions";
const INLINE: ReactionLocation = ReactionLocation::InlineButton;

type Shown = Vec<(String, u32, u32, bool, bool)>;
type Expected = fn(&ReactionError) -> bool;

fn target() -> MessageId {
    Snowflake::new(TARGET)
}

fn heart() -> ReactionEmoji {
    ReactionEmoji::unicode(HEART)
}

fn one(emoji: &str, normal: u32, me: bool) -> Shown {
    vec![(emoji.to_owned(), normal, 0, me, false)]
}

struct Online {
    account: Account,
    subscription: Subscription,
    server: MockServer,
    connection: FakeConnection,
    seq: u64,
    _fake: FakeGateway,
}

// Online with #general viewed and Mira's message in its window.
async fn reacting() -> Online {
    let mut fake = FakeGateway::start().await;
    let server = MockServer::start().await;
    let (account, subscription, mut connection) = sending(&mut fake, &server).await;
    connection.send(message(2, TARGET)).await;
    events_until(&subscription, &format!("MessageInserted({TARGET})")).await;
    Online {
        account,
        subscription,
        server,
        connection,
        seq: 2,
        _fake: fake,
    }
}

impl Online {
    async fn answer(&self, status: u16) {
        self.answer_with(ResponseTemplate::new(status)).await;
    }

    async fn answer_with(&self, answer: impl Respond + 'static) {
        Mock::given(path_regex(REACTIONS))
            .respond_with(answer)
            .mount(&self.server)
            .await;
    }

    async fn answer_once(&self, answer: impl Respond + 'static) {
        Mock::given(path_regex(REACTIONS))
            .respond_with(answer)
            .up_to_n_times(1)
            .with_priority(1)
            .mount(&self.server)
            .await;
    }

    // Answers 204 and counts the requests that weren't held.
    async fn count(&self) -> Arc<std::sync::atomic::AtomicUsize> {
        let seen = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        self.answer_with(Counting {
            seen: seen.clone(),
            status: 204,
        })
        .await;
        seen
    }

    // Holds the next reaction request: when it arrived, and the release.
    async fn hold(
        &self,
        status: u16,
    ) -> (std::sync::mpsc::Receiver<()>, std::sync::mpsc::Sender<()>) {
        let (arrived, on_arrival) = std::sync::mpsc::channel();
        let (release, on_release) = std::sync::mpsc::channel();
        self.answer_once(Held {
            arrived,
            release: std::sync::Mutex::new(on_release),
            answer: refusal(status, 0),
        })
        .await;
        (on_arrival, release)
    }

    async fn dispatch(&mut self, name: &str, data: Value) {
        self.seq += 1;
        self.connection
            .send(json!({"op": 0, "s": self.seq, "t": name, "d": data}))
            .await;
    }

    // The events since the last call, once everything sent so far is applied.
    async fn events(&mut self) -> Vec<String> {
        self.seq += 1;
        sync(&mut self.connection, &self.subscription, self.seq).await
    }

    async fn reaction(&mut self, name: &str, user: u64, emoji: &str, kind: u8) {
        let data = json!({
            "channel_id": GENERAL.to_string(), "message_id": TARGET.to_string(),
            "user_id": user.to_string(), "emoji": {"id": null, "name": emoji}, "type": kind,
        });
        self.dispatch(name, data).await;
    }

    async fn added_by(&mut self, user: u64, emoji: &str) {
        self.reaction("MESSAGE_REACTION_ADD", user, emoji, 0).await;
        self.events().await;
    }

    fn shown(&self) -> Shown {
        self.shown_in(GENERAL, TARGET)
    }

    fn shown_in(&self, channel: u64, message: u64) -> Shown {
        self.account
            .store()
            .reactions(Snowflake::new(channel), Snowflake::new(message))
            .iter()
            .map(|reaction| {
                (
                    reaction.emoji.name.as_deref().unwrap_or("").to_owned(),
                    reaction.normal,
                    reaction.burst,
                    reaction.me,
                    reaction.me_burst,
                )
            })
            .collect()
    }

    // Method, path and query of every reaction request, as they went over the wire.
    async fn lines(&self) -> Vec<String> {
        self.server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .filter(|request| {
                request.url.path().contains(REACTIONS) || request.method.as_str() == "PATCH"
            })
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

    async fn react(&self) -> Result<(), ReactionError> {
        self.account
            .react(general(), target(), &heart(), INLINE)
            .await
    }

    async fn unreact(&self) -> Result<(), ReactionError> {
        self.account
            .unreact(general(), target(), &heart(), INLINE)
            .await
    }
}

// Answers once the test lets it.
struct Held {
    arrived: std::sync::mpsc::Sender<()>,
    release: std::sync::Mutex<std::sync::mpsc::Receiver<()>>,
    answer: ResponseTemplate,
}

impl Respond for Held {
    fn respond(&self, _: &Received) -> ResponseTemplate {
        let _ = self.arrived.send(());
        let _ = self.release.lock().unwrap().recv_timeout(WAIT);
        self.answer.clone()
    }
}

// Counts what it answers: asking the server for its requests would wait behind a held one.
struct Counting {
    seen: Arc<std::sync::atomic::AtomicUsize>,
    status: u16,
}

impl Respond for Counting {
    fn respond(&self, _: &Received) -> ResponseTemplate {
        self.seen.fetch_add(1, Ordering::SeqCst);
        ResponseTemplate::new(self.status)
    }
}

async fn arrival(signal: &std::sync::mpsc::Receiver<()>) {
    timeout(WAIT, async {
        while signal.try_recv().is_err() {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .expect("the request never arrived");
}

// 204 for a status below 300; else Discord's error body with `code`.
fn refusal(status: u16, code: u32) -> ResponseTemplate {
    if status < 300 {
        return ResponseTemplate::new(status);
    }
    ResponseTemplate::new(status).set_body_json(json!({"message": "No", "code": code}))
}

fn changes(events: &[String]) -> usize {
    events
        .iter()
        .filter(|event| event.starts_with("MessageReactionsChanged"))
        .count()
}

fn invalid(outcome: &Result<(), ReactionError>) -> bool {
    matches!(
        outcome,
        Err(ReactionError::Request(RequestError::InvalidRequest))
    )
}

#[tokio::test]
async fn a_reaction_shows_before_discord_answers_and_its_echo_changes_nothing() {
    let mut online = reacting().await;
    let (arrived, release) = online.hold(204).await;

    let (outcome, ()) = tokio::join!(online.react(), async {
        arrival(&arrived).await;
        assert_eq!(online.shown(), one(HEART, 1, true), "before the answer");
        release.send(()).unwrap();
    });

    outcome.unwrap();
    assert_eq!(changes(&online.events().await), 1);
    assert_eq!(
        online.lines().await,
        [format!(
            "PUT {BASE}/{HEART_PATH}/@me?location=Message%20Inline%20Button&type=0"
        )]
    );
    online
        .reaction("MESSAGE_REACTION_ADD", ME_ID, HEART, 0)
        .await;
    assert_eq!(online.events().await, [] as [&str; 0], "the echo");
    assert_eq!(online.shown(), one(HEART, 1, true));
}

const ME_ID: u64 = 100_000_000_000_000_001;

#[tokio::test]
async fn taking_a_reaction_back_shows_at_once_and_deletes_the_typed_own_path() {
    let mut online = reacting().await;
    online.added_by(MIRA, HEART).await;
    online.added_by(ME_ID, HEART).await;
    let (arrived, release) = online.hold(204).await;

    let emoji = heart();
    let taken = online
        .account
        .unreact(general(), target(), &emoji, ReactionLocation::HoverBar);
    let (outcome, ()) = tokio::join!(taken, async {
        arrival(&arrived).await;
        assert_eq!(online.shown(), one(HEART, 1, false), "before the answer");
        release.send(()).unwrap();
    });

    outcome.unwrap();
    assert_eq!(changes(&online.events().await), 1);
    assert_eq!(
        online.lines().await,
        [format!(
            "DELETE {BASE}/{HEART_PATH}/0/@me?location=Message%20Hover%20Bar&burst=false"
        )]
    );
    online
        .reaction("MESSAGE_REACTION_REMOVE", ME_ID, HEART, 0)
        .await;
    assert_eq!(online.events().await, [] as [&str; 0], "the echo");
    assert_eq!(online.shown(), one(HEART, 1, false));
}

#[tokio::test]
async fn a_reaction_that_is_mine_only_as_a_super_reaction_is_removed_as_one() {
    let mut online = reacting().await;
    online
        .reaction("MESSAGE_REACTION_ADD", ME_ID, HEART, 1)
        .await;
    online.events().await;
    online.answer(204).await;

    online.react().await.unwrap();
    assert_eq!(online.lines().await, [] as [&str; 0], "never a second one");

    online.unreact().await.unwrap();

    assert_eq!(
        online.lines().await,
        [format!(
            "DELETE {BASE}/{HEART_PATH}/1/@me?location=Message%20Inline%20Button&burst=true"
        )]
    );
    assert_eq!(online.shown(), []);
}

#[tokio::test]
async fn each_place_names_itself_and_a_custom_emoji_goes_by_name_and_id() {
    let online = reacting().await;
    online.answer(204).await;
    let places = [
        (ReactionLocation::Message, "Message", "a"),
        (
            ReactionLocation::InlineButton,
            "Message%20Inline%20Button",
            "b",
        ),
        (ReactionLocation::HoverBar, "Message%20Hover%20Bar", "c"),
        (
            ReactionLocation::ContextMenu,
            "Message%20Context%20Menu",
            "d",
        ),
        (ReactionLocation::Picker, "Message%20Reaction%20Picker", "e"),
        (ReactionLocation::ForumToolbar, "Forum%20Toolbar", "f"),
    ];

    for (index, (location, _, name)) in places.iter().enumerate() {
        let emoji = ReactionEmoji::custom(Snowflake::new(index as u64 + 1), Some(name), index == 1);
        online
            .account
            .react(general(), target(), &emoji, *location)
            .await
            .unwrap();
    }

    let expected: Vec<String> = places
        .iter()
        .enumerate()
        .map(|(index, (_, query, name))| {
            format!(
                "PUT {BASE}/{name}:{}/@me?location={query}&type=0",
                index + 1
            )
        })
        .collect();
    assert_eq!(online.lines().await, expected);
}

#[tokio::test]
async fn a_refused_reaction_is_taken_out_again_and_says_why() {
    let cases: [(u16, u32, Expected); 8] = [
        (403, 30010, |err| matches!(err, ReactionError::TooMany)),
        (403, 90001, |err| matches!(err, ReactionError::Blocked)),
        (403, 50013, |err| matches!(err, ReactionError::Forbidden)),
        (403, 50001, |err| matches!(err, ReactionError::Forbidden)),
        (403, 0, |err| matches!(err, ReactionError::Forbidden)),
        (400, 10014, |err| matches!(err, ReactionError::UnknownEmoji)),
        (404, 10008, |err| {
            matches!(
                err,
                ReactionError::Request(RequestError::Discord { code: 10008, .. })
            )
        }),
        (500, 0, |err| {
            matches!(
                err,
                ReactionError::Request(RequestError::ServerError { .. })
            )
        }),
    ];
    for (status, code, expected) in cases {
        let mut online = reacting().await;
        online.added_by(MIRA, UP).await;
        online.answer_with(refusal(status, code)).await;

        let err = online.react().await.unwrap_err();

        assert!(expected(&err), "{status} {code}: {err:?}");
        assert_eq!(online.shown(), one(UP, 1, false), "{status} {code}");
        assert_eq!(changes(&online.events().await), 2, "{status} {code}");
        assert_eq!(
            online.lines().await.len(),
            1,
            "{status} {code}: not retried"
        );
    }
}

#[tokio::test]
async fn a_refused_removal_puts_the_reaction_back() {
    let mut online = reacting().await;
    online.added_by(ME_ID, HEART).await;
    online.added_by(MIRA, UP).await;
    online.answer_with(refusal(403, 50013)).await;

    let err = online.unreact().await.unwrap_err();

    assert!(matches!(err, ReactionError::Forbidden), "{err:?}");
    assert_eq!(
        online.shown(),
        [one(UP, 1, false), one(HEART, 1, true)].concat(),
        "at the row's end, as the official client puts it back"
    );
}

#[tokio::test]
async fn a_429_is_waited_out_once_with_the_reaction_showing() {
    let online = reacting().await;
    online
        .answer_once(
            ResponseTemplate::new(429)
                .set_body_json(json!({"message": "limited", "retry_after": 0.02, "global": false})),
        )
        .await;
    online.answer(204).await;

    online.react().await.unwrap();

    assert_eq!(online.shown(), one(HEART, 1, true));
    assert_eq!(online.lines().await.len(), 2);
}

#[tokio::test]
async fn a_second_429_takes_the_reaction_out() {
    let online = reacting().await;
    online
        .answer_with(
            ResponseTemplate::new(429)
                .set_body_json(json!({"message": "limited", "retry_after": 0.02, "global": false})),
        )
        .await;

    let err = online.react().await.unwrap_err();

    assert!(
        matches!(
            err,
            ReactionError::RateLimited {
                retry_after: Some(_)
            }
        ),
        "{err:?}"
    );
    assert_eq!(online.shown(), []);
    assert_eq!(online.lines().await.len(), 2, "one retry, never more");
}

#[tokio::test]
async fn a_502_is_asked_again_once() {
    let online = reacting().await;
    online.answer_once(ResponseTemplate::new(502)).await;
    online.answer(204).await;

    online.react().await.unwrap();

    assert_eq!(online.shown(), one(HEART, 1, true));
    assert_eq!(online.lines().await.len(), 2);

    let failing = reacting().await;
    failing.answer(502).await;
    let err = failing.react().await.unwrap_err();
    assert!(
        matches!(
            err,
            ReactionError::Request(RequestError::ServerError { .. })
        ),
        "{err:?}"
    );
    assert_eq!(failing.shown(), []);
    assert_eq!(failing.lines().await.len(), 2);
}

// PUT and DELETE in turn, as each answer finds a newer wish.
fn alternates(lines: &[String]) -> bool {
    lines
        .iter()
        .enumerate()
        .all(|(index, line)| line.starts_with(if index % 2 == 0 { "PUT " } else { "DELETE " }))
}

#[tokio::test]
async fn the_latest_wish_wins_with_one_request_at_a_time() {
    let online = reacting().await;
    let (arrived, release) = online.hold(204).await;
    let others = online.count().await;

    let (first, (second, third)) = tokio::join!(online.react(), async {
        arrival(&arrived).await;
        let mut taken = Box::pin(online.unreact());
        assert!(timeout(Duration::from_millis(1), &mut taken).await.is_err());
        assert_eq!(online.shown(), []);
        let mut again = Box::pin(online.react());
        assert!(timeout(Duration::from_millis(1), &mut again).await.is_err());
        assert_eq!(online.shown(), one(HEART, 1, true));
        assert_eq!(others.load(Ordering::SeqCst), 0, "one request at a time");
        release.send(()).unwrap();
        (taken.await, again.await)
    });

    first.unwrap();
    second.unwrap();
    third.unwrap();
    assert_eq!(online.shown(), one(HEART, 1, true), "the last wish");
    let lines = online.lines().await;
    assert!(alternates(&lines) && lines.len() % 2 == 1, "{lines:?}");
    assert!(lines.len() <= 3, "{lines:?}");
}

#[tokio::test]
async fn a_wish_made_while_one_is_asked_is_asked_next() {
    let online = reacting().await;
    let (arrived, release) = online.hold(204).await;
    let others = online.count().await;

    let (first, second) = tokio::join!(online.react(), async {
        arrival(&arrived).await;
        let taken = online.unreact();
        tokio::pin!(taken);
        assert!(
            timeout(Duration::from_millis(30), &mut taken)
                .await
                .is_err(),
            "waits for its own request"
        );
        assert_eq!(online.shown(), [], "shown at once");
        assert_eq!(others.load(Ordering::SeqCst), 0, "one request at a time");
        release.send(()).unwrap();
        taken.await
    });

    first.unwrap();
    second.unwrap();
    assert_eq!(online.shown(), []);
    let lines = online.lines().await;
    assert!(alternates(&lines), "{lines:?}");
    assert!(lines.last().unwrap().starts_with("DELETE "), "{lines:?}");
}

#[tokio::test]
async fn rapid_toggles_never_queue_a_request_each() {
    let online = reacting().await;
    let (arrived, release) = online.hold(204).await;
    let others = online.count().await;

    let online = &online;
    let (first, ()) = tokio::join!(online.react(), async {
        arrival(&arrived).await;
        let toggles: Vec<_> = (0..20)
            .map(|toggle| async move {
                if toggle % 2 == 0 {
                    online.unreact().await
                } else {
                    online.react().await
                }
            })
            .collect();
        let mut pending = Vec::new();
        for toggle in toggles {
            let mut toggle = Box::pin(toggle);
            if timeout(Duration::from_millis(1), &mut toggle)
                .await
                .is_err()
            {
                pending.push(toggle);
            }
        }
        let last = pending.pop();
        for superseded in pending {
            timeout(WAIT, superseded)
                .await
                .expect("a wish the user changed still waits")
                .unwrap();
        }
        assert_eq!(others.load(Ordering::SeqCst), 0, "one request at a time");
        release.send(()).unwrap();
        if let Some(last) = last {
            last.await.unwrap();
        }
    });

    first.unwrap();
    assert_eq!(online.shown(), one(HEART, 1, true), "the last wish");
    let lines = online.lines().await;
    assert!(alternates(&lines) && lines.len() <= 3, "{lines:?}");
}

#[tokio::test]
async fn a_refusal_of_a_wish_the_user_took_back_changes_nothing() {
    let mut online = reacting().await;
    let (arrived, release) = online.hold(403).await;
    online.events().await;

    let (first, second) = tokio::join!(online.react(), async {
        arrival(&arrived).await;
        let taken = online.unreact();
        tokio::pin!(taken);
        let _ = timeout(Duration::from_millis(10), &mut taken).await;
        release.send(()).unwrap();
        taken.await
    });

    first.unwrap();
    second.unwrap();
    assert_eq!(online.shown(), []);
    assert_eq!(
        online.lines().await.len(),
        1,
        "Discord has what was wanted last"
    );
    assert_eq!(
        changes(&online.events().await),
        2,
        "the two clicks, no undo"
    );
}

#[tokio::test]
async fn a_refusal_of_the_latest_wish_goes_back_to_what_discord_took() {
    let online = reacting().await;
    online.answer_once(ResponseTemplate::new(204)).await;
    let (arrived, release) = online.hold(403).await;

    online.react().await.unwrap();
    let (taken, ()) = tokio::join!(online.unreact(), async {
        arrival(&arrived).await;
        assert_eq!(online.shown(), []);
        release.send(()).unwrap();
    });

    assert!(matches!(taken, Err(ReactionError::Forbidden)), "{taken:?}");
    assert_eq!(online.shown(), one(HEART, 1, true));
}

#[tokio::test]
async fn a_refused_removal_never_puts_back_what_a_moderator_removed_meanwhile() {
    for removal in [
        "MESSAGE_REACTION_REMOVE_ALL",
        "MESSAGE_REACTION_REMOVE_EMOJI",
    ] {
        let mut online = reacting().await;
        online.added_by(ME_ID, HEART).await;
        let (arrived, release) = online.hold(403).await;
        let data = json!({
            "channel_id": GENERAL.to_string(), "message_id": TARGET.to_string(),
            "emoji": {"id": null, "name": HEART},
        });

        let account = &online.account;
        let emoji = heart();
        let mut taken = Box::pin(account.unreact(general(), target(), &emoji, INLINE));
        assert!(timeout(Duration::from_millis(1), &mut taken).await.is_err());
        arrival(&arrived).await;
        online.seq += 1;
        online
            .connection
            .send(json!({"op": 0, "s": online.seq, "t": removal, "d": data}))
            .await;
        online.seq += 1;
        sync(&mut online.connection, &online.subscription, online.seq).await;
        release.send(()).unwrap();
        let outcome = taken.await;

        assert!(
            matches!(outcome, Err(ReactionError::Forbidden)),
            "{removal}"
        );
        assert_eq!(online.shown(), [], "{removal}");
    }
}

#[tokio::test]
async fn a_reaction_removed_by_a_moderator_while_it_is_asked_stays_out_until_its_echo() {
    let mut online = reacting().await;
    let (arrived, release) = online.hold(204).await;

    let account = &online.account;
    let emoji = heart();
    let mut added = Box::pin(account.react(general(), target(), &emoji, INLINE));
    assert!(timeout(Duration::from_millis(1), &mut added).await.is_err());
    arrival(&arrived).await;
    online.seq += 1;
    online
        .connection
        .send(
            json!({"op": 0, "s": online.seq, "t": "MESSAGE_REACTION_REMOVE_ALL", "d": {
                "channel_id": GENERAL.to_string(), "message_id": TARGET.to_string(),
            }}),
        )
        .await;
    online.seq += 1;
    sync(&mut online.connection, &online.subscription, online.seq).await;
    release.send(()).unwrap();
    added.await.unwrap();

    assert_eq!(online.shown(), [], "the gateway's word stands");
    online
        .reaction("MESSAGE_REACTION_ADD", ME_ID, HEART, 0)
        .await;
    assert_eq!(changes(&online.events().await), 1);
    assert_eq!(online.shown(), one(HEART, 1, true));
}

#[tokio::test]
async fn a_message_deleted_while_its_reaction_is_asked_takes_the_refusal_quietly() {
    let mut online = reacting().await;
    online.added_by(ME_ID, HEART).await;
    let (arrived, release) = online.hold(404).await;

    let account = &online.account;
    let emoji = heart();
    let mut taken = Box::pin(account.unreact(general(), target(), &emoji, INLINE));
    assert!(timeout(Duration::from_millis(1), &mut taken).await.is_err());
    arrival(&arrived).await;
    online.seq += 1;
    online
        .connection
        .send(
            json!({"op": 0, "s": online.seq, "t": "MESSAGE_DELETE", "d": {
                "id": TARGET.to_string(), "channel_id": GENERAL.to_string(),
                "guild_id": "200000000000000001",
            }}),
        )
        .await;
    online.seq += 1;
    let before = sync(&mut online.connection, &online.subscription, online.seq).await;
    assert!(
        before.contains(&format!("MessageDeleted({TARGET})")),
        "{before:?}"
    );
    release.send(()).unwrap();
    let outcome = taken.await;

    assert!(outcome.is_err());
    assert_eq!(online.events().await, [] as [&str; 0]);
    assert_eq!(online.shown(), []);
}

#[tokio::test]
async fn what_the_store_shows_as_wanted_already_sends_nothing() {
    let mut online = reacting().await;
    online.added_by(MIRA, UP).await;
    online.added_by(ME_ID, HEART).await;
    online.answer(204).await;
    let up = ReactionEmoji::unicode(UP);

    online.react().await.unwrap();
    online
        .account
        .unreact(general(), target(), &up, INLINE)
        .await
        .unwrap();

    assert_eq!(online.lines().await, [] as [&str; 0]);
    assert_eq!(online.events().await, [] as [&str; 0]);
}

#[tokio::test]
async fn a_message_the_store_doesnt_have_is_refused_without_a_request() {
    let online = reacting().await;
    online.answer(204).await;
    let unknown = Snowflake::new(TARGET + 50);

    let added = online
        .account
        .react(general(), unknown, &heart(), INLINE)
        .await;
    let taken = online
        .account
        .unreact(general(), unknown, &heart(), INLINE)
        .await;
    let nothing = online
        .account
        .react(general(), target(), &ReactionEmoji::unicode(""), INLINE)
        .await;

    assert!(invalid(&added) && invalid(&taken) && invalid(&nothing));
    assert_eq!(online.lines().await, [] as [&str; 0]);
}

fn starter_row() -> Value {
    let mut row = message(0, STARTER_ROW)["d"].clone();
    row["channel_id"] = THREAD.to_string().into();
    row["type"] = 21.into();
    row["content"] = "".into();
    row["message_reference"] = json!({
        "message_id": TARGET.to_string(), "channel_id": GENERAL.to_string(),
        "guild_id": "200000000000000001",
    });
    row["referenced_message"] = message(0, TARGET)["d"].clone();
    row
}

// #general's message also shown by the starter row of its thread, which is viewed.
async fn with_a_starter_row() -> Online {
    let mut online = reacting().await;
    online.answer(204).await;
    online.account.view_channel(Snowflake::new(THREAD));
    online.dispatch("MESSAGE_CREATE", starter_row()).await;
    online.events().await;
    online
}

#[tokio::test]
async fn a_starter_rows_reaction_goes_to_the_message_it_shows() {
    let mut online = with_a_starter_row().await;
    let (thread, row) = (Snowflake::new(THREAD), Snowflake::new(STARTER_ROW));

    online
        .account
        .react(thread, row, &heart(), INLINE)
        .await
        .unwrap();

    assert_eq!(
        online.lines().await,
        [format!(
            "PUT {BASE}/{HEART_PATH}/@me?location=Message%20Inline%20Button&type=0"
        )]
    );
    assert_eq!(online.shown(), one(HEART, 1, true), "the parent's row");
    assert_eq!(online.shown_in(THREAD, STARTER_ROW), one(HEART, 1, true));
    assert_eq!(changes(&online.events().await), 2, "both rows are told");
}

// Mira's message in the viewed DM with her.
async fn in_a_dm() -> Online {
    let mut online = reacting().await;
    online.answer(204).await;
    online.account.view_channel(Snowflake::new(DM));
    let mut sent = message(0, TARGET + 1)["d"].clone();
    sent["channel_id"] = DM.to_string().into();
    sent.as_object_mut().unwrap().remove("guild_id");
    sent.as_object_mut().unwrap().remove("member");
    online.dispatch("MESSAGE_CREATE", sent).await;
    online.events().await;
    online
}

#[tokio::test]
async fn a_dms_reactions_go_to_the_dm() {
    let online = in_a_dm().await;
    let (dm, id) = (Snowflake::new(DM), Snowflake::new(TARGET + 1));

    online
        .account
        .react(dm, id, &heart(), INLINE)
        .await
        .unwrap();

    assert_eq!(online.shown_in(DM, TARGET + 1), one(HEART, 1, true));
    assert_eq!(
        online.lines().await,
        [format!(
            "PUT /api/v9/channels/{DM}/messages/{}/reactions/{HEART_PATH}/@me?location=Message%20Inline%20Button&type=0",
            TARGET + 1
        )]
    );
}

// The viewed thread archived, with a message in it.
async fn archive_the_thread(online: &mut Online) {
    online.account.view_channel(Snowflake::new(THREAD));
    let mut sent = message(0, TARGET + 1)["d"].clone();
    sent["channel_id"] = THREAD.to_string().into();
    online.dispatch("MESSAGE_CREATE", sent).await;
    let mut archived: Value = serde_json::from_str(READY).unwrap();
    let mut archived = archived["d"]["guilds"][0]["threads"][0].take();
    archived["thread_metadata"]["archived"] = true.into();
    online.dispatch("THREAD_UPDATE", archived).await;
    online.events().await;
}

#[tokio::test]
async fn an_archived_thread_is_opened_before_the_reaction() {
    let mut online = reacting().await;
    online.answer(204).await;
    Mock::given(method("PATCH"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .mount(&online.server)
        .await;
    let thread = Snowflake::new(THREAD);
    archive_the_thread(&mut online).await;
    let id = Snowflake::new(TARGET + 1);

    online
        .account
        .react(thread, id, &heart(), INLINE)
        .await
        .unwrap();

    let at = format!(
        "/api/v9/channels/{THREAD}/messages/{}/reactions/{HEART_PATH}",
        TARGET + 1
    );
    assert_eq!(
        online.lines().await,
        [
            format!("PATCH /api/v9/channels/{THREAD}"),
            format!("PUT {at}/@me?location=Message%20Inline%20Button&type=0"),
        ]
    );
}

#[tokio::test]
async fn a_thread_that_wont_open_takes_the_reaction_out() {
    let mut online = reacting().await;
    online.answer(204).await;
    Mock::given(method("PATCH"))
        .respond_with(refusal(403, 50013))
        .mount(&online.server)
        .await;
    let thread = Snowflake::new(THREAD);
    archive_the_thread(&mut online).await;

    let outcome = online
        .account
        .react(thread, Snowflake::new(TARGET + 1), &heart(), INLINE)
        .await;

    assert!(
        matches!(outcome, Err(ReactionError::Forbidden)),
        "{outcome:?}"
    );
    assert_eq!(online.shown_in(THREAD, TARGET + 1), []);
    assert_eq!(
        online.lines().await,
        [format!("PATCH /api/v9/channels/{THREAD}")]
    );
}

#[tokio::test]
async fn a_closed_account_refuses_reactions() {
    let online = reacting().await;
    online.answer(204).await;
    online.account.close();

    let outcome = online.react().await;

    assert!(
        matches!(outcome, Err(ReactionError::Request(RequestError::Closed))),
        "{outcome:?}"
    );
    assert_eq!(online.lines().await, [] as [&str; 0]);
    assert_eq!(online.shown(), []);
}

#[tokio::test]
async fn moderating_waits_for_the_gateway_to_change_the_store() {
    let mut online = reacting().await;
    online.added_by(MIRA, HEART).await;
    online.added_by(TOM, UP).await;
    online.answer(204).await;
    let mira = Snowflake::new(MIRA);
    let wave = ReactionEmoji::custom(Snowflake::new(7), Some("wave"), false);

    online
        .account
        .remove_reaction(general(), target(), &heart(), ReactionKind::Normal, mira)
        .await
        .unwrap();
    online
        .account
        .remove_reaction(general(), target(), &wave, ReactionKind::Burst, mira)
        .await
        .unwrap();
    online
        .account
        .remove_reactions(general(), target(), Some(&heart()))
        .await
        .unwrap();
    online
        .account
        .remove_reactions(general(), target(), None)
        .await
        .unwrap();

    assert_eq!(
        online.lines().await,
        [
            format!("DELETE {BASE}/{HEART_PATH}/0/{MIRA}?location=Message&burst=false"),
            format!("DELETE {BASE}/wave:7/1/{MIRA}?location=Message&burst=true"),
            format!("DELETE {BASE}/{HEART_PATH}"),
            format!("DELETE {BASE}"),
        ]
    );
    assert_eq!(online.events().await, [] as [&str; 0]);
    assert_eq!(
        online.shown(),
        [one(HEART, 1, false), one(UP, 1, false)].concat(),
        "until the gateway says so"
    );
}

#[tokio::test]
async fn a_refused_moderation_says_why() {
    let online = reacting().await;
    online.answer_with(refusal(403, 50013)).await;

    let outcome = online
        .account
        .remove_reactions(general(), target(), None)
        .await;

    assert!(
        matches!(outcome, Err(ReactionError::Forbidden)),
        "{outcome:?}"
    );
}

#[tokio::test]
async fn removing_ones_own_reaction_in_the_window_goes_as_taking_it_back() {
    let mut online = reacting().await;
    online.added_by(ME_ID, HEART).await;
    online.answer(204).await;

    online
        .account
        .remove_reaction(
            general(),
            target(),
            &heart(),
            ReactionKind::Normal,
            Snowflake::new(ME_ID),
        )
        .await
        .unwrap();

    assert_eq!(
        online.lines().await,
        [format!(
            "DELETE {BASE}/{HEART_PATH}/0/@me?location=Message&burst=false"
        )]
    );
    assert_eq!(online.shown(), [], "at once");
}

#[tokio::test]
async fn moderation_goes_to_the_message_a_row_shows_and_opens_an_archived_thread() {
    let online = with_a_starter_row().await;
    let (thread, row) = (Snowflake::new(THREAD), Snowflake::new(STARTER_ROW));
    online
        .account
        .remove_reactions(thread, row, None)
        .await
        .unwrap();
    assert_eq!(online.lines().await, [format!("DELETE {BASE}")]);

    let mut online = reacting().await;
    online.answer(204).await;
    Mock::given(method("PATCH"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .mount(&online.server)
        .await;
    archive_the_thread(&mut online).await;
    online
        .account
        .remove_reactions(thread, Snowflake::new(TARGET + 1), Some(&heart()))
        .await
        .unwrap();
    assert_eq!(
        online.lines().await,
        [
            format!("PATCH /api/v9/channels/{THREAD}"),
            format!(
                "DELETE /api/v9/channels/{THREAD}/messages/{}/reactions/{HEART_PATH}",
                TARGET + 1
            ),
        ]
    );
}

#[tokio::test]
async fn what_may_not_be_moderated_is_refused_without_a_request() {
    let online = in_a_dm().await;
    let (dm, id) = (Snowflake::new(DM), Snowflake::new(TARGET + 1));
    let mira = Snowflake::new(MIRA);

    let all = online.account.remove_reactions(dm, id, None).await;
    let others = online
        .account
        .remove_reaction(dm, id, &heart(), ReactionKind::Normal, mira)
        .await;
    let unknown = online
        .account
        .remove_reactions(general(), Snowflake::new(TARGET + 50), None)
        .await;

    assert!(
        invalid(&all) && invalid(&others),
        "nothing to moderate in a DM"
    );
    assert!(invalid(&unknown), "a message the store doesn't have");
    assert_eq!(online.lines().await, [] as [&str; 0]);
}
