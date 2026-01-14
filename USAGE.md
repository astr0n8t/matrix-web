# Usage Guide for Matrix Web Bot

This guide will help you set up and run your Matrix Web Bot.

## Prerequisites

1. **Rust Installation**: Make sure you have Rust 1.70 or later installed. If not, install it from [rustup.rs](https://rustup.rs/):
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

2. **Matrix Account**: You need a Matrix account. You can create one at:
   - [matrix.org](https://app.element.io/#/register) (official homeserver)
   - Or any other Matrix homeserver

3. **Matrix Room**: You need a room ID where the bot will operate. You can:
   - Create a new room in Element
   - Get the room ID from Room Settings → Advanced → Internal Room ID

## Configuration

1. **Copy the example configuration**:
   ```bash
   cp config.example.yaml config.yaml
   ```

2. **Edit config.yaml** with your settings:
   ```yaml
   # Matrix homeserver URL (e.g., https://matrix.org)
   homeserver: "https://matrix.org"
   
   # Your bot's Matrix username (without @homeserver)
   username: "your_bot_username"
   
   # Your bot's password
   password: "your_bot_password"
   
   # Room ID where the bot will operate (format: !xxxxx:homeserver)
   room_id: "!room:matrix.org"
   
   # Web server configuration
   web:
     host: "127.0.0.1"  # Change to "0.0.0.0" to allow external connections
     port: 8080          # Change if you want a different port
     
     # Optional: Header-based authentication for reverse proxy
     # IMPORTANT: Use SHA-256 hash of the token, not the plain value!
     # Generate hash: cargo run --bin hash-tool "your-secret-token"
     # auth:
     #   header_name: "X-Auth-Token"
     #   header_value_hash: "ea5add57437cbf20af59034d7ed17968dcc56767b41965fcc5b376d45db8b4a3"
   
   # Message history configuration (optional, defaults to 50)
   message_history:
     limit: 50  # Number of historical messages to load on startup
   ```

   **Important Notes**:
   - The `room_id` must start with `!` and include the full homeserver domain
   - To find your room ID in Element: Room Settings → Advanced → Internal Room ID
   - Keep `config.yaml` secure - it contains your bot credentials
   - **Security**: Authentication uses SHA-256 hashing - only store the hash, never the plain token
   - Header authentication is useful when deploying behind a reverse proxy like nginx

### Generating Authentication Hash

To generate a SHA-256 hash for your authentication token:

**Using the provided tool (recommended):**
```bash
cargo run --bin hash-tool "your-secret-token"
```

**Using command line:**
```bash
echo -n "your-secret-token" | sha256sum
```

The hash output should be placed in the `header_value_hash` field in your config.

### Environment Variable Support

All configuration values can be overridden using environment variables for better security:

| Environment Variable | Description | Example |
|---------------------|-------------|---------|
| `MATRIX_HOMESERVER` | Matrix homeserver URL | `https://matrix.org` |
| `MATRIX_USERNAME` | Bot username | `mybot` |
| `MATRIX_PASSWORD` | Bot password | `secret123` |
| `MATRIX_ROOM_ID` | Room ID to join | `!abc123:matrix.org` |
| `WEB_HOST` | Web server host | `127.0.0.1` |
| `WEB_PORT` | Web server port | `8080` |
| `WEB_AUTH_HEADER_NAME` | Auth header name | `X-Auth-Token` |
| `WEB_AUTH_HEADER_VALUE` | Auth header value (auto-hashed) | `secret-token` |
| `MESSAGE_HISTORY_LIMIT` | Number of messages to load | `50` |

**Note**: When using `WEB_AUTH_HEADER_VALUE` environment variable, the value is automatically hashed using SHA-256. In the config file, you must provide the pre-computed hash as `header_value_hash`.

**Usage Examples**:

1. **Override password only**:
   ```bash
   export MATRIX_PASSWORD="my-secret-password"
   cargo run --release
   ```

2. **Full environment configuration**:
   ```bash
   export MATRIX_HOMESERVER="https://matrix.example.com"
   export MATRIX_USERNAME="bot_user"
   export MATRIX_PASSWORD="secret-password"
   export MATRIX_ROOM_ID="!room123:example.com"
   export WEB_AUTH_HEADER_VALUE="my-secret-token"
   cargo run --release
   ```

3. **Docker/Container deployment**:
   ```bash
   docker run -e MATRIX_PASSWORD="secret" \
              -e WEB_AUTH_HEADER_VALUE="token" \
              matrix-web
   ```

**Best Practices**:
- Use environment variables for all sensitive data (passwords, tokens)
- Keep non-sensitive defaults in `config.yaml`
- Environment variables always override config file values

## Building and Running

### Development Mode

For development with faster compile times:
```bash
cargo run
```

### Release Mode

For production with optimized performance:
```bash
cargo build --release
./target/release/matrix-web
```

## Using the Web Interface

1. **Start the bot** using one of the methods above

2. **Open your browser** and navigate to:
   ```
   http://127.0.0.1:8080
   ```
   (or the host:port you configured)

3. **Send messages**:
   - Type your message in the input box at the bottom
   - Press Enter or click "Send"
   - Your message will be posted to the Matrix room

4. **Receive messages**:
   - Previous messages are loaded automatically on startup (configurable limit)
   - All new messages in the Matrix room will appear in real-time
   - Messages are displayed in IRC-like format: `@user:homeserver: message`

## Features

### Message History

The bot automatically loads recent messages when it starts:
- Configurable via `message_history.limit` in config (default: 50 messages)
- Messages are displayed in chronological order
- History is loaded from the Matrix server on each restart

### Header-Based Authentication

Optional authentication using HTTP headers for reverse proxy setups:
- Configure via `web.auth.header_name` and `web.auth.header_value` in config
- Useful when deploying behind nginx, Caddy, or other reverse proxies
- The reverse proxy validates the user and passes a header to the bot
- Example: nginx can validate OAuth and pass `X-Auth-Token` header

### End-to-End Encryption (E2EE)

The bot supports E2EE automatically through matrix-sdk:
- First time the bot joins an encrypted room, it will set up encryption
- Encryption keys are stored in the SQLite database (created automatically)
- The bot can send and receive encrypted messages

### Real-time Updates

- Uses Server-Sent Events (SSE) for real-time message streaming
- No polling - messages appear instantly
- Automatic reconnection if connection is lost

## Troubleshooting

### Bot can't connect to homeserver

- Check that the homeserver URL is correct and accessible
- Verify your username and password are correct
- Try logging in manually with Element to test credentials

### Bot can't join room

- Verify the room ID is correct (format: `!xxxxx:homeserver`)
- Make sure the bot account has been invited to the room
- Check that the bot has permission to join

### Web interface shows "Disconnected"

- Check that the bot is running
- Verify the web server is accessible at the configured host:port
- Check browser console for error messages

### Build errors

If you encounter build errors:
```bash
# Update Rust toolchain
rustup update

# Clean and rebuild
cargo clean
cargo build --release
```

## Security Considerations

1. **Config File**: The `config.yaml` file contains sensitive credentials
   - Never commit it to version control (it's in .gitignore)
   - Set appropriate file permissions: `chmod 600 config.yaml`
   - **Recommended**: Use environment variables for passwords and tokens instead of storing in config file

2. **Environment Variables**: Best practice for sensitive data
   - Store passwords, tokens, and secrets in environment variables
   - Environment variables are not stored in files or version control
   - Use `.env` files with tools like `direnv` or Docker secrets in production
   - Example: `export MATRIX_PASSWORD="secret"` instead of putting it in config.yaml

3. **Web Interface**: 
   - By default, only accessible from localhost (127.0.0.1)
   - To allow external access, change host to "0.0.0.0" and use a reverse proxy
   - Consider adding authentication if exposing to the internet

4. **E2EE**:
   - Encryption database is stored locally
   - Backup the database if you need to preserve encryption keys
   - Delete the database to reset encryption state

## Advanced Configuration

### Running as a Service (Linux)

Create a systemd service file at `/etc/systemd/system/matrix-web.service`:

```ini
[Unit]
Description=Matrix Web Bot
After=network.target

[Service]
Type=simple
User=your-user
WorkingDirectory=/path/to/matrix-web
ExecStart=/path/to/matrix-web/target/release/matrix-web
Restart=always
RestartSec=10

[Install]
WantedBy=multi-user.target
```

Enable and start:
```bash
sudo systemctl enable matrix-web
sudo systemctl start matrix-web
```

### Using with Reverse Proxy (nginx)

#### Basic Setup (No Authentication)

Example nginx configuration:
```nginx
server {
    listen 80;
    server_name bot.example.com;

    location / {
        proxy_pass http://127.0.0.1:8080;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection 'upgrade';
        proxy_set_header Host $host;
        proxy_cache_bypass $http_upgrade;
    }
}
```

#### With Header-Based Authentication

Example nginx configuration with authentication:
```nginx
server {
    listen 80;
    server_name bot.example.com;

    location / {
        # Add authentication check here (e.g., OAuth, basic auth, etc.)
        # This example uses a simple shared secret
        
        # Set the authentication header
        proxy_set_header X-Auth-Token "your-secret-token-here";
        
        proxy_pass http://127.0.0.1:8080;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection 'upgrade';
        proxy_set_header Host $host;
        proxy_cache_bypass $http_upgrade;
    }
}
```

Then configure your bot's `config.yaml`:
```yaml
web:
  auth:
    header_name: "X-Auth-Token"
    header_value: "your-secret-token-here"
```

This ensures only requests with the correct header can access the bot.

## Development

### Project Structure

```
matrix-web/
├── src/
│   ├── main.rs       # Application entry point
│   ├── bot.rs        # Matrix bot client and message handling
│   ├── config.rs     # Configuration parsing
│   └── web.rs        # Web server and API endpoints
├── static/
│   └── index.html    # Web interface
├── Cargo.toml        # Rust dependencies
└── config.yaml       # Your configuration (not in git)
```

### API Endpoints

- `GET /` - Web interface (HTML)
- `GET /api/history` - Get message history
  - Response: `{"messages": ["sender: message", ...]}`
- `POST /api/messages` - Send a message to Matrix
  - Body: `{"message": "your message"}`
  - Response: `{"success": true/false, "error": "..."}`
- `GET /api/stream` - SSE stream of incoming messages

**Note**: All endpoints require authentication header if configured in `config.yaml`.

## Getting Help

If you encounter issues:
1. Check the application logs for error messages
2. Verify your configuration is correct
3. Test your Matrix credentials with Element
4. Open an issue on GitHub with logs and configuration (redact credentials!)
