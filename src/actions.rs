use std::{sync::MutexGuard, collections::HashMap};

use crate::{user::UserIdentifier, message::Message, Server, sendables::{Sendable, SendableType}, user_db::UserDB};

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

pub fn send_message(message: Message, to_user: UserIdentifier, server: MutexGuard<Server>) {
    let mut event_stream_senders = server.event_stream_senders.lock().unwrap();
    let senders_option = event_stream_senders.get_mut(&to_user);
    let mut sent = false;
    let sendable = Sendable::new(SendableType::Message, serde_json::ser::to_string(&message).expect("couldn't serialize message"), None);
    if senders_option.is_some() {
        let mut new_senders = Vec::new();
        for sender in senders_option.unwrap() {
            match sender.send(sendable.clone()) {
                Ok(_) => {
                    sent = true;
                    new_senders.push(sender.clone());
                }
                Err(e) => {
                    println!("{e}");
                },
            }
        }
        event_stream_senders.insert(to_user.clone(), new_senders);
    }
    if !sent {
        if !server.message_queue.lock().unwrap().contains_key(&to_user) {
            server.message_queue.lock().unwrap().insert(to_user.clone(), HashMap::new());
        }
        server
        .message_queue
        .lock()
        .unwrap()
        .get_mut(&to_user)
        .unwrap_or(&mut HashMap::new())
        .insert(message.id, sendable.clone());
    }
    let mut user_db = server.user_db.lock().unwrap();
    if !user_db.contains_key(&to_user) {
        user_db.insert(to_user.clone(), UserDB::new());
    }
    let udb = user_db.get_mut(&to_user).unwrap();
    let messages = &mut udb.messages;
    if !messages.contains_key(&message.chat) {
        messages.insert(message.chat, HashMap::new());
    }
    messages.get_mut(&message.chat).unwrap().insert(message.id, message);
}