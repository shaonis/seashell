use anyhow::Result;
use crossterm::terminal::{self, disable_raw_mode, enable_raw_mode};
use log::info;
use russh::client::{AuthResult, Handle, KeyboardInteractiveAuthResponse, Msg};
use russh::keys::agent::client::AgentClient;
use russh::keys::{
    HashAlg, PrivateKey, PrivateKeyWithHashAlg, load_openssh_certificate, load_secret_key, ssh_key,
};
use russh::{Channel, ChannelMsg, MethodKind};
use secrecy::{ExposeSecret, SecretString};
use std::io::Write;
use std::mem;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::lookup_host;

use crate::client::data::ConnectionData;
use crate::client::handler::ClientHandler;
use crate::error::{ConnectionError, FileError, SessionError};

const MAX_PASSPHRASE_ATTEMPTS: u8 = 3;
const DEFAULT_TERM: &str = "xterm";
const SESSION_BUFFER_SIZE: usize = 4096;
const RESIZE_INTERVAL_MS: u64 = 200;
const STDIN_FD: i32 = 0;
const STDOUT_FD: i32 = 1;

// Single point of entry for the module
pub async fn initiate_connection(data: ConnectionData) -> Result<()> {
    let mut conn = Connection::new(data).await?;
    conn.establish().await?;
    conn.authenticate().await?;

    if let Some(cmd) = conn.data.remote_cmd.as_ref() {
        conn.execute_command(cmd).await?;
    } else {
        conn.start_interactive().await?;
    }

    Ok(())
}

// Represents an SSH connection
struct Connection {
    data: ConnectionData,
    socket: SocketAddr,
    session: Option<Handle<ClientHandler>>,
}

macro_rules! session {
    ($self:expr) => {
        $self.session.as_ref().expect("should be connected")
    };
    (mut $self:expr) => {
        $self.session.as_mut().expect("should be connected")
    };
}

macro_rules! prompt {
    (echo => $($arg:tt)*) => {{
        print!("{}: ", format_args!($($arg)*));
        std::io::stdout().flush()?;
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        input.trim().to_string()
    }};
    ($($arg:tt)*) => {{
        print!("{}: ", format_args!($($arg)*));
        std::io::stdout().flush()?;
        SecretString::from(rpassword::read_password()?)
    }};
}

impl Connection {
    async fn new(data: ConnectionData) -> Result<Self> {
        // Maybe not a socket (domain:port)
        let sock = format!("{}:{}", data.address, data.port);
        let socket = if let Ok(s) = sock.parse() {
            s
        } else {
            info!("Resolving address '{}'...", data.address);

            lookup_host(sock)
                .await
                .map_err(ConnectionError::Dns)?
                .next()
                .expect("address should be resolved")
        };

        Ok(Self {
            data,
            socket,
            session: None,
        })
    }

    async fn establish(&mut self) -> Result<()> {
        let handler = ClientHandler::new(self.socket.ip(), mem::take(&mut self.data.known_hosts));
        let config = Arc::new(mem::take(&mut self.data.config));

        info!(
            "Connecting to {}:{}...",
            self.socket.ip(),
            self.socket.port()
        );

        self.session = russh::client::connect(config, self.socket, handler)
            .await
            .map_err(SessionError::Connect)?
            .into();

        Ok(())
    }

    async fn authenticate(&mut self) -> Result<()> {
        info!("Trying none/hostbased authentication...");

        let session = session!(mut self);
        let allowed_methods = match session.authenticate_none(&self.data.user).await {
            Ok(AuthResult::Success) => return Ok(()),
            Ok(AuthResult::Failure {
                remaining_methods, ..
            }) => remaining_methods,
            Err(_) => return Err(SessionError::AuthUnavailable.into()),
        };
        info!(
            "Authorization required. Allowed methods: {:?}",
            allowed_methods
        );

        for method in allowed_methods.iter() {
            let authenticated = match method {
                MethodKind::PublicKey => {
                    let hash_alg = session!(self)
                        .best_supported_rsa_hash()
                        .await?
                        .unwrap_or(Some(HashAlg::default()));

                    self.try_agent_auth(hash_alg).await?
                        || self.try_certificate_auth().await?
                        || self.try_publickey_auth(hash_alg).await?
                }
                MethodKind::KeyboardInteractive => self.try_keyboard_interactive_auth().await?,
                MethodKind::Password => self.try_password_auth().await?,
                _ => continue,
            };
            if authenticated {
                return Ok(());
            }
        }
        let allowed_methods = allowed_methods
            .iter()
            .map(Into::into)
            .collect::<Vec<String>>()
            .join(", ");

        Err(SessionError::AuthFailed(allowed_methods).into())
    }

    async fn execute_command(&self, command: &str) -> Result<()> {
        info!("Executing command '{}'...", command);

        let session = session!(self);
        let mut channel = session.channel_open_session().await?;
        channel.exec(true, command).await?;

        let mut stdout = tokio::io::stdout();
        while let Some(msg) = channel.wait().await {
            match msg {
                ChannelMsg::Data { data } => {
                    stdout.write_all(&data).await?;
                    stdout.flush().await?;
                }
                ChannelMsg::ExitStatus { exit_status: _ } => break,
                _ => {}
            }
        }

        Ok(())
    }

    async fn start_interactive(&mut self) -> Result<()> {
        info!("Preparing interactive session...");

        let session = session!(mut self);
        let mut channel = session.channel_open_session().await?;

        let (width, height) = terminal::size().map_err(FileError::Std)?;
        let term = std::env::var("TERM").unwrap_or(DEFAULT_TERM.into());

        channel
            .request_pty(false, &term, width.into(), height.into(), 0, 0, &[])
            .await
            .map_err(SessionError::Terminal)?;

        channel
            .request_shell(true)
            .await
            .map_err(SessionError::Terminal)?;

        let result = run_session(&mut channel).await;
        println!("Connection to {} closed.", self.socket.ip());

        result
    }

    async fn try_agent_auth(&mut self, hash_alg: Option<HashAlg>) -> Result<bool> {
        info!("Trying SSH agent authentication...");

        let Ok(mut agent) = AgentClient::connect_env().await else {
            info!("SSH agent not available (environment variable 'SSH_AUTH_SOCK' not set)");
            return Ok(false);
        };
        let mut pub_keys = agent.request_identities().await?;

        if pub_keys.is_empty() {
            info!("No keys found in SSH agent");
            return Ok(false);
        }
        // Prioritize keys matching user/address
        pub_keys.sort_by_key(|key| {
            let comment = key.comment();
            let matches = comment.contains(&self.data.address) || comment.contains(&self.data.user);
            !matches
        });
        let session = session!(mut self);

        for key in pub_keys {
            if session
                .authenticate_publickey_with(&self.data.user, key, hash_alg, &mut agent)
                .await
                .is_ok()
            {
                info!("SSH agent authentication succeeded");
                return Ok(true);
            }
        }
        info!("SSH agent authentication failed (approval for use may be required)");

        Ok(false)
    }

    async fn try_certificate_auth(&mut self) -> Result<bool> {
        info!("Trying OpenSSH certificate authentication...");

        let (key_path, cert_path) = match (&self.data.private_key, &self.data.openssh_cert) {
            (Some(k), Some(c)) => (k, c),
            _ => {
                info!("OpenSSH certificate or private key not provided");
                return Ok(false);
            }
        };
        let key = load_private_key(key_path).map_err(SessionError::PrivateKey)?;
        let cert = load_openssh_certificate(cert_path).map_err(SessionError::OpenSSHCert)?;

        let session = session!(mut self);

        session
            .authenticate_openssh_cert(&self.data.user, Arc::new(key), cert)
            .await
            .map_err(SessionError::OpenSSHCertAuth)?;

        info!("OpenSSH certificate authentication succeeded");

        Ok(true)
    }

    async fn try_publickey_auth(&mut self, hash_alg: Option<HashAlg>) -> Result<bool> {
        info!("Trying public key authentication...");

        let key_path = match &self.data.private_key {
            Some(k) => k,
            None => {
                info!("No private key provided");
                return Ok(false);
            }
        };
        let key = load_private_key(key_path).map_err(SessionError::PrivateKey)?;
        let pair = PrivateKeyWithHashAlg::new(Arc::new(key), hash_alg);

        let session = session!(mut self);

        session
            .authenticate_publickey(&self.data.user, pair)
            .await
            .map_err(SessionError::PubKeyAuth)?;

        info!("Public key authentication succeeded");

        Ok(true)
    }

    async fn try_keyboard_interactive_auth(&mut self) -> Result<bool> {
        info!("Trying keyboard-interactive authentication...");

        let session = session!(mut self);
        let mut response = session
            .authenticate_keyboard_interactive_start(&self.data.user, None)
            .await
            .map_err(SessionError::KeyboardInteractive)?;

        loop {
            match response {
                KeyboardInteractiveAuthResponse::Success => {
                    info!("Keyboard-interactive authentication succeeded");
                    return Ok(true);
                }
                KeyboardInteractiveAuthResponse::Failure { .. } => {
                    info!("Keyboard-interactive authentication failed");
                    return Ok(false);
                }
                KeyboardInteractiveAuthResponse::InfoRequest {
                    name,
                    instructions,
                    prompts,
                } => {
                    info!("Keyboard-interactive authentication request received");

                    if !name.is_empty() {
                        println!("{}", name);
                    }
                    if !instructions.is_empty() {
                        println!("\n{}", instructions);
                    }
                    let mut responses = Vec::new();
                    for prompt in prompts {
                        let answer = if prompt.echo {
                            prompt!(echo => "{}", prompt.prompt)
                        } else {
                            prompt!("{}", prompt.prompt).expose_secret().into()
                        };

                        responses.push(answer);
                    }
                    response = session
                        .authenticate_keyboard_interactive_respond(responses)
                        .await
                        .map_err(SessionError::KeyboardInteractive)?;

                    info!("Keyboard-interactive authentication response sent");
                }
            }
        }
    }

    async fn try_password_auth(&mut self) -> Result<bool> {
        info!("Trying password authentication...");

        let session = session!(mut self);
        let password = prompt!("{}@{}'s password", self.data.user, self.data.address);

        session
            .authenticate_password(&self.data.user, password.expose_secret())
            .await
            .map_err(SessionError::PasswordAuth)?;

        info!("Password authentication succeeded");

        Ok(true)
    }
}

#[inline]
fn load_private_key(key_path: &Path) -> Result<PrivateKey, russh::keys::Error> {
    info!(
        "Trying to load private key from '{}'...",
        key_path.display()
    );

    match load_secret_key(key_path, None) {
        Ok(key) => {
            info!("Private key loaded successfully (without passphrase)");
            return Ok(key);
        }
        Err(russh::keys::Error::KeyIsEncrypted) => {}
        Err(e) => return Err(e),
    }
    info!("Private key is encrypted, prompting for passphrase...");

    let key_path_display = key_path.display();
    for _ in 1..=MAX_PASSPHRASE_ATTEMPTS {
        let passphrase = prompt!("Enter passphrase for key '{key_path_display}'");

        match load_secret_key(key_path, Some(passphrase.expose_secret())) {
            Ok(key) => {
                info!("Private key loaded successfully");
                return Ok(key);
            }
            Err(russh::keys::Error::SshKey(ssh_key::Error::Crypto)) => continue,
            Err(e) => return Err(e),
        }
    }
    info!(
        "Maximum passphrase attempts reached ({}), failed to load private key",
        MAX_PASSPHRASE_ATTEMPTS
    );

    Err(russh::keys::Error::SshKey(ssh_key::Error::Crypto))
}

struct RawModeGuard;

impl RawModeGuard {
    fn new() -> Result<Self> {
        enable_raw_mode()?;
        Ok(Self)
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
    }
}

async fn run_session(channel: &mut Channel<Msg>) -> Result<()> {
    let mut stdin = tokio_fd::AsyncFd::try_from(STDIN_FD)?;
    let mut stdout = tokio_fd::AsyncFd::try_from(STDOUT_FD)?;

    let mut buf = [0u8; SESSION_BUFFER_SIZE];
    let mut stdin_closed = false;

    let (mut width, mut height) = terminal::size()?;
    let mut resize_check = tokio::time::interval(Duration::from_millis(RESIZE_INTERVAL_MS));

    let _guard = RawModeGuard::new()?;

    loop {
        tokio::select! {
            outgoing = stdin.read(&mut buf), if !stdin_closed => {
                match outgoing {
                    Ok(0) => {
                        stdin_closed = true;
                        _ = channel.eof().await;
                    }
                    Ok(n) => channel.data(&buf[..n]).await?,
                    Err(e) => return Err(e.into()),
                }
            }
            incoming = channel.wait() => {
                if let Some(msg) = incoming {
                    match msg {
                        ChannelMsg::Data { data } => {
                            stdout.write_all(&data).await?;
                            stdout.flush().await?;
                        }
                        ChannelMsg::ExitStatus { exit_status: _ } => {
                            if !stdin_closed {
                                _ = channel.eof().await;
                            }
                            break;
                        }
                        _ => {}
                    }
                } else {
                    break;
                }
            }
            _ = resize_check.tick() => {
                if let Ok((w, h)) = terminal::size() && (w, h) != (width, height) {
                    (width, height) = (w, h);
                    channel.window_change(w.into(), h.into(), 0, 0).await?;
                }
            }
        }
    }

    Ok(())
}
