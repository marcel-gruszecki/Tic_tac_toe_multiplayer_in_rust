# Tic-Tac-Toe Multiplayer

A real-time multiplayer Tic-Tac-Toe server written in **Rust** using the **Axum** framework, **WebSocket** communication, and **PostgreSQL** for persistence. The frontend is a single self-contained `index.html` file with no external dependencies.

**Author:** Marcel Gruszecki  
**License:** MIT

---

> [!WARNING]
> `docker-compose.yml` contains default database credentials (`gracz` / `haslo`).
> **Change them before deploying to any non-local environment.**
> Update both the `db` service environment variables and the `DATABASE_URL` in the `server` service.

---

## Features

- User registration and login with bcrypt password hashing
- Matchmaking queue — players are automatically paired when two are searching
- Real-time gameplay over WebSocket
- Server-side move validation
- Automatic win awarded on opponent disconnect
- Live Top 10 leaderboard (score = wins − losses, minimum 0)
- Database schema is created automatically on first startup

---

## Tech Stack

| Layer | Technology |
|-------|-----------|
| Language | [Rust](https://www.rust-lang.org/) |
| HTTP / WebSocket | [Axum](https://github.com/tokio-rs/axum) 0.8 |
| Async runtime | [Tokio](https://tokio.rs/) |
| Database client | [sqlx](https://github.com/launchbadge/sqlx) |
| Password hashing | [bcrypt](https://docs.rs/bcrypt) |
| Database | [PostgreSQL](https://www.postgresql.org/) 16 |
| Frontend | HTML / CSS / JS |

---

## Quick Start — Docker Compose

The easiest way to run everything (database + server + web) in one command:

```bash
git clone https://github.com/marcel-gruszecki/Tic_tac_toe_multiplayer_in_rust
cd Tic_tac_toe_multiplayer_in_rust

docker compose up --build
```

Three containers come up: PostgreSQL, the Rust server, and an Apache container that
serves `index.html` and proxies `/api/` to the server on the same origin.

Open **`http://localhost`** in your browser — that's it.
The raw API/WebSocket is also reachable directly at `http://localhost:3000` if needed.

### Changing default credentials

Open `docker-compose.yml` and replace all occurrences of the default values:

Before:
```yaml
POSTGRES_USER: gracz
POSTGRES_PASSWORD: haslo
DATABASE_URL: postgresql://gracz:haslo@db:5432/tictactoe
```

After (example):
```yaml
POSTGRES_USER: myuser
POSTGRES_PASSWORD: str0ngPassw0rd!
DATABASE_URL: postgresql://myuser:str0ngPassw0rd!@db:5432/tictactoe
```

---

> [!NOTE]
> This project is only supported via Docker Compose — the server reads `DATABASE_URL`
> straight from the environment and does not load a `.env` file. There is no
> local (non-Docker) run path.

---

## Project Structure

```
.
├── src/
│   ├── main.rs        # Server bootstrap, routing, shared application state
│   ├── game.rs        # WebSocket handlers, matchmaking queue, game loop
│   └── database.rs    # PostgreSQL queries, schema init, password hashing
├── index.html         # Frontend (HTML / CSS / JS — no build step)
├── Dockerfile         # Multi-stage: builder -> server target, web (Apache) target
├── docker-compose.yml
├── apache.conf        # Apache reverse-proxy config, baked into the `web` image
└── Cargo.toml
```

---

## API Reference

### REST

| Method | Endpoint | Description |
|--------|----------|-------------|
| POST | `/api/register` | Create a new account |
| POST | `/api/login` | Authenticate; returns a UUID session token |
| GET | `/api/top10` | Fetch the top-10 leaderboard |

**Request body — register / login:**
```json
{
  "name": "playerName",
  "password": "secret",
  "token": ""
}
```

**Login — success (HTTP 202):**
```json
"xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
```

**Login — failure (HTTP 404):**
```json
"ERROR"
```

### WebSocket

| Endpoint | Description |
|----------|-------------|
| `GET /api/search` | Enter matchmaking queue; upgrades to WebSocket |

**1. Authenticate immediately after connecting (client → server):**
```json
{ "token": "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx" }
```

**2. Send a move (client → server):**
```json
{ "field": 4 }
```

`field` is the board cell index (0–8): 0 = top-left, 8 = bottom-right.

**Game state pushed after every move (server → client):**
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

| `response` | Meaning |
|-----------|---------|
| `Accepted` | Move was valid and applied |
| `Refused` | Wrong turn or cell already taken |
| `Waiting` | Waiting for the opponent's move |

| `status` | Meaning |
|---------|---------|
| `InGame` | Game in progress |
| `Player1Won` | Player with symbol `O` won |
| `Player2Won` | Player with symbol `X` won |
| `Draw` | Board full, no winner |
| `Error` | Opponent disconnected |

---

## Database Schema

```sql
CREATE TABLE IF NOT EXISTS users (
    id       SERIAL  PRIMARY KEY,
    username TEXT    NOT NULL UNIQUE,
    password TEXT    NOT NULL,           -- bcrypt hash, never stored in plain text
    wins     INTEGER DEFAULT 0,
    loses    INTEGER DEFAULT 0,
    points   INTEGER GENERATED ALWAYS AS (GREATEST(wins - loses, 0)) STORED,
    token    TEXT                        -- UUID, rotated on every login
);
```

The table is created automatically on first startup — no manual migration needed.

---

## Security Notes

- Passwords are hashed with **bcrypt** before storage and are never logged.
- Session tokens are random **UUID v4** values, rotated on every login.
- **Change the default `docker-compose.yml` credentials** before any non-local deployment.
- Consider placing the server behind a firewall and exposing only port 80/443 through Apache.
