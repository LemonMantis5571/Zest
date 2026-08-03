//! Supervision for the local CLIProxyAPI process.
//!
//! Codex and Claude reach their subscriptions through a gateway running on
//! localhost. That process is not a service and nothing was keeping it alive, so
//! the ordinary state of a fresh boot was "installed, signed in, not running" —
//! which surfaced as a failed turn several seconds later.
//!
//! Starting it is Zest's job rather than the user's: Zest already knows where the
//! binary is, because it launches the same binary to perform the sign-in.

use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::path::PathBuf;
use std::time::Duration;

use crate::auth::{cliproxy_exe, cliproxy_install, spawn_detached};
use crate::fsutil::atomic_write;

/// How long to wait for a TCP connect before calling the port dead. Loopback
/// either answers immediately or is not listening.
const PROBE_TIMEOUT: Duration = Duration::from_millis(400);
/// Budget for a cold start. The binary is ~64MB and reads its config before it
/// binds, so the first accept can be a second or two behind the spawn.
const START_TIMEOUT: Duration = Duration::from_secs(12);
const POLL_INTERVAL: Duration = Duration::from_millis(150);

/// What [`ensure_running`] found or did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GatewayState {
    /// Something is accepting connections — it was already up, or we started it.
    Listening,
    /// Not a local gateway URL, so its availability is not ours to manage.
    NotLocal,
    /// No CLIProxyAPI install found, so there is nothing to start. The user's
    /// gateway is someone else's process.
    NotInstalled,
    /// Installed, spawned, and still not accepting inside the budget.
    Unavailable(String),
}

/// Make sure a local gateway is accepting before a turn is attempted.
///
/// Returns [`GatewayState::Listening`] both when the process was already up and
/// when this call started it — the caller only needs to know whether to proceed.
/// Never an error type: every outcome here is something the caller reports
/// alongside a real failure rather than instead of one.
pub async fn ensure_running(base_url: &str) -> GatewayState {
    let Some(addr) = local_origin(base_url) else {
        return GatewayState::NotLocal;
    };
    if port_open(addr) {
        return GatewayState::Listening;
    }
    let (exe, config) = match runtime() {
        Ok(Some(pair)) => pair,
        Ok(None) => return GatewayState::NotInstalled,
        Err(e) => return GatewayState::Unavailable(e),
    };

    let args = vec!["-config".to_string(), config.to_string_lossy().into_owned()];
    if let Err(e) = spawn_detached(&exe, &args) {
        return GatewayState::Unavailable(format!("could not start {}: {e}", exe.display()));
    }

    // Poll rather than sleeping the whole budget: a warm start is usually ready
    // in well under a second, and this is on the path to the user's first turn.
    let deadline = START_TIMEOUT;
    let mut waited = Duration::ZERO;
    while waited < deadline {
        tokio::time::sleep(POLL_INTERVAL).await;
        waited += POLL_INTERVAL;
        if port_open(addr) {
            return GatewayState::Listening;
        }
    }

    GatewayState::Unavailable(format!(
        "started {} but {addr} did not accept within {}s",
        exe.display(),
        START_TIMEOUT.as_secs()
    ))
}

/// Whether a local gateway is accepting right now, without starting anything.
pub fn is_listening(base_url: &str) -> bool {
    local_origin(base_url).is_some_and(port_open)
}

/// Port every Zest-provisioned gateway binds. Fixed rather than chosen at random
/// because it is already written into `zest.toml` files as `base_url`.
pub const DEFAULT_PORT: u16 = 8317;

/// Environment variable the provider config resolves its gateway key through.
pub const GATEWAY_KEY_ENV: &str = "ZEST_GATEWAY_KEY";

/// Override for [`gateway_dir`]: portable installs that keep everything on one
/// volume, and tests that must not write into the real user profile.
pub const GATEWAY_DIR_ENV: &str = "ZEST_GATEWAY_DIR";

/// Zest's own directory for gateway files.
///
/// Not beside the binary: a bundled gateway is installed somewhere read-only
/// (Program Files, /Applications), so the config cannot live next to it the way
/// a hand-installed one does.
pub fn gateway_dir() -> Option<PathBuf> {
    if let Ok(raw) = std::env::var(GATEWAY_DIR_ENV) {
        let dir = PathBuf::from(raw.trim());
        if !dir.as_os_str().is_empty() {
            return Some(dir);
        }
    }
    Some(dirs::config_dir()?.join("zest").join("gateway"))
}

/// The gateway binary and the config to run it with, or `None` when no binary is
/// available at all.
///
/// One resolver for both starting the gateway and signing in through it, so a
/// login cannot land its credentials in a different `auth-dir` than the one the
/// serving process reads.
pub fn runtime() -> Result<Option<(PathBuf, PathBuf)>, String> {
    // A hand-installed gateway keeps its own config: it may be tuned, its
    // `api-keys` are already agreed with the user's `zest.toml`, and replacing
    // it with a generated one would break a setup that works.
    if let Some(pair) = cliproxy_install() {
        return Ok(Some(pair));
    }
    let Some(exe) = cliproxy_exe() else {
        return Ok(None);
    };
    Ok(Some((exe, provision()?.config)))
}

/// Config Zest generated (or adopted) for the local gateway.
#[derive(Debug, Clone)]
pub struct Provisioned {
    pub config: PathBuf,
    pub api_key: String,
}

/// Make sure a gateway config exists and that Zest knows its key.
///
/// Idempotent. An existing `config.yaml` is left exactly as it is — it may have
/// been hand-tuned, and silently rewriting a file the user edited is worse than
/// carrying their settings forward. Delete it to get a fresh one.
pub fn provision() -> Result<Provisioned, String> {
    let dir = gateway_dir().ok_or("no config directory for this user")?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("create {}: {e}", dir.display()))?;

    let api_key = resolve_key(&dir)?;
    let config = dir.join("config.yaml");
    if !config.is_file() {
        atomic_write(&config, config_yaml(&api_key).as_bytes())
            .map_err(|e| format!("write {}: {e}", config.display()))?;
    }

    // The provider config names this variable rather than holding a key. Set it
    // for this process when the environment did not already supply one, so a
    // generated key reaches the request that spends it.
    if std::env::var(GATEWAY_KEY_ENV).is_err() {
        std::env::set_var(GATEWAY_KEY_ENV, &api_key);
    }

    Ok(Provisioned { config, api_key })
}

/// The key to authenticate with, in preference order: the environment, a key
/// generated on a previous run, then a fresh one.
///
/// The environment wins so an existing hand-configured install keeps working —
/// its `zest.toml` and its gateway already agree on a key, and overriding that
/// would break a setup that was fine.
fn resolve_key(dir: &std::path::Path) -> Result<String, String> {
    if let Ok(key) = std::env::var(GATEWAY_KEY_ENV) {
        let key = key.trim().to_string();
        if !key.is_empty() {
            return Ok(key);
        }
    }

    let path = dir.join("gateway.key");
    if let Ok(saved) = std::fs::read_to_string(&path) {
        let saved = saved.trim().to_string();
        if !saved.is_empty() {
            return Ok(saved);
        }
    }

    let key = generate_key();
    atomic_write(&path, key.as_bytes()).map_err(|e| format!("write {}: {e}", path.display()))?;
    Ok(key)
}

/// 256 bits of OS entropy, hex encoded.
///
/// This key is the only thing standing between any local process and a
/// subscription, so it comes from the OS rather than from a clock the way
/// thread ids do.
fn generate_key() -> String {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("OS entropy unavailable");
    let mut key = String::from("zest-");
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(key, "{byte:02x}");
    }
    key
}

/// Loopback-only, management disabled, sharing the credential store the vendor
/// CLIs and any hand-installed gateway already use — so bundling Zest's own
/// gateway does not orphan sign-ins that already happened.
fn config_yaml(api_key: &str) -> String {
    format!(
        "# Generated by Zest. Delete this file to regenerate it.\n\
         host: \"127.0.0.1\"\n\
         port: {DEFAULT_PORT}\n\
         tls:\n  \
           enable: false\n\
         remote-management:\n  \
           allow-remote: false\n  \
           secret-key: \"\"\n  \
           disable-control-panel: true\n\
         auth-dir: \"~/.cli-proxy-api\"\n\
         api-keys:\n  \
           - \"{api_key}\"\n\
         debug: false\n"
    )
}

/// Resolve a base URL to a loopback socket address.
///
/// Returns `None` for anything not on this machine. A remote gateway may be
/// perfectly healthy and is in any case not ours to spawn, so it must not be
/// reported as "not installed".
fn local_origin(base_url: &str) -> Option<SocketAddr> {
    let url = reqwest::Url::parse(base_url).ok()?;
    let host = url.host_str()?;
    let port = url.port_or_known_default()?;
    let addr = (host, port).to_socket_addrs().ok()?.next()?;
    addr.ip().is_loopback().then_some(addr)
}

fn port_open(addr: SocketAddr) -> bool {
    TcpStream::connect_timeout(&addr, PROBE_TIMEOUT).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn a_listening_loopback_port_is_detected() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        assert!(is_listening(&format!("http://127.0.0.1:{port}")));
    }

    #[test]
    fn a_closed_loopback_port_is_not_listening() {
        // Bind then drop: a real port number with nothing accepting on it.
        let port = {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            listener.local_addr().unwrap().port()
        };
        assert!(!is_listening(&format!("http://127.0.0.1:{port}")));
    }

    /// A gateway on another host is not this machine's process to supervise, and
    /// reporting it as "not installed" would blame the wrong thing.
    #[tokio::test]
    async fn a_remote_gateway_is_left_alone() {
        assert_eq!(
            ensure_running("https://gateway.example.com").await,
            GatewayState::NotLocal
        );
        assert!(!is_listening("https://gateway.example.com"));
    }

    /// Opt-in: starts the real CLIProxyAPI and leaves it running.
    ///
    /// Ignored by default because it spawns a long-lived process and needs an
    /// actual install, which a CI box will not have. Run it when changing the
    /// spawn or the readiness poll:
    ///
    /// ```text
    /// cargo test -p zest-core --lib gateway -- --ignored > out.txt 2>&1
    /// ```
    ///
    /// Redirect rather than pipe. `std::process::Command` sets
    /// `bInheritHandles = TRUE`, so the daemon inherits the console pipe and a
    /// reader on the other end never sees EOF — the test passes and the shell
    /// appears to hang. Harmless for the desktop app, whose stdout is not a pipe.
    #[tokio::test]
    #[ignore = "spawns the real gateway process"]
    async fn starts_a_real_install() {
        if cliproxy_install().is_none() {
            eprintln!("no CLIProxyAPI install found — nothing to start");
            return;
        }
        let url = "http://127.0.0.1:8317";
        assert_eq!(ensure_running(url).await, GatewayState::Listening);
        assert!(is_listening(url), "should be accepting after ensure_running");
        // Idempotent: a second call finds it already up and must not spawn again.
        assert_eq!(ensure_running(url).await, GatewayState::Listening);
    }

    /// Serializes the tests that touch process-wide environment variables.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "zest-gateway-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn a_generated_key_is_unpredictable_and_long_enough() {
        let a = generate_key();
        let b = generate_key();
        assert_ne!(a, b, "two keys must never collide");
        // "zest-" plus 32 bytes hex.
        assert_eq!(a.len(), 5 + 64, "{a}");
        assert!(a.starts_with("zest-"));
        assert!(a[5..].chars().all(|c| c.is_ascii_hexdigit()), "{a}");
    }

    #[test]
    fn the_generated_config_binds_loopback_only() {
        let yaml = config_yaml("zest-abc");
        // Binding all interfaces would expose a subscription to the network.
        assert!(yaml.contains("host: \"127.0.0.1\""), "{yaml}");
        assert!(yaml.contains(&format!("port: {DEFAULT_PORT}")), "{yaml}");
        assert!(yaml.contains("- \"zest-abc\""), "{yaml}");
        // Remote management off, and pointed at the shared credential store so
        // sign-ins that already happened are not orphaned.
        assert!(yaml.contains("allow-remote: false"), "{yaml}");
        assert!(yaml.contains("auth-dir: \"~/.cli-proxy-api\""), "{yaml}");
    }

    #[test]
    fn provisioning_is_idempotent_and_keeps_its_key() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = scratch("provision");
        std::env::set_var(GATEWAY_DIR_ENV, &dir);
        std::env::remove_var(GATEWAY_KEY_ENV);

        let first = provision().unwrap();
        assert!(first.config.is_file());
        assert!(std::env::var(GATEWAY_KEY_ENV).unwrap() == first.api_key);

        // A second run must not mint a new key: the gateway is already holding
        // the old one, and rotating it silently would stop it authenticating.
        std::env::remove_var(GATEWAY_KEY_ENV);
        let second = provision().unwrap();
        assert_eq!(first.api_key, second.api_key);
        assert_eq!(first.config, second.config);

        std::env::remove_var(GATEWAY_DIR_ENV);
        std::env::remove_var(GATEWAY_KEY_ENV);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_existing_key_in_the_environment_wins() {
        // A hand-configured install already agreed a key with its `zest.toml`.
        // Generating a different one would break a setup that works.
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = scratch("adopt");
        std::env::set_var(GATEWAY_DIR_ENV, &dir);
        std::env::set_var(GATEWAY_KEY_ENV, "already-agreed");

        let provisioned = provision().unwrap();
        assert_eq!(provisioned.api_key, "already-agreed");
        let yaml = std::fs::read_to_string(&provisioned.config).unwrap();
        assert!(yaml.contains("- \"already-agreed\""), "{yaml}");
        assert!(
            !dir.join("gateway.key").exists(),
            "must not persist a key it did not generate"
        );

        std::env::remove_var(GATEWAY_DIR_ENV);
        std::env::remove_var(GATEWAY_KEY_ENV);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_hand_edited_config_is_never_overwritten() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = scratch("preserve");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("config.yaml"), "port: 9999\n").unwrap();
        std::env::set_var(GATEWAY_DIR_ENV, &dir);
        std::env::remove_var(GATEWAY_KEY_ENV);

        let provisioned = provision().unwrap();
        let yaml = std::fs::read_to_string(&provisioned.config).unwrap();
        assert_eq!(yaml, "port: 9999\n", "user edits survive");

        std::env::remove_var(GATEWAY_DIR_ENV);
        std::env::remove_var(GATEWAY_KEY_ENV);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Opt-in: the shape a downloaded Zest actually runs in.
    ///
    /// A sidecar binary with no `config.yaml` beside it and no `tools/` checkout
    /// to walk up to. Proves the gateway is provisioned and started from nothing
    /// but the bundled executable, which is the whole point of shipping it.
    ///
    /// Requires port 8317 to be free, so stop any running gateway first:
    ///
    /// ```text
    /// Stop-Process -Name cli-proxy-api -Force
    /// cargo test -p zest-core --lib bundled_shape -- --ignored > out.txt 2>&1
    /// ```
    // Holding the lock across the await is the point: the environment has to
    // stay ours for the whole spawn, which is the thing being awaited.
    #[allow(clippy::await_holding_lock)]
    #[tokio::test]
    #[ignore = "spawns the real gateway process"]
    async fn a_bundled_sidecar_needs_nothing_else() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        // The sidecar `scripts/fetch-gateway.ps1` installs, not a dev checkout.
        let sidecar = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("desktop")
            .join("binaries")
            .join(if cfg!(windows) {
                "cli-proxy-api-x86_64-pc-windows-msvc.exe"
            } else {
                "cli-proxy-api-x86_64-unknown-linux-gnu"
            });
        if !sidecar.is_file() {
            eprintln!("no sidecar at {} - run scripts/fetch-gateway.ps1", sidecar.display());
            return;
        }
        assert!(
            !sidecar.with_file_name("config.yaml").exists(),
            "the bundled binary must have no config beside it"
        );

        let dir = scratch("bundled");
        std::env::set_var("ZEST_CLIPROXY_PATH", &sidecar);
        std::env::set_var(GATEWAY_DIR_ENV, &dir);
        std::env::remove_var(GATEWAY_KEY_ENV);

        let url = format!("http://127.0.0.1:{DEFAULT_PORT}");
        let state = ensure_running(&url).await;

        // Restore before asserting, so a failure does not leak env into the
        // rest of the suite.
        std::env::remove_var("ZEST_CLIPROXY_PATH");
        std::env::remove_var(GATEWAY_DIR_ENV);
        let key = std::env::var(GATEWAY_KEY_ENV).ok();
        std::env::remove_var(GATEWAY_KEY_ENV);

        assert_eq!(state, GatewayState::Listening, "config dir: {}", dir.display());
        assert!(dir.join("config.yaml").is_file(), "config was provisioned");
        assert!(dir.join("gateway.key").is_file(), "key was persisted");
        // The key the caller will authenticate with reached the environment.
        assert!(key.is_some_and(|k| k.starts_with("zest-")));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_url_with_no_port_falls_back_to_the_scheme_default() {
        // Only that parsing succeeds — 127.0.0.1:80 is very unlikely to be up.
        assert!(local_origin("http://127.0.0.1").is_some());
        assert!(local_origin("not a url").is_none());
    }
}
