use crate::cli::parser::ServerUri;
use crate::{
    cli::{
        output::LsOutput,
        parser::{AlgoName, CipherName, KexName, MacName, empty_scope_is_none},
    },
    error::{CliError, FileError},
    storage::{
        context::Context,
        provider::{CONFIG_PATH, StorageProvider},
    },
};
use anyhow::Result;
use clap::{Args, Parser};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_with_macros::skip_serializing_none;
use smart_default::SmartDefault;
use std::ops::AddAssign;
use std::{clone::Clone, mem, path::PathBuf, sync::LazyLock};

/// The configuration is hierarchical: default settings can be overridden by
/// scopes, which can be overridden by individual server entries.
#[derive(Default, Deserialize, Serialize)]
pub struct Config {
    /// Default settings applied to all connections unless overridden
    #[serde(flatten)]
    #[serde(deserialize_with = "empty_scope_is_none")]
    pub default: Option<Scope>,
    /// Named scopes with specific connection settings
    pub scopes: IndexMap<String, Scope>,
    /// Server entries, either global or scoped
    pub servers: IndexMap<String, ServerEntry>,
}

/// A scope defines a set of SSH connection parameters.
#[skip_serializing_none]
#[derive(Args, Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct Scope {
    /// User to connect as
    #[arg(short, long)]
    #[arg(value_name = "STRING")]
    pub user: Option<String>,
    /// Port to connect to
    #[arg(short, long)]
    #[arg(value_name = "NUM")]
    pub port: Option<u16>,
    /// Path to the known_hosts file
    #[arg(short = 'h', long)]
    #[arg(value_name = "FILE_PATH")]
    pub known_hosts: Option<PathBuf>,
    /// Path to the private key
    #[arg(short = 'k', long)]
    #[arg(value_name = "FILE_PATH")]
    pub private_key: Option<PathBuf>,
    /// Path to the OpenSSH certificate
    #[arg(short = 'c', long)]
    #[arg(value_name = "FILE_PATH", requires = "private_key")]
    pub openssh_cert: Option<PathBuf>,
    /// Preferred key exchange algorithms
    #[arg(short = 'e', long)]
    #[arg(value_name = "CSV")]
    #[arg(value_delimiter = ',')]
    pub kex: Option<Vec<KexName>>,
    /// Preferred host & public key algorithms
    #[arg(short = 'a', long)]
    #[arg(value_name = "CSV")]
    #[arg(value_delimiter = ',')]
    pub alg: Option<Vec<AlgoName>>,
    /// Preferred symmetric ciphers
    #[arg(short = 'p', long)]
    #[arg(value_name = "CSV")]
    #[arg(value_delimiter = ',')]
    pub cipher: Option<Vec<CipherName>>,
    /// Preferred MAC algorithms
    #[arg(short = 'm', long)]
    #[arg(value_name = "CSV")]
    #[arg(value_delimiter = ',')]
    pub mac: Option<Vec<MacName>>,
    /// Set the time to wait for a connection
    #[arg(short = 't', long)]
    #[arg(value_name = "SECS")]
    pub timeout: Option<u64>,
    /// Duration between keepalive messages if the server is silent
    #[arg(short = 'i', long)]
    #[arg(value_name = "SECS")]
    pub interval: Option<u64>,
    /// Maximum number of keepalives allowed without a response
    #[arg(short = 'r', long)]
    #[arg(value_name = "NUM")]
    pub retries: Option<usize>,
}

/// Represents a server entry, either global or scoped.
#[derive(Debug, Deserialize, Serialize, SmartDefault)]
#[serde(untagged)]
pub enum ServerEntry {
    /// A server available globally
    #[default]
    Global(ScopedServer),
    /// A server available within specific scopes
    Scope(IndexMap<String, ScopedServer>),
}

/// A scoped server can either be a simple address or an overridden server.
#[derive(Debug, Deserialize, Serialize, SmartDefault)]
#[serde(untagged)]
pub enum ScopedServer {
    /// Just the address of the server
    #[default]
    Address(String),
    /// An overridden server
    Override(Box<Server>),
}

/// Represents a server with its address and scope-specific parameters.
#[skip_serializing_none]
#[derive(Clone, Debug, Default, Deserialize, Parser, Serialize)]
pub struct Server {
    /// Address of the server
    pub address: String,
    /// Scope-specific connection parameters
    #[command(flatten)]
    #[serde(flatten)]
    pub scope: Scope,
}

impl Config {
    pub fn check_scope(&self, scope: &str) -> bool {
        self.scopes.contains_key(scope)
    }

    pub fn list(&mut self, current_scope: String, all: bool, scopes: bool) -> Result<LsOutput> {
        if all {
            self.sort_servers();
            return Ok(LsOutput::All(mem::take(&mut self.servers)));
        }
        if scopes {
            self.scopes.sort_unstable_keys();
            return Ok(LsOutput::AllScopes(
                mem::take(&mut self.default).map(Box::new),
                mem::take(&mut self.scopes),
            ));
        }
        if self.scopes.contains_key(&current_scope) {
            let result = match self.servers.get_mut(&current_scope) {
                Some(ServerEntry::Scope(servers)) => {
                    servers.sort_unstable_keys();
                    mem::take(servers)
                }
                _ => IndexMap::new(),
            };
            return Ok(LsOutput::Scope(current_scope, result));
        }

        self.sort_servers();
        Ok(LsOutput::All(mem::take(&mut self.servers)))
    }

    pub fn add_server(mut self, name: String, server: Server, global: bool) -> Result<Self> {
        if global {
            self.add_global_server(name, server)?;
            return Ok(self);
        }
        let current_scope = Context::load_from_file()?.into_scope();
        if current_scope.is_empty() {
            self.add_global_server(name, server)?;
            return Ok(self);
        }
        let mut scope_servers;
        if let Some(ServerEntry::Scope(servers)) = self.servers.get_mut(&current_scope) {
            if servers.contains_key(&name) {
                return Err(CliError::ServerExists(name.into()).into());
            }
            scope_servers = mem::take(servers);
        } else {
            if !self.scopes.contains_key(&current_scope) {
                return Err(CliError::ScopeNotFound(current_scope.into()).into());
            }
            scope_servers = IndexMap::new();
        }
        if server.is_only_address() {
            scope_servers.insert(name, ScopedServer::Address(server.address));
        } else {
            scope_servers.insert(name, ScopedServer::Override(Box::new(server)));
        }
        self.servers
            .insert(current_scope, ServerEntry::Scope(scope_servers));

        Ok(self)
    }

    pub fn add_scope(mut self, name: String, scope: Scope) -> Result<Self> {
        if self.scopes.contains_key(&name) {
            return Err(CliError::ScopeExists(name.into()).into());
        }
        self.scopes.insert(name, scope);

        Ok(self)
    }

    pub fn remove(mut self, server: Option<String>, scope: Option<String>) -> Result<Self> {
        if let Some(scope_name) = scope {
            if self.scopes.swap_remove(&scope_name).is_none() {
                return Err(CliError::ScopeNotFound(scope_name.into()).into());
            }
            self.servers.swap_remove(&scope_name);
            let context = Context::load_from_file()?;
            if *context.scope() == scope_name {
                context.change_scope(None).save_to_file()?;
            }
            return Ok(self);
        }
        let name = server.expect("Server name is required");
        let current_scope = &Context::load_from_file()?.into_scope();
        if current_scope.is_empty() {
            if self.servers.swap_remove(&name).is_some() {
                return Ok(self);
            }
            return Err(CliError::ServerNotFound(name.into()).into());
        }
        if let Some(ServerEntry::Scope(scope_servers)) = self.servers.get_mut(current_scope) {
            if scope_servers.swap_remove(&name).is_none() {
                return Err(CliError::ServerNotFound(name.into()).into());
            }
            if scope_servers.is_empty() {
                self.servers.swap_remove(current_scope);
            }
            return Ok(self);
        }

        Err(CliError::ServerNotFound(name.into()).into())
    }

    pub fn set_default(mut self, scope: Scope) -> Result<Self> {
        self.default = Some(scope);

        Ok(self)
    }

    #[inline]
    fn add_global_server(&mut self, name: String, server: Server) -> Result<()> {
        if let Some(entry) = self.servers.get(&name) {
            let err = match entry {
                ServerEntry::Global(_) => CliError::ServerExists(name.into()),
                ServerEntry::Scope(_) => CliError::ScopeExists(name.into()),
            };
            return Err(err.into());
        }
        let server = if server.is_only_address() {
            ScopedServer::Address(server.address)
        } else {
            ScopedServer::Override(Box::new(server))
        };
        self.servers.insert(name, ServerEntry::Global(server));

        Ok(())
    }

    #[inline]
    fn sort_servers(&mut self) {
        self.servers.sort_unstable_keys();
        self.servers
            .values_mut()
            .filter_map(|entry| {
                if let ServerEntry::Scope(s) = entry {
                    Some(s)
                } else {
                    None
                }
            })
            .for_each(|s| s.sort_unstable_keys());
    }
}

impl StorageProvider for Config {
    #[inline]
    fn work_file() -> &'static LazyLock<Box<str>> {
        &CONFIG_PATH
    }

    fn serialize(&self) -> Result<String> {
        Ok(serde_yml::to_string(&self).map_err(FileError::Yaml)?)
    }

    fn deserialize(data: &str) -> Result<Self> {
        Ok(serde_yml::from_str(data).map_err(FileError::Yaml)?)
    }
}

impl Scope {
    pub fn is_empty(&self) -> bool {
        *self == Scope::default()
    }
}

impl Server {
    pub fn new(address: String) -> Self {
        Self {
            address,
            ..Default::default()
        }
    }

    pub fn from_uri_address(uri: &mut ServerUri) -> Self {
        Self::new(mem::take(&mut uri.address))
    }

    pub fn is_only_address(&self) -> bool {
        self.scope.is_empty()
    }

    pub fn apply_host_placeholder(&mut self, host: &str) {
        self.address = self.address.replace("$h", host);
    }

    pub fn apply_scope(&mut self, scope: Scope) {
        self.scope += scope;
    }
}

impl AddAssign for Scope {
    fn add_assign(&mut self, other: Self) {
        let Self {
            user,
            port,
            known_hosts,
            private_key,
            openssh_cert,
            kex,
            alg,
            cipher,
            mac,
            timeout,
            interval,
            retries,
        } = self;

        macro_rules! merge_fields {
            ($($field:ident),+ $(,)?) => {
                $(
                    *$field = $field.take().or(other.$field);
                )+
            };
        }

        merge_fields!(
            user,
            port,
            known_hosts,
            private_key,
            openssh_cert,
            kex,
            alg,
            cipher,
            mac,
            timeout,
            interval,
            retries,
        );
    }
}

impl From<ScopedServer> for Server {
    fn from(scoped_server: ScopedServer) -> Self {
        match scoped_server {
            ScopedServer::Address(address) => Server::new(address),
            ScopedServer::Override(server) => *server,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::provider::StorageProvider;

    // Config
    #[test]
    fn check_scope() {
        let mut cfg = Config::default();
        cfg.scopes.insert("test".into(), Scope::default());
        assert!(cfg.check_scope("test"));
        assert!(!cfg.check_scope("other"));
    }

    #[test]
    fn list_flags() {
        let mut cfg = Config::default();
        let res = cfg.list("".into(), true, false);
        assert!(matches!(res, Ok(LsOutput::All(_))));
        let res = cfg.list("".into(), false, true);
        assert!(matches!(res, Ok(LsOutput::AllScopes(_, _))));
        let res = cfg.list("".into(), true, true);
        assert!(matches!(res, Ok(LsOutput::All(_))));
        let res = cfg.list("".into(), false, false);
        assert!(matches!(res, Ok(LsOutput::All(_))));
    }

    #[test]
    fn list_content() {
        let mut cfg = Config::default();
        let (host1, host2) = (String::from("host1"), String::from("host2"));
        let (scope1, scope2) = (String::from("scope1"), String::from("scope2"));
        // All
        cfg.servers.insert(host1.clone(), ServerEntry::default());
        cfg.servers.insert(host2.clone(), ServerEntry::default());

        let res = cfg.list("".into(), true, false);
        assert!(matches!(res, Ok(LsOutput::All(_))));
        if let Ok(LsOutput::All(servers)) = res {
            assert_eq!(servers.len(), 2);
            assert!(servers.contains_key(&host1));
            assert!(servers.contains_key(&host2));
        }
        // All scopes
        cfg.scopes.insert(scope1.clone(), Scope::default());
        cfg.scopes.insert(scope2.clone(), Scope::default());

        let res = cfg.list("".into(), false, true);
        assert!(matches!(res, Ok(LsOutput::AllScopes(_, _))));
        if let Ok(LsOutput::AllScopes(_, scopes)) = res {
            assert_eq!(scopes.len(), 2);
            assert!(scopes.contains_key(&scope1));
            assert!(scopes.contains_key(&scope2));
        }
        // Scope servers
        cfg.scopes.insert(scope1.clone(), Scope::default());
        let mut scoped_servers = IndexMap::new();
        scoped_servers.insert(host1.clone(), ScopedServer::default());
        cfg.servers
            .insert(scope1.clone(), ServerEntry::Scope(scoped_servers));

        let res = cfg.list(scope1, false, false);
        assert!(matches!(res, Ok(LsOutput::Scope(_, _))));
        if let Ok(LsOutput::Scope(_, servers)) = res {
            assert_eq!(servers.len(), 1);
            assert!(servers.contains_key(&host1));
        }
    }

    #[test]
    fn add_scope() {
        let cfg = Config::default();
        let scope = String::from("scope");
        let cfg = cfg.add_scope(scope.clone(), Scope::default());
        if let Ok(cfg) = cfg {
            assert!(cfg.scopes.contains_key(&scope));
            let cfg = cfg.add_scope(scope, Scope::default());
            assert!(cfg.is_err());
        } else {
            panic!("Failed to add scope");
        }
    }

    #[test]
    fn serialize_deserialize() {
        let default_user = Some("admin".into());
        let cfg = Config {
            default: Some(Scope {
                user: default_user.clone(),
                ..Default::default()
            }),
            ..Default::default()
        };
        if let Ok(serialized) = StorageProvider::serialize(&cfg) {
            assert!(serialized.contains("user: admin"));
            let deserialized: Result<Config> = StorageProvider::deserialize(&serialized);
            assert!(deserialized.is_ok_and(|c| c.default.is_some_and(|s| s.user == default_user)));
        } else {
            panic!("Config serialization failed");
        }
    }

    #[test]
    fn deserialize_invalid_yaml() {
        let data = "invalid :: yaml";
        let res: Result<Config> = StorageProvider::deserialize(data);
        assert!(res.is_err());
    }

    // Server
    #[test]
    fn from_server_uri() {
        let mut uri = ServerUri {
            address: "host".into(),
            user: Some(String::default()),
            port: Some(22),
        };
        let srv = Server::from_uri_address(&mut uri);
        assert!(uri.address.is_empty() && !srv.address.is_empty());
        assert!(uri.user.is_some() && srv.scope.user.is_none());
        assert!(uri.port.is_some() && srv.scope.port.is_none());
    }

    #[test]
    fn server_is_only_address() {
        let srv = Server::new("host".into());
        assert!(srv.is_only_address());
    }

    #[test]
    fn server_apply_host_placeholder() {
        let mut srv = Server::new("$h.local".into());
        srv.apply_host_placeholder("vm-01");
        assert_eq!(srv.address, "vm-01.local");
    }
}
