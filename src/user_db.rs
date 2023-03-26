use std::{collections::HashMap, sync::{Mutex, Arc}};

use rocket::{State, http::ContentType};
use serde::{Deserialize, Serialize};
use crate::{Server, message::Message};

#[derive(Deserialize, Serialize, Clone)]
pub struct UserDB {
    pub messages: HashMap<u32, HashMap<u32, Message>>,
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
                return (ContentType::JSON, serde_json::ser::to_string(&udb.messages.get(&chat).unwrap().get(&message).unwrap()).expect("couldn't serialize message"));
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
                data = format!("{}{}: '{}',", data, mid, serde_json::ser::to_string(&message).expect("couldn't serialize message"));
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