//! SSH tunnel management.
//!
//! Establishes a local port-forward tunnel through a bastion/jump host via
//! the system `ssh` binary. The tunnel binds a random local port and forwards
//! it to the remote database host. The database driver connects to
//! `127.0.0.1:<local_port>`.
//!
//! ```text
//!   Catalyst DBench App
//!        │  SSH (encrypted, key-based or agent)
//!        ▼
//!   Bastion Host (jump server)
//!        │  Internal network
//!        ▼
//!   Database Server :5432
//! ```
//!
//! # Authentication
//! - `Agent`: uses the running `ssh-agent` / 1Password SSH agent (recommended)
//! - `PrivateKey`: key file path; passphrase (if any) retrieved from the OS keychain
//! - `Password`: not supported in batch mode — use key or agent auth

use std::{
    net::{SocketAddr, TcpListener},
    path::PathBuf,
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};

use crate::{Result, SshError};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// SSH authentication method.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum SshAuthMethod {
    /// Use the local SSH agent (`ssh-agent`, 1Password SSH agent, etc.).
    Agent,
    /// Authenticate with a private key file.
    PrivateKey {
        /// Path to the private key file (e.g. `~/.ssh/id_ed25519`).
        key_path: PathBuf,
        /// Connection ID for looking up the key passphrase from the OS keychain.
        /// `None` means the key has no passphrase.
        passphrase_keychain_id: Option<String>,
    },
    /// Password authentication (not supported in batch SSH mode).
    Password {
        /// Connection ID for looking up the SSH password from the OS keychain.
        keychain_id: String,
    },
}

/// Configuration for an SSH tunnel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshTunnelConfig {
    /// SSH server hostname (bastion / jump host).
    pub ssh_host: String,
    /// SSH server port (default: 22).
    #[serde(default = "default_ssh_port")]
    pub ssh_port: u16,
    /// SSH username on the bastion host.
    pub ssh_username: String,
    /// Authentication method.
    pub auth: SshAuthMethod,
    /// Database host as seen from the bastion (often `localhost` or an internal hostname).
    pub remote_host: String,
    /// Database port on the remote host.
    pub remote_port: u16,
}

const fn default_ssh_port() -> u16 { 22 }

// ---------------------------------------------------------------------------
// Active tunnel handle
// ---------------------------------------------------------------------------

/// A live SSH port-forward tunnel.
///
/// The underlying `ssh -N -L` child process is killed automatically when
/// this struct is dropped.
pub struct SshTunnel {
    /// Local `127.0.0.1:<port>` address that the database driver should connect to.
    pub local_addr: SocketAddr,
    // Held for its Drop impl which kills the ssh child process.
    _inner: SshTunnelInner,
}

struct SshTunnelInner {
    child: std::process::Child,
}

impl Drop for SshTunnelInner {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait(); // reap zombie to avoid resource leak
    }
}

impl SshTunnel {
    /// Establish an SSH port-forward tunnel as described by `config`.
    ///
    /// Returns once the local listener is confirmed to be accepting connections
    /// (timeout: 5 s). Fails if `ssh` is not in `PATH`, authentication fails,
    /// or the remote port cannot be forwarded.
    ///
    /// # Errors
    /// - [`SshError::Connect`] — `ssh` binary not found or TCP setup failure
    /// - [`SshError::Auth`] — unsupported auth method
    /// - [`SshError::PortForward`] — no free local port or tunnel did not start in time
    pub async fn connect(config: SshTunnelConfig) -> Result<Self> {
        which_ssh().map_err(|e| SshError::ConnectionFailed {
            host: "ssh".into(),
            port: 22,
            reason: e,
        })?;

        let local_port = find_free_port()
            .map_err(|e| SshError::PortForward(format!("no free local port: {e}")))?;

        tracing::info!(
            ssh_host = %config.ssh_host,
            ssh_port = config.ssh_port,
            remote = %format!("{}:{}", config.remote_host, config.remote_port),
            local_port,
            "Establishing SSH tunnel"
        );

        let ssh_host = config.ssh_host.clone();
        let ssh_port = config.ssh_port;
        let child = spawn_ssh(&config, local_port)
            .map_err(|e| SshError::ConnectionFailed {
                host: ssh_host,
                port: ssh_port,
                reason: e.to_string(),
            })?;

        let local_addr: SocketAddr = format!("127.0.0.1:{local_port}")
            .parse()
            .expect("valid addr");

        let tunnel = Self { local_addr, _inner: SshTunnelInner { child } };

        wait_for_port(local_port)
            .await
            .map_err(|e| SshError::PortForward(e))?;

        tracing::info!(local_addr = %tunnel.local_addr, "SSH tunnel ready");
        Ok(tunnel)
    }

    /// Returns `"127.0.0.1"` — the host string to pass to the database driver.
    #[must_use]
    pub fn local_host(&self) -> &str { "127.0.0.1" }

    /// Returns the local port to pass to the database driver.
    #[must_use]
    pub fn local_port(&self) -> u16 { self.local_addr.port() }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn which_ssh() -> std::result::Result<(), String> {
    std::process::Command::new("ssh")
        .arg("-V")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|_| ())
        .map_err(|_| {
            "ssh binary not found in PATH. Install OpenSSH to use SSH tunnels.".into()
        })
}

fn spawn_ssh(
    config: &SshTunnelConfig,
    local_port: u16,
) -> std::io::Result<std::process::Child> {
    let forward = format!(
        "127.0.0.1:{local_port}:{}:{}",
        config.remote_host, config.remote_port
    );

    let mut cmd = std::process::Command::new("ssh");
    cmd.args([
        "-N",
        "-o", "ExitOnForwardFailure=yes",
        "-o", "StrictHostKeyChecking=accept-new",
        "-o", "BatchMode=yes",
        "-o", "ServerAliveInterval=15",
        "-o", "ServerAliveCountMax=3",
        "-L", &forward,
        "-p", &config.ssh_port.to_string(),
    ]);

    match &config.auth {
        SshAuthMethod::Agent => {
            // ssh picks up SSH_AUTH_SOCK automatically.
        }
        SshAuthMethod::PrivateKey { key_path, .. } => {
            cmd.args(["-i", &key_path.to_string_lossy()]);
            cmd.args(["-o", "IdentitiesOnly=yes"]);
        }
        SshAuthMethod::Password { .. } => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "SSH password auth is not supported in batch mode. Use key or agent auth.",
            ));
        }
    }

    cmd.arg(format!("{}@{}", config.ssh_username, config.ssh_host));
    cmd.stdin(std::process::Stdio::null())
       .stdout(std::process::Stdio::null())
       .stderr(std::process::Stdio::null());

    cmd.spawn()
}

async fn wait_for_port(port: u16) -> std::result::Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "SSH tunnel listener on port {port} did not become ready within 5 s"
            ));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn find_free_port() -> std::io::Result<u16> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    Ok(listener.local_addr()?.port())
}
