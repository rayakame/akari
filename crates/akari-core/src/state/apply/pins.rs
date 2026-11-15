use super::State;
use crate::model::{self, ChannelId, Timestamp};
use crate::state::events::StoreEvent;
use crate::state::windows::{PinsAsk, PinsLoad, PinsPage};

impl State {
    pub(crate) fn pins(&self, channel: ChannelId) -> PinsPage {
        self.windows.pins(channel)
    }

    pub(crate) fn wants_pins(&self, channel: ChannelId, load: PinsLoad) -> bool {
        self.entities.channels.get(&channel).is_some_and(|stored| {
            self.windows
                .wants_pins(channel, load, stored.last_pin_timestamp)
        })
    }

    pub(crate) fn begin_pins(
        &mut self,
        channel: ChannelId,
        load: PinsLoad,
        events: &mut Vec<StoreEvent>,
    ) -> Option<PinsAsk> {
        let last_pin = self.entities.channels.get(&channel)?.last_pin_timestamp;
        self.windows.begin_pins(channel, load, last_pin, events)
    }

    pub(crate) fn finish_pins(
        &mut self,
        ask: PinsAsk,
        mut page: model::MessagePins,
        events: &mut Vec<StoreEvent>,
    ) {
        if !self.edits.is_empty() {
            for pin in &mut page.items {
                self.show_saved_text(&mut pin.message);
            }
        }
        self.windows
            .finish_pins(ask, page, &self.entities.users, events);
    }

    pub(crate) fn fail_pins(&mut self, ask: PinsAsk, events: &mut Vec<StoreEvent>) {
        self.windows.fail_pins(ask, events);
    }

    pub(crate) fn abandon_pins(&mut self, ask: PinsAsk, events: &mut Vec<StoreEvent>) {
        self.windows.abandon_pins(ask, events);
    }

    // The official client's popout acks only while the dot shows, up to the newest pin.
    pub(crate) fn ack_pins(&mut self, channel: ChannelId, events: &mut Vec<StoreEvent>) -> bool {
        let now = Timestamp::from_unix_millis(self.now_millis());
        let newest = self
            .entities
            .channels
            .get(&channel)
            .and_then(|stored| stored.last_pin_timestamp);
        let Some(newest) = newest.filter(|_| self.entities.pins_unread(channel, now)) else {
            return false;
        };
        self.changing_pins(channel, events, |state| {
            state.entities.read_states.ack_pins(channel, newest);
        });
        true
    }

    pub(super) fn name_pin(&mut self, channel: ChannelId, events: &mut Vec<StoreEvent>) {
        let last_pin = self
            .entities
            .channels
            .get(&channel)
            .and_then(|stored| stored.last_pin_timestamp);
        self.windows.name_pin(channel, last_pin, events);
    }
}
