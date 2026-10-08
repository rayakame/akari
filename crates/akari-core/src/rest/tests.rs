use std::time::Duration;

use reqwest::Url;
use serde_json::{Value, json};
use wiremock::matchers::{body_json, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::properties::{Arch, ClientBuild, ClientProperties, DesktopOs, HostInfo};
use crate::{Token, tls};

fn properties() -> ClientProperties {
    let host = HostInfo {
        os: DesktopOs::MacOs,
        os_version: "25.0.0".to_owned(),
        arch: Arch::Arm64,
        system_locale: "de-DE".to_owned(),
    };
    ClientProperties::desktop(&host, &ClientBuild::current(DesktopOs::MacOs))
}

async fn client(server: &MockServer) -> RestClient {
    let base = Url::parse(&format!("{}/api/v9/", server.uri())).unwrap();
    RestClient::new(&tls::client_config().unwrap(), base, &properties(), false).unwrap()
}

async fn error_for(response: ResponseTemplate) -> RestError {
    let server = MockServer::start().await;
    Mock::given(path("/api/v9/thing"))
        .respond_with(response)
        .mount(&server)
        .await;
    client(&server)
        .await
        .post_json::<_, Value>("thing", &json!({}), &RequestExtras::default())
        .await
        .unwrap_err()
}

#[tokio::test]
async fn requests_carry_the_client_headers() {
    let server = MockServer::start().await;
    let expected = properties();
    Mock::given(method("POST"))
        .and(path("/api/v9/auth/login"))
        .and(header(
            "x-super-properties",
            expected.super_properties().as_str(),
        ))
        .and(header("x-discord-locale", "de-DE"))
        .and(header("x-fingerprint", "1234.abcd"))
        .and(body_json(json!({"login": "a@example.com"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ok": true})))
        .expect(1)
        .mount(&server)
        .await;

    let response: Value = client(&server)
        .await
        .post_json(
            "auth/login",
            &json!({"login": "a@example.com"}),
            &RequestExtras {
                fingerprint: Some("1234.abcd"),
                ..RequestExtras::default()
            },
        )
        .await
        .unwrap();

    assert_eq!(response, json!({"ok": true}));
    let requests = server.received_requests().await.unwrap();
    // wiremock's header matcher splits on commas, and the user agent contains one.
    assert_eq!(
        requests[0].headers["user-agent"],
        expected.browser_user_agent.as_str()
    );
    assert!(requests[0].headers.get("authorization").is_none());
}

#[tokio::test]
async fn authorization_is_the_bare_token() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v9/auth/logout"))
        .and(header("authorization", "abc.def.ghi"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;

    let token = Token::new("abc.def.ghi".to_owned());
    client(&server)
        .await
        .post(
            "auth/logout",
            &json!({}),
            &RequestExtras {
                authorization: Some(&token),
                ..RequestExtras::default()
            },
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn get_json_reads_the_body() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v9/experiments"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"fingerprint": "f"})))
        .mount(&server)
        .await;

    let body: Value = client(&server)
        .await
        .get_json("experiments", &RequestExtras::default())
        .await
        .unwrap();

    assert_eq!(body["fingerprint"], "f");
}

#[tokio::test]
async fn captcha_challenges_keep_every_field() {
    let err = error_for(ResponseTemplate::new(400).set_body_json(json!({
        "captcha_key": ["captcha-required"],
        "captcha_sitekey": "site",
        "captcha_service": "hcaptcha",
        "captcha_rqdata": "rqdata",
        "captcha_rqtoken": "rqtoken",
        "captcha_session_id": "session",
        "should_serve_invisible": true,
    })))
    .await;

    let RestError::Captcha(challenge) = err else {
        panic!("expected a captcha, got {err:?}");
    };
    assert_eq!(challenge.service, "hcaptcha");
    assert_eq!(challenge.sitekey.as_deref(), Some("site"));
    assert_eq!(challenge.rqdata.as_deref(), Some("rqdata"));
    assert_eq!(challenge.rqtoken.as_deref(), Some("rqtoken"));
    assert_eq!(challenge.session_id.as_deref(), Some("session"));
    assert!(challenge.should_serve_invisible);
}

#[tokio::test]
async fn json_errors_flatten_their_field_errors() {
    let err = error_for(ResponseTemplate::new(400).set_body_json(json!({
        "code": 50035,
        "message": "Invalid Form Body",
        "errors": {
            "login": {"_errors": [{"code": "INVALID_LOGIN", "message": "Login or password is invalid."}]},
            "nested": {"0": {"name": {"_errors": [{"code": "BASE_TYPE_REQUIRED", "message": "Required"}]}}},
        },
    })))
    .await;

    let RestError::Api(api) = err else {
        panic!("expected an API error, got {err:?}");
    };
    assert_eq!(api.status, 400);
    assert_eq!(api.code, 50035);
    assert_eq!(api.message, "Invalid Form Body");
    assert_eq!(
        api.field_errors,
        [
            FieldError {
                path: "login".to_owned(),
                code: "INVALID_LOGIN".to_owned(),
                message: "Login or password is invalid.".to_owned(),
            },
            FieldError {
                path: "nested.0.name".to_owned(),
                code: "BASE_TYPE_REQUIRED".to_owned(),
                message: "Required".to_owned(),
            },
        ]
    );
}

#[tokio::test]
async fn rate_limits_come_from_the_body() {
    let err = error_for(ResponseTemplate::new(429).set_body_json(
        json!({"message": "You are being rate limited.", "retry_after": 1.5, "global": false}),
    ))
    .await;

    assert!(matches!(
        err,
        RestError::RateLimited { retry_after: Some(d), global: false } if d == Duration::from_millis(1500)
    ));
}

#[tokio::test]
async fn rate_limits_fall_back_to_the_headers() {
    let err = error_for(
        ResponseTemplate::new(429)
            .insert_header("retry-after", "3")
            .insert_header("x-ratelimit-global", "true")
            .set_body_string("<html>slow down</html>"),
    )
    .await;

    assert!(matches!(
        err,
        RestError::RateLimited { retry_after: Some(d), global: true } if d == Duration::from_secs(3)
    ));
}

#[tokio::test]
async fn rate_limit_without_a_delay_has_none() {
    let err = error_for(ResponseTemplate::new(429).set_body_string("<html></html>")).await;

    assert!(matches!(
        err,
        RestError::RateLimited {
            retry_after: None,
            global: false
        }
    ));
}

#[tokio::test]
async fn html_errors_are_unexpected_statuses() {
    for status in [403, 503] {
        let err =
            error_for(ResponseTemplate::new(status).set_body_string("<html>cloudflare</html>"))
                .await;

        assert!(
            matches!(err, RestError::UnexpectedStatus { status: s } if s == status),
            "{err:?}"
        );
    }
}

#[tokio::test]
async fn cloudflare_json_block_is_an_api_error() {
    let err = error_for(
        ResponseTemplate::new(403)
            .set_body_json(json!({"message": "internal network error", "code": 40333})),
    )
    .await;

    assert!(matches!(
        err,
        RestError::Api(ApiError {
            code: 40333,
            status: 403,
            ..
        })
    ));
}

#[tokio::test]
async fn suspension_drops_the_suspended_token() {
    let err = error_for(ResponseTemplate::new(403).set_body_json(json!({
        "user_id": "100000000000000001",
        "suspended_user_token": "very-secret-token",
    })))
    .await;

    assert!(matches!(err, RestError::Suspended));
    assert!(!format!("{err:?}").contains("very-secret-token"));
}

#[tokio::test]
async fn non_json_success_is_an_invalid_body() {
    let err = error_for(ResponseTemplate::new(200).set_body_string("token=abc")).await;

    assert!(matches!(err, RestError::InvalidBody));
    assert!(!format!("{err:?}").contains("abc"));
}

#[tokio::test]
async fn unreachable_servers_are_transport_errors() {
    let base = Url::parse("http://127.0.0.1:1/api/v9/").unwrap();
    let client =
        RestClient::new(&tls::client_config().unwrap(), base, &properties(), false).unwrap();

    let err = client
        .get_json::<Value>("experiments", &RequestExtras::default())
        .await
        .unwrap_err();

    assert!(matches!(err, RestError::Transport(_)));
}

#[tokio::test]
async fn captcha_solutions_go_in_headers() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v9/auth/login"))
        .and(header("x-captcha-key", "solution"))
        .and(header("x-captcha-rqtoken", "rq"))
        .and(header("x-captcha-session-id", "sid"))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    let solution = CaptchaSolution {
        key: "solution".to_owned(),
        rqtoken: Some("rq".to_owned()),
        session_id: Some("sid".to_owned()),
    };

    client(&server)
        .await
        .post(
            "auth/login",
            &json!({}),
            &RequestExtras {
                captcha: Some(&solution),
                ..RequestExtras::default()
            },
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn absurd_rate_limit_delays_count_as_unknown() {
    let from_body = error_for(
        ResponseTemplate::new(429).set_body_json(json!({"retry_after": 1e300, "global": false})),
    )
    .await;
    let from_header = error_for(
        ResponseTemplate::new(429)
            .insert_header("retry-after", "1e300")
            .set_body_string("<html></html>"),
    )
    .await;

    for err in [from_body, from_header] {
        assert!(
            matches!(
                err,
                RestError::RateLimited {
                    retry_after: None,
                    ..
                }
            ),
            "{err:?}"
        );
    }
}

#[test]
fn summaries_name_the_error_without_response_data() {
    let captcha = RestError::Captcha(Box::new(crate::auth::CaptchaChallenge {
        service: "hcaptcha".to_owned(),
        sitekey: Some("site".to_owned()),
        rqdata: Some("rqdata-secret".to_owned()),
        rqtoken: Some("rqtoken-secret".to_owned()),
        session_id: Some("session-secret".to_owned()),
        should_serve_invisible: false,
    }));
    let api = RestError::Api(ApiError {
        status: 400,
        code: 50035,
        message: "message-secret".to_owned(),
        field_errors: vec![FieldError {
            path: "login".to_owned(),
            code: "INVALID_LOGIN".to_owned(),
            message: "field-secret".to_owned(),
        }],
    });

    assert_eq!(captcha.summary(), "captcha required");
    assert_eq!(api.summary(), "Discord error 50035");
    assert_eq!(
        RestError::UnexpectedStatus { status: 503 }.summary(),
        "unexpected status 503"
    );
}

#[tokio::test]
async fn redirects_are_not_followed() {
    let server = MockServer::start().await;
    Mock::given(path("/api/v9/thing"))
        .respond_with(ResponseTemplate::new(302).insert_header("location", "/api/v9/elsewhere"))
        .mount(&server)
        .await;
    Mock::given(path("/api/v9/elsewhere"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .mount(&server)
        .await;

    let err = client(&server)
        .await
        .get_json::<Value>("thing", &RequestExtras::default())
        .await
        .unwrap_err();

    assert!(
        matches!(err, RestError::UnexpectedStatus { status: 302 }),
        "{err:?}"
    );
}

#[tokio::test]
async fn https_only_clients_refuse_plain_http() {
    let server = MockServer::start().await;
    Mock::given(path("/api/v9/thing"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .mount(&server)
        .await;
    let base = Url::parse(&format!("{}/api/v9/", server.uri())).unwrap();
    let client =
        RestClient::new(&tls::client_config().unwrap(), base, &properties(), true).unwrap();

    let err = client
        .get_json::<Value>("thing", &RequestExtras::default())
        .await
        .unwrap_err();

    assert!(matches!(err, RestError::Transport(_)), "{err:?}");
    assert!(server.received_requests().await.unwrap().is_empty());
}
