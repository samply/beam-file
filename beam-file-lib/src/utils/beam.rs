use anyhow::Context;
use beam_lib::{BeamClient, BlockingOptions, SocketTask};
use futures_util::{Stream, StreamExt};
use once_cell::sync::Lazy;
use reqwest::Upgraded;

pub fn stream_tasks(
    beam_client: &BeamClient,
) -> impl Stream<Item = anyhow::Result<SocketTask>> + use<'_> {
    static BLOCK: Lazy<BlockingOptions> = Lazy::new(|| BlockingOptions::from_count(1));
    futures_util::stream::repeat_with(move || beam_client.get_socket_tasks(&BLOCK)).filter_map(
        |v| async {
            match v.await {
                Ok(mut v) => Some(Ok(v.pop()?)),
                Err(e) => Some(
                    Err(anyhow::Error::from(e)).context("Failed to get socket tasks from beam"),
                ),
            }
        },
    )
}

pub async fn connect_socket(
    socket_task: SocketTask,
    beam_client: &BeamClient,
) -> anyhow::Result<(SocketTask, Upgraded)> {
    let id = socket_task.id;
    Ok((
        socket_task,
        beam_client
            .connect_socket(&id)
            .await
            .with_context(|| format!("Failed to connect to socket {id}"))?,
    ))
}
