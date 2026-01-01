use crate::error::CliError;
use crate::storage::config::{Scope, Server};
use anyhow::Result;
use clap::{Parser, Subcommand};
use clap_complete::Shell;
use itertools::Itertools;
use serde::Serialize;
use serde::Serializer;
use serde::{Deserialize, Deserializer};
use std::fmt::Display;
use std::str::FromStr;

#[derive(Debug, Parser)]
#[command(version)]
#[command(about = "🐚 Seashell is a handy SSH client written in Rust (sea noise inside)")]
#[command(arg_required_else_help = true)]
pub struct Cli {
    /// Connect to the server [user@]hostname[:port]
    pub server: Option<ServerUri>,
    /// Command to execute on the remote server
    pub remote_cmd: Option<String>,
    /// Explicitly specify connection details
    #[command(flatten)]
    pub conn_flags: Scope,
    #[command(subcommand)]
    pub subcommand: Option<CliSubcommand>,
    /// Enable detailed logging (-v INFO, -vv DEBUG)
    #[arg(short, long, action = clap::ArgAction::Count)]
    pub verbose: u8,
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
    /// Edit the configuration file
    Edit,
    /// Check the configuration syntax
    Test,
    /// Generate shell completions
    Generate {
        /// Shell type
        #[arg(value_enum)]
        shell: Shell,
    },
}

/// URI format: [user@]host[:port]
#[derive(Debug, Clone)]
pub struct ServerUri {
    pub address: String,
    pub user: Option<String>,
    pub port: Option<u16>,
}

impl FromStr for ServerUri {
    type Err = CliError;

    fn from_str(input: &str) -> std::result::Result<Self, Self::Err> {
        let (user, host_port) = match input.split_once('@') {
            None => (None, input),
            Some(("", _)) => return Err(CliError::UserMissing),
            Some((u, h)) => (Some(u.to_string()), h),
        };
        if host_port.is_empty() {
            return Err(CliError::HostMissing);
        }
        let (address, port) = if let Some(bracketed) = host_port.strip_prefix('[') {
            let (host, rest) = bracketed
                .split_once(']')
                .ok_or(CliError::InvalidIPv6("missing closing bracket ']'"))?;

            if host.is_empty() {
                return Err(CliError::InvalidIPv6("empty brackets []"));
            }
            let port = match rest {
                "" => None,
                s if s.starts_with(':') => {
                    let port_str = &s[1..];
                    if port_str.is_empty() {
                        return Err(CliError::PortMissing);
                    }
                    Some(port_str.parse().map_err(|_| CliError::PortMissing)?)
                }
                _ => return Err(CliError::InvalidIPv6("unexpected characters after ']'")),
            };

            (host.to_string(), port)
        } else if let Some((host, port)) = host_port.rsplit_once(':') {
            if host.is_empty() {
                return Err(CliError::HostMissing);
            }
            if port.is_empty() {
                return Err(CliError::PortMissing);
            }
            if let Ok(p) = port.parse()
                && !host.contains(':')
            {
                (host.to_string(), Some(p))
            } else {
                (host_port.to_string(), None)
            }
        } else {
            (host_port.to_string(), None)
        };

        Ok(ServerUri {
            user,
            address,
            port,
        })
    }
}

// serde makes default: Some(...), even though all Scope fields are None
pub fn empty_scope_is_none<'de, D>(deserializer: D) -> Result<Option<Scope>, D::Error>
where
    D: Deserializer<'de>,
{
    match Option::<Scope>::deserialize(deserializer)? {
        Some(s) if s.is_empty() => Ok(None),
        other => Ok(other),
    }
}

/// Macro for creating a "newtype" wrapper for compatibility with `clap` and `serde`,
/// which is in `russh::Preferred`. Automatically implements:
/// 1. `struct Wrapper(Inner)`
/// 2. `FromStr` (with an error that outputs supported variants)
/// 3. `Serialize` (as a string)
/// 4. `Deserialize` (from a string)
/// 5. `Display`
macro_rules! define_name_wrapper {
    (
        $wrapper_name:ident,
        $inner_type:ty,
        $parse_method:ident,
        $entity_name:expr,
        $preferred_field:ident,
    ) => {
        #[derive(Debug, Clone, PartialEq)]
        pub struct $wrapper_name(pub $inner_type);

        impl FromStr for $wrapper_name {
            type Err = String;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                <$inner_type>::$parse_method(s)
                    .map($wrapper_name)
                    .map_err(|_| {
                        format!(
                            "Invalid {} name: {}\n\nSupported are:\n- {}",
                            $entity_name,
                            s,
                            russh::Preferred::default()
                                .$preferred_field
                                .iter()
                                .map(|name| name.as_ref())
                                .join("\n- "),
                        )
                    })
            }
        }
        impl Serialize for $wrapper_name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serializer.serialize_str(self.0.as_ref())
            }
        }
        impl<'de> Deserialize<'de> for $wrapper_name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                Self::from_str(&String::deserialize(deserializer)?)
                    .map_err(serde::de::Error::custom)
            }
        }
        impl Display for $wrapper_name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}", self.0.as_ref())
            }
        }
    };
}

define_name_wrapper!(
    KexName,
    russh::kex::Name,
    try_from,
    "key exchange algorithm",
    kex,
);

define_name_wrapper!(
    AlgoName,
    russh::keys::Algorithm,
    from_str,
    "host & public key algorithm",
    key,
);

define_name_wrapper!(
    CipherName,
    russh::cipher::Name,
    try_from,
    "cipher algorithm",
    cipher,
);

define_name_wrapper!(MacName, russh::mac::Name, try_from, "MAC algorithm", mac,);

#[cfg(test)]
mod tests {
    use super::ServerUri;
    use std::str::FromStr;

    #[test]
    fn uri_parsing_success() {
        let cases = vec![
            ("user@host:22", Some("user"), "host", Some(22)),
            ("host:22", None, "host", Some(22)),
            ("user@host", Some("user"), "host", None),
            ("host", None, "host", None),
            ("user@[::1]:22", Some("user"), "::1", Some(22)),
            ("[::1]:22", None, "::1", Some(22)),
            ("user@[::1]", Some("user"), "::1", None),
            ("[::1]", None, "::1", None),
            ("::1", None, "::1", None),
        ];
        for (input, user, host, port) in cases {
            let uri = ServerUri::from_str(input).expect("Failed to parse URI");
            assert_eq!(
                (uri.user.as_deref(), uri.address.as_str(), uri.port),
                (user, host, port)
            );
        }
    }

    #[test]
    fn uri_parsing_failure() {
        let cases = vec![
            "user@",
            "@host",
            "host:",
            ":22",
            "user@[::1",
            "[::1]:",
            "[]",
            "user@[::1]:bruh",
            "user@[::1]bruh",
        ];
        for input in cases {
            assert!(ServerUri::from_str(input).is_err());
        }
    }
}
