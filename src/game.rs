use std::cmp::PartialEq;
use axum::{routing::{get, post}, http::StatusCode, Json, Router, Error};
use axum::extract::State;
use axum::extract::ws::{Message, Utf8Bytes};
use axum::extract::ws::WebSocket;
use axum::extract::ws::WebSocketUpgrade;
use axum::http::header::ACCEPT;
use axum::response::IntoResponse;
use serde::{Deserialize, Serialize};
use serde_json;
use sqlx::{Pool, Postgres};
use tokio::sync::oneshot;
use crate::AppMod;
use crate::database::{add_lose_id, add_win_id, does_token_exists, player_from_token};
use crate::game::BoardOptions::Null;

const WINNING_COMBINATIONS: [[usize; 3]; 8] = [
    [0, 1, 2], [3, 4, 5], [6, 7, 8],
    [0, 3, 6], [1, 4, 7], [2, 5, 8],
    [0, 4, 8], [2, 4, 6]
];

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Player {
    pub id: i32,
    pub name: String,
    pub token: String,
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
    pub fn first_response_player1() -> Self {
        Self {
            game: Game::default(),
            response: MoveResponse::Waiting,
            status: Status::InGame,
            your_symbol: BoardOptions::O,
        }
    }

    pub fn first_response_player2() -> Self {
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
            eprintln!("Problem z połączeniem lub brak wiadomości");
            return;
        }
    };

    let token_data: TokenRequest = match serde_json::from_str(&msg) {
        Ok(data) => data,
        Err(_) => {
            eprintln!("Otrzymano błędny format JSON zamiast tokena");
            return;
        }
    };

    if !does_token_exists(appmod.pool.clone(), &token_data.token).await {
        eprintln!("WebSocket function: token {} doesn't exist", token_data.token);
        return;
    }

    let player = player_from_token(appmod.pool.clone(), &token_data.token).await;

    let mut rx_to_wait = None;

    {
        let mut q = appmod.queue.lock().unwrap();

        if let Some(tx) = q.pop_front() {
            let _ = tx.send((socket, player));
            return;
        } else {
            let (tx, rx) = oneshot::channel::<(WebSocket, Player)>();
            q.push_back(tx);
            rx_to_wait = Some(rx);
        }
    }

    if let Some(rx) = rx_to_wait {
        if let Ok(opponent_socket) = rx.await {
            game(socket, opponent_socket.0, player, opponent_socket.1, appmod.pool.clone()).await;
        }
    }
}

async fn game(mut player1: WebSocket, mut player2: WebSocket, player1_info: Player, player2_info: Player, pool: Pool<Postgres>) {
    //player1 = O, player2 = X
    let mut player1_response = SerwerResponse::first_response_player1();
    let mut player2_response = SerwerResponse::first_response_player2();

    match serde_json::to_string(&player1_response) {
        Ok(json_res) => {let _ = player1.send(Message::Text(json_res.into())).await;}
        Err(err) => {
            eprintln!("Coudn't send first message to player1");
            player2_response.status = Status::Error;
        }
    }

    match serde_json::to_string(&player2_response) {
        Ok(json_res) => {let _ = player2.send(Message::Text(json_res.into())).await;}
        Err(err) => {
            eprintln!("Coudn't send first message to player2");
            player1_response.status = Status::Error;
        }
    }

    if player1_response.status == Status::Error || player2_response.status == Status::Error { return }

    loop {
        tokio::select! {
            res1 = player1.recv() => {
                match res1 {
                    Some(Ok(msg1)) => {
                        if let Ok(text) = msg1.to_text() {
                            match serde_json::from_str::<Move>(text) {
                                Ok(player_move) => {
                                    make_a_move(player_move, &mut player1_response, &mut player2_response);

                                    match send_json(&mut player1, &player1_response).await {
                                        Ok(_) => {}
                                        Err(err) => {
                                            eprintln!("Player1 sending error in send_json function");
                                            player2_response.status = Status::Error;
                                        }
                                    }

                                    match send_json(&mut player2, &player2_response).await {
                                        Ok(_) => {}
                                        Err(err) => {
                                            eprintln!("Player1 sending error in send_json function");
                                            player1_response.status = Status::Error;
                                        }
                                    }

                                    if player1_response.status != Status::InGame { break; }
                                }
                                Err(e) => {
                                    eprintln!("Wrong JSON format")
                                }
                            }
                        }
                    }

                    _ => {
                        player2_response.status = Status::Error;
                        add_win_id(pool.clone(), player1_info.id).await;
                        add_lose_id(pool.clone(), player2_info.id).await;
                        let _ = send_json(&mut player2, &player2_response).await;
                        break;
                    }
                }
            }

            res2 = player2.recv() => {
                match res2 {
                    Some(Ok(msg2)) => {
                        if let Ok(text) = msg2.to_text() {
                            match serde_json::from_str::<Move>(text) {
                                Ok(player_move) => {
                                    make_a_move(player_move, &mut player2_response, &mut player1_response);

                                    match send_json(&mut player2, &player2_response).await {
                                        Ok(_) => {}
                                        Err(_err) => {
                                            eprintln!("Player2 sending error in send_json function");
                                            player1_response.status = Status::Error;
                                        }
                                    }

                                    match send_json(&mut player1, &player1_response).await {
                                        Ok(_) => {}
                                        Err(_err) => {
                                            eprintln!("Player1 sending error in send_json function while P2 moved");
                                            player2_response.status = Status::Error;
                                        }
                                    }

                                    if player2_response.status != Status::InGame { break; }
                                }
                                Err(e) => {
                                    eprintln!("Wrong JSON format from Player 2: {:?}", e);
                                }
                            }
                        }
                    }
                    _ => {
                        eprintln!("Player 2 disconnected");
                        player1_response.status = Status::Error;
                        add_win_id(pool.clone(), player2_info.id).await;
                        add_lose_id(pool.clone(), player1_info.id).await;
                        let _ = send_json(&mut player1, &player1_response).await;
                        break;
                    }
                }
            }
        }
    }

    if player1_response.status == Status::Player1Won && player1_response.status == player2_response.status {
        println!("player1 won");
        add_win_id(pool.clone(), player1_info.id).await;
        add_lose_id(pool.clone(), player2_info.id).await;
    }

    if player2_response.status == Status::Player2Won && player1_response.status == player2_response.status {
        add_win_id(pool.clone(), player2_info.id).await;
        add_lose_id(pool.clone(), player1_info.id).await;
    }
}

async fn send_json<T: serde::Serialize>(socket: &mut WebSocket, from_struct: &T) -> Result<(), axum::Error> {
    let response_json = serde_json::to_string(&from_struct)
        .map_err(axum::Error::new)?;

    socket.send(Message::Text(response_json.into())).await?;

    Ok(())
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

