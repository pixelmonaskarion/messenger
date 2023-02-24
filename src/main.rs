#[macro_use]
extern crate rocket;
use rand::Rng;
use rocket::fairing::{Fairing, Info, Kind};
use rocket::fs::FileServer;
use rocket::http::ContentType;
use rocket::http::Header;
use rocket::response::stream::TextStream;
use rocket::serde::json::Json;
use rocket::tokio::time::{self, Duration};
use rocket::State;
use rocket::{get, routes};
use rocket::{Request, Response};
use std::collections::HashMap;
use std::fs::{self, create_dir, OpenOptions, File};
use std::io::{Write, Read};
use std::ops::DerefMut;
use std::path::Path;
use std::sync::Mutex;
use std::sync::{mpsc::*, Arc};

mod message;
mod user;
mod actions;
mod sendables;
use message::*;
use user::*;
use actions::*;
use sendables::*;

pub struct Server {
    users: Mutex<HashMap<UserIdentifier, UserProfile>>,
    event_stream_senders: Mutex<HashMap<UserIdentifier, Vec<Sender<Sendable>>>>,
    tokens: Mutex<HashMap<u32, UserIdentifier>>,
    chats: Mutex<HashMap<u32, Chat>>,
    passwords: Mutex<HashMap<UserIdentifier, String>>,
    message_queue: Mutex<HashMap<UserIdentifier, HashMap<u32, Sendable>>>,
    sendable_queue: Mutex<HashMap<UserIdentifier, Vec<Sendable>>>,
    chat_join_ids: Mutex<HashMap<u32, u32>>,
}

impl Server {
    pub fn new() -> Self {
        Self {
            users: Mutex::new(HashMap::new()),
            event_stream_senders: Mutex::new(HashMap::new()),
            tokens: Mutex::new(HashMap::new()),
            chats: Mutex::new(HashMap::new()),
            passwords: Mutex::new(HashMap::new()),
            message_queue: Mutex::new(HashMap::new()),
            sendable_queue: Mutex::new(HashMap::new()),
            chat_join_ids: Mutex::new(HashMap::new()),
        }
    }

    pub fn from_file() -> Self {
        if Path::new("save/chats.json").exists()
            && Path::new("save/tokens.json").exists()
            && Path::new("save/users.json").exists()
        {
            let users = Mutex::new(user::username_map_into(
                serde_json::from_str(
                    fs::read_to_string("save/users.json")
                        .expect("Should have been able to read the file")
                        .as_str(),
                )
                .expect("couldn't parse users"),
            ));
            let message_queue = Mutex::new(user::username_map_into(
                serde_json::from_str(
                    fs::read_to_string("save/message_queue.json")
                        .expect("Should have been able to read the file")
                        .as_str(),
                )
                .expect("couldn't parse message queue"),
            ));
            let sendable_queue = Mutex::new(user::username_map_into(
                serde_json::from_str(
                    fs::read_to_string("save/sendable_queue.json")
                        .expect("Should have been able to read the file")
                        .as_str(),
                )
                .expect("couldn't parse sendable queue"),
            ));
            let tokens = Mutex::new(
                serde_json::from_str(
                    fs::read_to_string("save/tokens.json")
                        .expect("Should have been able to read the file")
                        .as_str(),
                )
                .expect("couldn't parse tokens"),
            );
            let chats = Mutex::new(
                serde_json::from_str(
                    fs::read_to_string("save/chats.json")
                        .expect("Should have been able to read the file")
                        .as_str(),
                )
                .expect("couldn't parse chats"),
            );
            let passwords = Mutex::new(user::username_map_into(
                serde_json::from_str(
                    fs::read_to_string("save/passwords.json")
                        .expect("Should have been able to read the file")
                        .as_str(),
                )
                .expect("couldn't parse passwords"),
            ));
            let chat_join_ids = Mutex::new(
                serde_json::from_str(
                    fs::read_to_string("save/chat_join_ids.json")
                        .expect("Should have been able to read the file")
                        .as_str(),
                )
                .expect("couldn't parse chat join ids"),
            );
            return Self {
                users,
                event_stream_senders: Mutex::new(HashMap::new()),
                tokens,
                chats,
                passwords,
                message_queue,
                sendable_queue,
                chat_join_ids,
            };
        }
        return Self::new();
    }
}

#[get("/events/<token>")]
async fn events(token: u32, server_arc: &State<Arc<Mutex<Server>>>) -> TextStream![String + '_] {
    let server = server_arc.lock().unwrap();
    let (sender, receiver) = channel::<Sendable>();
    let tokens = server.tokens.lock().unwrap().clone();
    let uid = tokens.get(&token);
    let mut invalid_token = false;
    let mut messages = Vec::new();
    if uid.is_none() {
        invalid_token = true;
    } else {
        let mut event_stream_senders = server.event_stream_senders.lock().unwrap();
        let senders_option = event_stream_senders.get(&uid.unwrap().clone());
        let mut senders = Vec::new();
        if senders_option.is_some() {
            senders = senders_option.unwrap().clone();
        }
        senders.push(sender);
        event_stream_senders.insert(uid.unwrap().clone(), senders);
        if server.message_queue.lock().unwrap().contains_key(uid.unwrap()) {
            for message in server.message_queue.lock().unwrap().get(uid.unwrap()).unwrap().values() {
                messages.push(message.clone());
            }
        }
        if server.sendable_queue.lock().unwrap().contains_key(uid.unwrap()) {
            for message in server.sendable_queue.lock().unwrap().get(uid.unwrap()).unwrap() {
                messages.push(message.clone());
            }
        } 
        server.message_queue.lock().unwrap().insert(uid.unwrap().clone(), HashMap::new());
        server.sendable_queue.lock().unwrap().insert(uid.unwrap().clone(), Vec::new());
    }
    return TextStream! {
        if invalid_token {
            yield "{\"server_reponse\":\"invalid token\"}|".to_string();
        } else {
            for message in messages {
                //let message_json = serde_json::to_string(&message).expect("Couldn't Serialize Message!");
                yield format!("{}|", message.to_string());
            }
            let mut interval = time::interval(Duration::from_secs(1));
            let mut seconds = 31;
            loop {
                seconds += 1;
                if seconds >= 30 {
                    yield format!("{{\"server\":\"ping\"}}|");
                    seconds = 0;
                }
                let message_option = receiver.try_recv();
                if message_option.is_ok() {
                    yield format!("{}|", message_option.unwrap().to_string());
                }
                interval.tick().await;
            }
        }
    };
}

#[get("/login/<username>/<password>")]
fn login(
    username: String,
    password: String,
    server_arc: &State<Arc<Mutex<Server>>>,
) -> (ContentType, String) {
    let server = server_arc.lock().unwrap();
    if server.users.lock().unwrap().contains_key(&UserIdentifier {
        username: username.clone(),
    }) {
        if server
            .passwords
            .lock()
            .unwrap()
            .get(&UserIdentifier {
                username: username.clone(),
            })
            .unwrap()
            != &password
        {
            return (
                ContentType::JSON,
                "{\"server\":\"incorrect password\"}".to_string(),
            );
        }
    }
    let mut rng = rand::thread_rng();
    let mut token = rng.gen::<u32>();
    loop {
        if !server.tokens.lock().unwrap().contains_key(&token) {
            break;
        }
        token = rng.gen::<u32>();
    }
    server.passwords.lock().unwrap().insert(
        UserIdentifier {
            username: username.clone(),
        },
        password,
    );
    server.tokens.lock().unwrap().insert(
        token,
        UserIdentifier {
            username: username.clone(),
        },
    );
    if server
        .message_queue
        .lock()
        .unwrap()
        .get(&UserIdentifier {
            username: username.clone(),
        })
        .is_none()
    {
        server.message_queue.lock().unwrap().insert(
            UserIdentifier {
                username: username.clone(),
            },
            HashMap::new(),
        );
    }
    return (ContentType::JSON, format!("{{\"token\":{}}}", token));
}

#[post("/create-user/<token>", data = "<created_user>")]
fn create_user(token: u32, created_user: Json<CreateUser>, server_arc: &State<Arc<Mutex<Server>>>) {
    let server = server_arc.lock().unwrap();
    if !server.tokens.lock().unwrap().contains_key(&token) {
        return;
    }
    let username = server
        .tokens
        .lock()
        .unwrap()
        .get(&token)
        .unwrap()
        .username
        .clone();
    let user_profile = created_user.to_user_profile(username);
    server.users.lock().unwrap().insert(
        server.tokens.lock().unwrap().get(&token).unwrap().clone(),
        user_profile,
    );
}

#[post("/edit-chat/<chatid>/<token>", data = "<chat_edit>")]
fn edit_chat(chatid: u32, token: u32, chat_edit: Json<ChatEdit>, server_arc: &State<Arc<Mutex<Server>>>) {
    let server = server_arc.lock().unwrap();
    if server.tokens.lock().unwrap().contains_key(&token) {
        if server.chats.lock().unwrap().contains_key(&chatid) {
            if *server.tokens.lock().unwrap().get(&token).unwrap() == server.chats.lock().unwrap().get(&chatid).unwrap().admin {
                let mut chats_option = server.chats.lock();
                let chats = chats_option.as_mut().unwrap();
                let chat = chats.get_mut(&chatid).unwrap();
                chat.admin = chat_edit.new_admin.clone();
                chat.name = chat_edit.new_name.clone();
                chat.users.append(&mut chat_edit.added_users.clone());
                /*for user in &chat.users {
                    println!("{}", user.username);
                    let event_stream_senders = server.event_stream_senders.lock().unwrap();
                    let senders_option = event_stream_senders.get(user);
                    for new_user in chat_edit.added_users.clone() {
                        let sendable = Sendablenew(SendableType::Banner, format!("{{\"text\":\"{} joined this chat\", \"chat\": {}}}", server.users.lock().unwrap().get(&new_user).unwrap_or(&UserProfile { username: new_user.username, name: "".to_string(), color: "".to_string() }).name, chat.id));
                        server
                            .sendable_queue
                            .lock()
                            .unwrap()
                            .get_mut(user)
                            .unwrap_or(&mut Vec::new())
                            .push(sendable.clone());
                        if senders_option.is_some() {
                            for sender in senders_option.unwrap() {
                                match sender.send(sendable.clone()) {
                                    Ok(_) => {}
                                    Err(e) => println!("{e}"),
                                }
                            }
                        }
                    }
                }*/        
                for new_user in chat_edit.added_users.clone() {                
                    //let sendable = Sendable::(SendableType::Banner, format!("{{\"text\":\"{} joined this chat\", \"chat\": {}}}", server.users.lock().unwrap().get(&new_user).unwrap_or(&UserProfile { username: new_user.username, name: "".to_string(), color: "".to_string() }).name, chat.id));
                    let name = server.users.lock().unwrap().get(&new_user).unwrap_or(&UserProfile { username: new_user.username, name: "".to_string(), color: "".to_string() }).name.clone();
                    let sendable = banner(format!("{} joined this chat", name), chat.id);
                    send_sendable(sendable, &chat.users, &server);
                }
                println!("updated chat name: {} admin: {:?}", chat.name, chat.admin);
            } else {
                println!("user not admin");
            }
        } else {
            println!("invalid chat");
        }
    } else {
        println!("invalid token");
    }
}

#[get("/token-valid/<token>")]
fn token_valid(token: u32, server_arc: &State<Arc<Mutex<Server>>>) -> String {
    let server = server_arc.lock().unwrap();
    if server.tokens.lock().unwrap().contains_key(&token) {
        return "true".to_string();
    } else {
        return "false".to_string();
    }
}

#[get("/get-user/<username>")]
fn get_user(username: String, server_arc: &State<Arc<Mutex<Server>>>) -> (ContentType, String) {
    let server = server_arc.lock().unwrap();
    let uid = UserIdentifier { username };
    if server.users.lock().unwrap().contains_key(&uid) {
        let user_json = serde_json::to_string(server.users.lock().unwrap().get(&uid).unwrap())
            .expect("Couldn't parse user");
        println!("{user_json}");
        return (ContentType::JSON, user_json);
    }
    println!("got invalid user");
    return (ContentType::JSON, "{\"server\":\"no user\"}".to_string());
}
#[get("/get-chat/<chatid>/<token>")]
fn get_chat(
    chatid: u32,
    token: u32,
    server_arc: &State<Arc<Mutex<Server>>>,
) -> (ContentType, String) {
    let server = server_arc.lock().unwrap();
    if server.chats.lock().unwrap().contains_key(&chatid) {
        if server.tokens.lock().unwrap().contains_key(&token) {
            if server
                .chats
                .lock()
                .unwrap()
                .get(&chatid)
                .unwrap()
                .users
                .contains(server.tokens.lock().unwrap().get(&token).unwrap())
            {
                let chat_json =
                    serde_json::to_string(server.chats.lock().unwrap().get(&chatid).unwrap())
                        .expect("Couldn't parse chat");
                return (ContentType::JSON, chat_json);
            } else {
                return (
                    ContentType::JSON,
                    "{\"server\":\"user not in chat\"}".to_string(),
                );
            }
        } else {
            return (
                ContentType::JSON,
                "{\"server\":\"invalid token\"}".to_string(),
            );
        }
    } else {
        return (ContentType::JSON, "{\"server\":\"no chat\"}".to_string());
    }
}

#[get("/create-chat-link/<chatid>/<token>")]
fn create_chat_link(
    chatid: u32,
    token: u32,
    server_arc: &State<Arc<Mutex<Server>>>,
) -> (ContentType, String) {
    let server = server_arc.lock().unwrap();
    if server.chats.lock().unwrap().contains_key(&chatid) {
        if server.tokens.lock().unwrap().contains_key(&token) {
            if server
                .chats
                .lock()
                .unwrap()
                .get(&chatid)
                .unwrap()
                .users
                .contains(server.tokens.lock().unwrap().get(&token).unwrap())
            {
                let mut rng = rand::thread_rng();
                let join_code = rng.gen::<u32>();
                server.chat_join_ids.lock().unwrap().insert(join_code, chatid);
                return (
                    ContentType::JSON,
                    format!("{{\"join_code\":{}}}", join_code),
                );
            } else {
                return (
                    ContentType::JSON,
                    "{\"server\":\"user not in chat\"}".to_string(),
                );
            }
        } else {
            return (
                ContentType::JSON,
                "{\"server\":\"invalid token\"}".to_string(),
            );
        }
    } else {
        return (ContentType::JSON, "{\"server\":\"no chat\"}".to_string());
    }
}

#[post("/join-chat-link/<join_code>/<token>")]
fn join_chat_link(
    join_code: u32,
    token: u32,
    server_arc: &State<Arc<Mutex<Server>>>,
) {
    let server = server_arc.lock().unwrap();
    if server.chat_join_ids.lock().unwrap().contains_key(&join_code) {
        let chatid = server.chat_join_ids.lock().unwrap().get(&join_code).unwrap().clone();
        if server.chats.lock().unwrap().contains_key(&chatid) {
            if server.tokens.lock().unwrap().contains_key(&token) {
                if !server
                    .chats.lock().unwrap()
                    .get(&chatid).unwrap()
                    .users.contains(server.tokens.lock().unwrap().get(&token).unwrap())
                {
                    //server.chat_join_ids.lock().unwrap().remove(&join_code);
                    server.chats.lock().unwrap().get_mut(&chatid).unwrap().users.push(server.tokens.lock().unwrap().get(&token).unwrap().clone());
                    /*for user in &server.chats.lock().unwrap().get_mut(&chatid).unwrap().users {
                        println!("sending to {}", user.username);
                        let event_stream_senders = server.event_stream_senders.lock().unwrap();
                        let senders_option = event_stream_senders.get(user);
                        let uid = server.tokens.lock().unwrap().get(&token).unwrap().clone();
                        let sendable = Sendablenew(SendableType::Banner, format!("{{\"text\":\"{} joined this chat\", \"chat\": {}}}", server.users.lock().unwrap().get(&uid).unwrap_or(&UserProfile { username: uid.username.clone(), name: uid.username.clone(), color: "".to_string() }).name, chatid));
                        server
                            .sendable_queue
                            .lock()
                            .unwrap()
                            .get_mut(user)
                            .unwrap_or(&mut Vec::new())
                            .push(sendable.clone());
                        if senders_option.is_some() {
                            for sender in senders_option.unwrap() {
                                match sender.send(sendable.clone()) {
                                    Ok(_) => {}
                                    Err(e) => println!("{e}"),
                                }
                            }
                        }
                    }*/
                    let uid = server.tokens.lock().unwrap().get(&token).unwrap().clone();
                    let name = server.users.lock().unwrap().get(&uid).unwrap_or(&UserProfile { username: uid.username.clone(), name: "".to_string(), color: "".to_string() }).name.clone();
                    let sendable = banner(format!("{} joined this chat", name), chatid);
                    println!("user {} joining chat {}", uid.username, server.chats.lock().unwrap().get_mut(&chatid).unwrap().name);
                    send_sendable(sendable, &server.chats.lock().unwrap().get_mut(&chatid).unwrap().users, &server);
                }
            }
        }
    }
}




#[post("/received-message/<token>", data = "<message>")]
fn received_message(token: u32, message: Json<Message>, server_arc: &State<Arc<Mutex<Server>>>) {
    let server = server_arc.lock().unwrap();
    let tokens = server.tokens.lock().unwrap();
    let uid = tokens.get(&token);
    if uid.is_some() {
        server.message_queue.lock().unwrap().get_mut(uid.unwrap()).unwrap().remove(&message.id);
        let sender_uid = &message.from_user;
        //let sendable = Sendablenew(SendableType::Read, format!("{{\"status\":\"Delivered\", \"message\":{}, \"from\":\"{}\"}}", serde_json::ser::to_string(&message.0).expect("couldn't serialize message"), uid.unwrap().username));
        let sendable = read("Delivered".to_string(), uid.unwrap().username.clone(), &message.0);
        /*
        let senders_option = event_stream_senders.get(sender_uid);
        let event_stream_senders = server.event_stream_senders.lock().unwrap();
        server
            .sendable_queue
            .lock()
            .unwrap()
            .get_mut(sender_uid)
            .unwrap_or(&mut Vec::new())
            .push(sendable.clone());
        if senders_option.is_some() {
            for sender in senders_option.unwrap() {
                match sender.send(sendable.clone()) {
                    Ok(_) => {}
                    Err(e) => println!("{e}"),
                }
            }
        }*/
        send_sendable(sendable, &[sender_uid.clone()].to_vec(), &server);
    }
}

#[post("/read-message/<token>", data = "<message>")]
fn read_message(token: u32, message: Json<Message>, server_arc: &State<Arc<Mutex<Server>>>) {
    let server = server_arc.lock().unwrap();
    let tokens = server.tokens.lock().unwrap();
    let uid = tokens.get(&token);
    if uid.is_some() {
        let sender_uid = &message.from_user;
        //let sendable = Sendablenew(SendableType::Read, format!("{{\"status\":\"Read\", \"message\":{}}}", serde_json::ser::to_string(&message.0).expect("couldn't serialize message")));
        let sendable = read("Read".to_string(), uid.unwrap().username.clone(), &message.0);
        /*let event_stream_senders = server.event_stream_senders.lock().unwrap();
        let senders_option = event_stream_senders.get(sender_uid);
        let sendable = Sendablenew(SendableType::Read, format!("{{\"status\":\"Read\", \"message\":{}}}", serde_json::ser::to_string(&message.0).expect("couldn't serialize message")));
        server
            .sendable_queue
            .lock()
            .unwrap()
            .get_mut(sender_uid)
            .unwrap_or(&mut Vec::new())
            .push(sendable.clone());
        if senders_option.is_some() {
            for sender in senders_option.unwrap() {
                match sender.send(sendable.clone()) {
                    Ok(_) => {}
                    Err(e) => println!("{e}"),
                }
            }
        }*/
        send_sendable(sendable, &[sender_uid.clone()].to_vec(), &server);
    }
}

#[post("/logout/<token>")]
fn logout(token: u32, server_arc: &State<Arc<Mutex<Server>>>) {
    let server = server_arc.lock().unwrap();
    server.tokens.lock().unwrap().remove(&token);
}

#[post("/post-message", data = "<sent_message>")]
fn post_message(sent_message: Json<SendMessage>, server_arc: &State<Arc<Mutex<Server>>>) -> String {
    let server = server_arc.lock().unwrap();
    if !server
        .tokens
        .lock()
        .unwrap()
        .contains_key(&sent_message.from_user)
    {
        return "Invalid Token >:(".to_string();
    }
    let mut rng = rand::thread_rng();
    let message_id = rng.gen::<u32>();
    let message = sent_message.to_message(message_id, &server);
    send_message(message, server);
    /*
    let chats = server.chats.lock().unwrap();
    let chat = chats.get(&message.chat);
    if chat.is_none() {
        return "Invalid Chat".to_string();
    }
    for uid in &chat.unwrap().users {
        let event_stream_senders = server.event_stream_senders.lock().unwrap();
        let senders_option = event_stream_senders.get(&uid);
        server
            .message_queue
            .lock()
            .unwrap()
            .get_mut(uid)
            .unwrap_or(&mut HashMap::new())
            .insert(message_id, Sendablenew(SendableType::Message, serde_json::ser::to_string(&message).expect("couldn't serialize message")));
        if senders_option.is_some() {
            for sender in senders_option.unwrap() {
                match sender.send(Sendablenew(SendableType::Message, serde_json::ser::to_string(&message).expect("couldn't serialize message"))) {
                    Ok(_) => {}
                    Err(e) => println!("{e}"),
                }
            }
        }
    }
    */
    "Thank you :)".to_string()
}

#[post("/create-chat", data = "<created_chat>")]
fn create_chat(
    created_chat: Json<CreateChat>,
    server_arc: &State<Arc<Mutex<Server>>>,
) -> (ContentType, String) {
    let server = server_arc.lock().unwrap();
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

fn write_to_file(server_arc: Arc<Mutex<Server>>) -> std::io::Result<()> {
    println!("writing to files");
    let server = server_arc.lock().unwrap();
    if !Path::new("save").exists() {
        create_dir("save")?;
    }
    let mut users_file = create_or_open_file("save/users.json")?;
    users_file.write_all(
        serde_json::to_string(&user::uid_map_into(server.users.lock().unwrap().clone()))
            .expect("could not write to users file")
            .as_bytes(),
    )?;
    let mut message_queue_file = create_or_open_file("save/message_queue.json")?;
    message_queue_file.write_all(
        serde_json::to_string(&user::uid_map_into(
            server.message_queue.lock().unwrap().clone(),
        ))
        .expect("could not write to message queue file")
        .as_bytes(),
    )?;
    let mut sendable_queue_file = create_or_open_file("save/sendable_queue.json")?;
    sendable_queue_file.write_all(
        serde_json::to_string(&user::uid_map_into(
            server.sendable_queue.lock().unwrap().clone(),
        ))
        .expect("could not write to sendable queue file")
        .as_bytes(),
    )?;
    let mut tokens_file = create_or_open_file("save/tokens.json")?;
    tokens_file.write_all(
        serde_json::to_string(server.tokens.lock().unwrap().deref_mut())
            .expect("could not write to tokens file")
            .as_bytes(),
    )?;
    let mut chats_file = create_or_open_file("save/chats.json")?;
    chats_file.write_all(
        serde_json::to_string(server.chats.lock().unwrap().deref_mut())
            .expect("could not write to chats file")
            .as_bytes(),
    )?;
    let mut passwords_file = create_or_open_file("save/passwords.json")?;
    passwords_file.write_all(
        serde_json::to_string(&user::uid_map_into(server.passwords.lock().unwrap().clone()))
            .expect("could not write to passwords file")
            .as_bytes(),
    )?;
    let mut chat_join_ids_file = create_or_open_file("save/chat_join_ids.json")?;
    chat_join_ids_file.write_all(
        serde_json::to_string(&server.chat_join_ids.lock().unwrap().clone())
            .expect("could not write to chat join ids file")
            .as_bytes(),
    )?;
    Ok(())
}

fn create_or_open_file(path: &str) -> Result<std::fs::File, std::io::Error> {
    return OpenOptions::new()
        .write(true)
        .create(!Path::new(path).exists())
        .truncate(true)
        .open(path);
}

#[get("/?<joinchat>")]
fn join_headers(joinchat: u32, server_arc: &State<Arc<Mutex<Server>>>) -> (ContentType, String) {
    let server = server_arc.lock().unwrap();
    println!("join chat {joinchat}");
    let mut file = File::open("F:\\chris\\rust\\messenger\\messenger-client\\build\\index.html").expect("index.html does not exist!!!");
    let mut file_text = String::new();
    file.read_to_string(&mut file_text).expect("could not read from index.html!!!");
    let chat_join_ids = server.chat_join_ids.lock().unwrap();
    let chats = server.chats.lock().unwrap();
    let users = server.users.lock().unwrap();
    let chatid_option = chat_join_ids.get(&joinchat);
    if chatid_option.is_some() {
        let chat_option = chats.get(chatid_option.unwrap());
        if chat_option.is_some() {
            let user_option = users.get(&chat_option.unwrap().admin);
            if user_option.is_some() {
                let user = user_option.unwrap();
                file_text = file_text.replace("<meta name=\"description\" content=\"Christopher's Cool Messaging app\"/>", format!("<meta name=\"description\" content=\"You are invited to join {}'s chat: {}\nClick here to join\"/>", user.name, chat_option.unwrap().name).as_str());
            }
        }
    }
    return (ContentType::HTML, file_text);
}

#[options("/<_..>")]
fn all_options() {
    /* Intentionally left empty */
}

#[rocket::main]
async fn main() {
    let server = Arc::new(Mutex::new(Server::from_file()));
    let result = rocket::build()
        .attach(CORS)
        .manage(server.clone())
        .mount("/", FileServer::from("F:\\chris\\rust\\messenger\\messenger-client\\build"))
        .mount(
            "/",
            routes![
                join_headers,
                events,
                post_message,
                get_user,
                login,
                logout,
                create_chat,
                create_user,
                token_valid,
                all_options,
                get_chat,
                received_message,
                edit_chat,
                read_message,
                create_chat_link,
                join_chat_link,
            ],
        )
        .launch()
        .await;
    match result {
        Ok(_val) => {}
        Err(e) => println!("{e}"),
    }
    write_to_file(server.clone()).expect("Failed to write server data!")
}

pub struct CORS;

#[rocket::async_trait]
impl Fairing for CORS {
    fn info(&self) -> Info {
        Info {
            name: "Add CORS headers to responses",
            kind: Kind::Response,
        }
    }

    async fn on_response<'r>(&self, request: &'r Request<'_>, response: &mut Response<'r>) {
        if let Some(hostname) = request.headers().get_one("origin") {
            if hostname == "http://minecraft.themagicdoor.org:8000" || hostname == "http://minecraft.themagicdoor.org:3000" || hostname == "http://localhost:3000" || hostname.contains("http://localhost:") {
                response.set_header(Header::new("Access-Control-Allow-Origin", hostname));
            }
        } else {
            response.set_header(Header::new("Access-Control-Allow-Origin", "nothing lmao"));
        }
        response.set_header(Header::new(
            "Access-Control-Allow-Methods",
            "POST, GET, PATCH, OPTIONS",
        ));
        response.set_header(Header::new("Access-Control-Allow-Headers", "*"));
        response.set_header(Header::new("Access-Control-Allow-Credentials", "true"));
    }
}