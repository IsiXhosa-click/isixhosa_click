use fluent_templates::LanguageIdentifier;
use std::fmt::{Display, Formatter};
use std::num::NonZeroU64;

#[derive(Clone, Debug)]
pub struct User {
    pub user_id: NonZeroU64,
    pub username: String,
    pub permissions: Permissions,
    pub language: LanguageIdentifier,
}

#[cfg_attr(feature = "server", derive(clap::ValueEnum))]
#[derive(PartialEq, Eq, Ord, PartialOrd, Clone, Copy, Debug)]
pub enum Permissions {
    /// A regular user
    User,
    /// A moderator who is able to approve suggestions
    Moderator,
    /// An administrator who is able to configure the site itself
    Administrator,
}

impl Display for Permissions {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Permissions::User => "User",
            Permissions::Moderator => "Moderator",
            Permissions::Administrator => "Administrator",
        };

        f.write_str(s)
    }
}

impl Permissions {
    pub fn contains(&self, other: Permissions) -> bool {
        *self >= other
    }
}

#[derive(Clone, Debug)]
pub enum Auth {
    LoggedIn(User),
    NotLoggedIn,
    Offline,
}

impl Default for Auth {
    fn default() -> Self {
        Self::NotLoggedIn
    }
}

impl From<User> for Auth {
    fn from(user: User) -> Self {
        Auth::LoggedIn(user)
    }
}

impl Auth {
    pub fn user(&self) -> Option<&User> {
        match self {
            Auth::LoggedIn(user) => Some(user),
            _ => None,
        }
    }

    // used in templates (macros.askama.html)
    pub fn has_moderator_permissions(&self) -> bool {
        self.has_permissions(Permissions::Moderator)
    }

    // used in templates (macros.askama.html)
    pub fn has_administrator_permissions(&self) -> bool {
        self.has_permissions(Permissions::Administrator)
    }

    pub fn has_permissions(&self, permissions: Permissions) -> bool {
        self.user()
            .map(|user| user.permissions.contains(permissions))
            .unwrap_or_default()
    }

    pub fn username(&self) -> Option<String> {
        self.user().map(|user| user.username.clone())
    }

    pub fn user_id(&self) -> Option<NonZeroU64> {
        self.user().map(|user| user.user_id)
    }
}
