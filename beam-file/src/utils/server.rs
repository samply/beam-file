use axum::{
    extract::{Path, Request, State},
    http::{HeaderMap, StatusCode},
    routing::post,
    Router,
};
use axum_extra::{
    headers::{authorization, Authorization},
    TypedHeader,
};
use beam_file_lib::utils::config::FileMeta;
use beam_lib::{AppId, BeamClient};
use futures_util::TryStreamExt as _;
use std::{io, net::SocketAddr, sync::Arc};
use tokio::net::TcpListener;
use tokio_util::io::StreamReader;
use tracing::error;

#[derive(Clone)]
struct AppState {
    api_key: Arc<str>,
    beam_client: Arc<BeamClient>,
    beam_id: AppId,
}

pub async fn serve(
    addr: &SocketAddr,
    api_key: &str,
    beam_client: &BeamClient,
    beam_id: &AppId,
) -> anyhow::Result<()> {
    let state = AppState {
        api_key: Arc::from(api_key),
        beam_client: Arc::new(beam_client.clone()),
        beam_id: beam_id.clone(),
    };

    let app = Router::new()
        .route("/send/{to}", post(send_file))
        .with_state(state);

    axum::serve(TcpListener::bind(addr).await?, app.into_make_service())
        .with_graceful_shutdown(async { tokio::signal::ctrl_c().await.unwrap() })
        .await?;

    Ok(())
}

async fn send_file(
    Path(other_proxy_name): Path<String>,
    auth: TypedHeader<Authorization<authorization::Basic>>,
    headers: HeaderMap,
    State(state): State<AppState>,
    req: Request,
) -> Result<(), StatusCode> {
    if auth.password() != state.api_key.as_ref() {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let to = AppId::new_unchecked(format!(
        "{other_proxy_name}.{}",
        state
            .beam_id
            .as_ref()
            .splitn(3, '.')
            .nth(2)
            .expect("Invalid app id")
    ));
    let mut conn = state
        .beam_client
        .create_socket_with_metadata(
            &to,
            FileMeta {
                meta: headers.get("metadata").and_then(|v| {
                    serde_json::from_slice(v.as_bytes())
                        .map_err(|e| {
                            error!("Failed to deserialize metadata: {e}. Skipping metadata")
                        })
                        .ok()
                }),
                suggested_name: headers
                    .get("filename")
                    .and_then(|v| v.to_str().map(Into::into).ok()),
            },
        )
        .await
        .map_err(|e| {
            error!("Failed to tunnel request: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    tokio::spawn(async move {
        let mut reader = StreamReader::new(
            req.into_body()
                .into_data_stream()
                .map_err(|err| io::Error::new(io::ErrorKind::Other, err)),
        );
        if let Err(e) = tokio::io::copy(&mut reader, &mut conn).await {
            // TODO: Some of these are normal find out which
            error!("Error sending file: {e}")
        }
    });
    Ok(())
}
