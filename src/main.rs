mod bot;
mod config;
mod web;

use config::Config;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    // Load configuration
    let config = Config::load("config.yaml").unwrap_or_else(|_| {
        eprintln!("Failed to load config.yaml. Please create it from config.example.yaml");
        std::process::exit(1);
    });

    // Create Matrix bot
    let (bot, _) = bot::MatrixBot::new(
        &config.homeserver,
        &config.username,
        &config.password,
        &config.room_id,
        config.message_history.limit,
        &config.store.path,
        &config.store.passphrase,
    )
    .await?;

    // Join the configured room
    bot.join_room().await?;

    // Load message history
    bot.load_message_history(config.message_history.limit).await?;

    // Clone bot for web server
    let bot_for_web = bot.clone();

    // Start web server in a separate task
    let auth_config = config.web.auth.clone();
    let web_handle = tokio::spawn(async move {
        let state = web::AppState {
            bot: bot_for_web,
            auth: auth_config,
        };
        
        if let Err(e) = web::start_server(&config.web.host, config.web.port, state).await {
            eprintln!("Web server error: {}", e);
        }
    });

    // Start Matrix sync
    let sync_handle = tokio::spawn(async move {
        if let Err(e) = bot.start_sync().await {
            eprintln!("Matrix sync error: {}", e);
        }
    });

    // Wait for both tasks
    tokio::try_join!(web_handle, sync_handle)?;

    Ok(())
}
