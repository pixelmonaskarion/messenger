use std::{collections::HashMap, sync::{Mutex, Arc}};

use rocket::{State, http::ContentType};
use serde::{Deserialize, Serialize};
use crate::Server;

use crate::sendables::Sendable;

#[derive(Deserialize, Serialize, Clone)]
pub struct UserDB {
    pub messages: HashMap<u32, HashMap<u32, Sendable>>,
}

impl UserDB {
    pub fn new() -> Self {
        Self {
            messages: HashMap::new(),
        }
    }
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
                return (ContentType::JSON, udb.messages.get(&chat).unwrap().get(&message).unwrap().to_string());
            } else {
                return (ContentType::JSON, "{'server':'no message found'}".into());
            }
        } else {
            return (ContentType::JSON, "{'server':'no chat found'}".into());
        }
    } else {
        return (ContentType::JSON, "{'server':'invalid token'}".into());
    }
}

#[get("/db/chat-messages/<token>/<chat>")]
pub fn get_chat_messages(token: u32, chat: u32, server_arc: &State<Arc<Mutex<Server>>>) -> (ContentType, String) {
    let server = server_arc.lock().unwrap();
    if server.tokens.lock().unwrap().contains_key(&token) {
        let tokens = server.tokens.lock().unwrap();
        let user_db = server.user_db.lock().unwrap();
        let uid = tokens.get(&token).unwrap();
        let udb = user_db.get(uid).unwrap();
        if udb.messages.contains_key(&chat) {
            let mut data = "[".to_string();
            for (mid, message) in udb.messages.get(&chat).unwrap() {
                data = format!("{}{}: '{}',", data, mid, message.to_string());
            }
            data += "]";
            return (ContentType::JSON, data);
        } else {
            return (ContentType::JSON, "{'server':'no chat found'}".into());
        }
    } else {
        return (ContentType::JSON, "{'server':'invalid token'}".into());
    }
}