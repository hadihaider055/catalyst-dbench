//! SSH tunnel management.
//!
//! Many production databases are not publicly reachable. Catalyst DBench supports
//! connecting through a bastion/jump host via SSH port forwarding.
//!
//! ```text
//!   Catalyst DBench App
//!        │  SSH (encrypted, key-based)
//!        ▼
//!   Bastion Host (jump server)
//!        │  Internal network
//!        ▼
//!   Database Server :5432
//! ```
//!
//! The tunnel binds a random local port and forwards it to the remote DB.
//! The database driver then connects to `127.0.0.1:<local_port>`.

use std::{
    net::{SocketAddr, TcpListener},
    path::PathBuf,
    sync::Arc,
};

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use zeroize::ZeroizeOnDrop;

use crate::{Result, SecurityError, SshError};

/// SSH authentication method.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum SshAuthMethod {
    /// Authenticate using a private key file.
    /// The passphrase (if any) is retrieved from the OS keychain.
    PrivateKey {
        key_path: PathBuf,
        /// Connection ID used to look up the passphrase in the keychain.
        /// If `None`, the key is assumed to have no passphrase.
        passphrase_keychain_id: Option<String>,
    },
    /// Authenticate using a password (discouraged; stored in keychain).
    Password {
        /// Connection ID used to look up the SSH password in the keychain.
        keychain_id: String,
    },
    /// Use the local SSH agent (e.g., `ssh-agent`, 1Password SSH agent).
    Agent,
}

/// Configuration for an SSH tunnel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshTunnelConfig {
    /// SSH server hostname (the bastion/jump host).
    pub ssh_host: String,
    /// SSH server port (default: 22).
    #[serde(default = "default_ssh_port")]
    pub ssh_port: u16,
    /// SSH username on the bastion host.
    pub ssh_username: String,
    /// How to authenticate to the SSH server.
    pub auth: SshAuthMethod,
    /// The database host as seen from the bastion host (often `localhost` or an internal hostname).
    pub remote_host: String,
    /// The database port on the remote host.
    pub remote_port: u16,
}

const fn default_ssh_port() -> u16 {
    22
}

/// An active SSH tunnel, binding a local port to a remote host:port.
///
/// The tunnel is closed when this struct is dropped.
pub struct SshTunnel {
    /// The local address to connect the database driver to.
    pub local_addr: SocketAddr,
    config: SshTunnelConfig,
    // In a full implementation, this would hold the `ssh2::Session` and
    // the port-forward channel. For now it's the scaffold structure.
    _session: Arc<Mutex<SshTunnelInner>>,
}

struct SshTunnelInner {
    _local_port: u16,
}

impl SshTunnel {
    /// Establish an SSH tunnel as described by `config`.
    ///
    /// Binds a random local port and forwards it to `config.remote_host:config.remote_port`.
    ///
    /// Returns a handle that, when dropped, closes the tunnel.
    ///
    /// # Errors
    /// Fails if the TCP connection to the SSH host fails, authentication fails,
    /// or port forwarding cannot be established.
    pub async fn connect(config: SshTunnelConfig) -> Result<Self> {
        // Find a free local port.
        let local_port = find_free_port().map_err(|e| {
            SshError::PortForward(format!("Cannot find free local port: {e}"))
        })?;

        tracing::info!(
            ssh_host = %config.ssh_host,
            ssh_port = config.ssh_port,
            remote = %format!("{}:{}", config.remote_host, config.remote_port),
            local_port,
            "Establishing SSH tunnel"
        );

        // TODO: Full ssh2 implementation — connect, authenticate, forward port.
        // The below is the scaffold; the full implementation uses the `ssh2` crate:
        //
        // let tcp = TcpStream::connect((config.ssh_host.as_str(), config.ssh_port)).await?;
        // let mut session = ssh2::Session::new()?;
        // session.set_tcp_stream(tcp.into_std()?);
        // session.handshake()?;
        //
        // match &config.auth {
        //     SshAuthMethod::PrivateKey { key_path, .. } => {
        //         let passphrase = ...; // fetch from keychain
        //         session.userauth_pubkey_file(&config.ssh_username, None, key_path, passphrase)?;
        //     }
        //     SshAuthMethod::Agent => {
        //         let mut agent = session.agent()?;
        //         agent.connect()?;
        //         agent.list_identities()?;
        //         // try each identity
        //     }
        //     SshAuthMethod::Password { .. } => { ... }
        // }
        //
        // let channel = session.channel_direct_tcpip(&config.remote_host, config.remote_port, None)?;
        // // Spawn a background task forwarding between local TCP and the SSH channel.

        let local_addr: SocketAddr = format!("127.0.0.1:{local_port}").parse().unwrap();

        Ok(Self {
            local_addr,
            config,
            _session: Arc::new(Mutex::new(SshTunnelInner {
                _local_port: local_port,
            })),
        })
    }

    /// Returns the local `host:port` string to pass to the database driver.
    #[must_use]
    pub fn local_host(&self) -> String {
        self.local_addr.ip().to_string()
    }

    #[must_use]
    pub fn local_port(&self) -> u16 {
        self.local_addr.port()
    }
}

/// Find a free TCP port on localhost.
fn find_free_port() -> std::io::Result<u16> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    Ok(listener.local_addr()?.port())
}
