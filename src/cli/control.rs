use std::{env, mem, path::Path, process::Command};

use anyhow::Result;
use clap::{CommandFactory, Parser};
use clap_complete::generate;
use regex::Regex;
use tokio::runtime::Runtime;

use crate::{
    cli::{output::{OutputData, TestResult}, parser::{Cli, CliSubcommand}},
    client::{connect::establish_connection, data::{ConnectionData, ServerUri}},
    error::CliError,
    storage::{
        config::{Config, Scope, Server, ServerEntry},
        context::Context,
        provider::{CONFIG_PATH, ensure_work_dir, StorageProvider},
    },
};


pub fn start_cli() -> Result<()> {
    let args = Cli::parse();

    let res = match (args.server, args.subcommand, args.test, args.edit) {
        (Some(ssh_uri), _, _, _) => handle_server_connection(ssh_uri, args.conn_flags),
        (_, Some(cmd), _, _) => handle_subcommand(cmd),
        (_, _, true, _) => run_config_test(),
        (_, _, _, true) => edit_config_file(),
        _ => Ok(()),
    };
    if let Err(err) = res {
        eprintln!("{}", err);
    }

    Ok(())
}

#[inline]
fn handle_server_connection(mut server_uri: ServerUri, conn_flags: Scope) -> Result<()> {
    let mut config = Config::load_from_file()?;
    let current_scope = Context::load_from_file()?.into_scope();

    let server = match resolve_server(&server_uri.address, &mut config, current_scope) {
        Ok(Some(server)) => server,
        Ok(None) => Server::from_uri(&mut server_uri),
        Err(e) => return Err(e),
    };
    let data = ConnectionData::new(
        server_uri,
        conn_flags,
        server,
        config.default.unwrap_or_default(),
    )?;
    let rt  = Runtime::new()?;
    rt.block_on(establish_connection(data))?;

    Ok(())
}

#[inline]
fn handle_subcommand(cmd: CliSubcommand) -> Result<()> {
    let result = execute_subcommand(cmd)?;
    print!("{}", result);

    Ok(())
}

#[inline]
fn run_config_test() -> Result<()> {
    let test = TestResult::from(
        Config::load_from_file()
            .map(|_| (*CONFIG_PATH).clone())
            .map_err(|e| e.to_string())
    );
    println!("{}", test);

    Ok(())
}

#[inline]
fn edit_config_file() -> Result<()> {
    let config_path = &**CONFIG_PATH;
    if !Path::new(config_path).exists() {
        ensure_work_dir()?;
    }
    let editor = env::var("EDITOR").unwrap_or_else(|_| "nano".into());
    Command::new(editor).arg(config_path).status()?;

    Ok(())
}

fn execute_subcommand(cmd: CliSubcommand) -> Result<OutputData> {
    match cmd {
        CliSubcommand::Ls { all, scopes } => {
            Config::load_from_file()?
                .list(
                    Context::load_from_file()?.into_scope(),
                    all,
                    scopes,
                )
                .map(OutputData::from)
        },
        CliSubcommand::Use { scope } => {
            if !Config::load_from_file()?.check_scope(&scope) {
                return Err(CliError::ScopeNotFound(scope.into()).into())
            }
            Context::load_from_file()?
                .change_scope(Some(scope))
                .save_to_file()
                .map(|_| OutputData::None)
        },
        CliSubcommand::AddServer { name, server, global } => {
            Config::load_from_file()?
                .add_server(name, server, global)?
                .save_to_file()
                .map(|_| OutputData::None)
        },
        CliSubcommand::AddScope { name, scope } => {
            Config::load_from_file()?
                .add_scope(name, scope)?
                .save_to_file()
                .map(|_| OutputData::None)
        },
        CliSubcommand::Rm { server, scope } => {
            Config::load_from_file()?
                .remove(server, scope)?
                .save_to_file()
                .map(|_| OutputData::None)
        },
        CliSubcommand::Default { scope } => {
            Config::load_from_file()?
                .set_default(scope)?
                .save_to_file()
                .map(|_| OutputData::None)
        },
        CliSubcommand::Generate { shell } => {
            let mut cmd = Cli::command();
            let cmd_name = cmd.get_name().to_string();
            generate(shell, &mut cmd, cmd_name, &mut std::io::stdout());

            Ok(OutputData::None)
        },
    }
}

fn resolve_server(host: &str, config: &mut Config, current_scope: String) -> Result<Option<Server>> {
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
                    break
                }
            }
            server
        };
        if let Some(mut server) = server {
            let scope = config.scopes
                .get_mut(&current_scope)
                .ok_or_else(|| CliError::ScopeNotFound(current_scope.into()))?;

            server.apply_scope(mem::take(scope));
            return Ok(Some(server))
        }
    }
    // Search for the server in the global scope
    if let Some(ServerEntry::Global(server)) = config.servers.get_mut(host) {
        return Ok(Some(mem::take(server).into()));
    }
    for (pattern, entry) in config.servers.iter_mut() {
        if let ServerEntry::Global(server) = entry {
            if Regex::new(pattern)?.is_match(host) {
                let mut server: Server = mem::take(server).into();
                server.apply_host_placeholder(host);
                return Ok(Some(server))
            }
        }
    }

    Ok(None)
}
