use sqlx::postgres::PgPoolOptions;
use std::env;
use std::env::VarError;
use sqlx::{Executor, Pool, Postgres};
use crate::Login;
use bcrypt::{DEFAULT_COST, hash, verify};
use crate::game::Player;

pub async fn connect_to_database() -> Pool<Postgres> {
    dotenvy::dotenv().expect("Env error.");
    let db_url = env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set (check your .env file)");

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await
        .expect("Connecting to database failed.");

    database_init(pool.clone()).await;

    pool
}

async fn database_init(pool: Pool<Postgres>) {
    pool.execute(sqlx::query(
        "
        CREATE TABLE IF NOT EXISTS users (
            id SERIAL PRIMARY KEY,
            username TEXT NOT NULL UNIQUE,
            password TEXT NOT NULL,
            wins INTEGER DEFAULT 0,
            loses INTEGER DEFAULT 0,
            points INTEGER GENERATED ALWAYS AS (GREATEST(wins - loses)) STORED,
            token TEXT
    )
            "
    )).await.expect("Database failed in database_init.");

    pool.execute(sqlx::query(
            "
        CREATE TABLE IF NOT EXISTS games (
            id SERIAL PRIMARY KEY,
            player_x INTEGER NOT NULL,
            player_y INTEGER NOT NULL,
            board TEXT[],
            current_turn INTEGER NOT NULL,
            status TEXT NOT NULL
        )
                "
    )).await.expect("Database failed in database_init.");
}

pub async fn create_new_user(pool: Pool<Postgres>, log: &Login) -> bool {
    let does_exist: bool = does_user_exist(pool.clone(), &log).await;

    if !does_exist {
        let hashed_password = hash(&log.password, DEFAULT_COST).expect("Password hashing error.");

        sqlx::query("INSERT INTO users (username, password) VALUES ($1, $2)")
            .bind(&log.name)
            .bind(hashed_password)
            .execute(&pool)
            .await.expect("Inserting user error.");

        return true
    }

    false
}

pub async fn does_user_exist(pool: Pool<Postgres>, log: &Login) -> bool {
    sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE username = $1)")
        .bind(&log.name)
        .fetch_one(&pool)
        .await
        .expect("Checking if user exists error.")
}

pub async fn check_password(pool: Pool<Postgres>, log: &Login) -> (bool, String) {
    if does_user_exist(pool.clone(), &log).await {
        let result: String = sqlx::query_scalar("SELECT password FROM users WHERE username = $1")
            .bind(&log.name)
            .fetch_one(&pool)
            .await
            .expect("Error in password checking.");
        let token = new_token(pool.clone(), &log).await;
        (verify(&log.password, &result).expect("Hash verify error."), token)
    } else {
        (false, String::from(""))
    }
}

async fn new_token(pool: Pool<Postgres>, log: &Login) -> String {
    let token = uuid::Uuid::new_v4().to_string();
    sqlx::query("UPDATE users SET token = $1 WHERE username = $2")
        .bind(&token)
        .bind(&log.name)
        .execute(&pool)
        .await
        .expect("New_token function error");
    token
}

pub async fn does_token_exists(pool: Pool<Postgres>, token: &str) -> bool {
    sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE token = $1)")
        .bind(token)
        .fetch_one(&pool)
        .await
        .expect("Checking if token exists error.")
}

pub async fn player_from_token(pool: Pool<Postgres>, token: &str) -> Player {
    let username: String = sqlx::query_scalar("SELECT username FROM users WHERE token = $1")
        .bind(&token)
        .fetch_one(&pool)
        .await
        .expect("Error in username select in login_from_token function");

    let id: i32 = sqlx::query_scalar("SELECT id FROM users WHERE token = $1")
        .bind(&token)
        .fetch_one(&pool)
        .await
        .expect("Error in id select in login_from_token function");

    Player {
        name: username,
        id: id,
        token: String::from(token),
    }
}

pub async fn add_win_id(pool: Pool<Postgres>, id: i32) {
    sqlx::query("UPDATE users SET wins = wins + 1 WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .expect("Add win to database error.");
}

pub async fn add_lose_id(pool: Pool<Postgres>, id: i32) {
    sqlx::query("UPDATE users SET loses = loses + 1 WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await
        .expect("Add loose to database error.");
}

