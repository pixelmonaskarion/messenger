#[macro_use] extern crate rocket;
use rand::Rng;
use rocket::State;
use rocket::http::ContentType;
use std::sync::Mutex;
use std::collections::HashMap;
use rocket::serde::json::Json;
use rocket::{get, routes};
use rocket::fs::{relative, FileServer};
use rocket::response::stream::TextStream;
use rocket::tokio::time::{self, Duration};
use rocket::http::Header;
use rocket::{Request, Response};
use rocket::fairing::{Fairing, Info, Kind};
use std::sync::mpsc::*;

#[cfg(test)] mod tests;

mod user;
mod message;
use user::*;
use message::*;

pub struct Server {
    users: Mutex<HashMap<UserIdentifier, UserProfile>>,
    event_stream_senders: Mutex<HashMap<UserIdentifier, Sender<Message>>>,
    tokens: Mutex<HashMap<u32, UserIdentifier>>,
    chats: Mutex<HashMap<u32, Chat>>,
}

impl Server {
    pub fn new() -> Self {
        Self {
            users: Mutex::new(HashMap::new()),
            event_stream_senders: Mutex::new(HashMap::new()),
            tokens: Mutex::new(HashMap::new()),
            chats: Mutex::new(HashMap::new()),
        }
    }
}

#[get("/events/<token>")]
async fn events(token: u32, server: &State<Server>) -> TextStream![String+'_] {
    let (sender, receiver) = channel::<Message>();
    let tokens = server.tokens.lock().unwrap().clone();
    let uid = tokens.get(&token);
    let uid_clone = match uid {
        Some(val) => Some(val.clone()),
        None => None,
    };
    return TextStream! {
        if uid_clone.is_none() {
            yield "{\"server_reponse\":\"invalid token\"}".to_string();
        }
        let uid_unwrapped = uid_clone.unwrap();
        server.event_stream_senders.lock().unwrap().insert(uid_unwrapped.clone(), sender);
        let mut interval = time::interval(Duration::from_secs(1));
        let mut seconds = 31;
        loop {
            seconds += 1;
            if seconds >= 30 {
                yield format!("{{\"server\":\"ping\"}}");
                seconds = 0;
            }
            let message_option = receiver.try_recv();
            if message_option.is_ok() {
                let message_json = serde_json::to_string(&message_option.unwrap()).expect("Couldn't Serialize Message!");
                //println!("sent message {:?} to user {}", message_option.unwrap(), uid_unwrapped.username);
                yield format!("{{\"message\":{message_json}}}");
            }
            interval.tick().await;
        }
    };
}

#[get("/login/<username>")]
fn login(username: String, server: &State<Server>) -> (ContentType, String) {
    if server.tokens.lock().unwrap().values().any(|val| val.username == username) {
        return (ContentType::JSON, "{\"server_message\":\"locked account\"}".to_string());
    }
    let mut rng = rand::thread_rng();
    let mut token = rng.gen::<u32>();
    loop {
        if !server.tokens.lock().unwrap().contains_key(&token) {
            break;
        }
        token = rng.gen::<u32>();
    }
    server.tokens.lock().unwrap().insert(token, UserIdentifier { username });
    return (ContentType::JSON, format!("{{\"token\":{}}}", token));
}

#[post("/create-user/<token>", data="<created_user>")]
fn create_user(token: u32, created_user: Json<CreateUser>, server: &State<Server>) {
    if !server.tokens.lock().unwrap().contains_key(&token) {
        return;
    }
    let username = server.tokens.lock().unwrap().get(&token).unwrap().username.clone();
    let user_profile = created_user.to_user_profile(username);
    server.users.lock().unwrap().insert(server.tokens.lock().unwrap().get(&token).unwrap().clone(), user_profile);
}

#[get("/token-valid/<token>")]
fn token_valid(token: u32, server: &State<Server>) -> String {
    if server.tokens.lock().unwrap().contains_key(&token) {
        return "true".to_string();
    } else {
        return "false".to_string();
    }
}

#[get("/get-user/<username>")]
fn get_user(username: String, server: &State<Server>) -> (ContentType, String) {
    let uid = UserIdentifier { username };
    if server.users.lock().unwrap().contains_key(&uid) {
        let user_json = serde_json::to_string(server.users.lock().unwrap().get(&uid).unwrap()).expect("Couldn't parse user");
        return (ContentType::JSON, user_json);
    }
    return (ContentType::JSON, "{\"server\":\"no user\"}".to_string());
}
#[get("/get-chat/<chatid>/<token>")]
fn get_chat(chatid: u32, token: u32, server: &State<Server>) -> (ContentType, String) {
    if server.chats.lock().unwrap().contains_key(&chatid) {
        if server.tokens.lock().unwrap().contains_key(&token) {
            if server.chats.lock().unwrap().get(&chatid).unwrap().users.contains(server.tokens.lock().unwrap().get(&token).unwrap()) {
                let chat_json = serde_json::to_string(server.chats.lock().unwrap().get(&chatid).unwrap()).expect("Couldn't parse chat");
                return (ContentType::JSON, chat_json);
            } else {
                return (ContentType::JSON, "{\"server\":\"user not in chat\"}".to_string());
            }
        } else {
            return (ContentType::JSON, "{\"server\":\"invalid token\"}".to_string());
        }
    } else {
        return (ContentType::JSON, "{\"server\":\"no chat\"}".to_string());
    }
}

#[post("/logout/<token>")]
fn logout(token: u32, server: &State<Server>) {
    server.tokens.lock().unwrap().remove(&token);
}

#[post("/post-message", data = "<sent_message>")]
fn post_message(sent_message: Json<SendMessage>, server: &State<Server>) -> String {
    if !server.tokens.lock().unwrap().contains_key(&sent_message.from_user) {
        return "Invalid Token >:(".to_string();
    }
    let mut rng = rand::thread_rng();
    let message_id = rng.gen::<u32>();
    let message = sent_message.to_message(message_id, server);
    println!("recieved message {} from {} to chat {:?}", message.text, message.from_user.username, message.chat);
    let chats = server.chats.lock().unwrap();
    let chat = chats.get(&message.chat);
    if chat.is_none() {
        return "Invalid Chat".to_string();
    }
    for uid in &chat.unwrap().users {
        let event_stream_senders = server.event_stream_senders.lock().unwrap();
        let sender_option = event_stream_senders.get(&uid);
        if sender_option.is_some() {
            println!("sending message to {}", uid.username);
            match sender_option.unwrap().send(message.clone()) {
                Ok(_) => {},
                Err(e) => println!("{e}"),
            }
        }
    }
    "Thank you :)".to_string()
}

#[post("/create-chat", data="<created_chat>")]
fn create_chat(created_chat: Json<CreateChat>, server: &State<Server>) -> (ContentType, String) {
    let mut rng = rand::thread_rng();
    let mut id = rng.gen::<u32>();
    loop {
        if !server.chats.lock().unwrap().contains_key(&id) {
            break;
        }
        id = rng.gen::<u32>();
    }
    let chat = created_chat.to_chat(id);
    let chat_json = serde_json::to_string(&chat).expect("Couldn't Serialize Message!");
    server.chats.lock().unwrap().insert(id, chat);
    return (ContentType::JSON, chat_json);
}

#[options("/<_..>")]
fn all_options() {
    /* Intentionally left empty */
}

#[launch]
fn rocket() -> _ {
    let server = Server::new();
    rocket::build()
        .attach(CORS)
        .manage(server)
        .mount("/", FileServer::from(relative!("static")))
        .mount("/", routes![events, post_message, get_user, login, logout, create_chat, create_user, token_valid, all_options, get_chat])
}

pub struct CORS;

#[rocket::async_trait]
impl Fairing for CORS {
    fn info(&self) -> Info {
        Info {
            name: "Add CORS headers to responses",
            kind: Kind::Response
        }
    }

    async fn on_response<'r>(&self, _request: &'r Request<'_>, response: &mut Response<'r>) {
        response.set_header(Header::new("Access-Control-Allow-Origin", "*"));
        response.set_header(Header::new("Access-Control-Allow-Methods", "POST, GET, PATCH, OPTIONS"));
        response.set_header(Header::new("Access-Control-Allow-Headers", "*"));
        response.set_header(Header::new("Access-Control-Allow-Credentials", "true"));
    }
}