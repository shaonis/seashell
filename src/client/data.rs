use std::{env, path::PathBuf, str::FromStr};

use crate::{
    error::ConnectionError,
    storage::{
        config::{Scope, Server},
        provider::{WORK_DIR, get_full_path},
    },
};


#[derive(Debug, Clone)]
pub struct ConnectionData {
    pub address: String,
    pub user: String,
    pub port: u16,
    pub known_hosts: PathBuf,
    pub private_key: Option<PathBuf>,
    pub openssh_cert: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct ServerUri {
    pub address: String,
    pub user: Option<String>,
    pub port: Option<u16>,
}

impl FromStr for ServerUri {
    type Err = &'static str;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let (user, host_port) = match input.find('@') {
            Some(i) => (Some(input[..i].to_string()), &input[i+1..]),
            None => (None, input),
        };
        let (address, port) = match host_port.rfind(':') {
            Some(i) if host_port[i+1..].parse::<u16>().is_ok() => {
                let p = host_port[i+1..].parse().unwrap();
                (host_port[..i].to_string(), Some(p))
            }
            _ => (host_port.to_string(), None),
        };

        Ok(ServerUri { user, address, port })
    }
}

impl ConnectionData {
    pub fn new(
        uri: ServerUri,
        flags: Scope,
        server: Server,
        default: Scope,
    ) -> Result<Self, ConnectionError> {
        let Server { address, scope } = server;

        let user = uri.user
            .or(flags.user)
            .or(scope.user)
            .or(default.user)
            .or_else(|| env::var("USER").ok())
            .ok_or(ConnectionError::UserRequired)?;

        let port = uri.port
            .or(flags.port)
            .or(scope.port)
            .or(default.port)
            .unwrap_or(22);

        let known_hosts = flags.known_hosts
            .or(scope.known_hosts)
            .or(default.known_hosts)
            .map(get_full_path)
            .unwrap_or_else(|| WORK_DIR.join("known_hosts"));

        let private_key = flags.private_key
            .or(scope.private_key)
            .or(default.private_key)
            .map(get_full_path);

        let openssh_cert = flags.openssh_cert
            .or(scope.openssh_cert)
            .or(default.openssh_cert)
            .map(get_full_path);

        Ok(Self {
            address,
            user,
            port,
            private_key,
            openssh_cert,
            known_hosts,
        })
    }
}
