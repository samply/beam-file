use crate::utils::config::{Config, Mode, ReceiveMode};
#[cfg(feature = "server")]
use crate::utils::server;
use beam_file_lib::receiver::{forward_file, print_file, save_file};
use beam_file_lib::sender::send_file;
use beam_file_lib::utils::beam::{connect_socket, stream_tasks};
use beam_lib::BeamClient;
use clap::Parser;
use futures_util::{FutureExt, StreamExt, TryFutureExt, TryStreamExt};
use once_cell::sync::Lazy;
use reqwest::Client;
use std::process::ExitCode;
use tracing::{error, info};

mod utils;

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
async fn main() -> ExitCode {
    tracing_subscriber::fmt::init();
    let work = match &CONFIG.mode {
        Mode::Send(send_args) if send_args.file.to_string_lossy() == "-" => send_file(
            &BEAM_CLIENT,
            &CONFIG.beam_id,
            tokio::io::stdin(),
            send_args.to_spec(),
        )
        .boxed(),
        Mode::Send(send_args) => tokio::fs::File::open(&send_args.file)
            .err_into()
            .and_then(|f| send_file(&BEAM_CLIENT, &CONFIG.beam_id, f, send_args.to_spec()))
            .boxed(),
        Mode::Receive { count, mode } => stream_tasks(&BEAM_CLIENT)
            .and_then(|task| connect_socket(task, &BEAM_CLIENT))
            .inspect_ok(|(task, _)| error!("Receiving file from: {}", task.from))
            .and_then(move |(task, inc)| match mode {
                ReceiveMode::Print => print_file(task, inc).boxed(),
                ReceiveMode::Save { outdir, naming } => {
                    save_file(outdir, task, inc, naming).boxed()
                }
                ReceiveMode::Callback { url } => forward_file(task, inc, url, &CLIENT).boxed(),
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
        Mode::Server { bind_addr, api_key } => {
            server::serve(bind_addr, api_key, &BEAM_CLIENT, &CONFIG.beam_id).boxed()
        }
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
