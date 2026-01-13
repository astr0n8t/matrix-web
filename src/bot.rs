use matrix_sdk::{
    config::SyncSettings,
    room::Room,
    ruma::events::room::message::{
        MessageType, OriginalSyncRoomMessageEvent, RoomMessageEventContent,
    },
    Client,
};
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{error, info};

pub type MessageSender = broadcast::Sender<String>;
pub type MessageReceiver = broadcast::Receiver<String>;

#[derive(Clone)]
pub struct MatrixBot {
    client: Client,
    room_id: String,
    message_tx: MessageSender,
}

impl MatrixBot {
    pub async fn new(
        homeserver: &str,
        username: &str,
        password: &str,
        room_id: &str,
    ) -> anyhow::Result<(Self, MessageReceiver)> {
        let client = Client::builder()
            .homeserver_url(homeserver)
            .build()
            .await?;

        info!("Logging in as {}", username);
        client
            .matrix_auth()
            .login_username(username, password)
            .initial_device_display_name("Matrix Web Bot")
            .await?;

        info!("Login successful");

        // Create broadcast channel for messages
        let (message_tx, message_rx) = broadcast::channel(100);

        let bot = MatrixBot {
            client,
            room_id: room_id.to_string(),
            message_tx,
        };

        Ok((bot, message_rx))
    }

    pub async fn join_room(&self) -> anyhow::Result<()> {
        let room_id = <&matrix_sdk::ruma::RoomId>::try_from(self.room_id.as_str())?;
        
        // Try to join the room
        self.client.join_room_by_id(room_id).await?;
        info!("Joined room: {}", self.room_id);
        
        Ok(())
    }

    pub async fn send_message(&self, message: &str) -> anyhow::Result<()> {
        let room_id = <&matrix_sdk::ruma::RoomId>::try_from(self.room_id.as_str())?;
        
        if let Some(room) = self.client.get_room(room_id) {
            let content = RoomMessageEventContent::text_plain(message);
            room.send(content).await?;
            info!("Sent message to room");
        } else {
            error!("Room not found");
            anyhow::bail!("Room not found");
        }
        
        Ok(())
    }

    pub async fn start_sync(self) -> anyhow::Result<()> {
        let bot = Arc::new(self);
        let bot_clone = bot.clone();

        // Register event handler for incoming messages
        bot.client.add_event_handler(
            move |event: OriginalSyncRoomMessageEvent, room: Room| {
                let bot = bot_clone.clone();
                async move {
                    if room.room_id().to_string() != bot.room_id {
                        return;
                    }

                    let sender = event.sender.to_string();
                    let message = match event.content.msgtype {
                        MessageType::Text(text) => text.body.clone(),
                        _ => return,
                    };

                    let formatted_message = format!("{}: {}", sender, message);
                    info!("Received message: {}", formatted_message);
                    
                    // Broadcast to web clients
                    let _ = bot.message_tx.send(formatted_message);
                }
            },
        );

        info!("Starting sync loop");
        bot.client.sync(SyncSettings::default()).await?;

        Ok(())
    }

    pub fn subscribe(&self) -> MessageReceiver {
        self.message_tx.subscribe()
    }
}
