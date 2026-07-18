//! The operation model: a single, described maintenance or diagnostic action
//! the toolkit can run.
//!
//! Operations are plain data. The UI renders them (label, description,
//! consequences) and the platform layer executes their [`CommandSpec`]. Adding a
//! new tool is a matter of adding operations to a catalog — there is no plugin
//! system or dynamic registry.

use crate::command::CommandSpec;
use crate::{Elevation, Risk};

/// A single maintenance or diagnostic action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Operation {
    /// Stable machine identifier, used for logging and UI state.
    pub id: &'static str,
    /// Short human-facing name (e.g. "System File Checker").
    pub label: &'static str,
    /// What the operation does and its consequences, shown in the confirmation
    /// dialog before it runs.
    pub description: &'static str,
    /// A rough duration expectation shown to the user (e.g. "Usually 5-15
    /// minutes").
    pub duration_hint: &'static str,
    /// How consequential the operation is.
    pub risk: Risk,
    /// Whether the operation must run elevated.
    pub elevation: Elevation,
    /// Whether the operation can be safely cancelled once started. Long
    /// servicing operations launched as elevated children cannot be signalled
    /// from the unelevated GUI and are unsafe to interrupt, so they are not
    /// cancelable.
    pub cancelable: bool,
    /// The command to run.
    pub command: CommandSpec,
}

/// The System File Checker: scans protected system files and repairs corrupted
/// ones from the local component store.
#[must_use]
pub fn sfc_scannow() -> Operation {
    Operation {
        id: "sfc-scannow",
        label: "System File Checker (sfc /scannow)",
        description: "Scans all protected Windows system files and repairs corrupted ones from the \
                      local component store. Makes no other changes. Requires Administrator rights \
                      and cannot be safely interrupted once started.",
        duration_hint: "Usually 5-15 minutes",
        risk: Risk::High,
        elevation: Elevation::Administrator,
        cancelable: false,
        command: CommandSpec::new("sfc", &["/scannow"]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sfc_is_an_elevated_uninterruptible_command() {
        let op = sfc_scannow();
        assert_eq!(op.elevation, Elevation::Administrator);
        assert!(!op.cancelable);
        assert_eq!(op.command, CommandSpec::new("sfc", &["/scannow"]));
    }
}
