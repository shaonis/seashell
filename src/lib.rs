pub(crate) mod cli {
    pub mod control;
    pub mod output;
    pub mod parser;
}
pub(crate) mod storage {
    pub mod config;
    pub mod context;
    pub mod provider;
}
pub(crate) mod client {
    pub mod connect;
    pub mod data;
    pub mod handler;
}
pub(crate) mod error;

pub use crate::cli::control::start_cli;
use crate::cli::output::TestOutput;
use crate::cli::parser::{Cli, CliSubcommand, ServerUri};
use crate::client::connect::initiate_connection;
use crate::client::data::ConnectionData;
use crate::error::CliError;
use crate::storage::config::{Config, Scope, Server, ServerEntry};
use crate::storage::context::Context;
use crate::storage::provider::{CONFIG_PATH, StorageProvider, ensure_work_dir};
use clap::CommandFactory;
use clap_complete::generate;
use log::info;
use regex_lite::Regex;
use std::fmt::Display;
use std::path::Path;
use std::process::Command;
use std::{env, mem};
use tokio::runtime::Runtime;

pub(crate) fn handle_server_connection(
    mut server_uri: ServerUri,
    remote_cmd: Option<String>,
    conn_flags: Scope,
) -> anyhow::Result<()> {
    info!("Searching for server configuration...");

    let mut config = Config::load_from_file()?;
    let current_scope = Context::load_from_file()?.into_scope();

    let server =
        resolve_server(&server_uri.address, &mut config, current_scope)?.unwrap_or_else(|| {
            info!("No matching server configuration found");
            Server::from_uri_address(&mut server_uri)
        });

    let data = ConnectionData::new(
        server_uri,
        remote_cmd,
        conn_flags,
        server,
        config.default.unwrap_or_default(),
    )?;
    let rt = Runtime::new()?;
    rt.block_on(initiate_connection(data))?;

    Ok(())
}

pub(crate) fn execute_subcommand(cmd: CliSubcommand) -> anyhow::Result<Option<Box<dyn Display>>> {
    match cmd {
        CliSubcommand::Ls { all, scopes } => Config::load_from_file()?
            .list(Context::load_from_file()?.into_scope(), all, scopes)
            .map(|o| Some(Box::new(o) as Box<dyn Display>)),
        CliSubcommand::Use { scope } => {
            if !Config::load_from_file()?.check_scope(&scope) {
                return Err(CliError::ScopeNotFound(scope.into()).into());
            }
            Context::load_from_file()?
                .change_scope(Some(scope))
                .save_to_file()
                .map(|_| None)
        }
        CliSubcommand::AddServer {
            name,
            server,
            global,
        } => Config::load_from_file()?
            .add_server(name, server, global)?
            .save_to_file()
            .map(|_| None),
        CliSubcommand::AddScope { name, scope } => Config::load_from_file()?
            .add_scope(name, scope)?
            .save_to_file()
            .map(|_| None),
        CliSubcommand::Rm { server, scope } => Config::load_from_file()?
            .remove(server, scope)?
            .save_to_file()
            .map(|_| None),
        CliSubcommand::Default { scope } => Config::load_from_file()?
            .set_default(scope)?
            .save_to_file()
            .map(|_| None),
        CliSubcommand::Generate { shell } => {
            let mut cmd = Cli::command();
            let cmd_name = cmd.get_name().to_string();
            generate(shell, &mut cmd, cmd_name, &mut std::io::stdout());

            Ok(None)
        }
        CliSubcommand::Edit => edit_config_file().map(|_| None),
        CliSubcommand::Test => run_config_test().map(|_| None),
    }
}

#[inline]
fn edit_config_file() -> anyhow::Result<()> {
    let config_path = &**CONFIG_PATH;
    if !Path::new(config_path).exists() {
        ensure_work_dir()?;
    }
    let editor = env::var("EDITOR").unwrap_or_else(|_| "nano".into());
    Command::new(editor).arg(config_path).status()?;

    Ok(())
}

#[inline]
fn run_config_test() -> anyhow::Result<()> {
    let test = TestOutput(
        Config::load_from_file()
            .map(|_| (*CONFIG_PATH).clone())
            .map_err(|e| e.to_string()),
    );
    println!("{}", test);

    Ok(())
}

fn resolve_server(
    host: &str,
    config: &mut Config,
    current_scope: String,
) -> anyhow::Result<Option<Server>> {
    // Search for the server in the current scope
    if let Some(ServerEntry::Scope(scoped_servers)) = config.servers.get_mut(&current_scope) {
        let server = if let Some(scoped_server) = scoped_servers.get_mut(host) {
            Some(mem::take(scoped_server).into())
        } else {
            let mut server = None;
            for (pattern, scoped_server) in scoped_servers.iter_mut() {
                if Regex::new(pattern)?.is_match(host) {
                    let mut matched_server: Server = mem::take(scoped_server).into();
                    matched_server.apply_host_placeholder(host);
                    server = Some(matched_server);
                    break;
                }
            }

            server
        };
        if let Some(mut server) = server {
            let scope = config
                .scopes
                .get_mut(&current_scope)
                .ok_or_else(|| CliError::ScopeNotFound(current_scope.into()))?;

            server.apply_scope(mem::take(scope));
            return Ok(Some(server));
        }
    }
    // Search for the server in the global scope
    if let Some(ServerEntry::Global(server)) = config.servers.get_mut(host) {
        return Ok(Some(mem::take(server).into()));
    }
    for (pattern, entry) in config.servers.iter_mut() {
        if let ServerEntry::Global(server) = entry
            && Regex::new(pattern)?.is_match(host)
        {
            let mut server: Server = mem::take(server).into();
            server.apply_host_placeholder(host);
            return Ok(Some(server));
        }
    }

    Ok(None)
}
