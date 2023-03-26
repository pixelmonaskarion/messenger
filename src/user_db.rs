use std::{collections::HashMap, sync::{Mutex, Arc}};

use rocket::{State, http::ContentType};
use serde::{Deserialize, Serialize};
use crate::{Server, message::Message, sendables::{Sendable, SendableType}};

#[derive(Deserialize, Serialize, Clone)]
pub struct UserDB {
    pub messages: DBMap<DBMap<DBEntry>>,
}

impl UserDB {
    pub fn new() -> Self {
        Self {
            messages: DBMap::new(),
        }
    }
}

#[derive(Deserialize, Serialize, Clone)]
pub struct DBMap<T: TimeStamped> {
    pub map: HashMap<u32, T>,
    pub timestamp_sorted: Vec<u32>,
}

impl<T: TimeStamped> DBMap<T> {
    pub fn get(&self, key: &u32) -> Option<&T> {
        return self.map.get(key);
    }

    pub fn contains_key(&self, key: &u32) -> bool {
        return self.map.contains_key(key);
    }

    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
            timestamp_sorted: Vec::new(),
        }
    }

    pub fn insert(&mut self, key: u32, entry: T) {
        let timestamp = entry.get_timestamp();
        for i in 0..self.timestamp_sorted.len() {
            let other_ts = self.map.get(&self.timestamp_sorted[i]).unwrap().get_timestamp();
            if timestamp >= other_ts {
                self.timestamp_sorted.insert(i, key);
                break;
            }
        }
        self.map.insert(key, entry);
    }

    pub fn update(&mut self, key: u32, new_entry: T) {
        let timestamp_index = self.timestamp_sorted
        .iter()
        .position(|&x| x == key);
        if timestamp_index.is_some() {
            self.timestamp_sorted.remove(timestamp_index.unwrap());
        }
        self.insert(key, new_entry);
    }
}

impl<T: TimeStamped> TimeStamped for DBMap<T> {
    fn get_timestamp(&self) -> u128 {
        if self.timestamp_sorted.len() > 0 {
            return self.map.get(&self.timestamp_sorted[0]).unwrap().get_timestamp();
        }
        return 0;
    }
}

#[derive(Deserialize, Serialize, Clone)]
pub struct DBEntry {
    pub message: Option<Message>,
    pub sendable: Option<Sendable>,
    pub entry_type: DBEntryType,
}

impl DBEntry {
    pub fn message(message: Message) -> Self {
        Self {
            message: Some(message),
            sendable: None,
            entry_type: DBEntryType::Message,
        }
    }
    pub fn sendable(sendable: Sendable) -> Self {
        Self {
            message: None,
            sendable: Some(sendable),
            entry_type: DBEntryType::Sendable,
        }
    }
}

impl TimeStamped for DBEntry {
    fn get_timestamp(&self) -> u128 {
        let timestamp = match self.entry_type {
            DBEntryType::Message => self.message.as_ref().unwrap().timestamp,
            DBEntryType::Sendable => self.sendable.as_ref().unwrap().timestamp.unwrap_or(0),
        };
        return timestamp;
    }
}

#[derive(Deserialize, Serialize, Clone, PartialEq, Eq)]
pub enum DBEntryType {
    Message,
    Sendable,
}

pub trait TimeStamped {
    fn get_timestamp(&self) -> u128;
}

#[get("/db/message/<token>/<chat>/<message>")]
pub fn get_message(token: u32, chat: u32, message: u32, server_arc: &State<Arc<Mutex<Server>>>) -> (ContentType, String) {
    let server = server_arc.lock().unwrap();
    if server.tokens.lock().unwrap().contains_key(&token) {
        let tokens = server.tokens.lock().unwrap();
        let user_db = server.user_db.lock().unwrap();
        let uid = tokens.get(&token).unwrap();
        let udb = user_db.get(uid).unwrap();
        if udb.messages.contains_key(&chat) {
            if udb.messages.get(&chat).unwrap().contains_key(&message) {
                let entry = udb.messages.get(&chat).unwrap().get(&message).unwrap();
                let serialized = match entry.entry_type {
                    DBEntryType::Message => serde_json::ser::to_string(entry.message.as_ref().unwrap()).expect("couldn't serialize message"),
                    DBEntryType::Sendable => serde_json::ser::to_string(entry.sendable.as_ref().unwrap()).expect("couldn't serialize message"),
                };
                return (ContentType::JSON, serialized);
            } else {
                return (ContentType::JSON, "{\"server\":\"no message found\"}".into());
            }
        } else {
            return (ContentType::JSON, "{\"server\":\"no chat found\"}".into());
        }
    } else {
        return (ContentType::JSON, "{\"server\":\"invalid token\"}".into());
    }
}

#[get("/db/chat-messages/<token>/<chat>?<number>&<after>")]
pub fn get_chat_messages(token: u32, chat: u32, number: Option<usize>, after: Option<u128>, server_arc: &State<Arc<Mutex<Server>>>) -> (ContentType, String) {
    let server = server_arc.lock().unwrap();
    if server.tokens.lock().unwrap().contains_key(&token) {
        let tokens = server.tokens.lock().unwrap();
        let user_db = server.user_db.lock().unwrap();
        let uid = tokens.get(&token).unwrap();
        let udb = user_db.get(uid).unwrap();
        if udb.messages.contains_key(&chat) {
            let mut data = "[".to_string();
            let mut any_data = false;
            for (i, (_mid, entry)) in udb.messages.get(&chat).unwrap().map.iter().enumerate() {
                if number.is_some() {
                    if i+1 >= number.unwrap() {
                        break;
                    }
                }
                if after.is_some() {
                    if entry.get_timestamp() <= after.unwrap() {
                        break;
                    }
                }
                let serialized = match entry.entry_type {
                    DBEntryType::Message => Sendable::new(SendableType::Message, serde_json::ser::to_string(&entry.message.as_ref().unwrap()).expect("couldn't serialize message"), None).to_string(),
                    DBEntryType::Sendable => entry.sendable.as_ref().unwrap().to_string(),
                };
                data = format!("{}{},", data, serialized);
                any_data = true;
            }
            if any_data {
                data.pop();
            }
            data += "]";
            println!("{data}");
            return (ContentType::JSON, data);
        } else {
            return (ContentType::JSON, "{\"server\":\"no chat found\"}".into());
        }
    } else {
        return (ContentType::JSON, "{\"server\":\"invalid token\"}".into());
    }
}

#[get("/db/chats/<token>")]
pub fn get_chats(token: u32, server_arc: &State<Arc<Mutex<Server>>>) -> (ContentType, String) {
    let server = server_arc.lock().unwrap();
    if server.tokens.lock().unwrap().contains_key(&token) {
        let tokens = server.tokens.lock().unwrap();
        let user_db = server.user_db.lock().unwrap();
        let uid = tokens.get(&token).unwrap();
        let udb = user_db.get(uid).unwrap();
        let mut data = "[".to_string();
        let mut any_data = false;
        for chatid in udb.messages.map.keys() {
            data = format!("{}{},", data, chatid);
            any_data = true;
        }
        if any_data {
            data.pop();
        }
        data += "]";
        println!("{data}");
        return (ContentType::JSON, data);
    } else {
        return (ContentType::JSON, "{\"server\":\"invalid token\"}".into());
    }
}