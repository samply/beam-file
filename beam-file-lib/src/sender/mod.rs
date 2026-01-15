use beam_lib::{AppId, BeamClient};
use tokio::io::AsyncRead;
use crate::utils::config::SendSpec;

pub async fn send_file(
    beam_client: &BeamClient,
    beam_id: &beam_lib::AppId,
    mut stream: impl AsyncRead + Unpin,
    send_spec: SendSpec,
) -> anyhow::Result<()> {
    let full_to = AppId::new_unchecked(format!(
        "{}.{}",
        send_spec.to,
        beam_id
            .as_ref()
            .splitn(3, '.')
            .nth(2)
            .expect("Invalid app id")
    ));
    let mut conn = beam_client
        .create_socket_with_metadata(&full_to, send_spec.to_file_meta())
        .await?;
    tokio::io::copy(&mut stream, &mut conn).await?;
    Ok(())
}
