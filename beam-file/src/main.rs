use std::process::ExitCode;
use futures_util::FutureExt;
use beam_file_lib::sender::default::send_file;
use crate::utils::config::{Mode, CONFIG};

mod utils;

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt::init();
    let work = match &CONFIG.mode {
        Mode::Send(send_args) if send_args.file.to_string_lossy() == "-" => {
            send_file(tokio::io::stdin(), send_args).boxed()
        }
        Mode::Send(send_args) => tokio::fs::File::open(&send_args.file)
            .err_into()
            .and_then(|f| send_file(f, send_args))
            .boxed(),
        Mode::Receive { count, mode } => stream_tasks()
            .and_then(connect_socket)
            .inspect_ok(|(task, _)| error!("Receiving file from: {}", task.from))
            .and_then(move |(task, inc)| match mode {
                ReceiveMode::Print => print_file(task, inc).boxed(),
                ReceiveMode::Save { outdir, naming } => {
                    save_file(outdir, task, inc, naming).boxed()
                }
                ReceiveMode::Callback { url } => forward_file(task, inc, url).boxed(),
            })
            .take(*count as usize)
            .for_each(|v| {
                if let Err(e) = v {
                    error!("{e}");
                }
                futures_util::future::ready(())
            })
            .map(Ok)
            .boxed(),
        #[cfg(feature = "server")]
        Mode::Server { bind_addr, api_key } => server::serve(bind_addr, api_key).boxed(),
    };
    let result = tokio::select! {
        res = work => res,
        _ = tokio::signal::ctrl_c() => {
            info!("Shutting down");
            return ExitCode::from(130);
        }
    };
    if let Err(e) = result {
        error!("Failure: {e}");
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

pub fn stream_tasks() -> impl Stream<Item = anyhow::Result<SocketTask>> {
    static BLOCK: Lazy<BlockingOptions> = Lazy::new(|| BlockingOptions::from_count(1));
    futures_util::stream::repeat_with(move || BEAM_CLIENT.get_socket_tasks(&BLOCK)).filter_map(
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

pub async fn connect_socket(socket_task: SocketTask) -> anyhow::Result<(SocketTask, Upgraded)> {
    let id = socket_task.id;
    Ok((
        socket_task,
        BEAM_CLIENT
            .connect_socket(&id)
            .await
            .with_context(|| format!("Failed to connect to socket {id}"))?,
    ))
}
