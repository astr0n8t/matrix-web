mod bot;
mod config;
mod credentials;
mod web;

use config::Config;
use credentials::CredentialStore;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    // Load configuration
    let config = Config::load("config.yaml").unwrap_or_else(|_| {
        eprintln!("Failed to load config.yaml. Please create it from config.example.yaml");
        std::process::exit(1);
    });

    // Create Matrix bot (not connected yet)
    let (bot, _) = bot::MatrixBot::new(
        &config.homeserver,
        &config.username,
        &config.room_id,
        config.message_history.limit,
        &config.store.path,
    );

    // Clone bot for web server
    let bot_for_web = bot.clone();

    // Create credential store
    let credentials_store = CredentialStore::new(&config.database.path);

    // Start web server
    let auth_config = config.web.auth.clone();
    let state = web::AppState {
        bot: bot_for_web,
        auth: auth_config,
        credentials_store,
        username: config.username.clone(),
    };
    
    web::start_server(&config.web.host, config.web.port, state).await?;

    Ok(())
}
