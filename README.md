# Tic-Tac-Toe Multiplayer – Rust Server

A real-time multiplayer Tic-Tac-Toe server written in **Rust** using the **Axum** framework, **WebSocket** communication, and **PostgreSQL** for persistence. The frontend is a plain HTML/CSS/JS single file (`index.html`) with no external dependencies.

---

## Features

- User registration and login with bcrypt password hashing
- Matchmaking queue — players are automatically paired when two are waiting
- Real-time gameplay over WebSocket
- Server-side move validation
- Automatic win on opponent disconnect
- Top 10 leaderboard (score = wins − losses)
- Database schema is created automatically on startup

---

## Requirements

| Tool | Minimum version |
|------|----------------|
| Rust | 1.80+ |
| PostgreSQL | 14+ |
| Docker (optional) | 20+ |
| Apache (optional) | 2.4+ |

---

## Quick Start – Local Development

### 1. Clone the repository

```bash
git clone <repository-url>
cd Tic_tac_toe_multiplayer_in_rust
```

### 2. Set up PostgreSQL

```sql
CREATE DATABASE tictactoe;
CREATE USER player WITH PASSWORD 'password';
GRANT ALL PRIVILEGES ON DATABASE tictactoe TO player;
```

### 3. Configure the `.env` file

```env
DATABASE_URL=postgresql://player:password@localhost:5432/tictactoe
```

### 4. Build and run

```bash
cargo run --release
```

The database schema is created automatically on first run.  
The server listens on `http://0.0.0.0:3000`.

### 5. Open the frontend

Open `index.html` directly in a browser, or serve it through Apache (see the Apache section below).

---

## Project Structure

```
.
├── src/
│   ├── main.rs        # Server bootstrap, routing, matchmaking queue
│   ├── game.rs        # Game logic, WebSocket handlers
│   └── database.rs    # PostgreSQL queries
├── index.html         # Frontend (HTML/CSS/JS)
├── Dockerfile
├── docker-compose.yml
├── .env               # Environment variables (do not commit!)
└── Cargo.toml
```

---

## API

### REST Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/register` | POST | Register a new user |
| `/api/login` | POST | Log in, returns a UUID session token |
| `/api/top10` | POST | Fetch the top 10 ranked players |

**Request body (register / login):**
```json
{
  "name": "playerName",
  "password": "password",
  "token": ""
}
```

**Login response (HTTP 202):**
```json
{
  "token": "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
}
```

### WebSocket

| Endpoint | Description |
|----------|-------------|
| `/api/search` | Matchmaking and real-time gameplay |

**Initiate connection (client → server):**
```json
{ "token": "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx" }
```

**Send a move (client → server):**
```json
{ "field": 4 }
```
`field` is the board cell index (0–8), where 0 = top-left, 8 = bottom-right.

**Game state (server → client):**
```json
{
  "game": {
    "board": ["O", "Null", "X", "Null", "O", "Null", "Null", "Null", "X"],
    "current_move": "X"
  },
  "response": "Accepted",
  "status": "InGame",
  "your_symbol": "O"
}
```

| `response` value | Meaning |
|-----------------|---------|
| `Accepted` | Move accepted |
| `Refused` | Move rejected (wrong turn or occupied cell) |
| `OtherPlayer` | The opponent just moved |
| `Waiting` | Waiting for an opponent |

| `status` value | Meaning |
|---------------|---------|
| `InGame` | Game in progress |
| `Player1Won` | Player 1 won |
| `Player2Won` | Player 2 won |
| `Draw` | Draw |
| `Error` | Error or disconnection |

---

## Database Schema

```sql
CREATE TABLE IF NOT EXISTS users (
    id       SERIAL PRIMARY KEY,
    username TEXT    NOT NULL UNIQUE,
    password TEXT    NOT NULL,
    wins     INTEGER DEFAULT 0,
    loses    INTEGER DEFAULT 0,
    points   INTEGER GENERATED ALWAYS AS (GREATEST(wins - loses)) STORED,
    token    TEXT
);
```

> The table is created automatically on server startup — no manual setup needed.

---

## Apache Configuration (Reverse Proxy)

The Rust server runs on port `3000`. Apache acts as a reverse proxy, forwarding both HTTP API requests and WebSocket connections, while serving the static `index.html` frontend.

### Required modules

```bash
sudo a2enmod proxy
sudo a2enmod proxy_http
sudo a2enmod proxy_wstunnel
sudo a2enmod rewrite
sudo systemctl restart apache2
```

### VirtualHost – HTTP

Create `/etc/apache2/sites-available/tictactoe.conf`:

```apache
<VirtualHost *:80>
    ServerName yourdomain.com

    # Directory containing index.html
    DocumentRoot /var/www/tictactoe

    <Directory /var/www/tictactoe>
        Options -Indexes
        AllowOverride None
        Require all granted
    </Directory>

    # Proxy WebSocket connections
    RewriteEngine On
    RewriteCond %{HTTP:Upgrade} websocket [NC]
    RewriteCond %{HTTP:Connection} upgrade [NC]
    RewriteRule ^/api/(.*)$ ws://127.0.0.1:3000/api/$1 [P,L]

    # Proxy REST API requests
    ProxyPass        /api/ http://127.0.0.1:3000/api/
    ProxyPassReverse /api/ http://127.0.0.1:3000/api/

    ErrorLog  ${APACHE_LOG_DIR}/tictactoe_error.log
    CustomLog ${APACHE_LOG_DIR}/tictactoe_access.log combined
</VirtualHost>
```

### VirtualHost – HTTPS (Let's Encrypt)

```apache
<VirtualHost *:443>
    ServerName yourdomain.com

    DocumentRoot /var/www/tictactoe

    <Directory /var/www/tictactoe>
        Options -Indexes
        AllowOverride None
        Require all granted
    </Directory>

    SSLEngine on
    SSLCertificateFile    /etc/letsencrypt/live/yourdomain.com/fullchain.pem
    SSLCertificateKeyFile /etc/letsencrypt/live/yourdomain.com/privkey.pem

    # Proxy WebSocket connections (wss://)
    RewriteEngine On
    RewriteCond %{HTTP:Upgrade} websocket [NC]
    RewriteCond %{HTTP:Connection} upgrade [NC]
    RewriteRule ^/api/(.*)$ wss://127.0.0.1:3000/api/$1 [P,L]

    ProxyPass        /api/ http://127.0.0.1:3000/api/
    ProxyPassReverse /api/ http://127.0.0.1:3000/api/

    ErrorLog  ${APACHE_LOG_DIR}/tictactoe_ssl_error.log
    CustomLog ${APACHE_LOG_DIR}/tictactoe_ssl_access.log combined
</VirtualHost>
```

### Enable the site

```bash
# Copy index.html to the DocumentRoot
sudo mkdir -p /var/www/tictactoe
sudo cp index.html /var/www/tictactoe/

# Enable the site
sudo a2ensite tictactoe.conf
sudo systemctl reload apache2
```

> **Note:** When using HTTPS, the frontend must connect via `wss://` instead of `ws://`. Make sure the JavaScript in `index.html` uses the correct protocol or detects it automatically from `window.location.protocol`.

---

## Docker

### Build the image

sqlx validates SQL queries at compile time, so `DATABASE_URL` must be available during the build.

```bash
docker build \
  --build-arg DATABASE_URL="postgresql://player:password@host.docker.internal:5432/tictactoe" \
  -t tictactoe-server .
```

### Run the container

```bash
docker run -d \
  --name tictactoe \
  -p 3000:3000 \
  -e DATABASE_URL="postgresql://player:password@host.docker.internal:5432/tictactoe" \
  tictactoe-server
```

### Docker Compose (server + database)

```bash
docker compose up -d
```

---

## Environment Variables

| Variable | Required | Description | Example |
|----------|----------|-------------|---------|
| `DATABASE_URL` | Yes | PostgreSQL connection string | `postgresql://user:pass@localhost:5432/tictactoe` |

---

## Tech Stack

- **[Rust](https://www.rust-lang.org/)** — systems programming language
- **[Axum](https://github.com/tokio-rs/axum)** — async HTTP/WebSocket framework
- **[Tokio](https://tokio.rs/)** — async runtime
- **[sqlx](https://github.com/launchbadge/sqlx)** — async PostgreSQL client
- **[bcrypt](https://docs.rs/bcrypt)** — password hashing
- **[PostgreSQL](https://www.postgresql.org/)** — relational database
