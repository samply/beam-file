use anyhow::{bail, Context};
use beam_lib::SocketTask;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use reqwest::{Client, Url};
use std::path::Path;
use std::time::SystemTime;
use sync_wrapper::SyncStream;
use tokio::io::AsyncRead;
use tokio_util::io::ReaderStream;
use tracing::info;
use crate::utils::config::FileMeta;

pub async fn save_file(
    dir: &Path,
    socket_task: SocketTask,
    mut incoming: impl AsyncRead + Unpin,
    naming_scheme: &str,
) -> anyhow::Result<()> {
    let ts = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let from = socket_task
        .from
        .as_ref()
        .split('.')
        .take(2)
        .collect::<Vec<_>>()
        .join(".");
    let meta: FileMeta =
        serde_json::from_value(socket_task.metadata).context("Failed to deserialize metadata")?;
    let filename = naming_scheme
        .replace("%f", &from)
        .replace("%t", &ts.to_string())
        // Save because deserialize implementation of suggested_name does path traversal check
        .replace("%n", meta.suggested_name.as_deref().unwrap_or(""));
    let mut file = tokio::fs::File::create(dir.join(filename)).await?;
    tokio::io::copy(&mut incoming, &mut file).await?;
    Ok(())
}

pub async fn print_file(
    socket_task: SocketTask,
    mut incoming: impl AsyncRead + Unpin,
) -> anyhow::Result<()> {
    info!("Incoming file from {}", socket_task.from);
    tokio::io::copy(&mut incoming, &mut tokio::io::stdout()).await?;
    info!("Done printing file from {}", socket_task.from);
    Ok(())
}

pub async fn forward_file(
    socket_task: SocketTask,
    incoming: impl AsyncRead + Unpin + Send + 'static,
    cb: &Url,
    client: Client,
) -> anyhow::Result<()> {
    let FileMeta {
        suggested_name,
        meta,
    } = serde_json::from_value(socket_task.metadata).context("Failed to deserialize metadata")?;
    let mut headers = HeaderMap::with_capacity(2);
    if let Some(meta) = meta {
        headers.append(
            HeaderName::from_static("metadata"),
            HeaderValue::from_bytes(&serde_json::to_vec(&meta)?)?,
        );
    }
    if let Some(name) = suggested_name {
        headers.append(
            HeaderName::from_static("filename"),
            HeaderValue::from_str(&name)?,
        );
    }
    let res = client
        .post(cb.clone())
        .headers(headers)
        .body(reqwest::Body::wrap_stream(SyncStream::new(
            ReaderStream::new(incoming),
        )))
        .send()
        .await;
    match res {
        Ok(r) if !r.status().is_success() => bail!(
            "Got unsuccessful status code from callback server: {}",
            r.status()
        ),
        Err(e) => bail!("Failed to send file to {cb}: {e}"),
        _ => Ok(()),
    }
}
