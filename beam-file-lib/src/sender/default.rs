use beam_lib::{AppId, BeamClient};
use tokio::io::AsyncRead;
use crate::utils::config::{BeamRuntime, Context };

pub async fn send_file(
    mut stream: impl AsyncRead + Unpin,
    context: &Context,
    beam_client: BeamClient,
    beam_runtime: BeamRuntime,
) -> anyhow::Result<()> {
    let full_to = AppId::new_unchecked(format!(
        "{}.{}",
        context.to,
        beam_runtime
            .beam_id
            .as_ref()
            .rsplit('.')
            .next()
            .expect("AppId invalid"),
    ));
    let mut conn = beam_client
        .create_socket_with_metadata(&full_to, context.to_file_meta())
        .await?;
    tokio::io::copy(&mut stream, &mut conn).await?;
    Ok(())
}
