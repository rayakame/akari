use std::sync::Arc;

use akari_core::model::ChannelId;
use tokio::runtime::Runtime;

use crate::errors::GatewayError;
use crate::store::Store;

/// A logged-in account. Releasing the last reference ends the session like `close()`.
#[derive(uniffi::Object)]
pub struct Account {
    core: Arc<akari_core::Account>,
    runtime: &'static Runtime,
}

impl Account {
    pub(crate) fn new(core: akari_core::Account, runtime: &'static Runtime) -> Arc<Self> {
        Arc::new(Self {
            core: Arc::new(core),
            runtime,
        })
    }
}

#[uniffi::export]
impl Account {
    /// The account's state; it stays readable after the account is closed.
    pub fn store(&self) -> Arc<Store> {
        Store::new(self.core.store().clone())
    }

    /// Connects, or resumes after `disconnect()`. Fails with `Closed` after `close()`.
    pub fn connect(&self) -> Result<(), GatewayError> {
        let _entered = self.runtime.enter();
        self.core.connect().map_err(Into::into)
    }

    /// Closes the connection but keeps the session, e.g. before the Mac sleeps.
    pub fn disconnect(&self) {
        self.core.disconnect();
    }

    /// Ends the session.
    pub fn close(&self) {
        self.core.close();
    }

    /// Keeps the channel's messages and adds new ones from the gateway.
    pub fn view_channel(&self, channel_id: ChannelId) {
        self.core.view_channel(channel_id);
    }
}
