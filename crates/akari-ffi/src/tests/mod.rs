mod account;
mod client;
mod fake;
mod login;
mod messages;
mod support;

// akari-core's fake gateway helpers, shared instead of copied.
#[allow(dead_code)]
#[path = "../../../akari-core/tests/support/gateway.rs"]
mod gateway;
