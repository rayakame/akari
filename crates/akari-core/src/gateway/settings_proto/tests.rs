use base64::engine::general_purpose::STANDARD;
use proptest::prelude::*;

use super::encode::{Wire, field, varint};
use super::*;
use crate::model::Snowflake;

const GAMES: u64 = 200_000_000_000_000_001;
const FRIENDS: u64 = 200_000_000_000_000_002;
const WORK: u64 = 200_000_000_000_000_003;
const ALONE: u64 = 200_000_000_000_000_004;

fn every_wire_type(number: u64) -> Vec<u8> {
    [
        field(number, Wire::Varint(300)),
        field(number, Wire::Fixed64(7)),
        field(number, Wire::Bytes(b"skip me".to_vec())),
        field(number, Wire::Fixed32(9)),
    ]
    .concat()
}

fn packed_ids(ids: &[u64]) -> Vec<u8> {
    field(
        1,
        Wire::Bytes(ids.iter().flat_map(|id| id.to_le_bytes()).collect()),
    )
}

fn folder_id(id: i64) -> Vec<u8> {
    field(2, Wire::Bytes(field(1, Wire::Varint(id as u64))))
}

fn folder_name(name: &[u8]) -> Vec<u8> {
    field(3, Wire::Bytes(field(1, Wire::Bytes(name.to_vec()))))
}

fn folder_color(color: u64) -> Vec<u8> {
    field(4, Wire::Bytes(field(1, Wire::Varint(color))))
}

fn guild_folders(folders: &[Vec<u8>]) -> Vec<u8> {
    let folders: Vec<Vec<u8>> = folders
        .iter()
        .map(|folder| field(1, Wire::Bytes(folder.clone())))
        .collect();
    field(14, Wire::Bytes(folders.concat()))
}

fn decode(fields: &[Vec<u8>]) -> Result<PreloadedSettings, ProtoError> {
    decode_preloaded(&STANDARD.encode(fields.concat()))
}

fn folders_of(fields: &[Vec<u8>]) -> Vec<GuildFolder> {
    decode(fields)
        .unwrap()
        .guild_folders
        .expect("a guild_folders field")
        .folders
}

fn guilds(ids: &[u64]) -> Vec<GuildId> {
    ids.iter().map(|id| Snowflake::new(*id)).collect()
}

fn games_folder() -> Vec<u8> {
    [
        packed_ids(&[GAMES, FRIENDS, WORK]),
        folder_id(42),
        folder_name("Games".as_bytes()),
        folder_color(0x5865f2),
    ]
    .concat()
}

#[test]
fn folders_decode() {
    let folders = folders_of(&[guild_folders(&[games_folder(), packed_ids(&[ALONE])])]);

    assert_eq!(
        folders,
        [
            GuildFolder {
                guild_ids: guilds(&[GAMES, FRIENDS, WORK]),
                id: Some(42),
                name: Some("Games".to_owned()),
                color: Some(0x5865f2),
            },
            GuildFolder {
                guild_ids: guilds(&[ALONE]),
                id: None,
                name: None,
                color: None,
            },
        ]
    );
}

#[test]
fn folder_ids_are_twos_complement() {
    let folders = folders_of(&[guild_folders(&[
        [packed_ids(&[GAMES]), folder_id(-7)].concat(),
        [packed_ids(&[FRIENDS]), folder_id(i64::MIN)].concat(),
    ])]);

    let ids: Vec<_> = folders.iter().map(|folder| folder.id).collect();
    assert_eq!(ids, [Some(-7), Some(i64::MIN)]);
}

#[test]
fn packed_and_unpacked_guild_ids_read_alike() {
    let packed = folders_of(&[guild_folders(&[packed_ids(&[GAMES, FRIENDS, WORK])])]);
    let unpacked = folders_of(&[guild_folders(&[[
        field(1, Wire::Fixed64(GAMES)),
        field(1, Wire::Fixed64(FRIENDS)),
        field(1, Wire::Fixed64(WORK)),
    ]
    .concat()])]);
    let mixed = folders_of(&[guild_folders(&[[
        packed_ids(&[GAMES]),
        field(1, Wire::Fixed64(FRIENDS)),
        packed_ids(&[WORK]),
    ]
    .concat()])]);

    assert_eq!(packed[0].guild_ids, guilds(&[GAMES, FRIENDS, WORK]));
    assert_eq!(unpacked, packed);
    assert_eq!(mixed, packed);
}

#[test]
fn an_empty_wrapper_is_zero_or_empty() {
    let folders = folders_of(&[guild_folders(&[[
        packed_ids(&[GAMES]),
        field(2, Wire::Bytes(Vec::new())),
        field(3, Wire::Bytes(Vec::new())),
        field(4, Wire::Bytes(Vec::new())),
    ]
    .concat()])]);

    assert_eq!(folders[0].id, Some(0));
    assert_eq!(folders[0].name.as_deref(), Some(""));
    assert_eq!(folders[0].color, Some(0));
}

#[test]
fn an_absent_wrapper_is_none() {
    let folders = folders_of(&[guild_folders(&[packed_ids(&[GAMES])])]);

    assert_eq!(folders[0].id, None);
    assert_eq!(folders[0].name, None);
    assert_eq!(folders[0].color, None);
}

#[test]
fn unknown_fields_are_skipped() {
    let plain = folders_of(&[guild_folders(&[games_folder(), packed_ids(&[ALONE])])]);

    let games_with_extras = [every_wire_type(9), games_folder(), every_wire_type(5)].concat();
    let name_with_extras = field(
        3,
        Wire::Bytes([every_wire_type(2), field(1, Wire::Bytes(b"Games".to_vec()))].concat()),
    );
    let folders = [
        // Field 2 is the deprecated guild_positions, packed.
        field(2, Wire::Bytes(GAMES.to_le_bytes().to_vec())),
        every_wire_type(7),
        field(1, Wire::Bytes(games_with_extras)),
        field(1, Wire::Bytes(packed_ids(&[ALONE]))),
        every_wire_type(3),
    ]
    .concat();
    let settings = decode(&[
        every_wire_type(2),
        every_wire_type(3),
        field(14, Wire::Bytes(folders)),
        every_wire_type(13),
        every_wire_type(15),
    ])
    .unwrap();
    assert_eq!(settings.guild_folders.unwrap().folders, plain);

    let renamed = folders_of(&[guild_folders(&[
        [packed_ids(&[GAMES]), name_with_extras].concat()
    ])]);
    assert_eq!(renamed[0].name.as_deref(), Some("Games"));
}

#[test]
fn groups_are_rejected_at_their_offset() {
    for wire_type in [3, 4, 6, 7] {
        let bytes = [field(2, Wire::Varint(1)), vec![3 << 3 | wire_type]].concat();
        assert_eq!(
            decode(&[bytes]),
            Err(ProtoError::Malformed { offset: 2 }),
            "wire type {wire_type}"
        );
    }

    // The tag (1 byte) and length (1) of field 14, a folder's tag (1) and length (1), then
    // the packed IDs (1 + 1 + 8): the group starts at byte 14 of the whole proto.
    let folder = [packed_ids(&[GAMES]), vec![3 << 3 | 3]].concat();
    assert_eq!(
        decode(&[guild_folders(&[folder])]),
        Err(ProtoError::Malformed { offset: 14 })
    );
}

#[test]
fn a_length_past_the_end_is_truncated() {
    assert_eq!(
        decode(&[vec![14 << 3 | 2, 5, 0x0a, 0x00]]),
        Err(ProtoError::Truncated { offset: 1 })
    );

    // A folder longer than guild_folders, with bytes after it in the whole proto.
    let bytes = [vec![14 << 3 | 2, 2, 0x0a, 5], every_wire_type(3)].concat();
    assert_eq!(decode(&[bytes]), Err(ProtoError::Truncated { offset: 3 }));
}

#[test]
fn a_truncated_varint_is_truncated() {
    assert_eq!(
        decode(&[vec![2 << 3, 0x80, 0x80]]),
        Err(ProtoError::Truncated { offset: 1 })
    );
}

#[test]
fn a_ten_byte_varint_overflow_is_malformed() {
    let mut too_wide = vec![0xff; 9];
    too_wide.push(0x02);
    assert_eq!(
        decode(&[vec![2 << 3], too_wide]),
        Err(ProtoError::Malformed { offset: 1 })
    );

    let mut too_long = vec![0x80; 10];
    too_long.push(0x00);
    assert_eq!(
        decode(&[vec![2 << 3], too_long]),
        Err(ProtoError::Malformed { offset: 1 })
    );

    let widest = varint(u64::MAX);
    assert_eq!(widest.len(), 10);
    assert!(decode(&[vec![2 << 3], widest]).is_ok());
}

#[test]
fn a_name_that_isnt_utf8_is_malformed() {
    // Field 14 (2 bytes), the folder (2), the packed IDs (10), the name wrapper's tag and
    // length (2), then the string's tag (1): its length starts at byte 17.
    let folder = [packed_ids(&[GAMES]), folder_name(&[0x66, 0xff, 0xfe])].concat();

    assert_eq!(
        decode(&[guild_folders(&[folder])]),
        Err(ProtoError::Malformed { offset: 17 })
    );
}

#[test]
fn a_known_field_with_another_wire_type_is_malformed() {
    assert_eq!(
        decode(&[field(14, Wire::Varint(1))]),
        Err(ProtoError::Malformed { offset: 0 })
    );
    assert_eq!(
        decode(&[guild_folders(&[field(3, Wire::Fixed32(1))])]),
        Err(ProtoError::Malformed { offset: 4 })
    );
}

#[test]
fn field_number_zero_is_malformed() {
    assert_eq!(
        decode(&[vec![0x00, 0x01]]),
        Err(ProtoError::Malformed { offset: 0 })
    );
}

#[test]
fn bad_base64_is_an_error() {
    assert_eq!(decode_preloaded("not base64!"), Err(ProtoError::Base64));
}

#[test]
fn no_folders_field_is_none() {
    assert_eq!(decode_preloaded(""), Ok(PreloadedSettings::default()));
    let settings = decode(&[every_wire_type(2)]).unwrap();
    assert_eq!(settings.guild_folders, None);
    assert_eq!(settings.data_version, None);
}

#[test]
fn an_empty_folders_field_has_no_folders() {
    let settings = decode(&[field(14, Wire::Bytes(Vec::new()))]).unwrap();

    assert_eq!(settings.guild_folders, Some(GuildFolders::default()));
}

#[test]
fn the_data_version_is_read() {
    let versions = |fields: &[Vec<u8>]| field(1, Wire::Bytes(fields.concat()));

    let full = decode(&[versions(&[
        field(1, Wire::Varint(5)),
        field(2, Wire::Varint(6)),
        field(3, Wire::Varint(77)),
    ])])
    .unwrap();
    let without = decode(&[versions(&[field(1, Wire::Varint(5))])]).unwrap();

    assert_eq!(full.data_version, Some(77));
    assert_eq!(without.data_version, Some(0), "proto3's default");
}

#[test]
fn a_repeated_folders_field_appends() {
    let folders = folders_of(&[
        guild_folders(&[games_folder()]),
        guild_folders(&[packed_ids(&[ALONE])]),
    ]);

    let ids: Vec<_> = folders
        .iter()
        .map(|folder| folder.guild_ids.clone())
        .collect();
    assert_eq!(ids, [guilds(&[GAMES, FRIENDS, WORK]), guilds(&[ALONE])]);
}

// Run on a real READY with:
// AKARI_READY_FIXTURE=<absolute path to the capture> \
//   cargo test -p akari-core --lib -- --ignored captured_settings_proto_decodes --nocapture
#[test]
#[ignore = "needs AKARI_READY_FIXTURE, a READY captured with `akari-cli connect --capture`"]
fn captured_settings_proto_decodes() {
    let path = std::env::var("AKARI_READY_FIXTURE")
        .expect("AKARI_READY_FIXTURE must point to a captured READY message");
    let json = std::fs::read_to_string(&path).expect("AKARI_READY_FIXTURE is not readable");
    let raw: serde_json::Value =
        serde_json::from_str(&json).expect("AKARI_READY_FIXTURE is not JSON");
    let Some(proto) = raw["d"]["user_settings_proto"].as_str() else {
        println!("no user_settings_proto");
        return;
    };

    // Counts only: folder names and guild IDs stay out of the output.
    let settings = decode_preloaded(proto).unwrap_or_else(|err| panic!("{err}"));
    println!("data_version present: {}", settings.data_version.is_some());
    let Some(folders) = settings.guild_folders else {
        println!("no guild_folders field");
        return;
    };
    let with_id = folders.folders.iter().filter(|folder| folder.id.is_some());
    let without_id = folders.folders.iter().filter(|folder| folder.id.is_none());
    println!(
        "entries: {}, folders with an id: {} (named: {}, colored: {}, largest: {} guilds), \
         entries without an id: {} (with more than one guild: {}), guilds placed: {}",
        folders.folders.len(),
        with_id.clone().count(),
        with_id
            .clone()
            .filter(|folder| folder.name.is_some())
            .count(),
        with_id
            .clone()
            .filter(|folder| folder.color.is_some())
            .count(),
        with_id
            .map(|folder| folder.guild_ids.len())
            .max()
            .unwrap_or(0),
        without_id.clone().count(),
        without_id
            .filter(|folder| folder.guild_ids.len() > 1)
            .count(),
        folders
            .folders
            .iter()
            .map(|folder| folder.guild_ids.len())
            .sum::<usize>(),
    );
}

fn sample() -> Vec<u8> {
    [
        field(1, Wire::Bytes(field(3, Wire::Varint(77)))),
        every_wire_type(2),
        guild_folders(&[games_folder(), packed_ids(&[ALONE])]),
    ]
    .concat()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10_000))]

    #[test]
    fn arbitrary_bytes_never_panic(bytes in prop::collection::vec(any::<u8>(), 0..256)) {
        let _ = decode_preloaded(&STANDARD.encode(&bytes));
    }

    #[test]
    fn arbitrary_text_never_panics(text in ".{0,96}") {
        let _ = decode_preloaded(&text);
    }

    #[test]
    fn edited_protos_never_panic(
        edits in prop::collection::vec((any::<prop::sample::Index>(), any::<u8>()), 1..6),
        cut in any::<prop::sample::Index>(),
    ) {
        let mut bytes = sample();
        for (at, byte) in edits {
            let at = at.index(bytes.len());
            bytes[at] = byte;
        }
        bytes.truncate(cut.index(bytes.len() + 1));
        let _ = decode_preloaded(&STANDARD.encode(&bytes));
    }
}
