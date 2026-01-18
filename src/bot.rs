use matrix_sdk::{
    config::SyncSettings,
    encryption::EncryptionSettings,
    room::Room,
    ruma::{
        api::client::message::get_message_events,
        events::room::message::{
            MessageType, OriginalSyncRoomMessageEvent, RoomMessageEventContent,
        },
        UInt,
    },
    Client,
};
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tracing::{error, info, warn};

pub type MessageSender = broadcast::Sender<String>;
pub type MessageReceiver = broadcast::Receiver<String>;

#[derive(Clone)]
pub struct MatrixBot {
    client: Client,
    room_id: String,
    message_tx: MessageSender,
    message_history: Arc<RwLock<Vec<String>>>,
}

impl MatrixBot {
    pub async fn new(
        homeserver: &str,
        username: &str,
        password: &str,
        room_id: &str,
        history_limit: usize,
        store_path: &str,
        store_passphrase: &str,
    ) -> anyhow::Result<(Self, MessageReceiver)> {
        info!("Initializing Matrix client with store at: {}", store_path);
        
        // Configure encryption settings
        let encryption_settings = EncryptionSettings {
            auto_enable_cross_signing: true,
            auto_enable_backups: true,
            ..Default::default()
        };
        
        let client = Client::builder()
            .homeserver_url(homeserver)
            .sqlite_store(store_path, Some(store_passphrase))
            .with_encryption_settings(encryption_settings)
            .build()
            .await?;

        info!("Logging in as {}", username);
        client
            .matrix_auth()
            .login_username(username, password)
            .initial_device_display_name("Matrix Web Bot")
            .await?;

        info!("Login successful");
        
        // Set up encryption and cross-signing
        if let Err(e) = Self::setup_encryption(&client).await {
            warn!("Failed to setup encryption: {}. You may need to verify this device via another session.", e);
        }

        // Create broadcast channel for messages
        let (message_tx, message_rx) = broadcast::channel(100);

        let bot = MatrixBot {
            client,
            room_id: room_id.to_string(),
            message_tx,
            message_history: Arc::new(RwLock::new(Vec::with_capacity(history_limit))),
        };

        Ok((bot, message_rx))
    }
    
    async fn setup_encryption(client: &Client) -> anyhow::Result<()> {
        let encryption = client.encryption();
        
        // Check if cross-signing is already set up
        if let Some(cross_signing_status) = encryption.cross_signing_status().await {
            if !cross_signing_status.is_complete() {
                info!("Cross-signing is not completely set up. Attempting to bootstrap...");
                
                // Bootstrap cross-signing if needed
                if let Err(e) = encryption.bootstrap_cross_signing(None).await {
                    warn!("Failed to bootstrap cross-signing: {}. This device may need to be verified via another Element session.", e);
                    info!("To verify this device:");
                    info!("1. Open Element on another device where you're logged in");
                    info!("2. Go to Settings → Security & Privacy");
                    info!("3. Verify this new device session");
                } else {
                    info!("Cross-signing bootstrapped successfully");
                }
            } else {
                info!("Cross-signing is already complete");
            }
        } else {
            info!("Cross-signing is not available. Setting up cross-signing...");
            
            // Bootstrap cross-signing
            if let Err(e) = encryption.bootstrap_cross_signing(None).await {
                warn!("Failed to bootstrap cross-signing: {}. This device may need to be verified via another Element session.", e);
                info!("To verify this device:");
                info!("1. Open Element on another device where you're logged in");
                info!("2. Go to Settings → Security & Privacy");
                info!("3. Verify this new device session");
            } else {
                info!("Cross-signing bootstrapped successfully");
            }
        }
        
        // Note: Backups are automatically managed by the SDK when cross-signing is set up
        // The SDK will automatically restore and enable backups when the device is verified
        info!("Encryption setup complete. Key backups will be enabled automatically after device verification.");
        info!("To complete setup:");
        info!("1. Open Element on another device where you're logged in");
        info!("2. Verify this new session to enable key backups");
        
        Ok(())
    }

    pub async fn join_room(&self) -> anyhow::Result<()> {
        let room_id = <&matrix_sdk::ruma::RoomId>::try_from(self.room_id.as_str())?;
        
        // Try to join the room
        self.client.join_room_by_id(room_id).await?;
        info!("Joined room: {}", self.room_id);
        
        Ok(())
    }

    pub async fn load_message_history(&self, limit: usize) -> anyhow::Result<()> {
        let room_id = <&matrix_sdk::ruma::RoomId>::try_from(self.room_id.as_str())?;
        
        if self.client.get_room(room_id).is_some() {
            info!("Loading message history (limit: {})", limit);
            
            // Get room messages
            let mut request = get_message_events::v3::Request::backward(room_id.to_owned());
            request.limit = UInt::new(limit as u64).unwrap_or(UInt::new(50).unwrap());
            
            match self.client.send(request, None).await {
                Ok(response) => {
                    let mut history = Vec::new();
                    
                    // Process messages in reverse order (oldest first)
                    for event_raw in response.chunk.iter().rev() {
                        if let Ok(matrix_sdk::ruma::events::AnyTimelineEvent::MessageLike(
                            matrix_sdk::ruma::events::AnyMessageLikeEvent::RoomMessage(
                                matrix_sdk::ruma::events::room::message::RoomMessageEvent::Original(msg),
                            ),
                        )) = event_raw.deserialize()
                        {
                            let sender = msg.sender.to_string();
                            if let MessageType::Text(text) = msg.content.msgtype {
                                let formatted_message = format!("{}: {}", sender, text.body);
                                history.push(formatted_message);
                            }
                        }
                    }
                    
                    info!("Loaded {} messages from history", history.len());
                    let mut msg_history = self.message_history.write().await;
                    *msg_history = history;
                }
                Err(e) => {
                    error!("Failed to load message history: {}", e);
                }
            }
        }
        
        Ok(())
    }

    pub async fn get_message_history(&self) -> Vec<String> {
        let history = self.message_history.read().await;
        history.clone()
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
                    if room.room_id().as_str() != bot.room_id {
                        return;
                    }

                    let sender = event.sender.to_string();
                    let message = match event.content.msgtype {
                        MessageType::Text(text) => text.body.clone(),
                        _ => return,
                    };

                    let formatted_message = format!("{}: {}", sender, message);
                    info!("Received message: {}", formatted_message);
                    
                    // Add to history
                    let mut history = bot.message_history.write().await;
                    history.push(formatted_message.clone());
                    
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
