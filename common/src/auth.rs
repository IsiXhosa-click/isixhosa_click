use std::num::NonZeroU64;

#[derive(Clone, Debug)]
pub struct User {
    pub user_id: NonZeroU64,
    pub username: String,
    pub permissions: Permissions,
}

#[derive(PartialEq, Eq, Ord, PartialOrd, Clone, Copy, Debug)]
pub enum Permissions {
    User,
    Moderator,
    Administrator,
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
    fn user(&self) -> Option<&User> {
        match self {
            Auth::LoggedIn(user) => Some(user),
            _ => None,
        }
    }

    pub fn has_permissions(&self, permissions: Permissions) -> bool {
        self.user().map(|user| user.permissions.contains(permissions)).unwrap_or_default()
    }

    pub fn username(&self) -> Option<&str> {
        self.user().map(|user| &user.username as &str)
    }

    pub fn user_id(&self) -> Option<NonZeroU64> {
        self.user().map(|user| user.user_id)
    }
}
