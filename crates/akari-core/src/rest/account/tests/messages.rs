use std::sync::atomic::Ordering;

use wiremock::matchers::path_regex;

use super::reactions::{BAD_GATEWAY, flaky, flaky_then, rest_at};
use super::*;

const MESSAGE: u64 = 400_000_000_000_000_003;
const PATH: &str = "/api/v9/channels/300000000000000002/messages/400000000000000003";
const ANY_MESSAGE: &str = "/messages/[0-9]+$";

fn target() -> MessageId {
    Snowflake::new(MESSAGE)
}

fn edited() -> Value {
    let mut message = created();
    message["content"] = "new".into();
    message["edited_timestamp"] = "2026-11-15T12:00:00.000000+00:00".into();
    message
}

async fn answering(answer: ResponseTemplate) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(path_regex(ANY_MESSAGE))
        .respond_with(answer)
        .mount(&server)
        .await;
    server
}

async fn edit(rest: &AccountRest, quiet_reply: bool) -> Result<Option<model::Message>, Failure> {
    let body = EditMessage::new("new".to_owned(), quiet_reply);
    rest.edit_message(channel(), target(), &body).await
}

fn lines(requests: &[Request]) -> Vec<String> {
    requests
        .iter()
        .map(|request| format!("{} {}", request.method, request.url.path()))
        .collect()
}

#[tokio::test]
async fn an_edit_patches_the_message_with_its_text_alone() {
    let server = answering(ResponseTemplate::new(200).set_body_json(edited())).await;

    let answer = edit(&rest(&server).await, false).await.unwrap();

    let requests = requests(&server).await;
    assert_eq!(lines(&requests), [format!("PATCH {PATH}")]);
    assert_eq!(requests[0].url.query(), None);
    assert_eq!(
        String::from_utf8_lossy(&requests[0].body),
        r#"{"content":"new"}"#
    );
    let answer = answer.expect("the edited message");
    assert_eq!(answer.content, "new");
    assert!(answer.edited_timestamp.is_some());
}

#[tokio::test]
async fn an_edit_of_a_reply_sent_without_the_ping_keeps_it_off() {
    let server = answering(ResponseTemplate::new(200).set_body_json(edited())).await;

    edit(&rest(&server).await, true).await.unwrap();

    assert_eq!(
        String::from_utf8_lossy(&requests(&server).await[0].body),
        r#"{"content":"new","allowed_mentions":{"parse":["users","roles","everyone"],"replied_user":false}}"#
    );
}

#[tokio::test]
async fn an_answer_that_isnt_a_message_still_says_the_edit_was_taken() {
    let server = answering(ResponseTemplate::new(200).set_body_json(json!({"id": 3}))).await;

    let answer = edit(&rest(&server).await, false).await;

    assert!(matches!(answer, Ok(None)), "{answer:?}");
}

#[tokio::test]
async fn a_delete_deletes_the_messages_path_without_a_body() {
    let server = answering(ResponseTemplate::new(204)).await;

    rest(&server)
        .await
        .delete_message(channel(), target())
        .await
        .unwrap();

    let requests = requests(&server).await;
    assert_eq!(lines(&requests), [format!("DELETE {PATH}")]);
    assert_eq!(requests[0].url.query(), None);
    assert!(requests[0].body.is_empty());
}

#[tokio::test]
async fn a_published_messages_edit_limit_answers_at_once_and_holds_back_no_other_edit() {
    let server = MockServer::start().await;
    Mock::given(path_regex(format!("{MESSAGE}$")))
        .respond_with(ResponseTemplate::new(429).set_body_json(json!({
            "message": "This message cannot be edited due to announcement rate limits.",
            "code": 20022, "retry_after": 1200.5, "global": false,
        })))
        .mount(&server)
        .await;
    Mock::given(path_regex("/messages/400000000000000004$"))
        .respond_with(ResponseTemplate::new(200).set_body_json(edited()))
        .mount(&server)
        .await;
    let rest = rest(&server).await;

    let limited = edit(&rest, false).await;
    let another = rest
        .edit_message(
            channel(),
            Snowflake::new(MESSAGE + 1),
            &EditMessage::new("new".to_owned(), false),
        )
        .await;

    assert!(
        matches!(limited, Err(Failure::EditLimit(Some(wait))) if wait.as_secs() == 1200),
        "{limited:?}"
    );
    assert!(another.is_ok(), "the route isn't paused: {another:?}");
    assert_eq!(requests(&server).await.len(), 2, "never asked again");
}

#[tokio::test]
async fn an_edit_and_a_delete_are_asked_once_more_after_a_429_and_never_a_third_time() {
    for delete in [false, true] {
        let limited = answering(rate_limited(Some(0.02))).await;
        let rest = rest(&limited).await;

        let failed = if delete {
            rest.delete_message(channel(), target()).await
        } else {
            edit(&rest, false).await.map(|_| ()).map_err(Into::into)
        };

        assert!(
            matches!(failed, Err(RequestError::RateLimited { .. })),
            "delete {delete}: {failed:?}"
        );
        assert_eq!(requests(&limited).await.len(), 2, "delete {delete}");
    }
}

#[tokio::test]
async fn an_edit_and_a_delete_are_asked_once_more_after_a_502_504_or_a_broken_connection() {
    for delete in [false, true] {
        for status in [502, 504] {
            let failing = answering(ResponseTemplate::new(status)).await;
            let rest = rest(&failing).await;

            let failed = if delete {
                rest.delete_message(channel(), target()).await
            } else {
                edit(&rest, false).await.map(|_| ()).map_err(Into::into)
            };

            assert!(
                matches!(failed, Err(RequestError::ServerError { .. })),
                "delete {delete}, {status}: {failed:?}"
            );
            assert_eq!(requests(&failing).await.len(), 2, "{delete} {status}");
        }
        let (api, asked) = flaky_then(usize::MAX, BAD_GATEWAY).await;
        let rest = rest_at(api);
        let failed = if delete {
            rest.delete_message(channel(), target()).await
        } else {
            edit(&rest, false).await.map(|_| ()).map_err(Into::into)
        };
        assert!(
            matches!(failed, Err(RequestError::Network(_))),
            "delete {delete}: {failed:?}"
        );
        assert_eq!(asked.load(Ordering::SeqCst), 2, "delete {delete}");
    }
}

#[tokio::test]
async fn a_delete_after_a_connection_that_broke_is_asked_again() {
    let (api, asked) = flaky(1).await;

    let deleted = rest_at(api).delete_message(channel(), target()).await;

    assert!(deleted.is_ok(), "{deleted:?}");
    assert_eq!(asked.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn a_refused_edit_or_delete_is_not_asked_again() {
    let refusals = [
        (400, 200_000, "Message was blocked by AutoMod"),
        (403, 50005, "Cannot edit a message authored by another user"),
        (404, 10008, "Unknown Message"),
        (500, 0, "Internal Server Error"),
    ];
    for (status, code, text) in refusals {
        for delete in [false, true] {
            let server = answering(
                ResponseTemplate::new(status).set_body_json(json!({"message": text, "code": code})),
            )
            .await;
            let rest = rest(&server).await;

            let refused = if delete {
                rest.delete_message(channel(), target()).await
            } else {
                edit(&rest, false).await.map(|_| ()).map_err(Into::into)
            };

            match refused {
                Err(RequestError::Discord {
                    status: got,
                    code: got_code,
                    ..
                }) => assert_eq!((got, got_code), (status, code)),
                Err(RequestError::ServerError { status: 500 }) => assert_eq!(status, 500),
                other => panic!("{status} {code} delete {delete}: {other:?}"),
            }
            assert_eq!(requests(&server).await.len(), 1, "{status} {delete}");
        }
    }
}
