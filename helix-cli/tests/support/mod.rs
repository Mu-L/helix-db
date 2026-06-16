use assert_cmd::Command;
use std::fs;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use tempfile::{Builder, TempDir};

pub struct CliFixture {
    root: PathBuf,
    _tempdir: Option<TempDir>,
    home: PathBuf,
    helix_home: PathBuf,
    cache: PathBuf,
}

impl CliFixture {
    pub fn new() -> Self {
        let root = Builder::new()
            .prefix("helix-cli-e2e-")
            .tempdir()
            .expect("create e2e tempdir");
        let root_path = root.path().to_path_buf();
        let home = root_path.join("home");
        let helix_home = root_path.join("helix-home");
        let cache = root_path.join("helix-cache");
        fs::create_dir_all(&home).expect("create isolated home");
        fs::create_dir_all(&helix_home).expect("create isolated helix home");
        fs::create_dir_all(&cache).expect("create isolated cache");

        let tempdir = if std::env::var_os("HELIX_E2E_KEEP_TMP").is_some() {
            std::mem::forget(root);
            None
        } else {
            Some(root)
        };

        Self {
            root: root_path,
            _tempdir: tempdir,
            home,
            helix_home,
            cache,
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn command(&self) -> Command {
        let mut command = Command::cargo_bin("helix").expect("helix binary should be built");
        command
            .env("HELIX_NO_UPDATE_CHECK", "1")
            .env("HELIX_DISABLE_UPDATE_CHECK", "1")
            .env("NO_COLOR", "1")
            .env("HELIX_HOME", &self.helix_home)
            .env("HELIX_CACHE_DIR", &self.cache)
            .env("HOME", &self.home)
            .env("USERPROFILE", &self.home)
            .env("CLICOLOR", "0");
        command
    }
}

/// Pick a port the OS currently considers free.
///
/// There is an unavoidable TOCTOU window here: we release the listener before
/// the caller can bind, and the caller hands the port to a Docker container in
/// a child process, so we cannot keep the `fd` alive and pass it through (the
/// usual `from_std` / `SO_REUSEPORT` mitigations don't cross the container
/// boundary). This is acceptable for these e2e tests because they are
/// `#[ignore]`d behind Docker and are not run concurrently in CI; collisions
/// would surface as a loud `helix start` failure rather than silent corruption.
pub fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind free TCP port");
    let port = listener.local_addr().expect("read local addr").port();
    drop(listener);
    port
}
