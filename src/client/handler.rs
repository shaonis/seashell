use crate::error::FileError;
use log::info;
use russh::client::Handler;
use russh::keys::{HashAlg, PublicKey, PublicKeyBase64};
use std::io::Write;
use std::net::IpAddr;
use std::path::PathBuf;
use tokio::fs;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

#[derive(Debug)]
pub struct ClientHandler {
    server_ip: IpAddr,
    known_hosts: PathBuf,
}

impl ClientHandler {
    pub fn new(server_ip: IpAddr, known_hosts: PathBuf) -> Self {
        Self {
            server_ip,
            known_hosts,
        }
    }

    async fn handle_unknown_host(&self, key: &PublicKey) -> anyhow::Result<bool> {
        let fingerprint = key.fingerprint(HashAlg::default());

        print!(
            "*Alright, here is the door: {}*\n\
            - Knock, knock!\n\
            - \"Greetings! I am {} {}, and you?\"\n\
            *Hmm, I don't recognize this one...*\n\n\
            Trust and add to 'known_hosts'? (yes/no/[fingerprint]): ",
            self.server_ip,
            key.algorithm(),
            fingerprint,
        );
        std::io::stdout().flush()?;

        let mut input = String::new();
        std::io::stdin()
            .read_line(&mut input)
            .map_err(FileError::Std)?;
        let input = input.trim();

        if input.eq_ignore_ascii_case("y")
            || input.eq_ignore_ascii_case("yes")
            || input.as_bytes() == fingerprint.as_bytes()
        {
            self.trust_host(key).await?;
            return Ok(true);
        }

        Ok(false)
    }

    async fn handle_key_changed(&self, key: &PublicKey) -> anyhow::Result<bool> {
        eprintln!(
            "*Ah, home sweet home: {}*\n\
            - Knock, knock!\n\
            - \"Greetings! I am {} {}, and you?\"\n\
            *Wait a minute. You are not the guy who usually lives here.*\n\
            *Did he move out? ...or are you trying to pretend to be him? (Man-in-the-Middle)*\n\
            *I better get out of here fast!*\n\n\
            We should probably forget our old key and remove it from 'known_hosts'.\n\
            Or, if this is a trap... we should report this incident!",
            self.server_ip,
            key.algorithm(),
            key.fingerprint(HashAlg::default())
        );

        Ok(false)
    }

    async fn trust_host(&self, key: &PublicKey) -> anyhow::Result<()> {
        let entry = format!(
            "{} {} {}\n",
            self.server_ip,
            key.algorithm(),
            key.public_key_base64()
        );
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.known_hosts)
            .await
            .map_err(FileError::from)?;
        file.write_all(entry.as_bytes())
            .await
            .map_err(FileError::from)?;

        Ok(())
    }
}

impl Handler for ClientHandler {
    type Error = anyhow::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &PublicKey,
    ) -> anyhow::Result<bool, Self::Error> {
        info!(
            "Checking server public key in '{}'...",
            self.known_hosts.display()
        );

        if !self.known_hosts.exists() {
            if let Some(parent) = self.known_hosts.parent() {
                fs::create_dir_all(parent).await.map_err(FileError::from)?;
            }
            fs::write(&self.known_hosts, "")
                .await
                .map_err(FileError::from)?;
        }
        let (server_ip, key_alg, key_b64) = (
            self.server_ip.to_string(),
            server_public_key.algorithm(),
            server_public_key.public_key_base64(),
        );
        let key_alg = key_alg.as_str();

        let file = fs::File::open(&self.known_hosts)
            .await
            .map_err(FileError::from)?;
        let reader = BufReader::new(file);
        let mut lines = reader.lines();

        let mut key_changed = false;
        while let Some(line) = lines.next_line().await? {
            let line = line.trim_start();
            if line.starts_with('#') || line.is_empty() {
                continue;
            }
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 && parts[0] == server_ip {
                info!("Found existing host key for '{}'", server_ip);

                key_changed = true;
                if parts[1] == key_alg && parts[2] == key_b64 {
                    info!("Server public key matches known host entry");
                    return Ok(true);
                }
                break;
            }
        }

        if key_changed {
            self.handle_key_changed(server_public_key).await
        } else {
            self.handle_unknown_host(server_public_key).await
        }
    }
}
