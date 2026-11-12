use super::*;
use crate::state::StarterMessage;
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
            .begin_load(general, LoadKind::Around(Snowflake::new(20)), 1, &mut events)
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
