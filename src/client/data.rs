use crate::cli::parser::ServerUri;
use crate::{
    error::ConnectionError,
    storage::{
        config::{Scope, Server},
        provider::{WORK_DIR, get_full_path},
    },
};
use std::time::Duration;
use std::{env, path::PathBuf};

const DEFAULT_SSH_PORT: u16 = 22;
const DEFAULT_KNOWN_HOSTS_FILE: &str = "known_hosts";

/// Represents the data required to establish a connection to a server
#[derive(Debug)]
pub struct ConnectionData {
    // Fundamentals
    pub address: String,
    pub user: String,
    pub port: u16,
    pub remote_cmd: Option<String>,
    // Files
    pub known_hosts: PathBuf,
    pub private_key: Option<PathBuf>,
    pub openssh_cert: Option<PathBuf>,
    // russh Config
    pub config: russh::client::Config,
}

/// Cascades through multiple optional sources, applying optional transformations.
/// Syntax: field => source1, source2, ...; map = transform; default = hardcoded value
macro_rules! cascade {
    ($field:ident => $($source:expr),+ $(; map = $map:expr)? $(; default = $default:expr)? $(;)?) => {
        None$(.or($source.$field))+$(.map($map))?$(.unwrap_or($default))?
    };
}

impl ConnectionData {
    pub fn new(
        uri: ServerUri,
        remote_cmd: Option<String>,
        flags: Scope,
        server: Server,
        global: Scope,
    ) -> Result<Self, ConnectionError> {
        // Already in server
        _ = uri.address;

        let Server { address, scope } = server;

        let user = cascade!(user => uri, flags, scope, global;
            default = env::var("USER").ok().ok_or(ConnectionError::UserRequired)?;
        );
        let port = cascade!(port => uri, flags, scope, global;
            default = DEFAULT_SSH_PORT;
        );
        let known_hosts = cascade!(known_hosts => flags, scope, global;
            map = get_full_path;
            default = WORK_DIR.join(DEFAULT_KNOWN_HOSTS_FILE);
        );
        let private_key = cascade!(private_key => flags, scope, global;
            map = get_full_path;
        );
        let openssh_cert = cascade!(openssh_cert => flags, scope, global;
            map = get_full_path;
        );

        let default_preferred = russh::Preferred::default();
        let default_config = russh::client::Config::default();

        let kex = cascade!(kex => flags, scope, global;
            map = |v| v.into_iter().map(|n| n.0).collect();
            default = default_preferred.kex;
        );
        let alg = cascade!(alg => flags, scope, global;
            map = |v| v.into_iter().map(|n| n.0).collect();
            default = default_preferred.key;
        );
        let cipher = cascade!(cipher => flags, scope, global;
            map = |v| v.into_iter().map(|n| n.0).collect();
            default = default_preferred.cipher;
        );
        let mac = cascade!(mac => flags, scope, global;
            map = |v| v.into_iter().map(|n| n.0).collect();
            default = default_preferred.mac;
        );
        let timeout = cascade!(timeout => flags, scope, global;
            map = Duration::from_secs;
        );
        let interval = cascade!(interval => flags, scope, global;
            map = Duration::from_secs;
        );
        let retries = cascade!(retries => flags, scope, global;
            default = default_config.keepalive_max;
        );

        let preferred = russh::Preferred {
            kex,
            key: alg,
            cipher,
            mac,
            ..default_preferred
        };
        let config = russh::client::Config {
            preferred,
            inactivity_timeout: timeout,
            keepalive_interval: interval,
            keepalive_max: retries,
            ..default_config
        };

        Ok(Self {
            address,
            user,
            port,
            remote_cmd,
            private_key,
            openssh_cert,
            known_hosts,
            config,
        })
    }
}
