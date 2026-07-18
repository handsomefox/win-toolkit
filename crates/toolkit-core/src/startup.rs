//! Portable types for the Startup manager. Reading and writing the registry
//! lives in `toolkit-platform`; this module only defines the shape.

/// Where a startup entry is registered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupScope {
    /// `HKEY_CURRENT_USER` — this user only; togglable without elevation.
    CurrentUser,
    /// `HKEY_LOCAL_MACHINE` — all users; changing it requires Administrator.
    LocalMachine,
}

impl StartupScope {
    /// A short human label for the scope.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            StartupScope::CurrentUser => "Current user",
            StartupScope::LocalMachine => "All users",
        }
    }
}

/// A program registered to run at sign-in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartupEntry {
    /// The registry value name (the entry's display name).
    pub name: String,
    /// The command line the entry runs.
    pub command: String,
    /// Where the entry lives.
    pub scope: StartupScope,
    /// Whether the entry is currently enabled.
    pub enabled: bool,
    /// Whether this app can toggle the entry without elevation (current-user
    /// entries only).
    pub can_toggle: bool,
}
