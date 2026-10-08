use std::fmt;
use std::sync::Arc;

use crate::gateway::session::Timing;
use crate::gateway::{ConnectionEvent, DispatchEvent, Gateway, GatewayError};
use crate::model::ChannelId;
use crate::state::{ConnectionState, Store, WindowLimits};
use crate::{DiscordClient, Token};

/// A logged-in account: its gateway connection and the [`Store`] that connection keeps
/// current. Dropping it ends the session like [`Account::close`].
pub struct Account {
    gateway: Arc<Gateway>,
    store: Store,
}

impl Account {
    pub(crate) fn start(
        client: DiscordClient,
        token: Token,
        timing: Timing,
        limits: WindowLimits,
    ) -> Result<Self, GatewayError> {
        let runtime = tokio::runtime::Handle::try_current().map_err(|_| GatewayError::NoRuntime)?;
        let gateway = Arc::new(Gateway::start(client, token, timing)?);
        let store = Store::new(limits);
        // Built before the task starts, so it runs even if the task is never polled.
        let finish = Finish::new(gateway.clone(), store.clone());
        runtime.spawn(pump(finish));
        Ok(Self { gateway, store })
    }

    /// The account's state. It stays readable after the account is closed.
    pub fn store(&self) -> &Store {
        &self.store
    }

    /// Connects, or resumes after [`Account::disconnect`]. Does nothing while connected.
    /// Fails with [`GatewayError::Closed`] after `close()` or a fatal error.
    pub fn connect(&self) -> Result<(), GatewayError> {
        self.gateway.connect()?;
        self.store.begin_connecting();
        Ok(())
    }

    /// Closes the connection but keeps the session, e.g. while the app is suspended; the
    /// next `connect()` resumes it.
    pub fn disconnect(&self) {
        self.gateway.disconnect();
        self.store.set_connection(ConnectionState::Offline);
    }

    /// Ends the session. Subscriptions end once the connection is closed.
    pub fn close(&self) {
        self.gateway.close();
    }

    /// Marks the channel as viewed: the store keeps its messages and adds new ones from
    /// the gateway. Past 10 viewed channels, the least recently viewed loses its messages.
    pub fn view_channel(&self, channel: ChannelId) {
        self.store.view_channel(channel);
    }
}

// Closes the account however the pump ends: normally, by a panic, or because the runtime
// shut down and dropped the task. Without it, subscribers would wait forever.
pub(crate) struct Finish {
    gateway: Arc<Gateway>,
    store: Store,
    ended: Option<Option<Arc<GatewayError>>>,
}

impl Finish {
    pub(crate) fn new(gateway: Arc<Gateway>, store: Store) -> Self {
        Self {
            gateway,
            store,
            ended: None,
        }
    }
}

impl Drop for Finish {
    fn drop(&mut self) {
        // close() does nothing once the gateway has ended, so it runs on every path.
        self.gateway.close();
        let error = self
            .ended
            .take()
            .unwrap_or_else(|| Some(Arc::new(GatewayError::Stopped)));
        self.store.set_connection(ConnectionState::Closed { error });
        self.store.finish();
    }
}

impl Drop for Account {
    fn drop(&mut self) {
        self.gateway.close();
    }
}

impl fmt::Debug for Account {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Account")
            .field("connection", &self.store.connection())
            .finish_non_exhaustive()
    }
}

// The gateway buffers its events and the store never waits for subscribers, so nothing
// here waits on a consumer.
async fn pump(mut finish: Finish) {
    let gateway = finish.gateway.clone();
    let connecting = || gateway.wants_connection();
    let error = loop {
        match gateway.next().await {
            // A large READY takes milliseconds to convert; that mustn't hold a runtime worker.
            Ok(ConnectionEvent::Dispatch(DispatchEvent::Ready(ready))) => {
                let store = finish.store.clone();
                match tokio::task::spawn_blocking(move || store.prepare_ready(*ready)).await {
                    Ok(next) => {
                        finish.store.replace(next);
                        finish
                            .store
                            .set_connection_if(ConnectionState::Online, connecting);
                    }
                    Err(_) => break Some(Arc::new(GatewayError::Stopped)),
                }
            }
            Ok(event) => on_event(&finish.store, event, &connecting),
            Err(GatewayError::Closed) => break None,
            Err(error) => break Some(Arc::new(error)),
        }
    };
    finish.ended = Some(error);
}

// Queued events mustn't make a disconnected account look connected. disconnect() idles the
// gateway before it takes the store's lock, so `connecting` checked under that lock sees it.
fn on_event(store: &Store, event: ConnectionEvent, connecting: &dyn Fn() -> bool) {
    match event {
        ConnectionEvent::Dispatch(event) => {
            let ready = matches!(event, DispatchEvent::Ready(_) | DispatchEvent::Resumed);
            store.apply(event);
            if ready {
                store.set_connection_if(ConnectionState::Online, connecting);
            }
        }
        ConnectionEvent::Reconnecting { .. } => {
            store.set_connection_if(ConnectionState::Connecting, connecting);
        }
        #[cfg(feature = "capture")]
        ConnectionEvent::CapturedReady(_) => {}
    }
}

#[cfg(test)]
mod tests;
