use std::{sync::MutexGuard, collections::HashMap};

use crate::{user::UserIdentifier, message::Message, Server, sendables::{Sendable, SendableType}};

pub fn send_sendable(sendable: Sendable, users: &Vec<UserIdentifier>, server: &MutexGuard<Server>) {
    for user in users {
        println!("sending to {}", user.username);
        let mut event_stream_senders = server.event_stream_senders.lock().unwrap();
        let senders_option = event_stream_senders.get_mut(user);
        let mut sent = false;
        if senders_option.is_some() {
            let mut new_senders = Vec::new();
            for sender in senders_option.unwrap() {
                match sender.send(sendable.clone()) {
                    Ok(_) => {
                        sent = true;
                        new_senders.push(sender.clone());
                    }
                    Err(e) => println!("{e} when sending to user {}", user.username),
                }
            }
            event_stream_senders.insert(user.clone(), new_senders);
        } else {
            println!("user {} has no senders list", user.username);
        }
        if !sent {
            if !server.sendable_queue.lock().unwrap().contains_key(&user) {
                println!("creating sending queue for user {}", user.username);
                server.sendable_queue.lock().unwrap().insert(user.clone(), Vec::new());
            }
            server
                .sendable_queue
                .lock()
                .unwrap()
                .get_mut(user)
                .unwrap_or(&mut Vec::new())
                .push(sendable.clone());
        }
    }
}

pub fn send_message(message: Message, server: MutexGuard<Server>) -> String {
    let chats = server.chats.lock().unwrap();
    let chat = chats.get(&message.chat);
    if chat.is_none() {
        return "Invalid Chat".to_string();
    }
    for uid in &chat.unwrap().users {
        let mut event_stream_senders = server.event_stream_senders.lock().unwrap();
        let senders_option = event_stream_senders.get_mut(&uid);
        let mut sent = false;
        if senders_option.is_some() {
            let mut new_senders = Vec::new();
            for sender in senders_option.unwrap() {
                match sender.send(Sendable::new(SendableType::Message, serde_json::ser::to_string(&message).expect("couldn't serialize message"), None)) {
                    Ok(_) => {
                        sent = true;
                        new_senders.push(sender.clone());
                    }
                    Err(e) => {
                        println!("{e}");
                    },
                }
            }
            event_stream_senders.insert(uid.clone(), new_senders);
        }
        if !sent {
            if !server.message_queue.lock().unwrap().contains_key(&uid) {
                server.message_queue.lock().unwrap().insert(uid.clone(), HashMap::new());
            }
            server
            .message_queue
            .lock()
            .unwrap()
            .get_mut(uid)
            .unwrap_or(&mut HashMap::new())
            .insert(message.id, Sendable::new(SendableType::Message, serde_json::ser::to_string(&message).expect("couldn't serialize message"), None));
        }   
    }
    "Thank you :)".to_string()
}