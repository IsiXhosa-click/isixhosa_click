use crate::typesense::TypesenseClient;
use futures::stream::SplitSink;
use futures::SinkExt;
use std::time::{Duration, Instant};
use warp::ws::{self, WebSocket};
use xtra::prelude::*;

pub struct LiveSearchSession {
    pub sender: SplitSink<WebSocket, ws::Message>,
    pub typesense: TypesenseClient,
    heartbeat: Instant,
}

impl LiveSearchSession {
    pub fn new(sender: SplitSink<WebSocket, ws::Message>, typesense: TypesenseClient) -> Self {
        LiveSearchSession {
            sender,
            typesense,
            heartbeat: Instant::now(),
        }
    }
}

#[async_trait::async_trait]
impl Actor for LiveSearchSession {
    async fn started(&mut self, ctx: &mut Context<Self>) {
        tokio::spawn(
            ctx.notify_interval(Duration::from_secs(15), || Heartbeat)
                .unwrap(),
        );
    }

    async fn stopped(&mut self) {
        let _ = self.sender.close().await;
    }
}

pub struct Heartbeat;

impl Message for Heartbeat {
    type Result = ();
}

#[async_trait::async_trait]
impl Handler<Heartbeat> for LiveSearchSession {
    async fn handle(&mut self, _hb: Heartbeat, ctx: &mut Context<Self>) {
        if self.heartbeat.elapsed() > Duration::from_secs(30) {
            ctx.stop();
        }
    }
}

pub struct WsMessage(pub Result<ws::Message, warp::Error>);

impl Message for WsMessage {
    type Result = ();
}

#[async_trait::async_trait]
impl Handler<WsMessage> for LiveSearchSession {
    async fn handle(&mut self, message: WsMessage, ctx: &mut Context<Self>) {
        let msg = match message.0 {
            Ok(msg) => msg,
            Err(_) => return ctx.stop(),
        };

        self.heartbeat = Instant::now();

        if msg.is_text() {
            let query = msg.to_str().unwrap();

            if query.is_empty() {
                return;
            }

            let results = self.typesense.search_word_short(query).await.unwrap();
            let json = serde_json::to_string(&results).unwrap();

            if self.sender.send(ws::Message::text(json)).await.is_err() {
                ctx.stop();
            }
        }
    }
}
