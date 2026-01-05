mod database;
mod game;

use axum::{
    routing::{get, post},
    http::StatusCode,
    Json, Router,
};
use axum::extract::State;
use axum::extract::ws::Message;
use axum::extract::ws::WebSocket;
use axum::extract::ws::WebSocketUpgrade;
use axum::response::IntoResponse;
use serde::{Deserialize, Serialize};
use sqlx::{Error, Pool, Postgres};
use crate::database::{check_password, connect_to_database, create_new_user, does_user_exist, top10_from_database, UserRank};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use sqlx_postgres::PgRow;
use tokio::sync::{broadcast, oneshot};
use crate::game::{Player, websocket_connect};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Login {
    name: String,
    password: String,
    token: String,
}

#[derive(Clone)]
pub struct AppMod {
    pub queue: Arc<Mutex<VecDeque<oneshot::Sender<Player>>>>,
    pub pool: Pool<Postgres>,
}

#[tokio::main]
async fn main() {
    let pool = connect_to_database().await;
    let mut appmod = AppMod {
        queue: Arc::new(Mutex::new(VecDeque::new())),
        pool: pool,
    };

    let app = Router::new()
        .route("/api/register", post(check_register))
        .route("/api/login", post(check_login))
        .route("/api/search", get(websocket_connect))
        .route("/api/top10", post(top10))
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
    if create_new_user(appmod.pool, &payload).await {
        println!("Użytkownik {:?} zostal utworzony.", payload);
        StatusCode::ACCEPTED
    } else {
        println!("Użytkownik {:?} nie zostal utworzony.", payload);
        StatusCode::FOUND
    }
}