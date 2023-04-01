use std::collections::HashMap;

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

pub fn uid_map_into<T>(input_map: HashMap<UserIdentifier, T>) -> HashMap<String, T> {
    let mut output_map = HashMap::new();
    for (uid, value) in input_map {
        output_map.insert(uid.username, value);
    }
    return output_map;
}

pub fn username_map_into<T>(input_map: HashMap<String, T>) -> HashMap<UserIdentifier, T> {
    let mut output_map = HashMap::new();
    for (username, value) in input_map {
        output_map.insert(UserIdentifier { username }, value);
    }
    return output_map;
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UserProfile {
    pub username: String,
    pub name: String,
    pub color: String,
    pub pfp: String,
    pub public_key: String,
}

impl UserProfile {
    pub fn dummy(username: String) -> UserProfile {
        UserProfile { username, name: "".to_string(), color: "".to_string(), pfp:"".to_string(), public_key:"".to_string() }
    }
}

#[derive(Deserialize)]
pub struct CreateUser {
    pub name: String,
    pub color: String,
    pub public_key: String,
}

impl CreateUser {
    pub fn to_user_profile(&self, username: String, pfp: String) -> UserProfile {
        UserProfile {
            username,
            name: self.name.clone(),
            color: self.color.clone(),
            pfp: pfp,
            public_key: self.public_key.clone(),
        }
    }
}