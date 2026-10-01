//! [`Warning`]: a non-fatal problem encountered while scanning, e.g. a
//! malformed `About.xml` that had to be skipped.

use serde::{Deserialize, Serialize};

use super::mod_id::ModId;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Warning {
    /// The mod this warning is about, when it could be attributed to one.
    pub mod_id: Option<ModId>,
    pub message: String,
}

impl Warning {
    #[must_use]
    pub fn new(mod_id: Option<ModId>, message: impl Into<String>) -> Self {
        Self {
            mod_id,
            message: message.into(),
        }
    }
}
