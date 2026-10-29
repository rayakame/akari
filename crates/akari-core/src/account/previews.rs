use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::sync::Notify;

use crate::rest::{AccountRest, RequestError};
use crate::state::Store;

#[derive(Default)]
pub(crate) struct Wake {
    notify: Notify,
    closed: AtomicBool,
}

impl Wake {
    pub(crate) fn wake(&self) {
        self.notify.notify_one();
    }

    pub(crate) fn close(&self) {
        self.closed.store(true, Ordering::Release);
        self.notify.notify_one();
    }

    fn closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }
}

// Module 473503: one post-data request at a time for the whole account, a forum's posts 10 at a
// time, in the order their cards asked.
pub(crate) async fn run_post_data(
    store: Store,
    rest: Arc<AccountRest>,
    wake: Arc<Wake>,
    unauthorized: impl Fn() + Send + 'static,
) {
    loop {
        wake.notify.notified().await;
        if wake.closed() {
            return;
        }
        while let Some((forum, posts)) = store.next_post_batch() {
            let answer = match rest.post_data(forum, &posts).await {
                Ok(answer) => Some(answer),
                Err(err) => {
                    if matches!(err, RequestError::Unauthorized) {
                        unauthorized();
                    }
                    tracing::debug!(
                        target: "akari_core::load",
                        channel_id = forum.get(),
                        posts = posts.len(),
                        outcome = err.kind(),
                        "post data failed"
                    );
                    None
                }
            };
            store.finish_post_batch(forum, &posts, answer);
            if wake.closed() {
                return;
            }
        }
    }
}
