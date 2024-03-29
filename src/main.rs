mod config;

use std::{io, pin::pin, time::Duration};

use axum::{
    extract::{BodyStream, Path}, headers::{authorization, Authorization}, http::{header, HeaderMap, HeaderName, StatusCode}, routing::post, Router, TypedHeader
};
use beam_lib::{AppId, BeamClient, BlockingOptions, SocketTask};
use clap::Parser;
use config::Config;
use futures_util::stream::TryStreamExt;
use once_cell::sync::Lazy;
use reqwest::{Client, Url};
use serde::{Deserialize, Serialize};
use tokio_util::io::{ReaderStream, StreamReader};

pub static CONFIG: Lazy<Config> = Lazy::new(Config::parse);

pub static BEAM_CLIENT: Lazy<BeamClient> = Lazy::new(|| {
    BeamClient::new(
        &CONFIG.beam_id,
        &CONFIG.beam_secret,
        CONFIG.beam_url.clone(),
    )
});
pub static CLIENT: Lazy<Client> = Lazy::new(Client::new);

#[tokio::main]
async fn main() {
    let app = Router::new().route("/send/:to", post(send_file));
    let server = axum::Server::bind(&CONFIG.bind_addr)
        .serve(app.into_make_service())
        .with_graceful_shutdown(async { tokio::signal::ctrl_c().await.unwrap() });
    let (server_res, _) = tokio::join!(server, wait_for_files(&CONFIG));
    if let Err(e) = server_res {
        eprintln!("Server errored: {e}");
    }
}

pub async fn wait_for_files(config: &Config) {
    let Some(ref cb) = config.callback else {
        println!("No callback url registered only sending files.");
        return;
    };
    let mut abort = pin!(tokio::signal::ctrl_c());
    let block = BlockingOptions::from_count(1);
    loop {
        let socket_task_results = tokio::select! {
            _ = &mut abort => {
                println!("Shutting down gracefully");
                break;
            }
            res = BEAM_CLIENT.get_socket_tasks(&block) => {
                res
            }
        };
        let tasks = match socket_task_results {
            Ok(tasks) => tasks,
            Err(e) => {
                eprintln!("Error getting tasks from beam: {e}");
                tokio::time::sleep(Duration::from_secs(5)).await;
                continue;
            }
        };
        for task in tasks {
            forward_file(task, cb).await;
        }
    }
}

pub async fn forward_file(socket_task: SocketTask, cb: &Url) {
    let FileMetadata { related_headers } = serde_json::from_value(socket_task.metadata).expect("We only ever create this ourselves");
    let incoming = match BEAM_CLIENT.connect_socket(&socket_task.id).await {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Failed to connect to socket: {e}");
            return;
        }
    };
    let res = CLIENT
        .post(cb.clone())
        .headers(related_headers)
        .body(reqwest::Body::wrap_stream(ReaderStream::new(incoming)))
        .send()
        .await;
    match res {
        Ok(r) if !r.status().is_success() => eprintln!(
            "Got unsuccessful status code from callback server: {}",
            r.status()
        ),
        Err(e) => eprintln!("Failed to send file to {cb}: {e}"),
        _ => {}
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FileMetadata {
    #[serde(with = "http_serde::header_map")]
    related_headers: HeaderMap,
}

async fn send_file(
    Path(other_proxy_name): Path<String>,
    auth: TypedHeader<Authorization<authorization::Basic>>,
    headers: HeaderMap,
    body: BodyStream,
) -> Result<(), StatusCode> {
    if auth.password() != CONFIG.api_key {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let to = AppId::new_unchecked(format!(
        "{}.{other_proxy_name}.{}",
        CONFIG.beam_id.app_name(),
        CONFIG.beam_id.as_ref().splitn(3, '.').nth(2).expect("Invalid app id")
    ));
    const RELEVANT_HEADERS: [HeaderName; 5] = [
        header::CONTENT_LENGTH,
        header::CONTENT_DISPOSITION,
        header::CONTENT_ENCODING,
        header::CONTENT_TYPE,
        header::HeaderName::from_static("metadata")
    ];
    let related_headers = headers
        .into_iter()
        .filter_map(|(maybe_k, v)| {
            if let Some(k) = maybe_k {
                RELEVANT_HEADERS.contains(&k).then_some((k, v))
            } else {
                None
            }
        })
        .collect();
    let mut conn = BEAM_CLIENT
        .create_socket_with_metadata(&to, FileMetadata { related_headers })
        .await
        .map_err(|e| {
            eprintln!("Failed to tunnel request: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    tokio::spawn(async move {
        let mut reader = StreamReader::new(body.map_err(|err| io::Error::new(io::ErrorKind::Other, err)));
        if let Err(e) = tokio::io::copy(&mut reader, &mut conn).await {
            // TODO: Some of these are normal find out which
            eprintln!("Error sending file: {e}")
        }
    });
    Ok(())
}
