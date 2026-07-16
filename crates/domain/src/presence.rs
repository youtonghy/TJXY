use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum PresenceState {
    Present,
    TemporarilyUnavailable,
    ConfirmedAbsent,
}

impl PresenceState {
    #[must_use]
    pub const fn allows_detach(self) -> bool {
        matches!(self, Self::ConfirmedAbsent)
    }
}
