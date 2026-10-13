mod account;
mod cache;
mod client;
mod contents;
mod fake;
mod formatted;
mod login;
mod messages;
mod support;

// akari-core's fake gateway helpers, shared instead of copied.
#[allow(dead_code)]
#[path = "../../../akari-core/tests/support/gateway.rs"]
mod gateway;
