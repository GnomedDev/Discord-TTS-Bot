#![warn(clippy::pedantic)]

use std::{
    net::{Ipv4Addr, SocketAddrV4},
    sync::Arc,
};

use axum::routing::get;
use tokio::net::TcpListener;

use tts_core::{require, structs::Data};

pub async fn serve_activity_adapter(data: Arc<Data>) {
    let port = require!(data.config.activity_port);
    let route_handler = move |oauth_code| {
        let data = Arc::clone(&data);
        connection_handler(data, oauth_code)
    };

    let router = axum::Router::new().route("/", get(route_handler));

    let ip_addr = Ipv4Addr::new(0, 0, 0, 0);
    let socket_addr = SocketAddrV4::new(ip_addr, port.get());
    let tcp_listener = match TcpListener::bind(socket_addr).await {
        Ok(val) => val,
        Err(err) => {
            tracing::warn!("Failed to bind TcpListener to port {}: {err}", port.get());
            return;
        }
    };

    if let Err(err) = axum::serve(tcp_listener, router.into_make_service()).await {
        tracing::warn!("Failed to serve axum server: {err}");
    };
}

#[derive(Debug, thiserror::Error)]
enum Error {
    #[error("Failed to verify your discord account login.")]
    FailedOauthFlow(reqwest::Error),
}

impl axum::response::IntoResponse for Error {
    fn into_response(self) -> axum::response::Response {
        let status_code = match self {
            Error::FailedOauthFlow(_) => {}
        };

        todo!()
    }
}

async fn connection_handler(data: Arc<Data>, oauth_code: String) {
    todo!()
}
