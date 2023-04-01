use std::{sync::{Arc, Mutex, mpsc::channel}, convert::Infallible, time::Duration, net::SocketAddr};

use futures::SinkExt;
use rocket::tokio::time;
use warp::{Filter, Reply, ws::{Ws, WebSocket, Message}, Rejection};

use crate::{Server, sendables::Sendable};

pub async fn warp_start(server_arc: Arc<Mutex<Server>>) {
    let routes = warp::path!("events" / u32)
        // The `ws()` filter will prepare the Websocket handshake.
        .and(warp::ws())
        .and(with_server(server_arc.clone()))
        .and_then(|token: u32, ws, server: Arc<Mutex<Server>>| async move {
            return websocket(ws, token, server).await;
        });

    let addr: SocketAddr = "0.0.0.0:8008".parse().unwrap();

    println!("starting warp server");
    warp::serve(routes).tls()
    .cert_path("C:/Certbot/live/minecraft.themagicdoor.org/fullchain.pem")
    .key_path("C:/Certbot/live/minecraft.themagicdoor.org/privkey.pem")
    .run(addr).await;
}

fn with_server(server: Arc<Mutex<Server>>) -> impl Filter<Extract = (Arc<Mutex<Server>>,), Error = Infallible> + Clone {
    warp::any().map(move || server.clone())
}

pub async fn websocket(wb: Ws, token: u32, server_arc: Arc<Mutex<Server>>) -> Result<impl Reply, Rejection> {
    println!("STARTING WEBSOCKET!");
    let server = server_arc.lock().unwrap();
    let (sender, receiver) = channel::<Sendable>();
    let tokens = server.tokens.lock().unwrap().clone();
    let uid = tokens.get(&token);
    let mut invalid_token = false;
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
    }
    return Ok(wb.on_upgrade(move |mut websocket: WebSocket| async move {
        if invalid_token {
            match websocket.start_send_unpin(Message::text("{\"server_reponse\":\"invalid token\"}".to_string())) {
                Ok(_) => {},
                Err(e) => {println!("failed to send message{e}");}
            };
        } else {
            let mut interval = time::interval(Duration::from_secs(1));
            let mut seconds = 31;
            loop {
                seconds += 1;
                if seconds >= 30 {
                    match websocket.start_send_unpin(Message::text(format!("{{\"server\":\"ping\"}}"))) {
                        Ok(_) => {},
                        Err(e) => {println!("failed to send message{e}"); break;}
                    };
                    seconds = 0;
                }
                let message_option = receiver.try_recv();
                if message_option.is_ok() {
                    match websocket.start_send_unpin(Message::text(format!("{}", message_option.unwrap().to_string()))) {
                        Ok(_) => {},
                        Err(e) => {println!("failed to send message{e}"); break;}
                    };
                }
                interval.tick().await;
            }
        }
    }));
}