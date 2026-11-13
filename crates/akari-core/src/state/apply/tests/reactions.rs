use super::*;
use crate::state::StarterMessage;
use crate::state::apply::Wanted;
use crate::state::reactions::ReactionEmoji;
use crate::state::windows::StarterFetch;

const FORUM: u64 = 300_000_000_000_000_050;
const POST: u64 = 300_000_000_000_000_060;
const UP: &str = "👍";
const FIRE: &str = "🔥";

type Shown = Vec<(String, u32, u32, bool, bool)>;

fn shown(reactions: &[crate::state::Reaction]) -> Shown {
    reactions
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

fn one(emoji: &str, normal: u32, me: bool) -> Shown {
    vec![(emoji.to_owned(), normal, 0, me, false)]
}

fn reactions(state: &State, channel: u64, id: u64) -> Shown {
    shown(&state.reactions(Snowflake::new(channel), Snowflake::new(id)))
}

fn reaction(channel: u64, id: u64, user: u64, emoji: &str) -> Value {
    json!({
        "channel_id": channel.to_string(), "message_id": id.to_string(),
        "user_id": user.to_string(), "emoji": {"id": null, "name": emoji}, "type": 0,
    })
}

fn add(state: &mut State, channel: u64, id: u64, user: u64, emoji: &str) -> Vec<String> {
    apply(
        state,
        "MESSAGE_REACTION_ADD",
        reaction(channel, id, user, emoji),
    )
}

fn remove(state: &mut State, channel: u64, id: u64, user: u64, emoji: &str) -> Vec<String> {
    apply(
        state,
        "MESSAGE_REACTION_REMOVE",
        reaction(channel, id, user, emoji),
    )
}

fn changed(channel: u64, id: u64) -> String {
    format!("MessageReactionsChanged({channel}, {id})")
}

fn viewing(channel: u64) -> State {
    let mut state = ready_state();
    state.view_channel(Snowflake::new(channel), &mut Vec::new());
    state
}

fn with_reactions(mut message: Value, reactions: Value) -> Value {
    message["reactions"] = reactions;
    message
}

fn wire_reaction(emoji: &str, normal: u32, me: bool) -> Value {
    json!({
        "emoji": {"id": null, "name": emoji}, "count": normal,
        "count_details": {"normal": normal, "burst": 0}, "me": me, "me_burst": false,
    })
}

fn page(messages: &[Value]) -> Vec<model::Message> {
    messages
        .iter()
        .map(|message| serde_json::from_str(&message.to_string()).unwrap())
        .collect()
}

#[test]
fn a_message_arrives_with_its_reactions_in_discords_order() {
    let mut state = viewing(GENERAL);

    apply(
        &mut state,
        "MESSAGE_CREATE",
        with_reactions(
            message(10, GENERAL),
            json!([wire_reaction(FIRE, 2, true), wire_reaction(UP, 1, false)]),
        ),
    );
    apply(&mut state, "MESSAGE_CREATE", message(11, GENERAL));

    assert_eq!(
        reactions(&state, GENERAL, 10),
        [
            (FIRE.to_owned(), 2, 0, true, false),
            (UP.to_owned(), 1, 0, false, false)
        ]
    );
    let plain = state
        .message(Snowflake::new(GENERAL), Snowflake::new(11))
        .unwrap();
    assert_eq!(plain.reactions, None, "no list for a message without any");
    assert_eq!(reactions(&state, GENERAL, 11), []);
    assert_eq!(reactions(&state, GENERAL, 99), [], "not loaded");
}

#[test]
fn reactions_count_live_and_say_so_with_their_own_event() {
    let mut state = viewing(GENERAL);
    apply(&mut state, "MESSAGE_CREATE", message(10, GENERAL));

    let first = add(&mut state, GENERAL, 10, MIRA, UP);
    assert_eq!(first, [changed(GENERAL, 10)]);
    assert_eq!(reactions(&state, GENERAL, 10), one(UP, 1, false));

    add(&mut state, GENERAL, 10, ME, UP);
    assert_eq!(reactions(&state, GENERAL, 10), one(UP, 2, true));
    let echo = add(&mut state, GENERAL, 10, ME, UP);
    assert_eq!(echo, [] as [&str; 0], "mine is set already");
    assert_eq!(reactions(&state, GENERAL, 10), one(UP, 2, true));

    add(&mut state, GENERAL, 10, MIRA, FIRE);
    assert_eq!(
        reactions(&state, GENERAL, 10),
        [one(UP, 2, true), one(FIRE, 1, false)].concat(),
        "a new emoji goes to the end"
    );

    assert_eq!(
        remove(&mut state, GENERAL, 10, ME, UP),
        [changed(GENERAL, 10)]
    );
    assert_eq!(remove(&mut state, GENERAL, 10, ME, UP), [] as [&str; 0]);
    assert_eq!(
        reactions(&state, GENERAL, 10),
        [one(UP, 1, false), one(FIRE, 1, false)].concat()
    );

    remove(&mut state, GENERAL, 10, MIRA, UP);
    assert_eq!(reactions(&state, GENERAL, 10), one(FIRE, 1, false));
    let message = state
        .message(Snowflake::new(GENERAL), Snowflake::new(10))
        .unwrap();
    assert_eq!(message.reactions().len(), 1, "the record has them too");
}

#[test]
fn a_burst_reaction_counts_apart() {
    let mut state = viewing(GENERAL);
    apply(&mut state, "MESSAGE_CREATE", message(10, GENERAL));
    let burst = |user: u64, kind: Value| {
        let mut data = reaction(GENERAL, 10, user, UP);
        data.as_object_mut().unwrap().remove("type");
        data.as_object_mut().unwrap().extend(
            kind.as_object()
                .unwrap()
                .iter()
                .map(|(key, value)| (key.clone(), value.clone())),
        );
        data
    };

    add(&mut state, GENERAL, 10, MIRA, UP);
    apply(
        &mut state,
        "MESSAGE_REACTION_ADD",
        burst(ME, json!({"type": 1})),
    );
    apply(
        &mut state,
        "MESSAGE_REACTION_ADD",
        burst(MIRA, json!({"burst": true})),
    );
    assert_eq!(
        reactions(&state, GENERAL, 10),
        [(UP.to_owned(), 1, 2, false, true)]
    );

    apply(
        &mut state,
        "MESSAGE_REACTION_REMOVE",
        burst(ME, json!({"type": 1})),
    );
    assert_eq!(
        reactions(&state, GENERAL, 10),
        [(UP.to_owned(), 1, 1, false, false)]
    );
}

#[test]
fn removals_of_an_emoji_and_of_all_reach_the_message() {
    let mut state = viewing(GENERAL);
    apply(
        &mut state,
        "MESSAGE_CREATE",
        with_reactions(
            message(10, GENERAL),
            json!([wire_reaction(FIRE, 2, true), wire_reaction(UP, 1, false)]),
        ),
    );
    let target = json!({"channel_id": GENERAL.to_string(), "message_id": "10"});
    let mut emoji = target.clone();
    emoji["emoji"] = json!({"id": null, "name": FIRE});

    let events = apply(&mut state, "MESSAGE_REACTION_REMOVE_EMOJI", emoji.clone());
    assert_eq!(events, [changed(GENERAL, 10)]);
    assert_eq!(reactions(&state, GENERAL, 10), one(UP, 1, false));
    assert_eq!(
        apply(&mut state, "MESSAGE_REACTION_REMOVE_EMOJI", emoji),
        [] as [&str; 0]
    );

    let events = apply(&mut state, "MESSAGE_REACTION_REMOVE_ALL", target.clone());
    assert_eq!(events, [changed(GENERAL, 10)]);
    assert_eq!(reactions(&state, GENERAL, 10), []);
    let message = state
        .message(Snowflake::new(GENERAL), Snowflake::new(10))
        .unwrap();
    assert_eq!(message.reactions, None);
    assert_eq!(
        apply(&mut state, "MESSAGE_REACTION_REMOVE_ALL", target),
        [] as [&str; 0]
    );
}

#[test]
fn a_debounced_batch_adds_each_user_once() {
    let mut state = viewing(GENERAL);
    apply(&mut state, "MESSAGE_CREATE", message(10, GENERAL));
    add(&mut state, GENERAL, 10, ME, UP);

    let events = apply(
        &mut state,
        "MESSAGE_REACTION_ADD_MANY",
        json!({
            "channel_id": GENERAL.to_string(), "message_id": "10",
            "reactions": [
                {"users": [ME.to_string(), MIRA.to_string(), "100000000000000009"],
                 "emoji": {"id": null, "name": UP}},
                {"users": [MIRA.to_string()], "emoji": {"id": null, "name": FIRE}},
            ],
        }),
    );

    assert_eq!(events, [changed(GENERAL, 10)], "one event for the batch");
    assert_eq!(
        reactions(&state, GENERAL, 10),
        [one(UP, 3, true), one(FIRE, 1, false)].concat()
    );
}

#[test]
fn an_edit_keeps_the_reactions() {
    let mut state = viewing(GENERAL);
    apply(&mut state, "MESSAGE_CREATE", message(10, GENERAL));
    add(&mut state, GENERAL, 10, MIRA, UP);

    let events = apply(
        &mut state,
        "MESSAGE_UPDATE",
        json!({
            "id": "10", "channel_id": GENERAL.to_string(), "content": "edited",
            "edited_timestamp": "2026-10-08T12:00:00+00:00", "reactions": [],
        }),
    );

    assert_eq!(events, ["MessageUpdated(10)"]);
    assert_eq!(reactions(&state, GENERAL, 10), one(UP, 1, false));
}

#[test]
fn a_page_that_brings_the_message_again_replaces_its_reactions() {
    let mut state = viewing(GENERAL);
    load_page(&mut state, GENERAL, LoadKind::Latest, &[10, 11], true);
    add(&mut state, GENERAL, 10, MIRA, UP);
    add(&mut state, GENERAL, 11, MIRA, UP);

    let mut events = Vec::new();
    let ticket = state
        .begin_load(Snowflake::new(GENERAL), LoadKind::Latest, 2, &mut events)
        .unwrap();
    let fresh = [
        with_reactions(message(10, GENERAL), json!([wire_reaction(FIRE, 4, true)])),
        message(11, GENERAL),
    ];
    state.finish_load(ticket, page(&fresh), true, &mut events);

    let events: Vec<String> = events.iter().map(describe).collect();
    assert!(
        events.contains(&"MessageUpdated(10)".to_owned()),
        "{events:?}"
    );
    assert_eq!(reactions(&state, GENERAL, 10), one(FIRE, 4, true));
    assert_eq!(reactions(&state, GENERAL, 11), []);
}

#[test]
fn a_reaction_for_a_message_no_one_shows_changes_nothing() {
    let mut state = viewing(GENERAL);
    apply(&mut state, "MESSAGE_CREATE", message(10, GENERAL));

    assert_eq!(add(&mut state, GENERAL, 99, MIRA, UP), [] as [&str; 0]);
    assert_eq!(add(&mut state, VOICE, 10, MIRA, UP), [] as [&str; 0]);
    assert_eq!(reactions(&state, GENERAL, 10), []);
}

// The other user of a DM reacts with an emoji at most once: a second add is Discord repeating.
#[test]
fn a_dms_other_user_counts_once() {
    let mut state = viewing(DM);
    apply(&mut state, "MESSAGE_CREATE", dm_message(10, DM));

    add(&mut state, DM, 10, MIRA, UP);
    let repeated = add(&mut state, DM, 10, MIRA, UP);
    add(&mut state, DM, 10, ME, UP);

    assert_eq!(repeated, [] as [&str; 0]);
    assert_eq!(reactions(&state, DM, 10), one(UP, 2, true));

    let mut state = viewing(GROUP);
    apply(&mut state, "MESSAGE_CREATE", dm_message(10, GROUP));
    add(&mut state, GROUP, 10, MIRA, UP);
    add(&mut state, GROUP, 10, 100_000_000_000_000_003, UP);
    assert_eq!(reactions(&state, GROUP, 10), one(UP, 2, false));
}

// The copies of a message that take a reaction, each with the read that shows it.

struct Copy {
    name: &'static str,
    state: State,
    channel: u64,
    id: u64,
    read: fn(&State) -> Shown,
}

const ORIGIN: u64 = 400_000_000_000_000_019;

fn starter_row(with_origin: bool) -> Value {
    let mut starter = message(400_000_000_000_000_021, THREAD);
    starter["type"] = 21.into();
    starter["message_reference"] = json!({
        "message_id": ORIGIN.to_string(), "channel_id": GENERAL.to_string(),
    });
    if with_origin {
        starter["referenced_message"] = message(ORIGIN, GENERAL);
    }
    starter
}

fn starter_reactions(state: &State) -> Shown {
    match state.starter_message(Snowflake::new(THREAD)) {
        Some(StarterMessage::Loaded(message)) => shown(message.reactions()),
        other => panic!("expected a loaded starter, got {other:?}"),
    }
}

fn forum_post() -> State {
    let mut state = ready_state();
    let mut forum = fixture(include_str!(
        "../../../../tests/fixtures/channel_guild_text.json"
    ));
    forum["id"] = FORUM.to_string().into();
    forum["type"] = 15.into();
    apply(&mut state, "CHANNEL_CREATE", forum);
    let mut post = fixture(include_str!(
        "../../../../tests/fixtures/thread_create.json"
    ));
    post["id"] = POST.to_string().into();
    post["parent_id"] = FORUM.to_string().into();
    apply(&mut state, "THREAD_CREATE", post);
    state.open_forum(Some(Snowflake::new(FORUM)), &mut Vec::new());
    state
}

fn copies() -> Vec<Copy> {
    let mut window = viewing(GENERAL);
    apply(&mut window, "MESSAGE_CREATE", message(10, GENERAL));

    // Held until its stale window is refreshed.
    let mut held = viewing(GENERAL);
    held.windows.mark_stale(&mut Vec::new());
    apply(&mut held, "MESSAGE_CREATE", message(10, GENERAL));
    assert_eq!(held.windows.held(Snowflake::new(GENERAL)).len(), 1);

    let mut latest = ready_state();
    apply(&mut latest, "MESSAGE_CREATE", message(10, THREAD));
    assert!(latest.latest_message(Snowflake::new(THREAD)).is_some());

    let mut preview = forum_post();
    apply(&mut preview, "MESSAGE_CREATE", message(POST, POST));
    assert!(preview.post_preview(Snowflake::new(POST)).is_some());

    let mut fetched = viewing(THREAD);
    apply(&mut fetched, "MESSAGE_CREATE", starter_row(false));
    let thread = Snowflake::new(THREAD);
    assert!(fetched.starter_to_fetch(thread).is_some());
    let origin = page(&[message(ORIGIN, GENERAL)]).remove(0);
    fetched.settle_starter(
        thread,
        StarterFetch::Found(Box::new(origin)),
        &mut Vec::new(),
    );

    let mut sent_along = viewing(THREAD);
    apply(&mut sent_along, "MESSAGE_CREATE", starter_row(true));

    vec![
        Copy {
            name: "window",
            state: window,
            channel: GENERAL,
            id: 10,
            read: |state| reactions(state, GENERAL, 10),
        },
        Copy {
            name: "held",
            state: held,
            channel: GENERAL,
            id: 10,
            read: |state| shown(state.windows.held(Snowflake::new(GENERAL))[0].reactions()),
        },
        Copy {
            name: "thread box latest",
            state: latest,
            channel: THREAD,
            id: 10,
            read: |state| {
                shown(
                    state
                        .latest_message(Snowflake::new(THREAD))
                        .unwrap()
                        .reactions(),
                )
            },
        },
        Copy {
            name: "post preview",
            state: preview,
            channel: POST,
            id: POST,
            read: |state| match state.post_preview(Snowflake::new(POST)) {
                Some(crate::state::PostPreview::Loaded(message)) => shown(message.reactions()),
                other => panic!("expected a preview, got {other:?}"),
            },
        },
        Copy {
            name: "fetched starter",
            state: fetched,
            channel: GENERAL,
            id: ORIGIN,
            read: starter_reactions,
        },
        Copy {
            name: "starter sent along",
            state: sent_along,
            channel: GENERAL,
            id: ORIGIN,
            read: starter_reactions,
        },
    ]
}

#[test]
fn every_copy_of_a_message_takes_every_reaction_dispatch() {
    for mut copy in copies() {
        let Copy {
            name, channel, id, ..
        } = copy;
        let state = &mut copy.state;
        let read = copy.read;
        let target = json!({"channel_id": channel.to_string(), "message_id": id.to_string()});

        add(state, channel, id, MIRA, UP);
        assert_eq!(read(state), one(UP, 1, false), "{name}: add");

        add(state, channel, id, ME, UP);
        add(state, channel, id, ME, UP);
        assert_eq!(read(state), one(UP, 2, true), "{name}: my add, once");

        remove(state, channel, id, MIRA, UP);
        assert_eq!(read(state), one(UP, 1, true), "{name}: remove");

        remove(state, channel, id, ME, UP);
        remove(state, channel, id, ME, UP);
        assert_eq!(read(state), [], "{name}: my removal, once");

        let mut many = target.clone();
        many["reactions"] = json!([
            {"users": [MIRA.to_string(), ME.to_string()], "emoji": {"id": null, "name": UP}},
            {"users": [MIRA.to_string()], "emoji": {"id": null, "name": FIRE}},
        ]);
        apply(state, "MESSAGE_REACTION_ADD_MANY", many);
        assert_eq!(
            read(state),
            [one(UP, 2, true), one(FIRE, 1, false)].concat(),
            "{name}: add many"
        );

        let mut emoji = target.clone();
        emoji["emoji"] = json!({"id": null, "name": UP});
        apply(state, "MESSAGE_REACTION_REMOVE_EMOJI", emoji);
        assert_eq!(read(state), one(FIRE, 1, false), "{name}: remove emoji");

        apply(state, "MESSAGE_REACTION_REMOVE_ALL", target);
        assert_eq!(read(state), [], "{name}: remove all");
    }
}

// The read by IDs finds every copy that something shows; a held message isn't shown yet.
#[test]
fn reactions_reads_every_copy() {
    for mut copy in copies() {
        let Copy {
            name, channel, id, ..
        } = copy;

        add(&mut copy.state, channel, id, MIRA, UP);

        let expected = if name == "held" {
            vec![]
        } else {
            one(UP, 1, false)
        };
        assert_eq!(reactions(&copy.state, channel, id), expected, "{name}");
        assert_eq!((copy.read)(&copy.state), one(UP, 1, false), "{name}");
    }
}

#[test]
fn every_copy_keeps_its_reactions_over_an_edit() {
    for mut copy in copies() {
        let Copy {
            name, channel, id, ..
        } = copy;
        add(&mut copy.state, channel, id, MIRA, UP);

        apply(
            &mut copy.state,
            "MESSAGE_UPDATE",
            json!({
                "id": id.to_string(), "channel_id": channel.to_string(), "content": "edited",
                "edited_timestamp": "2026-10-08T12:00:00+00:00",
            }),
        );

        assert_eq!((copy.read)(&copy.state), one(UP, 1, false), "{name}");
    }
}

#[test]
fn each_copy_tells_what_shows_it() {
    let events: Vec<(&str, Vec<String>)> = copies()
        .into_iter()
        .map(|mut copy| {
            let events = add(&mut copy.state, copy.channel, copy.id, MIRA, UP);
            (copy.name, events)
        })
        .collect();

    let row = 400_000_000_000_000_021_u64;
    assert_eq!(
        events,
        [
            ("window", vec![changed(GENERAL, 10)]),
            ("held", vec![]),
            ("thread box latest", vec![]),
            ("post preview", vec![format!("PostPreviewChanged({POST})")]),
            ("fetched starter", vec![changed(THREAD, row)]),
            ("starter sent along", vec![changed(THREAD, row)]),
        ]
    );
}

// A viewed thread's starter row answers with the reactions of the message it shows, and follows
// the parent's window when that has the message.
#[test]
fn a_starter_row_reads_the_reactions_of_its_message() {
    let mut state = viewing(THREAD);
    apply(&mut state, "MESSAGE_CREATE", starter_row(true));
    state.view_channel(Snowflake::new(GENERAL), &mut Vec::new());
    apply(&mut state, "MESSAGE_CREATE", message(ORIGIN, GENERAL));
    let row = 400_000_000_000_000_021_u64;

    let events = add(&mut state, GENERAL, ORIGIN, MIRA, UP);

    assert_eq!(events, [changed(GENERAL, ORIGIN), changed(THREAD, row)]);
    assert_eq!(reactions(&state, THREAD, row), one(UP, 1, false));
    assert_eq!(reactions(&state, GENERAL, ORIGIN), one(UP, 1, false));
}

// The row reads the parent's window, so it is told of a change there that its own copy of the
// message doesn't take: here my reaction, which the copy sent along already had.
#[test]
fn a_starter_row_is_told_when_only_the_parents_row_changes() {
    let mut state = viewing(THREAD);
    let mut row = starter_row(true);
    row["referenced_message"]["reactions"] = json!([wire_reaction(UP, 1, true)]);
    apply(&mut state, "MESSAGE_CREATE", row);
    state.view_channel(Snowflake::new(GENERAL), &mut Vec::new());
    apply(&mut state, "MESSAGE_CREATE", message(ORIGIN, GENERAL));
    let row = 400_000_000_000_000_021_u64;
    assert_eq!(reactions(&state, THREAD, row), []);

    let events = add(&mut state, GENERAL, ORIGIN, ME, UP);

    assert_eq!(events, [changed(GENERAL, ORIGIN), changed(THREAD, row)]);
    assert_eq!(reactions(&state, THREAD, row), one(UP, 1, true));
}

// A post's card reads its window's first message when post-data brought none.
#[test]
fn a_card_reading_the_posts_window_is_told() {
    let mut state = forum_post();
    state.open_forum(None, &mut Vec::new());
    state.view_channel(Snowflake::new(POST), &mut Vec::new());
    apply(&mut state, "MESSAGE_CREATE", message(POST, POST));
    state.open_forum(Some(Snowflake::new(FORUM)), &mut Vec::new());

    let mut events = add(&mut state, POST, POST, MIRA, UP);
    events.sort();

    assert_eq!(
        events,
        [changed(POST, POST), format!("PostPreviewChanged({POST})")]
    );
}

#[test]
fn a_repeated_add_or_another_messages_reaction_tells_no_card() {
    let mut state = forum_post();
    state.open_forum(None, &mut Vec::new());
    state.view_channel(Snowflake::new(POST), &mut Vec::new());
    apply(&mut state, "MESSAGE_CREATE", message(POST, POST));
    apply(&mut state, "MESSAGE_CREATE", message(POST + 1, POST));
    state.open_forum(Some(Snowflake::new(FORUM)), &mut Vec::new());
    let card = format!("PostPreviewChanged({POST})");
    assert!(add(&mut state, POST, POST, ME, UP).contains(&card));

    let repeated = add(&mut state, POST, POST, ME, UP);
    let another = add(&mut state, POST, POST + 1, MIRA, UP);

    assert_eq!(repeated, [] as [&str; 0]);
    assert_eq!(another, [changed(POST, POST + 1)]);
}

// The debounced batch is counted like single adds: Discord may repeat the other user's.
#[test]
fn a_dm_batch_counts_the_other_user_once() {
    let mut state = viewing(DM);
    apply(&mut state, "MESSAGE_CREATE", dm_message(10, DM));
    add(&mut state, DM, 10, MIRA, UP);

    let events = apply(
        &mut state,
        "MESSAGE_REACTION_ADD_MANY",
        json!({
            "channel_id": DM.to_string(), "message_id": "10",
            "reactions": [
                {"users": [MIRA.to_string()], "emoji": {"id": null, "name": UP}},
            ],
        }),
    );
    assert_eq!(events, [] as [&str; 0]);
    assert_eq!(reactions(&state, DM, 10), one(UP, 1, false));

    apply(
        &mut state,
        "MESSAGE_REACTION_ADD_MANY",
        json!({
            "channel_id": DM.to_string(), "message_id": "10",
            "reactions": [
                {"users": [ME.to_string(), MIRA.to_string()], "emoji": {"id": null, "name": UP}},
                {"users": [MIRA.to_string(), ME.to_string()], "emoji": {"id": null, "name": FIRE}},
            ],
        }),
    );
    assert_eq!(
        reactions(&state, DM, 10),
        [one(UP, 2, true), one(FIRE, 2, true)].concat()
    );
}

const PARTY: u64 = 800_000_000_000_000_001;

fn custom(channel: u64, id: u64, user: u64, emoji: u64, name: Value) -> Value {
    json!({
        "channel_id": channel.to_string(), "message_id": id.to_string(),
        "user_id": user.to_string(), "emoji": {"id": emoji.to_string(), "name": name}, "type": 0,
    })
}

// A custom emoji is its ID: a rename or a missing name is the same reaction, another emoji of
// the same name is not.
#[test]
fn custom_emoji_are_counted_by_their_id() {
    let mut state = viewing(GENERAL);
    apply(&mut state, "MESSAGE_CREATE", message(10, GENERAL));
    let read = |state: &State| -> Vec<(Option<u64>, String, u32)> {
        state
            .reactions(Snowflake::new(GENERAL), Snowflake::new(10))
            .iter()
            .map(|reaction| {
                (
                    reaction.emoji.id.map(|id| id.get()),
                    reaction.emoji.name.as_deref().unwrap_or("").to_owned(),
                    reaction.normal,
                )
            })
            .collect()
    };

    for (user, emoji, name) in [
        (MIRA, PARTY, json!("party")),
        (ME, PARTY, json!("renamed")),
        (100_000_000_000_000_003, PARTY, Value::Null),
        (MIRA, PARTY + 1, json!("party")),
    ] {
        let add = custom(GENERAL, 10, user, emoji, name);
        apply(&mut state, "MESSAGE_REACTION_ADD", add);
    }
    assert_eq!(
        read(&state),
        [
            (Some(PARTY), "party".to_owned(), 3),
            (Some(PARTY + 1), "party".to_owned(), 1)
        ]
    );

    let renamed = custom(GENERAL, 10, MIRA, PARTY, json!("renamed"));
    apply(&mut state, "MESSAGE_REACTION_REMOVE", renamed);
    assert_eq!(read(&state)[0], (Some(PARTY), "party".to_owned(), 2));

    apply(
        &mut state,
        "MESSAGE_REACTION_REMOVE_EMOJI",
        json!({
            "channel_id": GENERAL.to_string(), "message_id": "10",
            "emoji": {"id": PARTY.to_string(), "name": null},
        }),
    );
    assert_eq!(read(&state), [(Some(PARTY + 1), "party".to_owned(), 1)]);
}

// A page asked before a reaction came may lack it; the reaction is put over the page.
#[test]
fn a_reaction_during_a_load_is_replayed_over_the_page() {
    let mut state = viewing(GENERAL);
    let mut events = Vec::new();
    let ticket = state
        .begin_load(Snowflake::new(GENERAL), LoadKind::Latest, 1, &mut events)
        .unwrap();

    add(&mut state, GENERAL, 10, MIRA, UP);
    add(&mut state, GENERAL, 10, ME, FIRE);
    remove(&mut state, GENERAL, 11, MIRA, UP);
    let stale = [
        message(10, GENERAL),
        with_reactions(message(11, GENERAL), json!([wire_reaction(UP, 2, false)])),
    ];
    state.finish_load(ticket, page(&stale), true, &mut events);

    assert_eq!(
        reactions(&state, GENERAL, 10),
        [one(UP, 1, false), one(FIRE, 1, true)].concat()
    );
    assert_eq!(reactions(&state, GENERAL, 11), one(UP, 1, false));
}

#[test]
fn removals_of_an_emoji_and_of_all_during_a_load_are_put_over_the_page() {
    let mut state = viewing(GENERAL);
    let mut events = Vec::new();
    let ticket = state
        .begin_load(Snowflake::new(GENERAL), LoadKind::Latest, 1, &mut events)
        .unwrap();

    apply(
        &mut state,
        "MESSAGE_REACTION_REMOVE_EMOJI",
        json!({
            "channel_id": GENERAL.to_string(), "message_id": "10",
            "emoji": {"id": null, "name": FIRE},
        }),
    );
    apply(
        &mut state,
        "MESSAGE_REACTION_REMOVE_ALL",
        json!({"channel_id": GENERAL.to_string(), "message_id": "11"}),
    );
    let stale = [
        with_reactions(
            message(10, GENERAL),
            json!([wire_reaction(FIRE, 2, true), wire_reaction(UP, 1, false)]),
        ),
        with_reactions(message(11, GENERAL), json!([wire_reaction(UP, 2, false)])),
        with_reactions(message(12, GENERAL), json!([wire_reaction(FIRE, 1, false)])),
    ];
    state.finish_load(ticket, page(&stale), true, &mut events);

    assert_eq!(reactions(&state, GENERAL, 10), one(UP, 1, false));
    assert_eq!(reactions(&state, GENERAL, 11), []);
    assert_eq!(reactions(&state, GENERAL, 12), one(FIRE, 1, false));
}

// A stale window's copy missed what came while the session was away, so it can't stand in
// for the page: the reaction goes over what the page brought.
#[test]
fn a_reaction_during_a_stale_reload_is_put_over_the_page() {
    let mut state = viewing(GENERAL);
    load_page(&mut state, GENERAL, LoadKind::Latest, &[10], true);
    state.windows.mark_stale(&mut Vec::new());
    let mut events = Vec::new();
    let ticket = state
        .begin_load(Snowflake::new(GENERAL), LoadKind::Refresh, 2, &mut events)
        .unwrap();

    add(&mut state, GENERAL, 10, MIRA, UP);
    let fresh = with_reactions(message(10, GENERAL), json!([wire_reaction(FIRE, 4, true)]));
    state.finish_load(ticket, page(&[fresh]), true, &mut events);

    assert_eq!(
        reactions(&state, GENERAL, 10),
        [one(FIRE, 4, true), one(UP, 1, false)].concat()
    );
}

// A message that came while a load ran waits beside the window and counts its reactions there.
#[test]
fn a_reaction_on_a_held_message_survives_the_page() {
    for page_has_it in [false, true] {
        let mut state = viewing(GENERAL);
        let general = Snowflake::new(GENERAL);
        let mut events = Vec::new();
        let around = state
            .begin_load(
                general,
                LoadKind::Around(Snowflake::new(20)),
                1,
                &mut events,
            )
            .unwrap();
        let middle = page(&[message(10, GENERAL), message(20, GENERAL)]);
        state.finish_load(around, middle, false, &mut events);
        let latest = state
            .begin_load(general, LoadKind::Latest, 2, &mut events)
            .unwrap();
        apply(&mut state, "MESSAGE_CREATE", message(30, GENERAL));
        assert_eq!(state.windows.held(general).len(), 1);

        add(&mut state, GENERAL, 30, MIRA, UP);
        let fresh = if page_has_it {
            with_reactions(message(30, GENERAL), json!([wire_reaction(UP, 1, false)]))
        } else {
            message(30, GENERAL)
        };
        state.finish_load(latest, page(&[fresh]), true, &mut events);

        assert_eq!(message_ids(&state, GENERAL), Some(vec![30]));
        assert_eq!(
            reactions(&state, GENERAL, 30),
            one(UP, 1, false),
            "page has it: {page_has_it}"
        );
    }
}

// The window's own copy counted every reaction since it was current: where the page brings a
// message it has, the copy's reactions are exact, whether or not the page had the reaction.
#[test]
fn a_reaction_during_a_reload_is_never_counted_twice() {
    for page_has_it in [false, true] {
        let mut state = viewing(GENERAL);
        load_page(&mut state, GENERAL, LoadKind::Latest, &[10], true);
        let mut events = Vec::new();
        let ticket = state
            .begin_load(Snowflake::new(GENERAL), LoadKind::Latest, 2, &mut events)
            .unwrap();

        add(&mut state, GENERAL, 10, MIRA, UP);
        let fresh = if page_has_it {
            with_reactions(message(10, GENERAL), json!([wire_reaction(UP, 1, false)]))
        } else {
            message(10, GENERAL)
        };
        state.finish_load(ticket, page(&[fresh]), true, &mut events);

        assert_eq!(
            reactions(&state, GENERAL, 10),
            one(UP, 1, false),
            "page has it: {page_has_it}"
        );
    }
}

// A reaction that came before a load began is in its page; only the loads already running
// when it came may lack it.
#[test]
fn a_reaction_is_replayed_only_over_the_loads_that_ran_when_it_came() {
    for add_it in [true, false] {
        let mut state = viewing(GENERAL);
        let general = Snowflake::new(GENERAL);
        let mut events = Vec::new();
        let around = state
            .begin_load(
                general,
                LoadKind::Around(Snowflake::new(20)),
                1,
                &mut events,
            )
            .unwrap();
        let middle = page(&[message(10, GENERAL), message(20, GENERAL)]);
        state.finish_load(around, middle, false, &mut events);
        let older = state
            .begin_load(general, LoadKind::Older, 2, &mut events)
            .unwrap();

        if add_it {
            add(&mut state, GENERAL, 30, MIRA, UP);
        } else {
            remove(&mut state, GENERAL, 30, MIRA, UP);
        }
        let newer = state
            .begin_load(general, LoadKind::Newer, 3, &mut events)
            .unwrap();
        state.finish_load(older, page(&[message(5, GENERAL)]), true, &mut events);
        let fresh = with_reactions(message(30, GENERAL), json!([wire_reaction(UP, 1, false)]));
        state.finish_load(newer, page(&[fresh]), true, &mut events);

        assert_eq!(message_ids(&state, GENERAL), Some(vec![5, 10, 20, 30]));
        assert_eq!(
            reactions(&state, GENERAL, 30),
            one(UP, 1, false),
            "added: {add_it}"
        );
    }
}

#[test]
fn many_messages_reactions_are_read_at_once_and_only_those_with_some() {
    let mut state = viewing(GENERAL);
    for id in [10, 11, 12] {
        apply(&mut state, "MESSAGE_CREATE", message(id, GENERAL));
    }
    add(&mut state, GENERAL, 12, MIRA, FIRE);
    add(&mut state, GENERAL, 10, MIRA, UP);
    let ids = [12, 99, 11, 10].map(Snowflake::new);

    let read: Vec<(u64, Shown)> = state
        .reactions_of(Snowflake::new(GENERAL), &ids)
        .iter()
        .map(|(id, reactions)| (id.get(), shown(reactions)))
        .collect();

    assert_eq!(read, [(12, one(FIRE, 1, false)), (10, one(UP, 1, false))]);
}

// Messages are held by the hundred; a reaction list costs one pointer pair on those without any.
#[test]
fn a_message_stays_small() {
    assert!(size_of::<Message>() <= 216, "{}", size_of::<Message>());
    assert!(size_of::<crate::state::Reaction>() <= 56);
    assert_eq!(
        size_of::<Option<Arc<[crate::state::Reaction]>>>(),
        size_of::<Arc<[crate::state::Reaction]>>()
    );
}

const STARTER_ROW: u64 = 400_000_000_000_000_021;

fn emoji(name: &str) -> ReactionEmoji {
    ReactionEmoji::unicode(name)
}

// The user's click, with the events it sent.
fn want(
    state: &mut State,
    channel: u64,
    row: u64,
    name: &str,
    wanted: bool,
) -> (Option<Wanted>, Vec<String>) {
    let mut events = Vec::new();
    let now = state.now_millis();
    let wanted = state.want_reaction(
        Snowflake::new(channel),
        Snowflake::new(row),
        &emoji(name),
        wanted,
        now,
        &mut events,
    );
    (wanted, events.iter().map(describe).collect())
}

fn settle(state: &mut State, wanted: &Wanted, restore: Option<(bool, bool)>) -> Vec<String> {
    let mut events = Vec::new();
    state.settle_reaction(
        wanted.channel,
        wanted.message,
        &wanted.emoji,
        restore,
        &mut events,
    );
    events.iter().map(describe).collect()
}

#[test]
fn the_users_click_shows_at_once_and_its_echo_changes_nothing() {
    let mut state = viewing(GENERAL);
    apply(&mut state, "MESSAGE_CREATE", message(10, GENERAL));
    add(&mut state, GENERAL, 10, MIRA, UP);

    let (wanted, events) = want(&mut state, GENERAL, 10, UP, true);

    let wanted = wanted.unwrap();
    assert!(wanted.changed);
    assert_eq!(
        (wanted.channel.get(), wanted.message.get(), wanted.burst),
        (GENERAL, 10, false)
    );
    assert_eq!(events, [changed(GENERAL, 10)]);
    assert_eq!(reactions(&state, GENERAL, 10), one(UP, 2, true));
    assert_eq!(add(&mut state, GENERAL, 10, ME, UP), [] as [&str; 0]);
    assert_eq!(reactions(&state, GENERAL, 10), one(UP, 2, true));

    let (again, events) = want(&mut state, GENERAL, 10, UP, true);
    assert!(!again.unwrap().changed, "already the user's");
    assert_eq!(events, [] as [&str; 0]);

    let (taken, events) = want(&mut state, GENERAL, 10, UP, false);
    assert!(taken.unwrap().changed);
    assert_eq!(events, [changed(GENERAL, 10)]);
    assert_eq!(reactions(&state, GENERAL, 10), one(UP, 1, false));
    assert_eq!(remove(&mut state, GENERAL, 10, ME, UP), [] as [&str; 0]);
    assert_eq!(reactions(&state, GENERAL, 10), one(UP, 1, false));

    let (again, events) = want(&mut state, GENERAL, 10, UP, false);
    assert!(!again.unwrap().changed, "not the user's");
    assert_eq!(events, [] as [&str; 0]);
}

// What the user can click on is a row or a card; the copies behind take the click with it.
#[test]
fn every_copy_of_a_message_takes_the_users_click() {
    for mut copy in copies() {
        let Copy {
            name, channel, id, ..
        } = copy;
        let state = &mut copy.state;
        let read = copy.read;
        let (at, row) = if name.contains("starter") {
            (THREAD, STARTER_ROW)
        } else {
            (channel, id)
        };

        let (wanted, _) = want(state, at, row, UP, true);

        if matches!(name, "held" | "thread box latest") {
            assert_eq!(wanted, None, "{name}: nothing shows it");
            continue;
        }
        let wanted = wanted.unwrap_or_else(|| panic!("{name}: refused"));
        assert_eq!(
            (wanted.channel.get(), wanted.message.get()),
            (channel, id),
            "{name}: the message the row shows"
        );
        assert_eq!(read(state), one(UP, 1, true), "{name}: reacted");

        settle(state, &wanted, Some((false, false)));
        assert_eq!(read(state), [], "{name}: refused, taken back");
    }
}

#[test]
fn a_click_on_the_row_reaches_the_starter_the_box_and_the_card() {
    let mut state = viewing(GENERAL);
    apply(&mut state, "MESSAGE_CREATE", message(ORIGIN, GENERAL));
    state.view_channel(Snowflake::new(THREAD), &mut Vec::new());
    apply(&mut state, "MESSAGE_CREATE", starter_row(true));
    apply(&mut state, "MESSAGE_CREATE", message(30, THREAD));

    let (_, events) = want(&mut state, GENERAL, ORIGIN, UP, true);
    assert_eq!(starter_reactions(&state), one(UP, 1, true));
    assert_eq!(
        events,
        [changed(GENERAL, ORIGIN), changed(THREAD, STARTER_ROW)]
    );

    let (_, events) = want(&mut state, THREAD, STARTER_ROW, FIRE, true);
    assert_eq!(
        reactions(&state, GENERAL, ORIGIN),
        [one(UP, 1, true), one(FIRE, 1, true)].concat(),
        "the parent's row"
    );
    assert_eq!(
        events,
        [changed(GENERAL, ORIGIN), changed(THREAD, STARTER_ROW)]
    );

    want(&mut state, THREAD, 30, UP, true);
    let latest = state.latest_message(Snowflake::new(THREAD)).unwrap();
    assert_eq!(shown(latest.reactions()), one(UP, 1, true), "the box");

    let mut forum = forum_post();
    apply(&mut forum, "MESSAGE_CREATE", message(POST, POST));
    forum.view_channel(Snowflake::new(POST), &mut Vec::new());
    apply(&mut forum, "MESSAGE_CREATE", message(POST + 1, POST));
    let (_, events) = want(&mut forum, POST, POST, UP, true);
    assert!(
        events
            .iter()
            .any(|event| event.starts_with("PostPreviewChanged")),
        "{events:?}"
    );
    match forum.post_preview(Snowflake::new(POST)) {
        Some(crate::state::PostPreview::Loaded(message)) => {
            assert_eq!(shown(message.reactions()), one(UP, 1, true), "the card");
        }
        other => panic!("expected a preview, got {other:?}"),
    }
}

#[test]
fn a_refused_click_goes_back_to_what_discord_had() {
    let mut state = viewing(GENERAL);
    apply(&mut state, "MESSAGE_CREATE", message(10, GENERAL));
    add(&mut state, GENERAL, 10, MIRA, UP);
    add(&mut state, GENERAL, 10, ME, FIRE);

    let (added, _) = want(&mut state, GENERAL, 10, UP, true);
    let events = settle(&mut state, &added.unwrap(), Some((false, false)));
    assert_eq!(events, [changed(GENERAL, 10)]);
    assert_eq!(
        reactions(&state, GENERAL, 10),
        [one(UP, 1, false), one(FIRE, 1, true)].concat()
    );

    let (taken, _) = want(&mut state, GENERAL, 10, FIRE, false);
    assert_eq!(reactions(&state, GENERAL, 10), one(UP, 1, false));
    let events = settle(&mut state, &taken.unwrap(), Some((true, false)));
    assert_eq!(events, [changed(GENERAL, 10)]);
    assert_eq!(
        reactions(&state, GENERAL, 10),
        [one(UP, 1, false), one(FIRE, 1, true)].concat(),
        "back at the end of the row"
    );
}

#[test]
fn a_settled_click_without_a_refusal_changes_nothing() {
    let mut state = viewing(GENERAL);
    apply(&mut state, "MESSAGE_CREATE", message(10, GENERAL));

    let (added, _) = want(&mut state, GENERAL, 10, UP, true);
    let events = settle(&mut state, &added.unwrap(), None);

    assert_eq!(events, [] as [&str; 0]);
    assert_eq!(reactions(&state, GENERAL, 10), one(UP, 1, true));
}

// What the gateway said while the request ran is newer than what Discord had before it.
#[test]
fn a_refusal_never_undoes_what_the_gateway_said_meanwhile() {
    let target = json!({"channel_id": GENERAL.to_string(), "message_id": "10"});
    let mut emoji_removal = target.clone();
    emoji_removal["emoji"] = json!({"id": null, "name": UP});
    let cases: [(&str, &str, Value); 3] = [
        ("remove all", "MESSAGE_REACTION_REMOVE_ALL", target),
        (
            "remove emoji",
            "MESSAGE_REACTION_REMOVE_EMOJI",
            emoji_removal,
        ),
        (
            "removed on another device",
            "MESSAGE_REACTION_REMOVE",
            reaction(GENERAL, 10, ME, UP),
        ),
    ];
    for (name, dispatch, data) in cases {
        let mut state = viewing(GENERAL);
        apply(&mut state, "MESSAGE_CREATE", message(10, GENERAL));
        add(&mut state, GENERAL, 10, ME, UP);

        let (taken, _) = want(&mut state, GENERAL, 10, UP, false);
        apply(&mut state, dispatch, data);
        let events = settle(&mut state, &taken.unwrap(), Some((true, false)));

        assert_eq!(events, [] as [&str; 0], "{name}");
        assert_eq!(reactions(&state, GENERAL, 10), [], "{name}: not put back");
    }

    let mut state = viewing(GENERAL);
    apply(&mut state, "MESSAGE_CREATE", message(10, GENERAL));
    let (added, _) = want(&mut state, GENERAL, 10, UP, true);
    add(&mut state, GENERAL, 10, ME, UP);
    settle(&mut state, &added.unwrap(), Some((false, false)));
    assert_eq!(
        reactions(&state, GENERAL, 10),
        one(UP, 1, true),
        "added on another device: kept"
    );

    let mut state = viewing(GENERAL);
    apply(&mut state, "MESSAGE_CREATE", message(10, GENERAL));
    add(&mut state, GENERAL, 10, ME, UP);
    let (taken, _) = want(&mut state, GENERAL, 10, UP, false);
    add(&mut state, GENERAL, 10, MIRA, FIRE);
    remove(&mut state, GENERAL, 10, MIRA, FIRE);
    add(&mut state, GENERAL, 10, ME, FIRE);
    remove(&mut state, GENERAL, 10, ME, FIRE);
    remove(&mut state, GENERAL, 11, ME, UP);
    settle(&mut state, &taken.unwrap(), Some((true, false)));
    assert_eq!(
        reactions(&state, GENERAL, 10),
        one(UP, 1, true),
        "others' reactions, other emoji and other messages say nothing of it"
    );
}

#[test]
fn a_refused_removal_adds_nothing_to_a_super_reaction_made_meanwhile() {
    let mut state = viewing(GENERAL);
    apply(&mut state, "MESSAGE_CREATE", message(10, GENERAL));
    add(&mut state, GENERAL, 10, ME, UP);

    let (taken, _) = want(&mut state, GENERAL, 10, UP, false);
    apply(&mut state, "MESSAGE_REACTION_ADD", burst_reaction(ME));
    settle(&mut state, &taken.unwrap(), Some((true, false)));

    assert_eq!(
        reactions(&state, GENERAL, 10),
        [(UP.to_owned(), 0, 1, false, true)],
        "the user's on another device, not a second one"
    );
}

#[test]
fn an_answer_from_discord_is_newer_than_what_the_gateway_said_before_it() {
    let mut state = viewing(GENERAL);
    apply(&mut state, "MESSAGE_CREATE", message(10, GENERAL));

    let (added, _) = want(&mut state, GENERAL, 10, UP, true);
    let added = added.unwrap();
    add(&mut state, GENERAL, 10, ME, UP);
    state.confirm_reaction(added.channel, added.message, &added.emoji);
    want(&mut state, GENERAL, 10, UP, false);
    want(&mut state, GENERAL, 10, UP, true);
    settle(&mut state, &added, Some((false, false)));

    assert_eq!(reactions(&state, GENERAL, 10), []);
}

#[test]
fn a_refusal_for_a_deleted_message_changes_nothing() {
    let mut state = viewing(GENERAL);
    apply(&mut state, "MESSAGE_CREATE", message(10, GENERAL));
    add(&mut state, GENERAL, 10, ME, UP);

    let (taken, _) = want(&mut state, GENERAL, 10, UP, false);
    apply(
        &mut state,
        "MESSAGE_DELETE",
        json!({"id": "10", "channel_id": GENERAL.to_string()}),
    );
    let events = settle(&mut state, &taken.unwrap(), Some((true, false)));

    assert_eq!(events, [] as [&str; 0]);
    assert_eq!(reactions(&state, GENERAL, 10), []);
}

fn burst_reaction(user: u64) -> Value {
    let mut data = reaction(GENERAL, 10, user, UP);
    data["type"] = 1.into();
    data
}

#[test]
fn a_super_reaction_of_the_users_is_taken_back_as_one_and_never_added() {
    let mut state = viewing(GENERAL);
    apply(&mut state, "MESSAGE_CREATE", message(10, GENERAL));
    apply(&mut state, "MESSAGE_REACTION_ADD", burst_reaction(ME));
    apply(&mut state, "MESSAGE_REACTION_ADD", burst_reaction(MIRA));

    let (again, events) = want(&mut state, GENERAL, 10, UP, true);
    assert!(!again.unwrap().changed, "mine as a super reaction");
    assert_eq!(events, [] as [&str; 0]);

    let (taken, _) = want(&mut state, GENERAL, 10, UP, false);
    let taken = taken.unwrap();
    assert!(taken.burst && taken.changed);
    assert_eq!(
        reactions(&state, GENERAL, 10),
        [(UP.to_owned(), 0, 1, false, false)]
    );

    settle(&mut state, &taken, Some((true, true)));
    assert_eq!(
        reactions(&state, GENERAL, 10),
        [(UP.to_owned(), 0, 2, false, true)],
        "a refused removal puts the super reaction back"
    );

    let (taken, _) = want(&mut state, GENERAL, 10, UP, false);
    settle(&mut state, &taken.unwrap(), None);
    let (added, _) = want(&mut state, GENERAL, 10, UP, true);
    assert!(!added.unwrap().burst);
    assert_eq!(
        reactions(&state, GENERAL, 10),
        [(UP.to_owned(), 1, 1, true, false)],
        "a click adds a plain one"
    );
}

#[test]
fn a_custom_emoji_is_sent_under_the_name_the_message_has() {
    let mut state = viewing(GENERAL);
    apply(&mut state, "MESSAGE_CREATE", message(10, GENERAL));
    apply(
        &mut state,
        "MESSAGE_REACTION_ADD",
        custom(GENERAL, 10, MIRA, 7, json!("wave")),
    );
    let nameless = ReactionEmoji::custom(Snowflake::new(7), None, false);
    let now = state.now_millis();

    let wanted = state
        .want_reaction(
            Snowflake::new(GENERAL),
            Snowflake::new(10),
            &nameless,
            true,
            now,
            &mut Vec::new(),
        )
        .unwrap();

    assert_eq!(wanted.emoji.name.as_deref(), Some("wave"));
    let empty = ReactionEmoji::unicode("");
    assert_eq!(
        state.want_reaction(
            Snowflake::new(GENERAL),
            Snowflake::new(10),
            &empty,
            true,
            now,
            &mut Vec::new(),
        ),
        None,
        "no emoji at all"
    );
}

#[test]
fn a_click_on_a_message_the_store_doesnt_have_is_refused() {
    let mut state = viewing(GENERAL);

    assert_eq!(want(&mut state, GENERAL, 99, UP, true).0, None);
    assert_eq!(want(&mut state, GENERAL, 99, UP, false).0, None);
}
