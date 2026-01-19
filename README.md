# matrix-web

An end-to-end encrypted Matrix/Element bot with a simple IRC-like web interface built with Rust.

> [!WARNING]
> This project is mostly vibe-coded using copilot

## Features

- **End-to-end encryption (E2EE)** with automatic cross-signing and backup support
- **Persistent encryption store** using SQLite to maintain encryption keys across restarts
- Simple IRC-like web interface
- Real-time message streaming using Server-Sent Events (SSE)
- **Message history**: Automatically loads and displays recent messages on startup
- **Header-based authentication**: Optional reverse proxy authentication with SHA-256 hashed tokens
- **Environment variable support**: Secure credential management for production deployments
- Configuration via YAML file
- Utility tool for generating authentication token hashes
- **Device verification**: Support for verifying bot device via Element client

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
# password is no longer stored in config - you will be prompted on first launch
room_id: "!room:matrix.org"
web:
  host: "127.0.0.1"
  port: 8080
  # Optional: Header-based authentication
  # IMPORTANT: Use SHA-256 hash of your secret token, not the plain value
  # Generate hash with: cargo run --bin hash-tool "your-secret-token"
  # auth:
  #   header_name: "X-Auth-Token"
  #   header_value_hash: "ea5add57437cbf20af59034d7ed17968dcc56767b41965fcc5b376d45db8b4a3"
# Optional: Message history settings
message_history:
  limit: 50  # Number of messages to load

# Optional: Encryption store settings
store:
  path: "./matrix_store"  # Directory for SQLite database
  passphrase: ""          # Optional passphrase for encrypting the store
```

### End-to-End Encryption Setup

The bot automatically configures end-to-end encryption with the following features:

1. **Persistent SQLite Store**: Encryption keys are stored in a local SQLite database (default: `./matrix_store`)
2. **Automatic Cross-Signing**: The bot attempts to bootstrap cross-signing on first run
3. **Device Verification**: To enable full E2EE functionality and avoid backup warnings:

   - On first run, the bot will log messages about device verification
   - Open Element on another device where you're logged in
   - Go to Settings → Security & Privacy → Cross-signing
   - Verify the new "Matrix Web Bot" device session
   - Once verified, key backups will automatically be enabled

**Note**: The warning `"Trying to backup room keys but no backup key was found"` will appear until you verify the device via another Element session. After verification, the bot will automatically join the backup system and the warning will stop.

### Generating Authentication Hash

To generate a hash for your authentication token:

```bash
# Using the provided hash tool
cargo run --bin hash-tool "your-secret-token"

# Or using command line
echo -n "your-secret-token" | sha256sum
```

### Environment Variable Support

All configuration values can be overridden using environment variables:

- `DATABASE_PATH` - Path to credentials database
- `MATRIX_HOMESERVER` - Matrix homeserver URL
- `MATRIX_USERNAME` - Bot username
- `MATRIX_ROOM_ID` - Room ID to join
- `MATRIX_STORE_PATH` - Path to SQLite encryption store
- `MATRIX_STORE_PASSPHRASE` - Passphrase for encryption store
- `WEB_HOST` - Web server host
- `WEB_PORT` - Web server port
- `WEB_AUTH_HEADER_NAME` - Authentication header name
- `WEB_AUTH_HEADER_VALUE` - Authentication header value (will be hashed automatically)
- `MESSAGE_HISTORY_LIMIT` - Number of messages to load

**Note:** Matrix password is no longer stored in configuration or environment variables. You will be prompted to enter it via the web interface on first launch.

Example using environment variables:
```bash
export WEB_AUTH_HEADER_VALUE="secret-token"  # Will be hashed automatically
cargo run --release
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

3. **Enter your credentials** in the login modal:
   - On first launch, you'll be prompted for both:
     - **Matrix password**: Your Matrix account password (will be encrypted and stored)
     - **SQLite database password**: A password to encrypt the credentials database
   - On subsequent launches, you only need the SQLite database password
   - The Matrix credentials are encrypted and stored in `credentials.db`

4. Once connected:
   - Type messages in the input field and press Enter or click Send to post to the Matrix room
   - Click the "Disconnect" button in the header to log out from the Matrix server
   - The bot will also automatically disconnect when you close the browser tab

## Docker Deployment

The application is designed to run in Docker with persistent storage for encryption keys and credentials.

### Building the Docker Image

```bash
docker build -t matrix-web .
```

### Running with Docker

The container uses `/data` as the working directory for storing:
- `matrix.db` - Encrypted credentials database
- `matrix_store/` - Matrix SDK encryption store (E2EE keys)

**Important:** Mount a volume to `/data` to persist data across container restarts:

```bash
# Create a directory for persistent data
mkdir -p ./matrix-data

# Run with volume mount
docker run -d \
  -p 8080:8080 \
  -v ./matrix-data:/data \
  -v ./config.yaml:/config.yaml:ro \
  matrix-web
```

### Docker Compose Example

```yaml
version: '3.8'
services:
  matrix-web:
    image: matrix-web
    build: .
    ports:
      - "8080:8080"
    volumes:
      - ./matrix-data:/data
      - ./config.yaml:/config.yaml:ro
    environment:
      - RUST_LOG=info
      # Optional: Override config with environment variables
      # - MATRIX_HOMESERVER=https://matrix.org
      # - MATRIX_USERNAME=your_bot_username
      # - MATRIX_ROOM_ID=!room:matrix.org
    restart: unless-stopped
```

### Environment Variables for Docker

All configuration options can be set via environment variables, which is useful for Docker deployments:

- `DATABASE_PATH` - Path to credentials database (default: `./matrix.db`)
- `MATRIX_STORE_PATH` - Path to encryption store (default: `./matrix_store`)
- `MATRIX_STORE_PASSPHRASE` - Passphrase for encrypting the store
- `MATRIX_HOMESERVER` - Matrix homeserver URL
- `MATRIX_USERNAME` - Bot username
- `MATRIX_ROOM_ID` - Room ID to join
- `WEB_HOST` - Web server host (default: `127.0.0.1`, use `0.0.0.0` for Docker)
- `WEB_PORT` - Web server port
- `WEB_AUTH_HEADER_NAME` - Authentication header name
- `WEB_AUTH_HEADER_VALUE` - Authentication header value (hashed automatically)
- `MESSAGE_HISTORY_LIMIT` - Number of messages to load

Example with environment variables:

```bash
docker run -d \
  -p 8080:8080 \
  -v ./matrix-data:/data \
  -e MATRIX_HOMESERVER=https://matrix.org \
  -e MATRIX_USERNAME=mybot \
  -e MATRIX_ROOM_ID='!room:matrix.org' \
  -e WEB_HOST=0.0.0.0 \
  -e MATRIX_STORE_PASSPHRASE=my-secure-passphrase \
  matrix-web
```

**Note:** When running in Docker, set `WEB_HOST=0.0.0.0` to allow connections from outside the container.

## How It Works

- The web server starts but the bot does NOT connect to Matrix on startup
- When you access the web interface, a login modal prompts for credentials:
  - **First launch**: Enter both Matrix password and SQLite database password
  - **Subsequent launches**: Enter only the SQLite database password
- The Matrix password is encrypted using the SQLite password and stored in the database file (default: `./matrix.db`, `/data/matrix.db` in Docker)
- After you enter your credentials, the bot connects to your Matrix homeserver
- The bot initializes E2EE with a persistent SQLite store using the provided passphrase
- Cross-signing is automatically set up (requires device verification via Element)
- The bot joins the specified room and loads recent message history
- Real-time syncing begins and messages are displayed in the IRC-like interface
- Messages sent through the web interface are posted to the Matrix room
- When you disconnect, the bot logs out from Matrix and clears the session
- Encryption keys and device state persist in the Matrix store directory (default: `./matrix_store`, `/data/matrix_store` in Docker)
- Matrix credentials persist in encrypted form in the database file
- Optional header-based authentication protects the web interface when behind a reverse proxy

## Architecture

- **Bot Module** (`src/bot.rs`): Handles Matrix client, E2EE, authentication, and message sync
- **Web Module** (`src/web.rs`): Axum-based web server with REST API and SSE endpoints
- **Config Module** (`src/config.rs`): YAML configuration parsing with environment variable overrides
- **Frontend** (`static/index.html`): Single-page IRC-like interface

## Security

- **End-to-end encryption**: All messages in encrypted rooms are automatically encrypted/decrypted
- **Persistent encryption state**: Keys are stored securely in an SQLite database (default: `./matrix_store`, `/data/matrix_store` in Docker)
- **Device verification**: Verify the bot device via Element to enable full E2EE features and key backups
- **Store encryption**: Optionally protect the encryption store with a passphrase
- **Credential encryption**: Matrix credentials are encrypted and stored locally (default: `./matrix.db`, `/data/matrix.db` in Docker)
- **No plaintext passwords**: Passwords are never stored in plaintext in configuration files
- The configuration file is in `.gitignore` to prevent accidental commits
- Keep your database file and Matrix store directory secure
- When using Docker, ensure the `/data` volume has appropriate permissions
- See [SECURITY.md](SECURITY.md) for detailed security considerations

## License

See LICENSE file for details.
