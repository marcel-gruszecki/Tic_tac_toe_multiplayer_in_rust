use std::cmp::PartialEq;
use axum::{routing::{get, post}, http::StatusCode, Json, Router, Error};
use axum::extract::State;
use axum::extract::ws::{Message, Utf8Bytes};
use axum::extract::ws::WebSocket;
use axum::extract::ws::WebSocketUpgrade;
use axum::response::IntoResponse;
use serde::{Deserialize, Serialize};
use serde_json;
use sqlx::{Pool, Postgres};
use tokio::sync::oneshot;
use crate::AppMod;
use crate::database::{add_lose_id, add_win_id, does_token_exists, player_from_token};

const WINNING_COMBINATIONS: [[usize; 3]; 8] = [
    [0, 1, 2], [3, 4, 5], [6, 7, 8],
    [0, 3, 6], [1, 4, 7], [2, 5, 8],
    [0, 4, 8], [2, 4, 6]
];

pub struct Player {
    id: i32,
    name: String,
    token: String,
    response: SerwerResponse,
    socket: WebSocket,
}

impl Player {
    async fn new(socket: WebSocket, token: String, pool: Pool<Postgres>) -> Self {
        let (id, name) = player_from_token(pool.clone(), &token).await;
        Self {
            id,
            name,
            token,
            socket,
            response: SerwerResponse::new(),
        }
    }
}
#[derive(Deserialize, Serialize, Clone, Debug, Copy, PartialEq)]
enum BoardOptions {
    X,
    O,
    Null,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
struct Move {
    field: usize,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
struct Game {
    board: [BoardOptions; 9],
    current_move: BoardOptions,
}

impl Default for Game {
    fn default() -> Self {
        Self {
            board: [BoardOptions::Null; 9],
            current_move: BoardOptions::O,
        }
    }
}

#[derive(Deserialize, Serialize, Clone, Debug)]
enum MoveResponse {
    Accepted,
    Refused,
    OtherPlayer,
    Waiting,
}

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
enum Status {
    InGame,
    Player1Won,
    Player2Won,
    Draw,
    Error,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
struct SerwerResponse {
    game: Game,
    response: MoveResponse,
    status: Status,
    your_symbol: BoardOptions,
}

impl SerwerResponse {
    fn new() -> Self {
        Self {
            game: Game::default(),
            response: MoveResponse::Waiting,
            status: Status::InGame,
            your_symbol: BoardOptions::Null,
        }
    }
    fn first_response_player1() -> Self {
        Self {
            game: Game::default(),
            response: MoveResponse::Waiting,
            status: Status::InGame,
            your_symbol: BoardOptions::O,
        }
    }

    fn first_response_player2() -> Self {
        Self {
            game: Game::default(),
            response: MoveResponse::Waiting,
            status: Status::InGame,
            your_symbol: BoardOptions::X,
        }
    }
}

#[derive(Deserialize)]
struct TokenRequest {
    token: String,
}

pub async fn websocket_connect(ws: WebSocketUpgrade, State(appmod): State<AppMod>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| search_game(socket, appmod))
}

async fn search_game(mut socket: WebSocket, appmod: AppMod) {
    let msg = match socket.recv().await {
        Some(Ok(Message::Text(t))) => t,
        _ => {
            eprintln!("Websocket connection problem in search_game function");
            return;
        }
    };

    let token_data: TokenRequest = match serde_json::from_str(&msg) {
        Ok(data) => data,
        Err(_) => {
            eprintln!("Wrong JSON format");
            return;
        }
    };

    let token = token_data.token;
    let pool = appmod.pool.clone();
    if !does_token_exists(pool.clone(), &token).await {
        eprintln!("Token doesn't exist");
        return;
    }

    let mut player = Player::new(socket, token, pool.clone()).await;

    let mut rx_to_wait = None;

    {
        let mut queue = appmod.queue.lock().unwrap();

        if let Some(tx) = queue.pop_front() {
            let _ = tx.send(player);
            return;
        } else {
            let (tx, rx) = oneshot::channel::<Player>();
            queue.push_back(tx);
            rx_to_wait = Some(rx);
        }
    }

    if let Some(rx) = rx_to_wait {
        if let Ok(mut opponent) = rx.await {
            player.response = SerwerResponse::first_response_player1();
            opponent.response = SerwerResponse::first_response_player2();

            game(player, opponent, pool.clone()).await;
        }
    }
}

async fn game(mut player1: Player, mut player2: Player, pool: Pool<Postgres>) {
    let player1 = &mut player1;
    let player2 = &mut player2;

    match full_send(player1, player2, pool.clone()).await {
        Ok(_) => {}
        Err(err) => { eprintln!("{} disconnected", player1.name); return }
    };

    match full_send(player2, player1, pool.clone()).await {
        Ok(_) => {}
        Err(err) => { eprintln!("{} disconnected", player2.name); return }
    };

    loop {
        tokio::select! {
            result1 = player1.socket.recv() => {
                match player_handler(player1, player2, &result1, pool.clone()).await {
                    Ok(_) => {
                        if player1.response.status != Status::InGame { break }
                        if player2.response.status != Status::InGame { break }
                    }
                    Err(err) => {
                        break;
                    }
                }
            }

            result2 = player2.socket.recv() => {
                match player_handler(player2, player1, &result2, pool.clone()).await {
                    Ok(_) => {
                        if player1.response.status != Status::InGame { break }
                        if player2.response.status != Status::InGame { break }
                    }
                    Err(err) => {
                        break;
                    }
                }
            }
        }
    }
}

async fn player_handler(sender: &mut Player, waiting_player: &mut Player, result: &Option<Result<Message, Error>>, pool: Pool<Postgres>) -> Result<(), Error> {
    match result {
        Some(Ok(message)) => {
            match message.to_text() {
                Ok(text) => {
                    match serde_json::from_str::<Move>(text) {
                        Ok(player_move) => {
                            make_a_move(player_move, &mut sender.response, &mut waiting_player.response);

                            match full_send(sender, waiting_player, pool.clone()).await {
                                Ok(_) => {}
                                Err(err) => {
                                    eprintln!("{} disconnected", sender.name);
                                    return Err(err)
                                }
                            };

                            match full_send(waiting_player, sender, pool.clone()).await {
                                Ok(_) => {}
                                Err(err) => {
                                    eprintln!("{} disconnected", waiting_player.name);
                                    return Err(err)
                                }
                            };

                            Ok(())
                        }
                        Err(err) => {
                            sender.response.response = MoveResponse::Refused;
                            match full_send(sender, waiting_player, pool.clone()).await {
                                Ok(_) => {}
                                Err(err) => {
                                    eprintln!("{} sent an wrong JSON format", waiting_player.name);
                                    return Err(err)
                                }
                            };
                            Ok(())
                        }
                    }
                }
                Err(err) => {
                    Err(err)
                }
            }
        }

        _ => {
            eprintln!("{} lost connection", waiting_player.name);
            waiting_player.response.status = Status::Error;
            add_win_id(pool.clone(), waiting_player.id).await;
            add_lose_id(pool.clone(), sender.id).await;
            let _ = send_json(&mut waiting_player.socket, &waiting_player.response).await;
            Err(Error::new("Player disconnected or invalid state"))
        }
    }
}

fn make_a_move(from_user: Move, current_player: &mut SerwerResponse, waiting_player: &mut SerwerResponse) {
    if current_player.your_symbol != current_player.game.current_move {
        current_player.status = Status::InGame;
        current_player.response = MoveResponse::Refused;
        return
    }

    if current_player.status != Status::InGame {
        current_player.response = MoveResponse::Refused;
        return
    }

    let board = &mut current_player.game.board;
    let symbol = current_player.your_symbol;

    if from_user.field > 8 || from_user.field < 0 || board[from_user.field] != BoardOptions::Null  {
        current_player.response = MoveResponse::Refused;
        return
    }

    board[from_user.field] = symbol;

    let status = check_winner(board);

    if status != Status::InGame {
        current_player.game.current_move = BoardOptions::Null;
        waiting_player.game.current_move = BoardOptions::Null;
    } else {
        if current_player.game.current_move == BoardOptions::O {
            current_player.game.current_move = BoardOptions::X;
            waiting_player.game.current_move = BoardOptions::X;
        } else {
            current_player.game.current_move = BoardOptions::O;
            waiting_player.game.current_move = BoardOptions::O;
        }
    }

    current_player.status = status.clone();
    waiting_player.status = status.clone();

    waiting_player.game.board = current_player.game.board.clone();
    waiting_player.response = MoveResponse::Waiting;

    current_player.response = MoveResponse::Accepted;
}

fn check_winner(board: &[BoardOptions; 9]) -> Status {
    for combo in WINNING_COMBINATIONS {
        let [a, b, c] = combo;
        if board[a] != BoardOptions::Null && board[a] == board[b] && board[a] == board[c] {
            return if board[a] == BoardOptions::O {
                Status::Player1Won
            } else {
                Status::Player2Won
            };
        }
    }

    let mut is_draw = true;

    for i in board {
        if *i == BoardOptions::Null {
            is_draw = false;
            break
        }
    }

    if is_draw {
        Status::Draw
    } else {
        Status::InGame
    }
}

async fn full_send(receiver: &mut Player, waiting_player: &mut Player, pool: Pool<Postgres>) -> Result<(), Error> {
    match send_json(&mut receiver.socket, &receiver.response).await {
        Ok(_) => {Ok(())}
        Err(err) => {
            eprintln!("{} coudn't recieve message.", receiver.name);
            waiting_player.response.status = Status::Error;
            add_win_id(pool.clone(), waiting_player.id).await;
            add_lose_id(pool.clone(), receiver.id).await;
            Err(err)
        }
    }
}
async fn send_json<T: serde::Serialize>(socket: &mut WebSocket, from_struct: &T) -> Result<(), axum::Error> {
    let response_json = serde_json::to_string(&from_struct)
        .map_err(axum::Error::new)?;

    socket.send(Message::Text(response_json.into())).await?;

    Ok(())
}