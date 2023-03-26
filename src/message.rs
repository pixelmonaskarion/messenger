use std::collections::HashMap;

use crate::user::UserIdentifier;
use serde::Serialize;
use rocket::serde::Deserialize;

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Message {
    pub id: u32,
    pub text: String,
    pub from_user: UserIdentifier,
    pub chat: u32,
    pub timestamp: u128,
    pub read: String,
    pub reactions: HashMap<u32, String>,
}

#[derive(Deserialize)]
pub struct SendMessage {
    pub text: String,
    pub from_user: u32,
    pub chat: u32,
    pub timestamp: u128,
}

#[derive(Deserialize)]
pub struct EncryptedMessages {
    pub encrypted_messages: HashMap<String, SendMessage>,
}

#[derive(Deserialize)]
pub struct EncryptedMessages {
    pub encrypted_messages: HashMap<String, SendMessage>,
}

impl SendMessage {
    pub fn to_message(&self, id: u32, from_user: UserIdentifier ) -> Message {
    pub fn to_message(&self, id: u32, from_user: UserIdentifier ) -> Message {
        Message {
            id,
            text: self.text.clone(),
            from_user,
            from_user,
            chat: self.chat,
            timestamp: self.timestamp,
            read: "Sent".into(),
            reactions: HashMap::new(),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct Chat {
    pub users: Vec<UserIdentifier>,
    pub name: String,
    pub id: u32,
    pub admin: UserIdentifier,
}

#[derive(Deserialize)]
pub struct CreateChat {
    pub users: Vec<UserIdentifier>,
    pub name: String,
    pub admin: UserIdentifier,
}

impl CreateChat {
    pub fn to_chat(&self, id: u32) -> Chat {
        Chat {
            users: self.users.clone(),
            name: self.name.clone(),
            id,
            admin: self.admin.clone(),
        }
    }
}

#[derive(Deserialize)]
pub struct ChatEdit {
    pub added_users: Vec<UserIdentifier>,
    pub new_name: String,
    pub new_admin: UserIdentifier,
}