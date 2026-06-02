use axum::extract::ws::{Message, WebSocket};
use axum::{
    extract::{State, WebSocketUpgrade},
    response::Response,
};
use std::sync::Arc;
use tokio::sync::broadcast;

use crate::routes::AppState;

pub async fn ws_handler(ws: WebSocketUpgrade, State(app): State<Arc<AppState>>) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, app))
}

async fn handle_socket(mut socket: WebSocket, app: Arc<AppState>) {
    // Grab a snapshot receiver from the current run, if any.
    let mut rx: Option<broadcast::Receiver<String>> = {
        let lock = app.handle.lock().await;
        lock.as_ref().map(|h| h.snapshot_rx.resubscribe())
    };

    loop {
        // If we don't have a receiver yet, poll for a new run every 200 ms.
        if rx.is_none() {
            tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
            let lock = app.handle.lock().await;
            rx = lock.as_ref().map(|h| h.snapshot_rx.resubscribe());
            continue;
        }

        let receiver = rx.as_mut().unwrap();
        match receiver.recv().await {
            Ok(json) => {
                if socket.send(Message::Text(json.into())).await.is_err() {
                    break; // client disconnected
                }
            }
            Err(broadcast::error::RecvError::Lagged(n)) => {
                tracing::warn!("WebSocket client lagged by {n} messages");
                // Keep going — just skip missed frames
            }
            Err(broadcast::error::RecvError::Closed) => {
                break; // simulation ended
            }
        }
    }
}
