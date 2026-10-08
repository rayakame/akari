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
        runtime.spawn(pump(gateway.clone(), store.clone()));
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
async fn pump(gateway: Arc<Gateway>, store: Store) {
    let error = loop {
        match gateway.next().await {
            Ok(event) => on_event(&store, event, gateway.wants_connection()),
            Err(GatewayError::Closed) => break None,
            Err(error) => break Some(Arc::new(error)),
        }
    };
    store.set_connection(ConnectionState::Closed { error });
    store.finish();
}

// Events queued before a disconnect() mustn't make the account look connected again.
fn on_event(store: &Store, event: ConnectionEvent, connecting: bool) {
    match event {
        ConnectionEvent::Dispatch(event) => {
            let ready = matches!(event, DispatchEvent::Ready(_) | DispatchEvent::Resumed);
            store.apply(event);
            if ready && connecting {
                store.set_connection(ConnectionState::Online);
            }
        }
        ConnectionEvent::Reconnecting { .. } if connecting => {
            store.set_connection(ConnectionState::Connecting);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests;
