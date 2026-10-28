use std::sync::Arc;

use tokio::sync::Notify;

use crate::rest::AccountRest;
use crate::state::Store;

// Module 473503: one post-data request at a time for the whole account, a forum's posts 10 at a
// time, in the order their cards asked.
pub(crate) async fn run_post_data(store: Store, rest: Arc<AccountRest>, wake: Arc<Notify>) {
    loop {
        wake.notified().await;
        while let Some((forum, posts)) = store.next_post_batch() {
            let answer = match rest.post_data(forum, &posts).await {
                Ok(answer) => Some(answer),
                Err(err) => {
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
        }
    }
}
