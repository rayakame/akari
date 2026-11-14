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
    online.answer(204).await;

    let (first, (second, third)) = tokio::join!(online.react(), async {
        arrival(&arrived).await;
        let mut taken = Box::pin(online.unreact());
        assert!(timeout(Duration::from_millis(1), &mut taken).await.is_err());
        assert_eq!(online.shown(), []);
        let mut again = Box::pin(online.react());
        assert!(timeout(Duration::from_millis(1), &mut again).await.is_err());
        assert_eq!(online.shown(), one(HEART, 1, true));
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

// The removal is refused: only a call that waited for its own request can say so.
#[tokio::test]
async fn a_wish_made_while_one_is_asked_is_asked_next() {
    let online = reacting().await;
    let (arrived, release) = online.hold(204).await;
    online.answer_with(refusal(403, 50013)).await;
    let account = &online.account;
    let emoji = heart();

    let (first, second) = tokio::join!(online.react(), async {
        arrival(&arrived).await;
        let taken =
            begun(account.unreact(general(), target(), &emoji, ReactionLocation::HoverBar)).await;
        assert_eq!(online.shown(), [], "shown at once");
        release.send(()).unwrap();
        taken.await
    });

    first.unwrap();
    assert!(
        matches!(second, Err(ReactionError::Forbidden)),
        "its own request's answer: {second:?}"
    );
    assert_eq!(online.shown(), one(HEART, 1, true), "what Discord has");
    assert_eq!(
        online.lines().await,
        [
            format!("PUT {BASE}/{HEART_PATH}/@me?location=Message%20Inline%20Button&type=0"),
            format!("DELETE {BASE}/{HEART_PATH}/0/@me?location=Message%20Hover%20Bar&burst=false"),
        ],
        "each request from its own wish's place"
    );
}

#[tokio::test]
async fn rapid_toggles_never_queue_a_request_each() {
    let online = reacting().await;
    let (arrived, release) = online.hold(204).await;
    online.answer(204).await;

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

        outcome.unwrap_or_else(|err| panic!("{removal}: gone as the user wanted, {err}"));
        assert_eq!(online.shown(), [], "{removal}");
    }
}

// No echo came before the removal, so the reaction reached Discord after it.
#[tokio::test]
async fn a_reaction_asked_while_a_moderator_removes_all_shows_once_discord_answers() {
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
    assert_eq!(online.shown(), [], "removed with all the others");
    release.send(()).unwrap();
    added.await.unwrap();

    assert_eq!(online.shown(), one(HEART, 1, true));
    online.events().await;
    online
        .reaction("MESSAGE_REACTION_ADD", ME_ID, HEART, 0)
        .await;
    assert_eq!(online.events().await, [] as [&str; 0], "the echo");
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

// The viewed thread, open, with a message in it.
async fn in_the_thread(online: &mut Online) {
    online.account.view_channel(Snowflake::new(THREAD));
    let mut sent = message(0, TARGET + 1)["d"].clone();
    sent["channel_id"] = THREAD.to_string().into();
    online.dispatch("MESSAGE_CREATE", sent).await;
    online.events().await;
}

// The viewed thread archived, with a message in it.
async fn archive_the_thread(online: &mut Online) {
    in_the_thread(online).await;
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
    let mut online = reacting().await;
    online.answer(204).await;
    online.events().await;
    online.account.close();

    let outcome = online.react().await;

    assert!(
        matches!(outcome, Err(ReactionError::Request(RequestError::Closed))),
        "{outcome:?}"
    );
    assert_eq!(online.lines().await, [] as [&str; 0]);
    assert_eq!(online.shown(), []);
    let buffered = online.subscription.buffered();
    let told = if buffered == 0 {
        Vec::new()
    } else {
        online.subscription.next_batch(buffered).await
    };
    let told: Vec<String> = told.iter().map(describe).collect();
    assert_eq!(changes(&told), 0, "never shown: {told:?}");
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

fn user(id: u64, name: &str) -> Value {
    json!({"id": id.to_string(), "username": name, "discriminator": "0", "avatar": null})
}

#[tokio::test]
async fn reactors_come_by_page_as_the_stores_users() {
    let online = reacting().await;
    let ready: Value = serde_json::from_str(READY).unwrap();
    let mira = ready["d"]["users"][0].clone();
    Mock::given(method("GET"))
        .and(path_regex(REACTIONS))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            mira,
            user(100_000_000_000_000_050, "ada"),
            user(100_000_000_000_000_051, "bo"),
        ])))
        .mount(&online.server)
        .await;
    let emoji = heart();
    let ask = |kind, after, limit| {
        online
            .account
            .reactors(general(), target(), &emoji, kind, after, limit)
    };

    let full = ask(ReactionKind::Normal, None, 3).await.unwrap();
    let short = ask(ReactionKind::Burst, Some(Snowflake::new(MIRA)), 100)
        .await
        .unwrap();
    ask(ReactionKind::Normal, None, 0).await.unwrap();
    ask(ReactionKind::Normal, None, 200).await.unwrap();

    assert_eq!(
        online.lines().await,
        [
            format!("GET {BASE}/{HEART_PATH}?limit=3&type=0"),
            format!("GET {BASE}/{HEART_PATH}?limit=100&after={MIRA}&type=1"),
            format!("GET {BASE}/{HEART_PATH}?limit=1&type=0"),
            format!("GET {BASE}/{HEART_PATH}?limit=100&type=0"),
        ]
    );
    let names: Vec<&str> = full.users.iter().map(|user| &*user.username).collect();
    assert_eq!(names, ["mira", "ada", "bo"]);
    assert!(full.has_more, "a full page");
    assert!(!short.has_more, "a short page is the last");
    let known = online.account.store().user(Snowflake::new(MIRA)).unwrap();
    assert!(
        Arc::ptr_eq(&full.users[0], &known),
        "the store's own record"
    );
}

#[tokio::test]
async fn a_starter_rows_reactors_are_those_of_the_message_it_shows() {
    let online = with_a_starter_row().await;

    let page = online
        .account
        .reactors(
            Snowflake::new(THREAD),
            Snowflake::new(STARTER_ROW),
            &heart(),
            ReactionKind::Normal,
            None,
            3,
        )
        .await;

    assert!(
        matches!(page, Err(RequestError::UnexpectedResponse)),
        "204 is no page: {page:?}"
    );
    assert_eq!(
        online.lines().await,
        [format!("GET {BASE}/{HEART_PATH}?limit=3&type=0")]
    );
}

// The gateway's side of `Online`, to tell it something while calls on the account wait.
struct Wire<'a> {
    connection: &'a mut FakeConnection,
    subscription: &'a Subscription,
    seq: &'a mut u64,
}

impl Wire<'_> {
    // The events since the last call, once `name` is applied.
    async fn tell(&mut self, name: &str, data: Value) -> Vec<String> {
        *self.seq += 1;
        self.connection
            .send(json!({"op": 0, "s": *self.seq, "t": name, "d": data}))
            .await;
        self.events().await
    }

    async fn events(&mut self) -> Vec<String> {
        *self.seq += 1;
        sync(self.connection, self.subscription, *self.seq).await
    }

    // The user's own reaction with the heart, as a dispatch.
    async fn own(&mut self, name: &str, kind: u8) -> Vec<String> {
        let data = json!({
            "channel_id": GENERAL.to_string(), "message_id": TARGET.to_string(),
            "user_id": ME_ID.to_string(), "emoji": {"id": null, "name": HEART}, "type": kind,
        });
        self.tell(name, data).await
    }

    async fn removal(&mut self, name: &str) -> Vec<String> {
        let data = json!({
            "channel_id": GENERAL.to_string(), "message_id": TARGET.to_string(),
            "emoji": {"id": null, "name": HEART},
        });
        self.tell(name, data).await
    }
}

impl Online {
    fn wire(&mut self) -> (&Account, &MockServer, Wire<'_>) {
        let wire = Wire {
            connection: &mut self.connection,
            subscription: &self.subscription,
            seq: &mut self.seq,
        };
        (&self.account, &self.server, wire)
    }
}

fn shown_by(account: &Account) -> Shown {
    account
        .store()
        .reactions(general(), target())
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

// The call with its change in the store, waiting for Discord.
async fn begun<T>(call: impl Future<Output = T>) -> std::pin::Pin<Box<impl Future<Output = T>>> {
    let mut call = Box::pin(call);
    let first =
        std::future::poll_fn(|context| std::task::Poll::Ready(call.as_mut().poll(context))).await;
    assert!(first.is_pending(), "answered before Discord");
    call
}

async fn requests(server: &MockServer) -> Vec<String> {
    server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|request| request.url.path().contains(REACTIONS))
        .map(|request| request.method.to_string())
        .collect()
}

const ADD: &str = "MESSAGE_REACTION_ADD";
const REMOVE: &str = "MESSAGE_REACTION_REMOVE";

#[tokio::test]
async fn the_late_echo_of_a_reaction_taken_back_never_shows_it_again() {
    let mut online = reacting().await;
    let (put, answer_put) = online.hold(204).await;
    let (delete, answer_delete) = online.hold(204).await;
    let (account, _, mut wire) = online.wire();
    let emoji = heart();

    let added = begun(account.react(general(), target(), &emoji, INLINE)).await;
    arrival(&put).await;
    let taken = begun(account.unreact(general(), target(), &emoji, INLINE)).await;
    answer_put.send(()).unwrap();
    arrival(&delete).await;
    assert_eq!(changes(&wire.events().await), 2, "the two clicks");

    let echo = wire.own(ADD, 0).await;
    assert_eq!(echo, [] as [&str; 0], "the reaction's echo");
    assert_eq!(shown_by(account), []);

    answer_delete.send(()).unwrap();
    added.await.unwrap();
    taken.await.unwrap();
    assert_eq!(wire.own(REMOVE, 0).await, [] as [&str; 0]);
    assert_eq!(shown_by(account), []);
}

#[tokio::test]
async fn the_late_echo_of_a_removal_taken_back_never_hides_the_reaction() {
    let mut online = reacting().await;
    online.added_by(MIRA, HEART).await;
    online.added_by(ME_ID, HEART).await;
    let (delete, answer_delete) = online.hold(204).await;
    let (put, answer_put) = online.hold(204).await;
    let (account, _, mut wire) = online.wire();
    let emoji = heart();

    let taken = begun(account.unreact(general(), target(), &emoji, INLINE)).await;
    arrival(&delete).await;
    let added = begun(account.react(general(), target(), &emoji, INLINE)).await;
    answer_delete.send(()).unwrap();
    arrival(&put).await;
    wire.events().await;

    assert_eq!(wire.own(REMOVE, 0).await, [] as [&str; 0]);
    assert_eq!(shown_by(account), one(HEART, 2, true));

    answer_put.send(()).unwrap();
    taken.await.unwrap();
    added.await.unwrap();
    assert_eq!(wire.own(ADD, 0).await, [] as [&str; 0]);
    assert_eq!(shown_by(account), one(HEART, 2, true));
}

// Discord has no reaction of the user's left, whatever the removal's answer says.
#[tokio::test]
async fn a_reaction_made_again_after_a_moderator_removed_all_is_asked() {
    let mut online = reacting().await;
    online.added_by(ME_ID, HEART).await;
    let (delete, answer_delete) = online.hold(403).await;
    online.answer(204).await;
    let (account, server, mut wire) = online.wire();
    let emoji = heart();

    let taken = begun(account.unreact(general(), target(), &emoji, INLINE)).await;
    arrival(&delete).await;
    wire.removal("MESSAGE_REACTION_REMOVE_ALL").await;
    let added = begun(account.react(general(), target(), &emoji, INLINE)).await;
    answer_delete.send(()).unwrap();

    taken.await.unwrap();
    added.await.unwrap();
    assert_eq!(requests(server).await, ["DELETE", "PUT"]);
    assert_eq!(shown_by(account), one(HEART, 1, true));
}

#[tokio::test]
async fn a_refused_removal_of_a_super_reaction_the_user_made_again_keeps_it_a_super_reaction() {
    let mut online = reacting().await;
    online.reaction(ADD, ME_ID, HEART, 1).await;
    online.events().await;
    let (delete, answer_delete) = online.hold(403).await;
    online.answer(204).await;
    let (account, server, mut wire) = online.wire();
    let emoji = heart();

    let taken = begun(account.unreact(general(), target(), &emoji, INLINE)).await;
    arrival(&delete).await;
    let added = begun(account.react(general(), target(), &emoji, INLINE)).await;
    assert_eq!(shown_by(account), one(HEART, 1, true), "the click, at once");
    answer_delete.send(()).unwrap();

    taken.await.unwrap();
    added.await.unwrap();
    assert_eq!(requests(server).await, ["DELETE"], "Discord kept it");
    assert_eq!(
        shown_by(account),
        [(HEART.to_owned(), 0, 1, false, true)],
        "as Discord has it"
    );
    wire.events().await;
    let again = account.unreact(general(), target(), &emoji, INLINE).await;
    again.unwrap();
    let lines = online.lines().await;
    assert!(
        lines[1].contains("/1/@me?") && lines[1].ends_with("burst=true"),
        "{lines:?}"
    );
}

#[tokio::test]
async fn a_super_reaction_removed_elsewhere_says_nothing_of_the_plain_one() {
    let mut online = reacting().await;
    online.added_by(ME_ID, HEART).await;
    online.reaction(ADD, ME_ID, HEART, 1).await;
    online.events().await;
    let (delete, answer_delete) = online.hold(403).await;
    let (account, _, mut wire) = online.wire();
    let emoji = heart();

    let taken = begun(account.unreact(general(), target(), &emoji, INLINE)).await;
    arrival(&delete).await;
    wire.own(REMOVE, 1).await;
    answer_delete.send(()).unwrap();

    let outcome = taken.await;
    assert!(
        matches!(outcome, Err(ReactionError::Forbidden)),
        "{outcome:?}"
    );
    assert_eq!(
        shown_by(account),
        one(HEART, 1, true),
        "the plain one is back"
    );
}

#[tokio::test]
async fn a_refused_removal_puts_the_plain_reaction_back_beside_a_super_reaction() {
    let mut online = reacting().await;
    online.added_by(ME_ID, HEART).await;
    online.reaction(ADD, ME_ID, HEART, 1).await;
    online.events().await;
    online.answer_with(refusal(403, 50013)).await;

    let outcome = online.unreact().await;

    assert!(
        matches!(outcome, Err(ReactionError::Forbidden)),
        "{outcome:?}"
    );
    assert_eq!(online.shown(), [(HEART.to_owned(), 1, 1, true, true)]);
}

// The reaction reached Discord (its echo), then a moderator removed it, then the answer came.
#[tokio::test]
async fn an_answer_never_brings_back_what_a_moderator_removed_after_the_echo() {
    for removal in [
        "MESSAGE_REACTION_REMOVE_ALL",
        "MESSAGE_REACTION_REMOVE_EMOJI",
        REMOVE,
    ] {
        let mut online = reacting().await;
        let (put, answer_put) = online.hold(204).await;
        online.answer_with(refusal(403, 50013)).await;
        let (account, server, mut wire) = online.wire();
        let emoji = heart();

        let added = begun(account.react(general(), target(), &emoji, INLINE)).await;
        arrival(&put).await;
        let taken = begun(account.unreact(general(), target(), &emoji, INLINE)).await;
        wire.own(ADD, 0).await;
        if removal == REMOVE {
            wire.own(REMOVE, 0).await;
        } else {
            wire.removal(removal).await;
        }
        answer_put.send(()).unwrap();

        added.await.unwrap();
        let _ = taken.await;
        assert_eq!(shown_by(account), [], "{removal}");
        assert_eq!(
            requests(server).await,
            ["PUT"],
            "{removal}: nothing left to take back"
        );
    }
}

// Both answers come before either echo, which then says nothing the store doesn't show.
#[tokio::test]
async fn echoes_later_than_both_answers_never_show_a_reaction_taken_back() {
    let mut online = reacting().await;
    let (put, answer_put) = online.hold(204).await;
    online.answer(204).await;
    let (account, _, mut wire) = online.wire();
    let emoji = heart();

    let added = begun(account.react(general(), target(), &emoji, INLINE)).await;
    arrival(&put).await;
    let taken = begun(account.unreact(general(), target(), &emoji, INLINE)).await;
    answer_put.send(()).unwrap();
    added.await.unwrap();
    taken.await.unwrap();
    assert_eq!(changes(&wire.events().await), 2, "the two clicks");

    assert_eq!(
        wire.own(ADD, 0).await,
        [] as [&str; 0],
        "the reaction's echo"
    );
    assert_eq!(shown_by(account), []);
    assert_eq!(
        wire.own(REMOVE, 0).await,
        [] as [&str; 0],
        "the removal's echo"
    );
    assert_eq!(shown_by(account), []);
    assert_eq!(changes(&wire.own(ADD, 0).await), 1, "made again elsewhere");
    assert_eq!(shown_by(account), one(HEART, 1, true));
}

#[tokio::test]
async fn echoes_later_than_both_answers_never_hide_a_reaction_made_again() {
    let mut online = reacting().await;
    online.added_by(MIRA, HEART).await;
    online.added_by(ME_ID, HEART).await;
    let (delete, answer_delete) = online.hold(204).await;
    online.answer(204).await;
    let (account, _, mut wire) = online.wire();
    let emoji = heart();

    let taken = begun(account.unreact(general(), target(), &emoji, INLINE)).await;
    arrival(&delete).await;
    let added = begun(account.react(general(), target(), &emoji, INLINE)).await;
    answer_delete.send(()).unwrap();
    taken.await.unwrap();
    added.await.unwrap();
    assert_eq!(changes(&wire.events().await), 2, "the two clicks");

    assert_eq!(
        wire.own(REMOVE, 0).await,
        [] as [&str; 0],
        "the removal's echo"
    );
    assert_eq!(shown_by(account), one(HEART, 2, true));
    assert_eq!(
        wire.own(ADD, 0).await,
        [] as [&str; 0],
        "the reaction's echo"
    );
    assert_eq!(shown_by(account), one(HEART, 2, true));
}

#[tokio::test]
async fn echoes_of_a_quick_toggle_that_come_after_it_say_nothing() {
    let mut online = reacting().await;
    online.answer(204).await;
    online.react().await.unwrap();
    online.unreact().await.unwrap();
    online.events().await;

    online.reaction(ADD, ME_ID, HEART, 0).await;
    assert_eq!(online.events().await, [] as [&str; 0]);
    online.reaction(REMOVE, ME_ID, HEART, 0).await;
    assert_eq!(online.events().await, [] as [&str; 0]);
    assert_eq!(online.shown(), []);
}

// A super reaction made on another device shows as it comes, between late echoes that don't.
#[tokio::test]
async fn the_other_kind_shows_at_once_between_late_echoes() {
    let mut online = reacting().await;
    online.answer(204).await;
    online.react().await.unwrap();
    online.unreact().await.unwrap();
    online.events().await;

    online.reaction(ADD, ME_ID, HEART, 1).await;
    assert_eq!(changes(&online.events().await), 1, "the super reaction");
    online.reaction(ADD, ME_ID, HEART, 0).await;
    online.reaction(REMOVE, ME_ID, HEART, 0).await;
    assert_eq!(online.events().await, [] as [&str; 0], "the echoes");
    assert_eq!(online.shown(), [(HEART.to_owned(), 0, 1, false, true)]);
}

// The first click took the super reaction back; the last takes back the reaction the store
// shows then, and Discord still has the super reaction.
#[tokio::test]
async fn a_super_reaction_taken_back_again_after_a_click_meanwhile_reports_the_refusal() {
    let mut online = reacting().await;
    online.reaction(ADD, ME_ID, HEART, 1).await;
    online.events().await;
    let (delete, answer_delete) = online.hold(403).await;
    online.answer(204).await;
    let (account, server, _) = online.wire();
    let emoji = heart();

    let taken = begun(account.unreact(general(), target(), &emoji, INLINE)).await;
    arrival(&delete).await;
    let added = begun(account.react(general(), target(), &emoji, INLINE)).await;
    let again = begun(account.unreact(general(), target(), &emoji, INLINE)).await;
    assert_eq!(shown_by(account), [], "the last click, at once");
    answer_delete.send(()).unwrap();

    taken.await.unwrap();
    added.await.unwrap();
    let outcome = again.await;
    assert!(
        matches!(outcome, Err(ReactionError::Forbidden)),
        "{outcome:?}"
    );
    assert_eq!(requests(server).await, ["DELETE"], "refused already");
    assert_eq!(
        shown_by(account),
        [(HEART.to_owned(), 0, 1, false, true)],
        "as Discord has it"
    );
}

#[tokio::test]
async fn removing_one_kind_of_ones_own_reaction_in_the_window_leaves_the_other() {
    let cases = [
        (
            ReactionKind::Burst,
            "1/@me?location=Message&burst=true",
            (HEART.to_owned(), 1, 0, true, false),
        ),
        (
            ReactionKind::Normal,
            "0/@me?location=Message&burst=false",
            (HEART.to_owned(), 0, 1, false, true),
        ),
    ];
    for (kind, path, left) in cases {
        let mut online = reacting().await;
        online.added_by(ME_ID, HEART).await;
        online.reaction(ADD, ME_ID, HEART, 1).await;
        online.events().await;
        online.answer(204).await;

        online
            .account
            .remove_reaction(general(), target(), &heart(), kind, Snowflake::new(ME_ID))
            .await
            .unwrap();

        assert_eq!(
            online.lines().await,
            [format!("DELETE {BASE}/{HEART_PATH}/{path}")],
            "{kind:?}"
        );
        assert_eq!(online.shown(), [left], "{kind:?}");
    }
}

#[tokio::test]
async fn removing_a_kind_of_ones_own_reaction_that_isnt_there_asks_nothing() {
    let mut online = reacting().await;
    online.added_by(ME_ID, HEART).await;
    online.answer(204).await;

    online
        .account
        .remove_reaction(
            general(),
            target(),
            &heart(),
            ReactionKind::Burst,
            Snowflake::new(ME_ID),
        )
        .await
        .unwrap();

    assert_eq!(online.lines().await, [] as [&str; 0]);
    assert_eq!(online.shown(), one(HEART, 1, true));
}

#[tokio::test]
async fn a_thread_that_is_open_is_not_opened_again() {
    let mut online = reacting().await;
    online.answer(204).await;
    in_the_thread(&mut online).await;

    online
        .account
        .react(
            Snowflake::new(THREAD),
            Snowflake::new(TARGET + 1),
            &heart(),
            INLINE,
        )
        .await
        .unwrap();

    assert_eq!(
        online.lines().await,
        [format!(
            "PUT /api/v9/channels/{THREAD}/messages/{}/reactions/{HEART_PATH}/@me?location=Message%20Inline%20Button&type=0",
            TARGET + 1
        )]
    );
}

#[tokio::test]
async fn a_401_on_any_reaction_request_ends_the_session() {
    for call in ["react", "remove", "reactors"] {
        let mut online = reacting().await;
        online.added_by(MIRA, HEART).await;
        online.answer(401).await;
        let account = &online.account;

        let unauthorized = match call {
            "react" => matches!(
                online.react().await,
                Err(ReactionError::Request(RequestError::Unauthorized))
            ),
            "remove" => matches!(
                account.remove_reactions(general(), target(), None).await,
                Err(ReactionError::Request(RequestError::Unauthorized))
            ),
            _ => matches!(
                account
                    .reactors(general(), target(), &heart(), ReactionKind::Normal, None, 3)
                    .await,
                Err(RequestError::Unauthorized)
            ),
        };

        assert!(unauthorized, "{call}");
        assert_eq!(
            online.connection.client_close_code().await,
            Some(1000),
            "{call}"
        );
        assert!(
            matches!(
                online.account.store().connection(),
                ConnectionState::Closed { error: Some(error) }
                    if matches!(*error, GatewayError::AuthenticationFailed)
            ),
            "{call}"
        );
    }
}

// Both are clicked while the heart is asked; their requests go after it, in the channel's turn.
#[tokio::test]
async fn each_message_and_emoji_has_its_own_requests() {
    let mut online = reacting().await;
    online
        .dispatch("MESSAGE_CREATE", message(0, TARGET + 2)["d"].clone())
        .await;
    online.events().await;
    let (arrived, release) = online.hold(204).await;
    online.answer(204).await;
    let account = &online.account;
    let (wave, emoji) = (
        ReactionEmoji::custom(Snowflake::new(7), Some("wave"), false),
        heart(),
    );
    let other = Snowflake::new(TARGET + 2);

    let (held, (waved, elsewhere)) = tokio::join!(online.react(), async {
        arrival(&arrived).await;
        let waved = begun(account.react(general(), target(), &wave, INLINE)).await;
        let elsewhere = begun(account.react(general(), other, &emoji, INLINE)).await;
        release.send(()).unwrap();
        (waved.await, elsewhere.await)
    });

    held.unwrap();
    waved.unwrap();
    elsewhere.unwrap();
    assert_eq!(
        online.shown(),
        [one(HEART, 1, true), one("wave", 1, true)].concat()
    );
    assert_eq!(online.shown_in(GENERAL, TARGET + 2), one(HEART, 1, true));
    let mut lines = online.lines().await;
    lines.sort();
    let mut expected = [
        format!("PUT {BASE}/{HEART_PATH}/@me?location=Message%20Inline%20Button&type=0"),
        format!("PUT {BASE}/wave:7/@me?location=Message%20Inline%20Button&type=0"),
        format!(
            "PUT /api/v9/channels/{GENERAL}/messages/{}/reactions/{HEART_PATH}/@me?location=Message%20Inline%20Button&type=0",
            TARGET + 2
        ),
    ];
    expected.sort();
    assert_eq!(lines, expected);
}

#[tokio::test]
async fn once_the_requests_are_over_the_gateway_says_what_is_the_users() {
    let mut online = reacting().await;
    online.answer(204).await;
    online.react().await.unwrap();
    online.reaction(ADD, ME_ID, HEART, 0).await;
    online.events().await;

    online.reaction(REMOVE, ME_ID, HEART, 0).await;

    assert_eq!(changes(&online.events().await), 1, "taken back elsewhere");
    assert_eq!(online.shown(), []);
}

// The window and the menu may know a custom emoji by its ID alone.
#[tokio::test]
async fn a_custom_emoji_without_a_name_goes_by_the_name_the_message_has() {
    let mut online = reacting().await;
    online
        .dispatch(
            ADD,
            json!({
                "channel_id": GENERAL.to_string(), "message_id": TARGET.to_string(),
                "user_id": MIRA.to_string(), "emoji": {"id": "7", "name": "wave"}, "type": 0,
            }),
        )
        .await;
    online.events().await;
    Mock::given(method("GET"))
        .and(path_regex(REACTIONS))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .with_priority(1)
        .mount(&online.server)
        .await;
    online.answer(204).await;
    let nameless = ReactionEmoji::custom(Snowflake::new(7), None, false);
    let account = &online.account;
    let mira = Snowflake::new(MIRA);

    account
        .remove_reaction(general(), target(), &nameless, ReactionKind::Normal, mira)
        .await
        .unwrap();
    account
        .remove_reactions(general(), target(), Some(&nameless))
        .await
        .unwrap();
    account
        .reactors(
            general(),
            target(),
            &nameless,
            ReactionKind::Normal,
            None,
            3,
        )
        .await
        .unwrap();

    assert_eq!(
        online.lines().await,
        [
            format!("DELETE {BASE}/wave:7/0/{MIRA}?location=Message&burst=false"),
            format!("DELETE {BASE}/wave:7"),
            format!("GET {BASE}/wave:7?limit=3&type=0"),
        ]
    );
}

// The starter row is in the thread; the message it shows, and its reactions, are #general's.
#[tokio::test]
async fn moderating_a_starter_rows_reactions_leaves_its_archived_thread_closed() {
    let mut online = with_a_starter_row().await;
    online.added_by(MIRA, HEART).await;
    Mock::given(method("PATCH"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .mount(&online.server)
        .await;
    archive_the_thread(&mut online).await;
    let (thread, row) = (Snowflake::new(THREAD), Snowflake::new(STARTER_ROW));
    let mira = Snowflake::new(MIRA);

    online
        .account
        .remove_reaction(thread, row, &heart(), ReactionKind::Normal, mira)
        .await
        .unwrap();
    online
        .account
        .remove_reactions(thread, row, None)
        .await
        .unwrap();

    assert_eq!(
        online.lines().await,
        [
            format!("DELETE {BASE}/{HEART_PATH}/0/{MIRA}?location=Message&burst=false"),
            format!("DELETE {BASE}"),
        ]
    );
}

#[tokio::test]
async fn a_page_is_full_by_what_discord_sent_and_has_its_users_as_sent() {
    let online = reacting().await;
    let ready: Value = serde_json::from_str(READY).unwrap();
    let mut renamed = ready["d"]["users"][0].clone();
    renamed["global_name"] = "Mira, renamed".into();
    Mock::given(method("GET"))
        .and(path_regex(REACTIONS))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            renamed,
            {"id": "broken"},
            user(100_000_000_000_000_051, "bo"),
        ])))
        .mount(&online.server)
        .await;

    let page = online
        .account
        .reactors(general(), target(), &heart(), ReactionKind::Normal, None, 3)
        .await
        .unwrap();

    let names: Vec<&str> = page.users.iter().map(|user| user.display_name()).collect();
    assert_eq!(
        names,
        ["Mira, renamed", "bo"],
        "a user that doesn't decode is left out"
    );
    assert!(page.has_more, "but Discord filled the page");
    let known = online.account.store().user(Snowflake::new(MIRA)).unwrap();
    assert_ne!(
        known.display_name(),
        "Mira, renamed",
        "the store's own is older"
    );
}

// PR D finding 9: a forum's default reaction names a custom emoji by its ID alone. The user's
// server has its name, so the path is `name:id`, as for any custom emoji, not `null:id`.
#[tokio::test]
async fn a_nameless_custom_emoji_takes_its_name_from_the_users_server() {
    let mut online = reacting().await;
    online
        .dispatch(
            "GUILD_EMOJIS_UPDATE",
            json!({"guild_id": G1.to_string(), "emojis": [{"id": "7", "name": "wave"}]}),
        )
        .await;
    online.events().await;
    online.answer(204).await;
    let nameless = ReactionEmoji::custom(Snowflake::new(7), None, false);

    online
        .account
        .reactors(
            general(),
            target(),
            &nameless,
            ReactionKind::Normal,
            None,
            3,
        )
        .await
        .ok();
    online
        .account
        .react(general(), target(), &nameless, INLINE)
        .await
        .unwrap();

    let lines = online.lines().await;
    assert!(
        lines[0].starts_with(&format!("GET {BASE}/wave:7?")),
        "{lines:?}"
    );
    assert_eq!(
        lines[1],
        format!("PUT {BASE}/wave:7/@me?location=Message%20Inline%20Button&type=0")
    );
    assert_eq!(online.shown(), one("wave", 1, true));
}

// Reacting through the account counts the emoji as used: the hover bar's quick reactions follow.
#[tokio::test]
async fn a_reaction_made_is_a_quick_reaction_after() {
    let online = reacting().await;
    online.answer(204).await;
    let newest = crate::emoji::EmojiVersion::NEWEST;

    online
        .account
        .react(general(), target(), &ReactionEmoji::unicode("🔥"), INLINE)
        .await
        .unwrap();

    let quick: Vec<String> = online
        .account
        .store()
        .quick_reactions(general(), newest)
        .iter()
        .map(|emoji| emoji.name().into_owned())
        .collect();
    assert_eq!(quick, ["fire", "100", "laughing"]);
}
