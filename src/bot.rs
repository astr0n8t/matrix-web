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
use tokio::sync::{broadcast, RwLock, Mutex};
use tracing::{error, info, warn};

pub type MessageSender = broadcast::Sender<String>;
pub type MessageReceiver = broadcast::Receiver<String>;

#[derive(Clone)]
pub struct MatrixBot {
    homeserver: String,
    username: String,
    matrix_password: String,
    room_id: String,
    store_path: String,
    history_limit: usize,
    client: Arc<Mutex<Option<Client>>>,
    message_tx: MessageSender,
    message_history: Arc<RwLock<Vec<String>>>,
    sync_handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
}

impl MatrixBot {
    pub fn new(
        homeserver: &str,
        username: &str,
        password: &str,
        room_id: &str,
        history_limit: usize,
        store_path: &str,
    ) -> (Self, MessageReceiver) {
        info!("Creating Matrix bot instance (not connected yet)");
        
        // Create broadcast channel for messages
        let (message_tx, message_rx) = broadcast::channel(100);

        let bot = MatrixBot {
            homeserver: homeserver.to_string(),
            username: username.to_string(),
            matrix_password: password.to_string(),
            room_id: room_id.to_string(),
            store_path: store_path.to_string(),
            history_limit,
            client: Arc::new(Mutex::new(None)),
            message_tx,
            message_history: Arc::new(RwLock::new(Vec::with_capacity(history_limit))),
            sync_handle: Arc::new(Mutex::new(None)),
        };

        (bot, message_rx)
    }
    
    pub async fn is_connected(&self) -> bool {
        self.client.lock().await.is_some()
    }
    
    pub async fn connect(&self, store_passphrase: &str) -> anyhow::Result<()> {
        // Check if already connected
        if self.is_connected().await {
            return Ok(());
        }
        
        info!("Connecting to Matrix with store passphrase...");
        
        // Configure encryption settings
        let encryption_settings = EncryptionSettings {
            auto_enable_cross_signing: true,
            auto_enable_backups: true,
            ..Default::default()
        };
        
        // Use None for empty passphrase, Some for non-empty
        let store_passphrase_opt = if store_passphrase.is_empty() {
            None
        } else {
            Some(store_passphrase)
        };
        
        let client = Client::builder()
            .homeserver_url(&self.homeserver)
            .sqlite_store(&self.store_path, store_passphrase_opt)
            .with_encryption_settings(encryption_settings)
            .build()
            .await?;

        info!("Logging in as {}", self.username);
        client
            .matrix_auth()
            .login_username(&self.username, &self.matrix_password)
            .initial_device_display_name("Matrix Web Bot")
            .await?;

        info!("Login successful");
        
        // Set up encryption and cross-signing
        if let Err(e) = Self::setup_encryption(&client).await {
            warn!("Failed to setup encryption: {}. You may need to verify this device via another session.", e);
        }
        
        // Join room
        let room_id = <&matrix_sdk::ruma::RoomId>::try_from(self.room_id.as_str())?;
        client.join_room_by_id(room_id).await?;
        info!("Joined room: {}", self.room_id);
        
        // Load message history
        self.load_message_history_with_client(&client, self.history_limit).await?;
        
        // Start sync in background
        self.start_sync_with_client(client.clone()).await;
        
        // Store client
        *self.client.lock().await = Some(client);
        
        info!("Bot connected and syncing");
        Ok(())
    }
    
    pub async fn disconnect(&self) -> anyhow::Result<()> {
        info!("Disconnecting from Matrix...");
        
        // Stop sync task
        if let Some(handle) = self.sync_handle.lock().await.take() {
            handle.abort();
        }
        
        // Logout
        if let Some(client) = self.client.lock().await.take() {
            if let Err(e) = client.matrix_auth().logout().await {
                warn!("Error during logout: {}", e);
            }
        }
        
        // Clear message history
        self.message_history.write().await.clear();
        
        info!("Bot disconnected");
        Ok(())
    }
    
    async fn setup_encryption(client: &Client) -> anyhow::Result<()> {
        let encryption = client.encryption();
        
        // Check if cross-signing is already set up
        if let Some(cross_signing_status) = encryption.cross_signing_status().await {
            if !cross_signing_status.is_complete() {
                info!("Cross-signing is not completely set up. Attempting to bootstrap...");
                Self::try_bootstrap_cross_signing(&encryption).await;
            } else {
                info!("Cross-signing is already complete");
            }
        } else {
            info!("Cross-signing is not available. Setting up cross-signing...");
            Self::try_bootstrap_cross_signing(&encryption).await;
        }
        
        // Note: Backups are automatically managed by the SDK when cross-signing is set up
        // The SDK will automatically restore and enable backups when the device is verified
        info!("Encryption setup complete. Key backups will be enabled automatically after device verification.");
        info!("To complete setup:");
        info!("1. Open Element on another device where you're logged in");
        info!("2. Verify this new session to enable key backups");
        
        Ok(())
    }
    
    async fn try_bootstrap_cross_signing(encryption: &matrix_sdk::encryption::Encryption) {
        if let Err(e) = encryption.bootstrap_cross_signing(None).await {
            warn!("Failed to bootstrap cross-signing: {}. This device may need to be verified via another Element session.", e);
            Self::log_verification_instructions();
        } else {
            info!("Cross-signing bootstrapped successfully");
        }
    }
    
    fn log_verification_instructions() {
        info!("To verify this device:");
        info!("1. Open Element on another device where you're logged in");
        info!("2. Go to Settings → Security & Privacy");
        info!("3. Verify this new device session");
    }

    async fn load_message_history_with_client(&self, client: &Client, limit: usize) -> anyhow::Result<()> {
        let room_id = <&matrix_sdk::ruma::RoomId>::try_from(self.room_id.as_str())?;
        
        if client.get_room(room_id).is_some() {
            info!("Loading message history (limit: {})", limit);
            
            // Get room messages
            let mut request = get_message_events::v3::Request::backward(room_id.to_owned());
            request.limit = UInt::new(limit as u64).unwrap_or(UInt::new(50).unwrap());
            
            match client.send(request, None).await {
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

    async fn start_sync_with_client(&self, client: Client) {
        let bot_for_sync = self.clone();
        let room_id = self.room_id.clone();
        
        let handle = tokio::spawn(async move {
            // Register event handler for incoming messages
            client.add_event_handler(
                move |event: OriginalSyncRoomMessageEvent, room: Room| {
                    let bot = bot_for_sync.clone();
                    let room_id_clone = room_id.clone();
                    async move {
                        if room.room_id().as_str() != room_id_clone {
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
            if let Err(e) = client.sync(SyncSettings::default()).await {
                error!("Sync error: {}", e);
            }
        });
        
        *self.sync_handle.lock().await = Some(handle);
    }

    pub async fn join_room(&self) -> anyhow::Result<()> {
        let client_guard = self.client.lock().await;
        let client = client_guard.as_ref().ok_or_else(|| anyhow::anyhow!("Not connected"))?;
        
        let room_id = <&matrix_sdk::ruma::RoomId>::try_from(self.room_id.as_str())?;
        
        // Try to join the room
        client.join_room_by_id(room_id).await?;
        info!("Joined room: {}", self.room_id);
        
        Ok(())
    }

    pub async fn load_message_history(&self, limit: usize) -> anyhow::Result<()> {
        let client_guard = self.client.lock().await;
        let client = client_guard.as_ref().ok_or_else(|| anyhow::anyhow!("Not connected"))?;
        self.load_message_history_with_client(client, limit).await
    }

    pub async fn get_message_history(&self) -> Vec<String> {
        let history = self.message_history.read().await;
        history.clone()
    }

    pub async fn send_message(&self, message: &str) -> anyhow::Result<()> {
        let client_guard = self.client.lock().await;
        let client = client_guard.as_ref().ok_or_else(|| anyhow::anyhow!("Not connected"))?;
        
        let room_id = <&matrix_sdk::ruma::RoomId>::try_from(self.room_id.as_str())?;
        
        if let Some(room) = client.get_room(room_id) {
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
        // This method is no longer used in the new architecture
        // Sync is started automatically by connect()
        Ok(())
    }

    pub fn subscribe(&self) -> MessageReceiver {
        self.message_tx.subscribe()
    }
}
