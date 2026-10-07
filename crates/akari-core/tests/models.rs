use akari_core::model::{Permissions, Snowflake, Timestamp};
use serde::de::DeserializeOwned;

#[track_caller]
fn parse<T: DeserializeOwned>(json: &str) -> T {
    serde_json::from_str(json).unwrap_or_else(|err| panic!("failed to parse: {err}"))
}

#[test]
fn snowflakes_parse_from_strings_and_integers() {
    assert_eq!(
        parse::<Snowflake>(r#""100000000000000001""#),
        Snowflake(100_000_000_000_000_001)
    );
    assert_eq!(parse::<Snowflake>("373"), Snowflake(373));
    assert_eq!(
        parse::<Snowflake>(r#""18446744073709551615""#),
        Snowflake(u64::MAX)
    );
}

#[test]
fn values_that_are_not_snowflakes_are_rejected() {
    for json in [r#""abc""#, r#""""#, "-1", "1.5", "null"] {
        assert!(
            serde_json::from_str::<Snowflake>(json).is_err(),
            "{json} parsed"
        );
    }
}

#[test]
fn timestamps_parse_with_and_without_fraction() {
    let precise: Timestamp = parse(r#""2023-02-17T19:52:19.184000+00:00""#);
    let whole: Timestamp = parse(r#""2023-02-17T09:22:28+00:00""#);

    assert_eq!(precise.unix_millis(), 1_676_663_539_184);
    assert_eq!(whole.unix_millis(), 1_676_625_748_000);
}

#[test]
fn values_that_are_not_timestamps_are_rejected() {
    for json in [r#""yesterday""#, r#""2023-02-17""#, "1676625748"] {
        assert!(
            serde_json::from_str::<Timestamp>(json).is_err(),
            "{json} parsed"
        );
    }
}

#[test]
fn permissions_parse_from_strings() {
    assert_eq!(
        parse::<Permissions>(r#""110917634608832""#),
        Permissions(110_917_634_608_832)
    );
}
