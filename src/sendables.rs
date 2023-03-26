use std::{fmt, time::{SystemTime, UNIX_EPOCH}};
use serde::Serialize;
use rocket::serde::Deserialize;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Sendable {
    pub sendable_type: SendableType,
    pub data: String,
    pub timestamp: Option<u128>,
}

impl fmt::Display for Sendable {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.to_string())
    }
}

impl Sendable {
    pub fn to_string(&self) -> String {
        if self.timestamp.is_some() {
            return format!("{{\"{}\":{}, \"timestamp\":{}}}", self.sendable_type.to_string(), self.data, self.timestamp.unwrap());
        } else {
            return format!("{{\"{}\":{}}}", self.sendable_type.to_string(), self.data);
        }
    }

    pub fn new(sendable_type: SendableType, data: String, timestamp: Option<u128>) -> Self {
        Self {
            sendable_type,
            data,
            timestamp,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum SendableType {
    Message,
    Read,
    Banner,
    Reaction,
}

impl SendableType {
    pub fn to_string(&self) -> String {
        match self {
            SendableType::Message => "message".to_string(),
            SendableType::Read => "read".to_string(),
            SendableType::Banner => "banner".to_string(),
            SendableType::Reaction => "reaction".to_string(),
        }
    }
}

pub fn banner(text: String, chat: u32, id: u32) -> Sendable {
    let start = SystemTime::now();
    let since_the_epoch = start
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards");
    let timestamp = since_the_epoch.as_millis();
    let sendable = Sendable::new(SendableType::Banner, format!("{{\"text\":\"{}\", \"chat\": {}, 'id': {}}}", text, chat, id), Some(timestamp));
    sendable
}
pub fn read(status: String, username: String, messageid: u32, chatid: u32) -> Sendable {
    let start = SystemTime::now();
    let since_the_epoch = start
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards");
    let timestamp = since_the_epoch.as_millis();
    let sendable = Sendable::new(SendableType::Read, format!("{{\"status\":\"{}\", \"message\":{{\"id\":{}, \"chat\":{}}}, \"from\":\"{}\"}}", status, messageid, chatid, username), Some(timestamp));
    sendable
}

pub fn reaction(emoji: String, username: String, messageid: u32, chatid: u32) -> Sendable {
    let start = SystemTime::now();
    let since_the_epoch = start
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards");
    let timestamp = since_the_epoch.as_millis();
    let sendable = Sendable::new(SendableType::Reaction, format!("{{\"emoji\":\"{}\", \"message\":{}, \"from\":\"{}\", \"chat\":{}}}", emoji, messageid, username, chatid), Some(timestamp));
    sendable
}