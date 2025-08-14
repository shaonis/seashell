use clap::{Parser, Subcommand};
use clap_complete::Shell;

use crate::{
    client::data::ServerUri,
    storage::config::{Scope, Server},
};


#[derive(Debug, Parser)]
#[command(version)]
#[command(about = "🐚 Seashell is a handy SSH client written in Rust (sea noise inside)")]
#[command(arg_required_else_help = true)]
pub struct Cli {
    /// Connect to the server ([user@]hostname[:port])
    pub server: Option<ServerUri>,
    #[command(subcommand)]
    pub subcommand: Option<CliSubcommand>,
    /// Explicitly specify connection details
    #[command(flatten)]
    pub conn_flags: Scope,
    /// Check the configuration syntax
    #[arg(short, long)]
    pub test: bool,
    /// Edit the configuration file
    #[arg(short, long)]
    pub edit: bool,
}

#[derive(Debug, Subcommand)]
pub enum CliSubcommand {
    /// List servers
    Ls {
        /// Show all servers
        #[arg(short, long)]
        all: bool,
        /// Show all scopes
        #[arg(short, long)]
        scopes: bool,
    },
    /// Change scope
    Use {
        /// Scope to switch to
        scope: String,
    },
    /// Add server
    #[command(visible_alias = "server")]
    AddServer {
        /// Name of the server
        name: String,
        #[command(flatten)]
        server: Server,
        /// Global server
        #[arg(short, long)]
        global: bool,
    },
    /// Add scope
    #[command(visible_alias = "scope")]
    AddScope {
        /// Name of the scope
        name: String,
        #[command(flatten)]
        scope: Scope,
    },
    /// Remove server or scope
    #[command(visible_alias = "remove")]
    Rm {
        /// Name of the server
        #[arg(required_unless_present = "scope")]
        server: Option<String>,
        /// Name of the scope
        #[arg(short, long = "scope", conflicts_with = "server")]
        scope: Option<String>,
    },
    /// Set default connection data
    Default {
        #[command(flatten)]
        scope: Scope,
    },
    /// Generate shell completions
    Generate {
        /// Shell type
        #[arg(value_enum)]
        shell: Shell,
    },
}
