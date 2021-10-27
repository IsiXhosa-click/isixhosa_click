use crate::search::{IncludeResults, TantivyClient, WordHit};
use futures::stream::SplitSink;
use futures::SinkExt;
use serde::{Deserialize, Serialize};
use std::num::NonZeroU64;
use std::sync::Arc;
use std::time::{Duration, Instant};
use warp::ws::{self, WebSocket};
use xtra::prelude::*;

pub struct LiveSearchSession {
    pub sender: SplitSink<WebSocket, ws::Message>,
    pub tantivy: Arc<TantivyClient>,
    include: IncludeResults,
    heartbeat: Instant,
}

impl LiveSearchSession {
    pub fn new(
        sender: SplitSink<WebSocket, ws::Message>,
        tantivy: Arc<TantivyClient>,
        include_suggestions_from_user: Option<NonZeroU64>,
        is_moderator: bool,
    ) -> Self {
        let include = match (include_suggestions_from_user, is_moderator) {
            (Some(_), true) => IncludeResults::AcceptedAndAllSuggestions,
            (Some(user), false) => IncludeResults::AcceptedAndSuggestionsFrom(user),
            (None, _) => IncludeResults::AcceptedOnly,
        };

        LiveSearchSession {
            sender,
            tantivy,
            include,
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
            #[derive(Deserialize)]
            struct Query {
                search: String,
                state: String,
            }

            if msg.to_str().unwrap().is_empty() {
                return;
            }

            let json = match serde_json::from_str::<Query>(msg.to_str().unwrap()) {
                Ok(query) => {
                    if query.search.is_empty() {
                        return;
                    }

                    #[derive(Serialize)]
                    struct Reply {
                        results: Vec<WordHit>,
                        state: String,
                    }

                    let reply = Reply {
                        results: self
                            .tantivy
                            .search(query.search, self.include, false)
                            .await
                            .unwrap(),
                        state: query.state,
                    };

                    serde_json::to_string(&reply).unwrap()
                }
                _ => {
                    let query = msg.to_str().unwrap();

                    if query.is_empty() {
                        return;
                    }

                    let results = self
                        .tantivy
                        .search(query.to_owned(), IncludeResults::AcceptedOnly, false)
                        .await
                        .unwrap();
                    serde_json::to_string(&results).unwrap()
                }
            };

            if self.sender.send(ws::Message::text(json)).await.is_err() {
                ctx.stop();
            }
        }
    }
}
