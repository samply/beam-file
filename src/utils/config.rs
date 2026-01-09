use anyhow::anyhow;
use beam_lib::{reqwest::Url, AppId, BeamClient};
use clap::{Args, Parser, Subcommand, ValueHint};
use once_cell::sync::Lazy;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;

pub static CONFIG: Lazy<Config> = Lazy::new(Config::parse);

pub static BEAM_CLIENT: Lazy<BeamClient> = Lazy::new(|| {
    BeamClient::new(
        &CONFIG.beam_id,
        &CONFIG.beam_secret,
        CONFIG.beam_url.clone(),
    )
});
pub static CLIENT: Lazy<Client> = Lazy::new(Client::new);

/// Samply.Beam.File
#[derive(Debug, Parser)]
pub struct Config {
    /// Url of the local beam proxy which is required to have sockets enabled
    #[clap(env, long, default_value = "http://beam-proxy:8081")]
    pub beam_url: Url,

    /// Beam api key
    #[clap(env, long)]
    pub beam_secret: String,

    /// The app id of this application
    #[clap(long, env, value_parser = parse_beam_id)]
    pub beam_id: AppId,

    #[clap(subcommand)]
    pub mode: Mode,
}

#[derive(Subcommand, Debug)]
pub enum Mode {
    /// Send files
    Send(SendArgs),
    /// Receive files from other Samply.Beam.File instances
    Receive {
        #[clap(subcommand)]
        mode: ReceiveMode,

        /// Only receive count files
        #[clap(long, short = 'n', default_value_t = u32::MAX, hide_default_value = true)]
        count: u32,
    },
    #[cfg(feature = "server")]
    Server {
        /// Address the server should bind to
        #[clap(env, long, default_value = "0.0.0.0:8080")]
        bind_addr: std::net::SocketAddr,

        /// Api key required for uploading files
        #[clap(env, long)]
        api_key: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, Args)]
pub struct SendArgs {
    /// Name of the receiving beam app without broker id e.g. app1.proxy2
    #[clap(long)]
    #[serde(skip)]
    pub to: String,

    /// Name of the file to be read or '-' to read from stdin
    #[clap(value_hint = ValueHint::FilePath)]
    pub file: PathBuf,

    /// A suggestion for the new name the receiver should use. Will default to the uploaded files name if it is not read from stdin.
    #[clap(long)]
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,

    /// Additional metadata for the file
    #[clap(long)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
}

impl SendArgs {
    fn get_suggested_name(&self) -> Option<&str> {
        self.name
            .as_deref()
            .or_else(|| (self.file.as_os_str() != "-").then_some(self.file.file_name()?.to_str()?))
            .and_then(|v| validate_filename(v).ok())
    }

    pub fn to_file_meta(&self) -> FileMeta {
        FileMeta {
            suggested_name: self.get_suggested_name().map(ToOwned::to_owned),
            meta: self.meta.clone(),
        }
    }
}

#[derive(Subcommand, Debug)]
pub enum ReceiveMode {
    Print,
    Save {
        /// Directory files will be written to
        #[clap(long, short = 'o', value_hint = ValueHint::DirPath)]
        outdir: PathBuf,

        /// Naming scheme used to save files. %t -> unix timestamp %f beam app id e.g. app1.proxy2 %n suggested name from upload see send arguments
        #[clap(long, short = 'p', default_value = "%f_%t")]
        naming: String,
    },
    Callback {
        /// A url to an endpoint that will be called when we are receiving a new file
        #[clap(value_hint = ValueHint::Url)]
        url: Url,
    },
}

fn parse_beam_id(id: &str) -> Result<AppId, String> {
    match id.split('.').collect::<Vec<_>>().as_slice() {
        [app, proxy, broker] if !app.is_empty() && !proxy.is_empty() && !broker.is_empty() => {
            Ok(AppId::new_unchecked(id))
        }
        _ => Err("beam-id must be <app>.<proxy>.<broker>".into()),
    }
}

fn validate_filename(name: &str) -> anyhow::Result<&str> {
    if name
        .chars()
        .all(|c| c.is_alphanumeric() || ['_', '.', '-'].contains(&c))
    {
        Ok(name)
    } else {
        Err(anyhow!("Invalid filename: {name}"))
    }
}

fn deserialize_filename<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> anyhow::Result<Option<String>, D::Error> {
    let s = Option::<String>::deserialize(deserializer)?;
    if let Some(ref f) = s {
        validate_filename(f).map_err(serde::de::Error::custom)?;
    }
    Ok(s)
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FileMeta {
    #[serde(deserialize_with = "deserialize_filename")]
    pub suggested_name: Option<String>,

    pub meta: Option<serde_json::Value>,
}
