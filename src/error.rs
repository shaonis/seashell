use thiserror::Error;


#[derive(Error, Debug)]
pub enum CliError {
    #[error("Server '{0}' not found")]
    ServerNotFound(Box<str>),
    #[error("Scope '{0}' not found")]
    ScopeNotFound(Box<str>),
    #[error("Server '{0}' already exists")]
    ServerExists(Box<str>),
    #[error("Scope '{0}' already exists")]
    ScopeExists(Box<str>),
}

#[derive(Error, Debug)]
pub enum FileError {
    #[error("I/O problem: {0}")]
    Std(#[from] std::io::Error),
    #[error("Bad yaml: {0} (hint: check the config file)")]
    Yaml(#[from] serde_yml::Error),
    #[error("Bad json: {0} (hint: check the context file)")]
    Json(#[from] serde_json::Error),
}

#[derive(Error, Debug)]
pub enum ConnectionError {
    #[error("No user is specified for the server (hint: check the config file)")]
    UserRequired,
    #[error("Bad regular expression: {0} (hint: check the config file)")]
    Regex(#[from] regex::Error),
    #[error("DNS resolution error: {0}")]
    Dns(#[from] std::io::Error),
}

#[derive(Error, Debug)]
pub enum SessionError {
    #[error("Problem with SSH private key: {0}")]
    PrivateKey(#[source] russh::keys::Error),
    #[error("Problem with OpenSSH certificate: {0}")]
    OpenSSHCert(#[source] russh::keys::ssh_key::Error),
    #[error("Failed to authenticate with OpenSSH certificate: {0}")]
    OpenSSHCertAuth(#[source] russh::Error),
    #[error("Failed to authenticate with public key: {0}")]
    PubKeyAuth(#[source] russh::Error),
    #[error("Failed to authenticate with password: {0}")]
    PasswordAuth(#[source] russh::Error),
    #[error("Failed to adjust terminal: {0}")]
    Terminal(#[source] russh::Error),
}
