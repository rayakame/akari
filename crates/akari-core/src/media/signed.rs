//! Signed attachment links: when one is due, and the batched `refresh-urls` that renews it.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use reqwest::Url;
use tokio::sync::{mpsc, oneshot};

use super::url::is_attachment_link;
use crate::rest::AccountRest;

const LOG: &str = "akari_core::media";
const SIGNATURE: [&str; 3] = ["ex", "is", "hm"];
// The official client's margin before `ex` (module 679164).
const MARGIN: i64 = 3600;
// Discord's limit per request.
const BATCH: usize = 50;

/// When the link stops working, in Unix seconds (`ex`, hex).
pub(crate) fn expires(url: &Url) -> Option<i64> {
    let (_, ex) = url.query_pairs().find(|(name, _)| name == "ex")?;
    i64::from_str_radix(&ex, 16).ok()
}

/// Whether an attachment link needs a new signature before use: within the hour before its
/// expiry, or without one. Other links are never signed.
pub(crate) fn due(url: &Url, now: i64) -> bool {
    is_attachment_link(url) && expires(url).is_none_or(|ex| ex < now + MARGIN)
}

/// `target` with `signed`'s signature in place of its own, ahead of its other parameters.
pub(crate) fn resign(target: &Url, signed: &Url) -> Url {
    let signature = signed
        .query_pairs()
        .filter(|(name, _)| SIGNATURE.contains(&name.as_ref()));
    let rest = target
        .query_pairs()
        .filter(|(name, _)| !SIGNATURE.contains(&name.as_ref()));
    let pairs: Vec<(String, String)> = signature
        .chain(rest)
        .map(|(name, value)| (name.into_owned(), value.into_owned()))
        .collect();
    let mut url = target.clone();
    url.set_query(None);
    url.query_pairs_mut().extend_pairs(pairs);
    url
}

/// What a renewal got.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Renewal {
    Renewed(Url),
    /// Discord answered without this link: it won't sign it again.
    Refused,
    /// The request failed; a later one may work.
    Failed,
}

struct Ask {
    url: String,
    reply: oneshot::Sender<Renewal>,
}

/// Renews signed links through the account's REST: asks within `window` of each other share a
/// request of at most 50, and one request runs at a time.
#[derive(Clone)]
pub(crate) struct Refresher {
    asks: mpsc::UnboundedSender<Ask>,
}

impl Refresher {
    pub(crate) fn start(
        runtime: &tokio::runtime::Handle,
        rest: Arc<AccountRest>,
        window: Duration,
    ) -> Self {
        let (asks, receiver) = mpsc::unbounded_channel();
        runtime.spawn(run(rest, receiver, window));
        Self { asks }
    }

    /// The link with a fresh signature.
    pub(crate) async fn refresh(&self, url: &Url) -> Renewal {
        let (reply, answer) = oneshot::channel();
        let asked = self.asks.send(Ask {
            url: url.to_string(),
            reply,
        });
        if asked.is_err() {
            return Renewal::Failed;
        }
        answer.await.unwrap_or(Renewal::Failed)
    }
}

// Ends once every Refresher is dropped.
async fn run(rest: Arc<AccountRest>, mut asks: mpsc::UnboundedReceiver<Ask>, window: Duration) {
    while let Some(first) = asks.recv().await {
        let mut batch = vec![first];
        let deadline = tokio::time::Instant::now() + window;
        while batch.len() < BATCH {
            tokio::select! {
                ask = asks.recv() => match ask {
                    Some(ask) => batch.push(ask),
                    None => break,
                },
                () = tokio::time::sleep_until(deadline) => break,
            }
        }
        // Repeats beyond the 50 that fit wait for the next request.
        let mut urls: Vec<String> = Vec::new();
        for ask in &batch {
            if !urls.contains(&ask.url) {
                urls.push(ask.url.clone());
            }
        }
        let refreshed: Option<HashMap<String, Url>> = match rest.refresh_urls(&urls).await {
            Ok(pairs) => Some(
                pairs
                    .into_iter()
                    .filter_map(|(original, fresh)| Some((original, Url::parse(&fresh).ok()?)))
                    .collect(),
            ),
            Err(err) => {
                tracing::debug!(target: LOG, links = urls.len(), error = err.kind(), "links not refreshed");
                None
            }
        };
        if let Some(refreshed) = &refreshed {
            tracing::debug!(
                target: LOG,
                links = urls.len(),
                refreshed = refreshed.len(),
                "links refreshed"
            );
        }
        for ask in batch {
            let answer = match &refreshed {
                None => Renewal::Failed,
                Some(refreshed) => refreshed
                    .get(&ask.url)
                    .cloned()
                    .map_or(Renewal::Refused, Renewal::Renewed),
            };
            let _ = ask.reply.send(answer);
        }
    }
}

#[cfg(test)]
mod tests;
