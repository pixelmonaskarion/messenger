#[macro_use]
extern crate rocket;
use ::futures::executor::block_on;
use rand::Rng;
use rocket::fairing::{Fairing, Info, Kind};
use rocket::form::Form;
use rocket::fs::{FileServer, TempFile};
use rocket::http::ContentType;
use rocket::http::Header;
use rocket::response::stream::TextStream;
use rocket::serde::json::Json;
use rocket::tokio::time::{self, Duration};
use rocket::State;
use rocket::{get, routes};
use rocket::{Request, Response};
use std::collections::HashMap;
use std::fs::{self, create_dir, File, OpenOptions};
use std::io::{Read, Write};
use std::ops::DerefMut;
use std::path::Path;
use std::sync::Mutex;
use std::sync::{mpsc::*, Arc};

mod actions;
mod message;
mod sendables;
mod user;
mod user_db;
use actions::*;
use message::*;
use sendables::*;
use user::*;
use user_db::*;

pub struct Server {
    users: Mutex<HashMap<UserIdentifier, UserProfile>>,
    event_stream_senders: Mutex<HashMap<UserIdentifier, Vec<Sender<Sendable>>>>,
    tokens: Mutex<HashMap<u32, UserIdentifier>>,
    chats: Mutex<HashMap<u32, Chat>>,
    passwords: Mutex<HashMap<UserIdentifier, String>>,
    message_queue: Mutex<HashMap<UserIdentifier, HashMap<u32, Sendable>>>,
    sendable_queue: Mutex<HashMap<UserIdentifier, Vec<Sendable>>>,
    chat_join_ids: Mutex<HashMap<u32, u32>>,
    connect_device_senders: Mutex<HashMap<u32, Sender<String>>>,
    user_db: Mutex<HashMap<UserIdentifier, UserDB>>,
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
            connect_device_senders: Mutex::new(HashMap::new()),
            user_db: Mutex::new(HashMap::new()),
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
            let user_db = Mutex::new(
                user::username_map_into(serde_json::from_str(
                    fs::read_to_string("save/user_db.json")
                        .expect("Should have been able to read the file")
                        .as_str(),
                )
                .expect("couldn't parse user db")),
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
                connect_device_senders: Mutex::new(HashMap::new()),
                user_db,
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
        if server
            .message_queue
            .lock()
            .unwrap()
            .contains_key(uid.unwrap())
        {
            for message in server
                .message_queue
                .lock()
                .unwrap()
                .get(uid.unwrap())
                .unwrap()
                .values()
            {
                messages.push(message.clone());
            }
        }
        if server
            .sendable_queue
            .lock()
            .unwrap()
            .contains_key(uid.unwrap())
        {
            for message in server
                .sendable_queue
                .lock()
                .unwrap()
                .get(uid.unwrap())
                .unwrap()
            {
                messages.push(message.clone());
            }
        }
        server
            .message_queue
            .lock()
            .unwrap()
            .insert(uid.unwrap().clone(), HashMap::new());
        server
            .sendable_queue
            .lock()
            .unwrap()
            .insert(uid.unwrap().clone(), Vec::new());
    }
    return TextStream! {
        if invalid_token {
            yield "{\"server_reponse\":\"invalid token\"}|endmessage|".to_string();
        } else {
            for message in messages {
                yield format!("{}|endmessage|", message.to_string());
            }
            let mut interval = time::interval(Duration::from_secs(1));
            let mut seconds = 31;
            loop {
                seconds += 1;
                if seconds >= 30 {
                    yield format!("{{\"server\":\"ping\"}}|endmessage|");
                    seconds = 0;
                }
                let message_option = receiver.try_recv();
                if message_option.is_ok() {
                    yield format!("{}|endmessage|", message_option.unwrap().to_string());
                }
                interval.tick().await;
            }
        }
    };
}

#[get("/connect-device/<id>")]
fn connect_device_get(id: u32, server: &State<Arc<Mutex<Server>>>) -> TextStream![String + '_] {
    let (sender, receiver) = channel::<String>();
    server
        .lock()
        .unwrap()
        .connect_device_senders
        .lock()
        .unwrap()
        .insert(id, sender);
    return TextStream! {
        let mut interval = time::interval(Duration::from_secs(1));
        loop {
            let message_option = receiver.try_recv();
            if message_option.is_ok() {
                server.lock().unwrap().connect_device_senders.lock().unwrap().remove(&id);
                yield format!("{}", message_option.unwrap());
                break;
            }
            interval.tick().await;
        }
    };
}

#[post("/connect-device/<id>", data = "<data>")]
fn connect_device_post(id: u32, data: String, server_arc: &State<Arc<Mutex<Server>>>) {
    let server = server_arc.lock().unwrap();
    let senders = server.connect_device_senders.lock().unwrap();
    let sender = senders.get(&id);
    if sender.is_some() {
        match sender.unwrap().send(data) {
            Ok(()) => {}
            Err(e) => {
                println!("yo, the device connection died {e}")
            }
        }
    }
}

#[post("/create-account/<username>/<password>", data = "<created_user>")]
fn create_account(
    username: String,
    password: String,
    created_user: Json<CreateUser>,
    server_arc: &State<Arc<Mutex<Server>>>,
) -> (ContentType, String) {
    let server = server_arc.lock().unwrap();
    if server.users.lock().unwrap().contains_key(&UserIdentifier {
        username: username.clone(),
    }) {
        return (ContentType::JSON, "{\"server\":\"exists\"}".to_string());
        /*if server
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
        }*/
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
    server.user_db.lock().unwrap().insert(UserIdentifier {username: username.clone(),}, UserDB::new());
    let mut pfp = "undefined".to_string();

    //setting up profile
    if server
        .users
        .lock()
        .unwrap()
        .get(server.tokens.lock().unwrap().get(&token).unwrap())
        .is_some()
    {
        pfp = server
            .users
            .lock()
            .unwrap()
            .get(server.tokens.lock().unwrap().get(&token).unwrap())
            .unwrap()
            .pfp
            .clone();
    }
    let user_profile = created_user.to_user_profile(username, pfp);
    server.users.lock().unwrap().insert(
        server.tokens.lock().unwrap().get(&token).unwrap().clone(),
        user_profile,
    );
    return (ContentType::JSON, format!("{{\"token\":{}}}", token));
}

#[get("/login/<username>/<password>")]
fn login(
    username: String,
    password: String,
    server_arc: &State<Arc<Mutex<Server>>>,
) -> (ContentType, String) {
    let server = server_arc.lock().unwrap();
    if !server.users.lock().unwrap().contains_key(&UserIdentifier {
        username: username.clone(),
    }) {
        return (ContentType::JSON, "{\"server\":\"user does not exist\"}".to_string());
    }
    if *server.passwords.lock().unwrap().get(&UserIdentifier {username: username.clone(),}).unwrap() == password {
        let mut rng = rand::thread_rng();
        let mut token = rng.gen::<u32>();
        loop {
            if !server.tokens.lock().unwrap().contains_key(&token) {
                break;
            }
            token = rng.gen::<u32>();
        }
        server.tokens.lock().unwrap().insert(
            token,
            UserIdentifier {
                username: username.clone(),
            },
        );
        return (ContentType::JSON, format!("{{\"token\":{}}}", token));
    } else {
        return (ContentType::JSON, "{\"server\":\"incorrect password\"}".to_string());
    }
}

#[post("/edit-chat/<chatid>/<token>", data = "<chat_edit>")]
fn edit_chat(
    chatid: u32,
    token: u32,
    chat_edit: Json<ChatEdit>,
    server_arc: &State<Arc<Mutex<Server>>>,
) {
    let server = server_arc.lock().unwrap();
    if server.tokens.lock().unwrap().contains_key(&token) {
        if server.chats.lock().unwrap().contains_key(&chatid) {
            if *server.tokens.lock().unwrap().get(&token).unwrap()
                == server.chats.lock().unwrap().get(&chatid).unwrap().admin
            {
                let mut chats_option = server.chats.lock();
                let chats = chats_option.as_mut().unwrap();
                let chat = chats.get_mut(&chatid).unwrap();
                chat.admin = chat_edit.new_admin.clone();
                chat.name = chat_edit.new_name.clone();
                chat.users.append(&mut chat_edit.added_users.clone());
                for new_user in chat_edit.added_users.clone() {
                    let name = server
                        .users
                        .lock()
                        .unwrap()
                        .get(&new_user)
                        .unwrap_or(&UserProfile::dummy(new_user.username))
                        .name
                        .clone();
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
                server
                    .chat_join_ids
                    .lock()
                    .unwrap()
                    .insert(join_code, chatid);
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
fn join_chat_link(join_code: u32, token: u32, server_arc: &State<Arc<Mutex<Server>>>) {
    let server = server_arc.lock().unwrap();
    if server
        .chat_join_ids
        .lock()
        .unwrap()
        .contains_key(&join_code)
    {
        let chatid = server
            .chat_join_ids
            .lock()
            .unwrap()
            .get(&join_code)
            .unwrap()
            .clone();
        if server.chats.lock().unwrap().contains_key(&chatid) {
            if server.tokens.lock().unwrap().contains_key(&token) {
                if !server
                    .chats
                    .lock()
                    .unwrap()
                    .get(&chatid)
                    .unwrap()
                    .users
                    .contains(server.tokens.lock().unwrap().get(&token).unwrap())
                {
                    server
                        .chats
                        .lock()
                        .unwrap()
                        .get_mut(&chatid)
                        .unwrap()
                        .users
                        .push(server.tokens.lock().unwrap().get(&token).unwrap().clone());
                    let uid = server.tokens.lock().unwrap().get(&token).unwrap().clone();
                    let name = server
                        .users
                        .lock()
                        .unwrap()
                        .get(&uid)
                        .unwrap_or(&UserProfile::dummy(uid.username.clone()))
                        .name
                        .clone();
                    let sendable = banner(format!("{} joined this chat", name), chatid);
                    println!(
                        "user {} joining chat {}",
                        uid.username,
                        server.chats.lock().unwrap().get_mut(&chatid).unwrap().name
                    );
                    send_sendable(
                        sendable,
                        &server.chats.lock().unwrap().get_mut(&chatid).unwrap().users,
                        &server,
                    );
                }
            }
        }
    }
}

#[post("/received-message/<token>/<chatid>/<messageid>/<to_user>")]
fn received_message(
    token: u32,
    chatid: u32,
    messageid: u32,
    to_user: String,
    server_arc: &State<Arc<Mutex<Server>>>,
) {
    let server = server_arc.lock().unwrap();
    let tokens = server.tokens.lock().unwrap();
    let uid = tokens.get(&token);
    if uid.is_some() {
        server
            .message_queue
            .lock()
            .unwrap()
            .get_mut(uid.unwrap())
            .unwrap()
            .remove(&messageid);
        let sender_uid = UserIdentifier { username: to_user };
        let sendable = read(
            "Delivered".to_string(),
            uid.unwrap().username.clone(),
            messageid,
            chatid,
        );
        send_sendable(sendable, &[sender_uid.clone()].to_vec(), &server);
    }
}

#[derive(FromForm)]
struct PfpImage<'f> {
    extension: String,
    pfp_image: TempFile<'f>,
}

#[post(
    "/change-pfp/<token>",
    format = "multipart/form-data",
    data = "<pfp_form>"
)]
fn change_pfp(token: u32, mut pfp_form: Form<PfpImage>, server_arc: &State<Arc<Mutex<Server>>>) {
    let server = server_arc.lock().unwrap();
    let tokens = server.tokens.lock().unwrap();
    let uid = tokens.get(&token);
    if uid.is_some() {
        let username = uid.unwrap().username.clone();
        let new_path = format!(
            "F:\\chris\\rust\\messenger\\pfps\\{}.{}",
            username, pfp_form.extension
        );
        block_on(save_pfp(&mut pfp_form.pfp_image, new_path));
        let mut users = server.users.lock().unwrap();
        let mut user = users.get_mut(uid.unwrap());
        if user.is_some() {
            let url = format!(
                "https://minecraft.themagicdoor.org:8000/pfps/{}.{}",
                username, pfp_form.extension
            );
            user.as_mut().unwrap().pfp = url;
            println!(
                "set {}'s pfp to {}",
                uid.unwrap().username,
                user.as_mut().unwrap().pfp
            );
        }
    }
}

#[post("/delete-pfp/<token>")]
fn delete_pfp(token: u32, server_arc: &State<Arc<Mutex<Server>>>) {
    let server = server_arc.lock().unwrap();
    let tokens = server.tokens.lock().unwrap();
    let uid = tokens.get(&token);
    if uid.is_some() {
        let mut users = server.users.lock().unwrap();
        let user = users.get_mut(uid.unwrap());
        if user.is_some() {
            println!("deleting {}'s pfp", uid.unwrap().username);
            user.unwrap().pfp = "undefined".to_string();
        }
    }
}

async fn save_pfp<'f>(pfp: &mut TempFile<'f>, new_path: String) {
    println!("copying to {}", new_path);
    match pfp.move_copy_to(new_path).await {
        Ok(()) => {
            println!("saved pfp")
        }
        Err(e) => {
            println!("error saving image {e}")
        }
    };
}

#[get("/pfps/<pfp>")]
fn get_pfp(pfp: String) -> Option<File> {
    let filename = format!("F:\\chris\\rust\\messenger\\pfps\\{}", pfp);
    File::open(&filename).ok()
}

#[post("/read-message/<token>/<chatid>/<messageid>/<to_user>")]
fn read_message(
    token: u32,
    chatid: u32,
    messageid: u32,
    to_user: String,
    server_arc: &State<Arc<Mutex<Server>>>,
) {
    let server = server_arc.lock().unwrap();
    let tokens = server.tokens.lock().unwrap();
    let uid = tokens.get(&token);
    if uid.is_some() {
        let sender_uid = UserIdentifier { username: to_user };
        let sendable = read(
            "Read".to_string(),
            uid.unwrap().username.clone(),
            messageid,
            chatid,
        );
        send_sendable(sendable, &[sender_uid.clone()].to_vec(), &server);
    }
}

#[post("/logout/<token>")]
fn logout(token: u32, server_arc: &State<Arc<Mutex<Server>>>) {
    let server = server_arc.lock().unwrap();
    server.tokens.lock().unwrap().remove(&token);
}

#[post("/post-message/<token>", data = "<encrypted_messages>")]
fn post_message(
    token: u32,
    encrypted_messages: Json<EncryptedMessages>,
    server_arc: &State<Arc<Mutex<Server>>>,
) {
    if !server_arc
        .lock()
        .unwrap()
        .tokens
        .lock()
        .unwrap()
        .contains_key(&token)
    {
        return;
    }
    let tokens = server_arc.lock().unwrap().tokens.lock().unwrap().clone();
    let mut rng = rand::thread_rng();
    let message_id = rng.gen::<u32>();
    for (to_user, sent_message) in &encrypted_messages.encrypted_messages {
        let from_user = tokens.get(&sent_message.from_user).unwrap().clone();
        let message = sent_message.to_message(message_id, from_user);
        send_message(
            message,
            UserIdentifier {
                username: to_user.clone(),
            },
            server_arc.lock().unwrap(),
        );
    }
}

#[post("/react-message/<token>/<chatid>/<messageid>/<emoji>")]
fn react_message(
    token: u32,
    chatid: u32,
    messageid: u32,
    emoji: String,
    server_arc: &State<Arc<Mutex<Server>>>,
) -> String {
    let server = server_arc.lock().unwrap();
    if !server.tokens.lock().unwrap().contains_key(&token) {
        return "Invalid Token >:(".to_string();
    }
    let user = server.tokens.lock().unwrap().get(&token).unwrap().clone();
    if server.chats.lock().unwrap().contains_key(&chatid) {
        let chats = server.chats.lock().unwrap();
        let chat = chats.get(&chatid).unwrap();
        if chat.users.contains(&user) {
            let reaction = reaction(emoji, user.username, messageid, chatid);
            send_sendable(reaction, &chat.users, &server);
        }
    }
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
        serde_json::to_string(&user::uid_map_into(
            server.passwords.lock().unwrap().clone(),
        ))
        .expect("could not write to passwords file")
        .as_bytes(),
    )?;
    let mut chat_join_ids_file = create_or_open_file("save/chat_join_ids.json")?;
    chat_join_ids_file.write_all(
        serde_json::to_string(&server.chat_join_ids.lock().unwrap().clone())
            .expect("could not write to chat join ids file")
            .as_bytes(),
    )?;
    let mut user_db_file = create_or_open_file("save/user_db.json")?;
    user_db_file.write_all(
        serde_json::to_string(&user::uid_map_into(server.user_db.lock().unwrap().clone()))
            .expect("could not write to user_db file")
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
    let mut file = File::open("F:\\chris\\rust\\messenger\\messenger-client\\build\\index.html")
        .expect("index.html does not exist!!!");
    let mut file_text = String::new();
    file.read_to_string(&mut file_text)
        .expect("could not read from index.html!!!");
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
        .mount(
            "/",
            FileServer::from("C:\\Users\\chris\\rust\\messenger\\messenger-client\\build"),
        )
        .mount(
            "/",
            routes![
                join_headers,
                events,
                post_message,
                get_user,
                create_account,
                logout,
                create_chat,
                // create_user,
                token_valid,
                all_options,
                get_chat,
                received_message,
                edit_chat,
                read_message,
                create_chat_link,
                join_chat_link,
                change_pfp,
                get_pfp,
                delete_pfp,
                react_message,
                connect_device_get,
                connect_device_post,
                login,
                get_message,
                get_chat_messages,
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
            if hostname == "http://minecraft.themagicdoor.org:8000"
                || hostname == "http://minecraft.themagicdoor.org:3000"
                || hostname == "http://localhost:3000"
                || hostname.contains("http://localhost:")
            {
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