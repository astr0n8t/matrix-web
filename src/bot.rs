use matrix_sdk::{
    config::SyncSettings,
    encryption::{
        verification::{Verification},
        EncryptionSettings,
    },
    matrix_auth::{MatrixSession, MatrixSessionTokens},
    room::Room,
    ruma::{
        api::client::message::get_message_events,
        events::room::message::{
            MessageType, OriginalSyncRoomMessageEvent, RoomMessageEventContent,
        },
        UInt, UserId,
    },
    Client, SessionMeta,
};
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock, Mutex};
use tracing::{error, info, warn};
use serde::{Deserialize, Serialize};
use anyhow::Context;
use crate::credentials::CredentialStore;

pub type MessageSender = broadcast::Sender<String>;
pub type MessageReceiver = broadcast::Receiver<String>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationRequestInfo {
    pub request_id: String,
    pub other_user_id: String,
    pub other_device_id: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SasInfo {
    pub request_id: String,
    pub emoji: Option<Vec<(String, String)>>,
    pub decimals: Option<(u16, u16, u16)>,
}

#[derive(Clone)]
pub struct MatrixBot {
    homeserver: String,
    username: String,
    room_id: String,
    store_path: String,
    history_limit: usize,
    client: Arc<Mutex<Option<Client>>>,
    message_tx: MessageSender,
    message_history: Arc<RwLock<Vec<String>>>,
    sync_handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    verification_requests: Arc<RwLock<Vec<VerificationRequestInfo>>>,
    active_sas: Arc<RwLock<Option<SasInfo>>>,
}

impl MatrixBot {
    pub fn new(
        homeserver: &str,
        username: &str,
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
            room_id: room_id.to_string(),
            store_path: store_path.to_string(),
            history_limit,
            client: Arc::new(Mutex::new(None)),
            message_tx,
            message_history: Arc::new(RwLock::new(Vec::with_capacity(history_limit))),
            sync_handle: Arc::new(Mutex::new(None)),
            verification_requests: Arc::new(RwLock::new(Vec::new())),
            active_sas: Arc::new(RwLock::new(None)),
        };

        (bot, message_rx)
    }
    
    pub async fn is_connected(&self) -> bool {
        self.client.lock().await.is_some()
    }
    
    pub async fn connect(&self, matrix_password: &str, store_passphrase: &str, credentials_store: &CredentialStore) -> anyhow::Result<()> {
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

        // Check if we have an existing session to restore
        let session_exists = match credentials_store.session_exists() {
            Ok(exists) => exists,
            Err(e) => {
                warn!("Failed to check if session exists: {}. Assuming no session.", e);
                false
            }
        };
        
        if session_exists {
            info!("Found existing session, attempting to restore...");
            match credentials_store.get_session(store_passphrase) {
                Ok((device_id, access_token, user_id)) => {
                    info!("Restoring session with device_id: {}", device_id);
                    
                    let user_id = user_id.as_str().try_into()
                        .with_context(|| format!("Invalid user ID format: {}", user_id))?;
                    
                    let session = MatrixSession {
                        meta: SessionMeta {
                            user_id,
                            device_id: device_id.as_str().into(),
                        },
                        tokens: MatrixSessionTokens {
                            access_token,
                            refresh_token: None,
                        },
                    };
                    
                    client.matrix_auth().restore_session(session).await?;
                    info!("Session restored successfully");
                }
                Err(e) => {
                    warn!("Failed to restore session: {}. Falling back to login.", e);
                    // Fall back to login if session restore fails
                    self.login_and_store_session(&client, matrix_password, store_passphrase, credentials_store).await?;
                }
            }
        } else {
            info!("No existing session found, logging in as {}", self.username);
            self.login_and_store_session(&client, matrix_password, store_passphrase, credentials_store).await?;
        }

        info!("Login successful");
        
        // Set up verification handlers
        self.setup_verification_handlers(client.clone()).await;
        
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
    
    /// Helper method to perform login and store session
    async fn login_and_store_session(
        &self,
        client: &Client,
        matrix_password: &str,
        store_passphrase: &str,
        credentials_store: &CredentialStore,
    ) -> anyhow::Result<()> {
        client
            .matrix_auth()
            .login_username(&self.username, matrix_password)
            .initial_device_display_name("Matrix Web Bot")
            .await?;
        
        // Save the session after successful login
        if let Some(session) = client.session() {
            if let Err(e) = credentials_store.store_session(
                session.meta().device_id.as_str(),
                session.access_token(),
                session.meta().user_id.as_str(),
                store_passphrase,
            ) {
                warn!("Failed to store session: {}", e);
            }
        }
        
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

    pub fn subscribe(&self) -> MessageReceiver {
        self.message_tx.subscribe()
    }

    // Verification methods
    pub async fn get_verification_requests(&self) -> Vec<VerificationRequestInfo> {
        self.verification_requests.read().await.clone()
    }

    pub async fn get_active_sas(&self) -> Option<SasInfo> {
        self.active_sas.read().await.clone()
    }

    pub async fn accept_verification(&self, request_id: &str, other_user_id: &str) -> anyhow::Result<()> {
        let client_guard = self.client.lock().await;
        let client = client_guard.as_ref().ok_or_else(|| anyhow::anyhow!("Not connected"))?;
        
        let user_id = <&UserId>::try_from(other_user_id)?;
        
        // Try to get the verification request with retries (in case SDK is still processing)
        for attempt in 0..5 {
            if attempt > 0 {
                info!("Retry attempt {} to get verification request", attempt);
                tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
            }
            
            // Get the verification request
            if let Some(request) = client.encryption().get_verification_request(user_id, request_id).await {
                info!("Accepting verification request: {}", request_id);
                request.accept().await?;
                
                // After accepting the request, wait for it to transition to SAS verification
                // and accept the SAS verification to start the emoji/decimal generation
                info!("Waiting for verification request to transition to SAS...");
                for sas_attempt in 0..10 {
                    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                    
                    if let Some(verification) = client.encryption().get_verification(user_id, request_id).await {
                        if let Verification::SasV1(sas) = verification {
                            info!("Verification transitioned to SAS, accepting it");
                            match sas.accept().await {
                                Ok(_) => {
                                    info!("Successfully accepted SAS verification, emojis should be available soon");
                                    return Ok(());
                                }
                                Err(e) => {
                                    let err_str = e.to_string();
                                    if err_str.contains("already") || err_str.contains("accepted") {
                                        info!("SAS verification was already accepted");
                                        return Ok(());
                                    } else {
                                        warn!("Failed to accept SAS verification: {}", e);
                                        // Continue retrying
                                    }
                                }
                            }
                        }
                    }
                    
                    if sas_attempt < 9 {
                        info!("SAS not ready yet, waiting... (attempt {}/10)", sas_attempt + 1);
                    }
                }
                
                warn!("Verification request accepted but SAS did not become available in time");
                return Ok(());
            }
            
            // The verification request might have already transitioned to a verification flow
            // Check if we can find it as a verification instead
            if let Some(verification) = client.encryption().get_verification(user_id, request_id).await {
                info!("Verification request already transitioned to verification flow");
                if let Verification::SasV1(sas) = verification {
                    // If it's already in SAS mode and can be presented, it still needs to be accepted
                    if sas.can_be_presented() {
                        info!("SAS verification is ready for presentation, accepting it");
                        match sas.accept().await {
                            Ok(_) => {
                                info!("Successfully accepted SAS verification");
                                return Ok(());
                            }
                            Err(e) => {
                                // Check if the error is benign (already accepted)
                                let err_str = e.to_string();
                                if err_str.contains("already") || err_str.contains("accepted") {
                                    info!("SAS verification was already accepted");
                                    return Ok(());
                                } else {
                                    warn!("Failed to accept SAS verification: {}", e);
                                    return Err(e.into());
                                }
                            }
                        }
                    }
                    // If it's in another state, try to accept it anyway
                    info!("Attempting to accept SAS verification");
                    match sas.accept().await {
                        Ok(_) => {
                            info!("Successfully accepted SAS verification");
                            return Ok(());
                        }
                        Err(e) => {
                            let err_str = e.to_string();
                            if err_str.contains("already") || err_str.contains("accepted") || err_str.contains("state") {
                                info!("SAS verification might already be accepted or in different state: {}", e);
                                return Ok(());
                            } else {
                                warn!("Failed to accept SAS verification: {}", e);
                                return Err(e.into());
                            }
                        }
                    }
                } else {
                    warn!("Verification is not SasV1 type, other verification types not currently supported");
                    return Err(anyhow::anyhow!("Unsupported verification type"));
                }
            }
        }
        
        Err(anyhow::anyhow!("Verification request not found after retries"))
    }

    pub async fn confirm_verification(&self, request_id: &str, other_user_id: &str) -> anyhow::Result<()> {
        let client_guard = self.client.lock().await;
        let client = client_guard.as_ref().ok_or_else(|| anyhow::anyhow!("Not connected"))?;
        
        let user_id = <&UserId>::try_from(other_user_id)?;
        
        // Try to get the verification with retries (in case SDK is still processing)
        for attempt in 0..5 {
            if attempt > 0 {
                info!("Retry attempt {} to get SAS verification", attempt);
                tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
            }
            
            // Get the verification
            if let Some(Verification::SasV1(sas)) = client.encryption().get_verification(user_id, request_id).await {
                info!("Confirming SAS verification");
                sas.confirm().await?;
                
                // Wait a moment for verification to complete
                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                
                if sas.is_done() {
                    info!("Verification completed successfully!");
                    *self.active_sas.write().await = None;
                }
                return Ok(());
            }
        }
        
        Err(anyhow::anyhow!("SAS verification not found after retries"))
    }

    pub async fn cancel_verification(&self, request_id: &str, other_user_id: &str) -> anyhow::Result<()> {
        let client_guard = self.client.lock().await;
        let client = client_guard.as_ref().ok_or_else(|| anyhow::anyhow!("Not connected"))?;
        
        let user_id = <&UserId>::try_from(other_user_id)?;
        
        // Try to cancel the verification request
        if let Some(request) = client.encryption().get_verification_request(user_id, request_id).await {
            info!("Cancelling verification request: {}", request_id);
            request.cancel().await?;
            
            // Remove from our tracking
            self.verification_requests.write().await.retain(|r| r.request_id != request_id);
            
            // Clear active SAS if it matches
            let mut active_sas = self.active_sas.write().await;
            if let Some(ref sas) = *active_sas {
                if sas.request_id == request_id {
                    *active_sas = None;
                }
            }
            
            return Ok(());
        }
        
        // The request might have already transitioned to a verification
        if let Some(verification) = client.encryption().get_verification(user_id, request_id).await {
            info!("Verification has transitioned, cancelling the verification instead");
            if let Verification::SasV1(sas) = verification {
                sas.cancel().await?;
            }
            
            // Remove from our tracking
            self.verification_requests.write().await.retain(|r| r.request_id != request_id);
            
            // Clear active SAS if it matches
            let mut active_sas = self.active_sas.write().await;
            if let Some(ref sas) = *active_sas {
                if sas.request_id == request_id {
                    *active_sas = None;
                }
            }
            
            return Ok(());
        }
        
        // If we can't find it, it might have already been cancelled or completed
        // Remove from our tracking anyway
        self.verification_requests.write().await.retain(|r| r.request_id != request_id);
        *self.active_sas.write().await = None;
        
        info!("Verification request not found, assuming already cancelled or completed");
        Ok(())
    }

    async fn setup_verification_handlers(&self, client: Client) {
        let bot = self.clone();
        
        // Handle incoming verification requests
        client.add_event_handler(move |ev: matrix_sdk::ruma::events::key::verification::request::ToDeviceKeyVerificationRequestEvent| {
            let bot = bot.clone();
            async move {
                info!("Received verification request from {}", ev.sender);
                
                let request_info = VerificationRequestInfo {
                    request_id: ev.content.transaction_id.to_string(),
                    other_user_id: ev.sender.to_string(),
                    other_device_id: ev.content.from_device.to_string(),
                    status: "pending".to_string(),
                };
                
                bot.verification_requests.write().await.push(request_info);
                info!("Added verification request to queue");
            }
        });
        
        let bot = self.clone();
        
        // Monitor for SAS verification updates
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                
                let client = {
                    let client_guard = bot.client.lock().await;
                    if let Some(ref client) = *client_guard {
                        client.clone()
                    } else {
                        break;
                    }
                };
                
                // Check all pending verification requests for SAS data
                let requests = bot.verification_requests.read().await.clone();
                
                for req_info in requests {
                    if let Ok(user_id) = <&UserId>::try_from(req_info.other_user_id.as_str()) {
                        if let Some(Verification::SasV1(sas)) = client.encryption().get_verification(user_id, &req_info.request_id).await {
                            if sas.can_be_presented() {
                                // Only update if we don't have active SAS or it's for the same request
                                let should_update = {
                                    let active = bot.active_sas.read().await;
                                    active.is_none() || active.as_ref().map(|a| &a.request_id) == Some(&req_info.request_id)
                                };
                                
                                if should_update {
                                    // Get emoji or decimals
                                    let emoji = sas.emoji().map(|emojis| {
                                        emojis.iter()
                                            .map(|e| (e.symbol.to_string(), e.description.to_string()))
                                            .collect()
                                    });
                                    
                                    let decimals = sas.decimals();
                                    
                                    let sas_info = SasInfo {
                                        request_id: req_info.request_id.clone(),
                                        emoji,
                                        decimals,
                                    };
                                    
                                    *bot.active_sas.write().await = Some(sas_info);
                                    info!("SAS verification ready for presentation");
                                }
                            }
                            
                            if sas.is_done() {
                                info!("SAS verification completed");
                                *bot.active_sas.write().await = None;
                                // Remove from verification requests
                                bot.verification_requests.write().await.retain(|r| r.request_id != req_info.request_id);
                            }
                        }
                    }
                }
            }
        });
    }
}
