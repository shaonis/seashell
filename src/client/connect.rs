use std::{
    env,
    io::Write,
    net::{IpAddr, SocketAddr},
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use anyhow::Result;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use russh::{
    Channel, ChannelMsg,
    client::{self, Config, Handle, Handler, Msg},
    keys::{
        load_openssh_certificate,
        load_secret_key,
        ssh_key,
        HashAlg,
        PrivateKey,
        PrivateKeyWithHashAlg,
        PublicKey,
        PublicKeyBase64,
    },
};
use tokio::{fs, io::{AsyncReadExt, AsyncWriteExt}, net::lookup_host};

use crate::{
    client::data::ConnectionData,
    error::{ConnectionError, FileError, SessionError},
};


pub async fn establish_connection(data: ConnectionData) -> Result<()> {
    // Maybe not a socket
    let sock = format!("{}:{}", data.address, data.port);
    let socket: SocketAddr;
    if let Ok(s) = sock.parse::<SocketAddr>() {
        socket = s;
    } else {
        socket = lookup_host(sock)
            .await
            .map_err(ConnectionError::Dns)?
            .next()
            .expect("address should be resolved");
    }

    let handler = ClientHandler::new(
        socket.ip(),
        data.known_hosts.clone(),
    );
    let config = Config {
        inactivity_timeout: Some(Duration::from_secs(300)),
        keepalive_interval: Some(Duration::from_secs(60)),
        keepalive_max: 3,
        ..Default::default()
    };
    let mut session = client::connect(Arc::new(config), socket, handler).await?;
    auth_session(&data, &mut session).await?;
    let mut channel = session.channel_open_session().await?;
    adjust_terminal(&mut channel).await?;
    let result = run_session(&mut channel).await;
    println!("Connection to {} closed.", socket.ip());

    result
}

#[inline]
fn get_private_key(key_path: &Path) -> Result<PrivateKey, russh::keys::Error> {
    // Try without passphrase
    match load_secret_key(key_path, None) {
        Ok(key) => return Ok(key),
        Err(russh::keys::Error::KeyIsEncrypted) => {},
        Err(e) => return Err(e),
    }
    let key_path_display = key_path.display();
    loop {
        print!("Enter passphrase for key '{key_path_display}': ");
        std::io::stdout().flush()?;
        let passphrase = rpassword::read_password()?;
        match load_secret_key(key_path, Some(&passphrase)) {
            Ok(key) => return Ok(key),
            Err(russh::keys::Error::SshKey(ssh_key::Error::Crypto)) => continue,
            Err(e) => return Err(e),
        }
    }
}

async fn auth_session(data: &ConnectionData, session: &mut Handle<ClientHandler>) -> Result<()> {
    // Certificate
    if let (Some(key_path), Some(cert_path)) = (&data.private_key, &data.openssh_cert) {
        let key = get_private_key(key_path).map_err(SessionError::PrivateKey)?;
        let cert = load_openssh_certificate(cert_path).map_err(SessionError::OpenSSHCert)?;
        session.authenticate_openssh_cert(&data.user, Arc::new(key), cert)
            .await
            .map_err(SessionError::OpenSSHCertAuth)?;

        return Ok(())
    }
    // Public key
    if let Some(key_path) = &data.private_key {
        let key = get_private_key(key_path).map_err(SessionError::PrivateKey)?;
        let hash_alg = session.best_supported_rsa_hash()
            .await?
            .unwrap_or(Some(HashAlg::default()));

        let pair = PrivateKeyWithHashAlg::new(Arc::new(key), hash_alg);
        session.authenticate_publickey(&data.user, pair)
            .await
            .map_err(SessionError::PubKeyAuth)?;

        return Ok(())
    }
    // Password
    print!("{}@{}'s password: ", data.user, data.address);
    std::io::stdout().flush()?;
    let password = rpassword::read_password()?;
    session.authenticate_password(&data.user, password)
        .await
        .map_err(SessionError::PasswordAuth)?;

    Ok(())
}

async fn adjust_terminal(channel: &mut Channel<Msg>) -> Result<()> {
    let (width, height) = crossterm::terminal::size().map_err(FileError::from)?;
    let term = env::var("TERM").unwrap_or_else(|_| "xterm".into());
    channel
        .request_pty(false, &term, width.into(), height.into(), 0, 0, &[])
        .await
        .map_err(SessionError::Terminal)?;

    channel
        .request_shell(true)
        .await
        .map_err(SessionError::Terminal)?;

    Ok(())
}

async fn run_session(channel: &mut Channel<Msg>) -> Result<()> {
    // Async stdin/stdout
    let mut stdin = tokio_fd::AsyncFd::try_from(0)?;
    let mut stdout = tokio_fd::AsyncFd::try_from(1)?;
    // Buffer
    let mut buf = [0u8; 4096];
    let mut stdin_closed = false;
    // For terminal dynamic resizing
    let (mut width, mut height) = crossterm::terminal::size()?;
    let mut resize_check = tokio::time::interval(Duration::from_millis(200));

    enable_raw_mode()?;

    loop {
        tokio::select! {
            // Read from stdin and forward to channel
            outgoing = stdin.read(&mut buf), if !stdin_closed => {
                match outgoing {
                    Ok(0) => {
                        stdin_closed = true;
                        _ = channel.eof().await;
                    },
                    Ok(n) => channel.data(&buf[..n]).await?,
                    Err(e) => return Err(e.into()),
                };
            },
            // Process incoming channel messages
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
                            break
                        }
                        _ => {}
                    }
                } else {
                    // Channel closed
                    break
                }
            },
            // Handle terminal resizing
            _ = resize_check.tick() => {
                if let Ok((w, h)) = crossterm::terminal::size() {
                    if (w, h) != (width, height) {
                        (width, height) = (w, h);
                        channel.window_change(w.into(), h.into(), 0, 0).await?;
                    }
                }
            },
        }
    }
    disable_raw_mode()?;

    Ok(())
}

#[derive(Debug)]
struct ClientHandler {
    server_ip: IpAddr,
    known_hosts: PathBuf,
}

impl Handler for ClientHandler {
    type Error = anyhow::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &PublicKey,
    ) -> Result<bool, Self::Error> {
        // Ensure known_hosts exists
        if !self.known_hosts.exists() {
            if let Some(parent) = self.known_hosts.parent() {
                fs::create_dir_all(parent).await.map_err(FileError::from)?;
            }
            fs::write(&self.known_hosts, "").await.map_err(FileError::from)?;
        }
        let (server_ip, key_alg, key_b64) = (
            self.server_ip.to_string(),
            server_public_key.algorithm(),
            server_public_key.public_key_base64(),
        );
        let content = fs::read_to_string(&self.known_hosts).await.map_err(FileError::from)?;
        if content.lines().any(|line| self.is_valid_entry(line, &server_ip, key_alg.as_str(), &key_b64)) {
            return Ok(true);
        }

        self.handle_unknown_host(server_public_key).await
    }
}

impl ClientHandler {
    fn new(server_ip: IpAddr, known_hosts: PathBuf) -> Self {
        Self { server_ip, known_hosts }
    }

    fn is_valid_entry(&self, line: &str, server_ip: &str, key_alg: &str, key_b64: &str) -> bool {
        if line.trim_start().starts_with('#') {
            return false
        }
        let mut parts = line.split_whitespace();
        if [
            parts.next().map(|host| host == server_ip),
            parts.next().map(|alg| alg == key_alg),
            parts.next().map(|b64| b64 == key_b64),
        ]
            .into_iter()
            .all(|x| x.unwrap_or(false))
        {
            return true
        }

        false
    }

    async fn handle_unknown_host(&self, key: &PublicKey) -> Result<bool> {
        let key_alg = key.algorithm();
        let fingerprint = key.fingerprint(HashAlg::default());

        print!(
            "The authenticity of host '{}' can't be established.\n\
            {} key fingerprint is SHA256:{}.\n\
            This key is not known by any other names.\n\
            Are you sure you want to continue connecting (yes/no/[fingerprint])? ",
            self.server_ip,
            key_alg,
            fingerprint,
        );
        std::io::stdout().flush()?;

        let mut input = String::new();
        std::io::stdin().read_line(&mut input).map_err(FileError::from)?;
        let input = input.trim();

        if input.eq_ignore_ascii_case("yes") || input.as_bytes() == fingerprint.as_bytes() {
            self.trust_host(key).await?;
            println!(
                "Warning: Permanently added '{}' ({}) to the list of known hosts.",
                self.server_ip, key_alg,
            );
            return Ok(true)
        }

        Ok(false)
    }

    async fn trust_host(&self, key: &PublicKey) -> Result<()> {
        let entry = format!(
            "{} {} {}\n",
            self.server_ip,
            key.algorithm(),
            key.public_key_base64()
        );
        fs::write(&self.known_hosts, entry)
            .await
            .map_err(FileError::from)?;

        Ok(())
    }
}
