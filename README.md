# matrix-web

An end-to-end encrypted Matrix/Element bot with a simple IRC-like web interface built with Rust.

## Features

- End-to-end encryption support via matrix-sdk
- Simple IRC-like web interface
- Real-time message streaming using Server-Sent Events (SSE)
- Configuration via YAML file
- No database required - all configuration in a single file

## Prerequisites

- Rust (1.70 or later)
- A Matrix account and homeserver
- Access to a Matrix room

## Installation

1. Clone the repository:
```bash
git clone https://github.com/astr0n8t/matrix-web.git
cd matrix-web
```

2. Create configuration file:
```bash
cp config.example.yaml config.yaml
```

3. Edit `config.yaml` with your Matrix credentials:
```yaml
homeserver: "https://matrix.org"
username: "your_bot_username"
password: "your_bot_password"
room_id: "!room:matrix.org"
web:
  host: "127.0.0.1"
  port: 8080
```

## Usage

1. Build and run the application:
```bash
cargo run --release
```

2. Open your web browser and navigate to:
```
http://127.0.0.1:8080
```

3. Type messages in the input field and press Enter or click Send to post to the Matrix room.

## How It Works

- The bot logs in to your Matrix homeserver with the provided credentials
- It joins the specified room and starts syncing messages
- The web interface connects to the bot via SSE for real-time message updates
- Messages sent through the web interface are posted to the Matrix room
- All messages in the room are displayed in the IRC-like interface

## Architecture

- **Bot Module** (`src/bot.rs`): Handles Matrix client, authentication, and message sync
- **Web Module** (`src/web.rs`): Axum-based web server with REST API and SSE endpoints
- **Config Module** (`src/config.rs`): YAML configuration parsing
- **Frontend** (`static/index.html`): Single-page IRC-like interface

## Security

- The bot supports end-to-end encryption through matrix-sdk
- Keep your `config.yaml` file secure as it contains credentials
- The configuration file is in `.gitignore` to prevent accidental commits

## License

See LICENSE file for details.