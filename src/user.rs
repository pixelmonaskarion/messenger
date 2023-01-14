use serde::Serialize;
use rocket::serde::Deserialize;

#[derive(Deserialize, Eq, Hash, Clone, Debug, Serialize)]
pub struct UserIdentifier {
    pub username: String,
}

impl PartialEq for UserIdentifier {
    fn eq(&self, other: &Self) -> bool {
        self.username == other.username
    }
}

#[derive(Serialize)]
pub struct UserProfile {
    pub username: String,
    pub name: String,
}

#[derive(Deserialize)]
pub struct CreateUser {
    pub name: String,
}

impl CreateUser {
    pub fn to_user_profile(&self, username: String) -> UserProfile {
        UserProfile {
            username: username,
            name: self.name.clone(),
        }
    }
}
