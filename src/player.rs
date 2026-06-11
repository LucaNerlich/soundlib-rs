use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};

const DAEMON_START_TIMEOUT: Duration = Duration::from_secs(8);
const DAEMON_POLL_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Debug, Clone)]
pub struct Player {
    cliamp_bin: String,
    auto_daemon: bool,
    socket_override: Option<PathBuf>,
}

impl Player {
    pub fn new(cliamp_bin: impl Into<String>) -> Self {
        Self::with_options(cliamp_bin, true)
    }

    pub fn with_options(cliamp_bin: impl Into<String>, auto_daemon: bool) -> Self {
        Self {
            cliamp_bin: cliamp_bin.into(),
            auto_daemon,
            socket_override: None,
        }
    }

    #[doc(hidden)]
    pub fn with_socket_override(
        cliamp_bin: impl Into<String>,
        auto_daemon: bool,
        socket_override: PathBuf,
    ) -> Self {
        Self {
            cliamp_bin: cliamp_bin.into(),
            auto_daemon,
            socket_override: Some(socket_override),
        }
    }

    fn socket_path(&self) -> Result<PathBuf> {
        if let Some(path) = &self.socket_override {
            return Ok(path.clone());
        }
        let config_dir = dirs::config_dir().context("could not determine config directory")?;
        Ok(config_dir.join("cliamp").join("cliamp.sock"))
    }

    fn pid_path(&self) -> Result<PathBuf> {
        if let Some(path) = &self.socket_override {
            return Ok(path.with_extension("sock.pid"));
        }
        let config_dir = dirs::config_dir().context("could not determine config directory")?;
        Ok(config_dir.join("cliamp").join("cliamp.sock.pid"))
    }

    pub(crate) fn is_daemon_running(&self) -> bool {
        self.socket_path().map(|path| path.exists()).unwrap_or(false)
    }

    /// Stops playback and shuts down the cliamp daemon so a fresh playlist can be loaded.
    pub fn stop_daemon(&self) -> Result<()> {
        if self.is_daemon_running() {
            let _ = self.run_without_ensure(&["stop"]);
        }
        self.shutdown_daemon_process()
    }

    fn shutdown_daemon_process(&self) -> Result<()> {
        if let Ok(pid_path) = self.pid_path() {
            if let Ok(pid_raw) = fs::read_to_string(&pid_path) {
                if let Ok(pid) = pid_raw.trim().parse::<u32>() {
                    let _ = Command::new("kill").arg(pid.to_string()).status();
                }
            }
            let _ = fs::remove_file(&pid_path);
        }

        if let Ok(socket) = self.socket_path() {
            let _ = fs::remove_file(&socket);
        }

        let mut waited = Duration::from_secs(0);
        while waited < Duration::from_secs(2) {
            if !self.is_daemon_running() {
                return Ok(());
            }
            thread::sleep(DAEMON_POLL_INTERVAL);
            waited += DAEMON_POLL_INTERVAL;
        }

        Ok(())
    }

    /// Replace the current cliamp session with new local file(s) or folder(s).
    ///
    /// cliamp only loads local paths when they are passed as startup arguments to
    /// `cliamp --daemon --auto-play`. The IPC `queue` command appends to whatever
    /// is already loaded (e.g. the default radio stream) and does not switch playlists.
    pub fn play_replace(&self, paths: &[PathBuf]) -> Result<()> {
        if paths.is_empty() {
            anyhow::bail!("no paths to play");
        }

        if self.is_daemon_running() || self.pid_path().is_ok_and(|p| p.exists()) {
            self.stop_daemon()?;
        }

        self.spawn_daemon_with_paths(paths, true)?;
        self.wait_for_socket()
    }

    fn spawn_daemon_with_paths(&self, paths: &[PathBuf], auto_play: bool) -> Result<()> {
        let mut command = Command::new(&self.cliamp_bin);
        command
            .arg("--daemon")
            .arg("--log-level")
            .arg("error");

        if auto_play {
            command.arg("--auto-play");
        }

        for path in paths {
            command.arg(path);
        }

        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .with_context(|| {
                format!(
                    "failed to start {} with paths {:?}",
                    self.cliamp_bin, paths
                )
            })?;

        Ok(())
    }

    fn wait_for_socket(&self) -> Result<()> {
        let socket = self.socket_path()?;
        let mut waited = Duration::from_secs(0);

        while waited < DAEMON_START_TIMEOUT {
            if socket.exists() {
                return Ok(());
            }
            thread::sleep(DAEMON_POLL_INTERVAL);
            waited += DAEMON_POLL_INTERVAL;
        }

        anyhow::bail!(
            "cliamp daemon did not start (no socket at {})",
            socket.display()
        )
    }

    pub fn ensure_daemon(&self) -> Result<()> {
        if self.is_daemon_running() {
            return Ok(());
        }

        if !self.auto_daemon {
            anyhow::bail!(
                "cliamp is not running — press Enter on a track or folder to start playback"
            );
        }

        anyhow::bail!(
            "cliamp is not running — select something to play (Enter) to start the daemon"
        )
    }

    pub fn queue_track(&self, path: &Path) -> Result<()> {
        self.run(&["queue", &path.to_string_lossy()])
    }

    pub fn queue_tracks(&self, paths: &[impl AsRef<Path>]) -> Result<usize> {
        self.ensure_daemon()?;
        let mut queued = 0;
        for path in paths {
            self.queue_track(path.as_ref())?;
            queued += 1;
        }
        Ok(queued)
    }

    pub fn play(&self) -> Result<()> {
        self.run(&["play"])
    }

    #[allow(dead_code)]
    pub fn pause(&self) -> Result<()> {
        self.run(&["pause"])
    }

    pub fn toggle(&self) -> Result<()> {
        self.run(&["toggle"])
    }

    pub fn next(&self) -> Result<()> {
        self.run(&["next"])
    }

    pub fn prev(&self) -> Result<()> {
        self.run(&["prev"])
    }

    pub fn stop(&self) -> Result<()> {
        self.run(&["stop"])
    }

    pub(crate) fn run_for_output(&self, args: &[&str]) -> Result<String> {
        let output = Command::new(&self.cliamp_bin)
            .args(args)
            .output()
            .with_context(|| format!("failed to run {} {:?}", self.cliamp_bin, args))?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            Err(Self::command_error(
                &output.stderr,
                &output.stdout,
                &self.cliamp_bin,
                output.status,
            ))
        }
    }

    fn run_without_ensure(&self, args: &[&str]) -> Result<()> {
        self.run_for_output(args).map(|_| ())
    }

    pub(crate) fn run(&self, args: &[&str]) -> Result<()> {
        self.ensure_daemon()?;
        self.run_without_ensure(args)
    }

    fn command_error(
        stderr: &[u8],
        stdout: &[u8],
        cliamp_bin: &str,
        status: std::process::ExitStatus,
    ) -> anyhow::Error {
        let stderr = String::from_utf8_lossy(stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(stdout).trim().to_string();
        let message = if !stderr.is_empty() {
            stderr
        } else if !stdout.is_empty() {
            stdout
        } else {
            format!("{cliamp_bin} exited with {status}")
        };
        anyhow::anyhow!(message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;
    use tempfile::TempDir;

    fn mock_cliamp(log: &Path, fail: bool) -> TempDir {
        let dir = TempDir::new().expect("tempdir");
        let script = dir.path().join("mock_cliamp.sh");
        let body = if fail {
            format!(
                "#!/bin/sh\necho \"$@\" >> \"{}\"\necho fail >&2\nexit 1\n",
                log.display()
            )
        } else {
            format!("#!/bin/sh\necho \"$@\" >> \"{}\"\nexit 0\n", log.display())
        };
        fs::write(&script, body).expect("script");
        let mut perms = fs::metadata(&script).expect("meta").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script, perms).expect("chmod");
        dir
    }

    #[test]
    fn run_records_arguments_in_log() {
        let dir = TempDir::new().expect("tempdir");
        let log = dir.path().join("log.txt");
        let mock_dir = mock_cliamp(&log, false);
        let bin = mock_dir.path().join("mock_cliamp.sh");

        let player = Player::with_options(bin.to_string_lossy().to_string(), false);
        player
            .run_without_ensure(&["queue", "/tmp/song.mp3"])
            .expect("run");

        let contents = fs::read_to_string(&log).expect("log");
        assert!(contents.contains("queue"));
        assert!(contents.contains("/tmp/song.mp3"));
    }

    #[test]
    fn run_surfaces_stderr_on_failure() {
        let dir = TempDir::new().expect("tempdir");
        let log = dir.path().join("log.txt");
        let mock_dir = mock_cliamp(&log, true);
        let bin = mock_dir.path().join("mock_cliamp.sh");

        let player = Player::with_options(bin.to_string_lossy().to_string(), false);
        let err = player
            .run_without_ensure(&["play"])
            .expect_err("should fail");
        assert!(err.to_string().contains("fail"));
    }

    #[test]
    fn ensure_daemon_errors_when_not_running() {
        let dir = TempDir::new().expect("tempdir");
        let socket = dir.path().join("missing.sock");
        let player = Player::with_socket_override("cliamp", true, socket);
        let err = player.ensure_daemon().expect_err("not running");
        assert!(err.to_string().contains("not running"));
    }

    #[test]
    fn play_replace_spawns_daemon_with_paths() {
        let dir = TempDir::new().expect("tempdir");
        let log = dir.path().join("log.txt");
        let socket = dir.path().join("cliamp.sock");
        let mock_dir = mock_cliamp(&log, false);
        let bin = mock_dir.path().join("mock_cliamp.sh");
        let track = dir.path().join("song.mp3");
        fs::write(&track, b"mp3").expect("track");

        let player = Player::with_socket_override(bin.to_string_lossy().to_string(), false, socket);
        player
            .spawn_daemon_with_paths(&[track.clone()], true)
            .expect("spawn");
        thread::sleep(Duration::from_millis(50));

        let contents = fs::read_to_string(&log).expect("log");
        assert!(contents.contains("--daemon"));
        assert!(contents.contains("--auto-play"));
        assert!(contents.contains("song.mp3"));
    }
}
