//! # Tic-Tac-Toe Multiplayer — Server Entry Point
//!
//! Asynchronous HTTP/WebSocket server built with [Axum](https://github.com/tokio-rs/axum)
//! and [Tokio](https://tokio.rs). Manages player authentication, real-time game
//! matchmaking via a shared queue, and a live leaderboard backed by PostgreSQL.
//!
//! ## API
//!
//! | Method | Path            | Description                                        |
//! |--------|-----------------|----------------------------------------------------|
//! | POST   | `/api/register` | Create a new account                               |
//! | POST   | `/api/login`    | Authenticate and receive a UUID session token      |
//! | GET    | `/api/search`   | Upgrade to WebSocket and enter the matchmaking queue |
//! | GET    | `/api/top10`    | Return the top-10 leaderboard (JSON)               |
//!
//! ## Author
//! Marcel Gruszecki
//!
//! ## License
//! MIT — see `LICENSE` in the repository root.

mod database;
mod game;

use axum::{
    routing::{get, post},
    http::StatusCode,
    Json, Router,
};
use axum::extract::State;
use axum::response::IntoResponse;
use serde::{Deserialize, Serialize};
use sqlx::{Pool, Postgres};
use crate::database::{check_password, connect_to_database, create_new_user, top10_from_database, UserRank};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;
use crate::game::{Player, websocket_connect};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Login {
    name: String,
    password: String,
    token: String,
}

#[derive(Clone)]
pub struct AppMod {
    pub queue: Arc<Mutex<VecDeque<(i32, oneshot::Sender<Player>)>>>,
    pub pool: Pool<Postgres>,
}

#[tokio::main]
async fn main() {
    let pool = connect_to_database().await;
    let appmod = AppMod {
        queue: Arc::new(Mutex::new(VecDeque::new())),
        pool: pool,
    };

    let app = Router::new()
        .route("/api/register", post(check_register))
        .route("/api/login", post(check_login))
        .route("/api/search", get(websocket_connect))
        .route("/api/top10", get(top10))
        .with_state(appmod);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn top10(State(appmod): State<AppMod>) -> impl IntoResponse {
    if let Ok(result) = top10_from_database(appmod.pool.clone()).await {
        (StatusCode::OK, Json(result))
    } else {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(vec![UserRank::new()]))
    }
}

async fn check_login(State(appmod): State<AppMod>, Json(payload): Json<Login>) -> impl IntoResponse {
    println!("Przyszedl login {:?}", payload);
    let (result, token) = check_password(appmod.pool, &payload).await;
    if result {
        println!("Użytkownik {:?} zostal zalogowany. Token {}", payload, token);
        (StatusCode::ACCEPTED, Json(token))
    } else {
        println!("Użytkownik {:?} nie zostal zalogowany. Id {}", payload, token);
        (StatusCode::NOT_FOUND, Json(String::from("ERROR")))
    }
}

async fn check_register(State(appmod): State<AppMod>, Json(payload): Json<Login>) -> StatusCode {
    println!("Przyszla rejstracja {:?}", payload);

    let name_len = payload.name.trim().chars().count();
    let password_len = payload.password.chars().count();

    if name_len < 8 || password_len < 8 {
        println!("Rejestracja odrzucona: nazwa lub haslo krotsze niz 8 znakow.");
        return StatusCode::BAD_REQUEST;
    }

    if create_new_user(appmod.pool, &payload).await {
        println!("Użytkownik {:?} zostal utworzony.", payload);
        StatusCode::ACCEPTED
    } else {
        println!("Użytkownik {:?} nie zostal utworzony.", payload);
        StatusCode::FOUND
    }
}