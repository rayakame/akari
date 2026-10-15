use std::sync::{Mutex, PoisonError};

use reqwest::header::{DATE, HeaderMap};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc2822;
use tokio::time::Instant;

// `Date` has whole seconds and is stamped before the answer travels: never ahead of Discord.
#[derive(Default)]
pub(crate) struct ServerClock {
    sample: Mutex<Option<Sample>>,
}

#[derive(Clone, Copy)]
struct Sample {
    unix_millis: i64,
    at: Instant,
}

impl ServerClock {
    pub(crate) fn observe(&self, headers: &HeaderMap, at: Instant) {
        let Some(unix_millis) = headers
            .get(DATE)
            .and_then(|value| value.to_str().ok())
            .and_then(date_millis)
        else {
            return;
        };
        *self.sample.lock().unwrap_or_else(PoisonError::into_inner) =
            Some(Sample { unix_millis, at });
    }

    pub(crate) fn now_millis(&self) -> Option<i64> {
        let sample = (*self.sample.lock().unwrap_or_else(PoisonError::into_inner))?;
        let elapsed = i64::try_from(sample.at.elapsed().as_millis()).unwrap_or(i64::MAX);
        Some(sample.unix_millis.saturating_add(elapsed))
    }
}

// IMF-fixdate (`Sun, 06 Nov 1994 08:49:37 GMT`) is RFC 2822 with the obsolete GMT zone.
fn date_millis(value: &str) -> Option<i64> {
    let date = OffsetDateTime::parse(value, &Rfc2822).ok()?;
    i64::try_from(date.unix_timestamp_nanos() / 1_000_000).ok()
}
