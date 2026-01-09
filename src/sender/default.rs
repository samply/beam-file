use crate::utils::config::{SendArgs, BEAM_CLIENT, CONFIG};
use beam_lib::AppId;
use tokio::io::AsyncRead;

pub async fn send_file(
    mut stream: impl AsyncRead + Unpin,
    meta @ SendArgs { to, .. }: &SendArgs,
) -> anyhow::Result<()> {
    let full_to = AppId::new_unchecked(format!(
        "{to}.{}",
        CONFIG
            .beam_id
            .as_ref()
            .rsplit('.')
            .next()
            .expect("AppId invalid"),
    ));
    let mut conn = BEAM_CLIENT
        .create_socket_with_metadata(&full_to, meta.to_file_meta())
        .await?;
    tokio::io::copy(&mut stream, &mut conn).await?;
    Ok(())
}
