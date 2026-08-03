use std::{
    collections::BTreeMap,
    env,
    fs::{self, File, OpenOptions},
    io::{self, Read},
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpStream},
    path::{Path, PathBuf},
    process::{ChildStderr, ChildStdout, Command, Output, Stdio},
    sync::{Arc, OnceLock},
    thread,
    time::Duration,
};
use url::Url;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildMode {
    Release,
    Debug,
    None,
}

impl BuildMode {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Release => "release",
            Self::Debug => "debug",
            Self::None => "none",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessInfo {
    pub pid: u32,
    pub elapsed: String,
    pub rss: String,
    pub cpu: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct StatusSnapshot {
    pub server: Option<ProcessInfo>,
    pub server_managed: bool,
    pub server_instances: usize,
    pub server_listeners: Vec<String>,
    pub server_bind: String,
    pub server_port_open: bool,
    pub admin_port_open: bool,
    pub database: DatabaseStatus,
    pub build_mode: BuildMode,
    pub binary_size: String,
    pub rust_version: String,
    pub node_version: String,
    pub npm_version: String,
    pub admin_deps: bool,
    pub admin_dist: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseBackend {
    SQLite,
    PostgreSql,
    Unknown,
}

impl DatabaseBackend {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::SQLite => "SQLite",
            Self::PostgreSql => "PostgreSQL",
            Self::Unknown => "Unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatabaseStatus {
    pub backend: DatabaseBackend,
    pub target: String,
    pub connected: bool,
    pub exists: bool,
    pub size: String,
    sqlite_path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ObservedServer {
    process: ProcessInfo,
    listeners: Vec<String>,
    database_connections: Vec<String>,
}

#[derive(Debug)]
struct ToolVersions {
    rust: String,
    node: String,
    npm: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionReport {
    pub ok: bool,
    pub message: String,
}

impl ActionReport {
    pub fn ok(message: impl Into<String>) -> Self {
        Self {
            ok: true,
            message: message.into(),
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            ok: false,
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    StartServer,
    StopServer,
    RestartServer,
    BuildDebug,
    BuildRelease,
    BuildAdmin,
    BuildAll,
    CheckProject,
    BackupDatabase,
    IntegrityCheck,
    VacuumDatabase,
}

#[must_use]
pub fn format_bytes(bytes: u64) -> String {
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut divisor = 1_u64;
    let mut index = 0;
    while bytes / divisor >= 1024 && index < units.len() - 1 {
        divisor *= 1024;
        index += 1;
    }
    if index == 0 {
        format!("{bytes} {}", units[index])
    } else {
        let whole = bytes / divisor;
        let decimal = (bytes % divisor) * 10 / divisor;
        format!("{whole}.{decimal} {}", units[index])
    }
}

#[must_use]
pub fn mask_env_value(key: &str, value: &str) -> String {
    if key == "TJXY_DATABASE_URL" {
        return redact_database_url(value);
    }
    if sensitive_name(key) {
        if value.is_empty() {
            "(empty)".to_owned()
        } else {
            "****".to_owned()
        }
    } else if value.chars().count() > 72 {
        let shortened: String = value.chars().take(69).collect();
        format!("{shortened}...")
    } else {
        value.to_owned()
    }
}

#[must_use]
pub fn parse_env_lines(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let (key, value) = line.split_once('=')?;
            let key = key.trim();
            if key.is_empty() {
                return None;
            }
            let value = value.trim().trim_matches('"').trim_matches('\'');
            Some((key.to_owned(), value.to_owned()))
        })
        .collect()
}

fn merge_environment(
    mut values: BTreeMap<String, String>,
    overrides: impl IntoIterator<Item = (String, String)>,
) -> BTreeMap<String, String> {
    values.extend(overrides);
    values
}

pub fn tail_lines(text: &str, count: usize) -> Vec<String> {
    if count == 0 {
        return Vec::new();
    }
    text.lines()
        .rev()
        .take(count)
        .map(str::to_owned)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

#[must_use]
pub fn detect_build_mode(
    release_path: &Path,
    debug_path: &Path,
    release_exists: bool,
    debug_exists: bool,
) -> BuildMode {
    if release_exists && !release_path.as_os_str().is_empty() {
        BuildMode::Release
    } else if debug_exists && !debug_path.as_os_str().is_empty() {
        BuildMode::Debug
    } else {
        BuildMode::None
    }
}

#[derive(Clone, Debug)]
pub struct Project {
    pub root: PathBuf,
    target_dir: PathBuf,
    admin_dir: PathBuf,
    log_path: PathBuf,
    pid_path: PathBuf,
    tool_versions: Arc<OnceLock<ToolVersions>>,
}

impl Project {
    #[must_use]
    pub fn discover() -> Self {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let root = manifest_dir.parent().unwrap_or(&manifest_dir).to_path_buf();
        Self::new(root)
    }

    #[must_use]
    pub fn new(root: PathBuf) -> Self {
        Self {
            target_dir: root.join("target"),
            admin_dir: root.join("admin"),
            log_path: root.join("data/server.log"),
            pid_path: root.join("target/tjxy-server.pid"),
            tool_versions: Arc::new(OnceLock::new()),
            root,
        }
    }

    #[must_use]
    pub fn server_binary(&self, mode: BuildMode) -> PathBuf {
        self.target_dir.join(mode.label()).join("tjxy-server")
    }

    #[must_use]
    pub fn snapshot(&self) -> StatusSnapshot {
        let release = self.server_binary(BuildMode::Release);
        let debug = self.server_binary(BuildMode::Debug);
        let mode = detect_build_mode(&release, &debug, release.is_file(), debug.is_file());

        let environment = self.runtime_environment();
        let server_bind = environment
            .get("TJXY_BIND")
            .cloned()
            .unwrap_or_else(|| "127.0.0.1:8096".to_owned());
        let configured_port = server_bind
            .parse::<SocketAddr>()
            .map_or(8096, |address| address.port());
        let observed_servers = self.observed_servers();
        let managed_pid = self
            .read_pid()
            .filter(|pid| process_alive(*pid) && self.pid_matches_server(*pid));
        let primary = select_primary_server(&observed_servers, managed_pid, configured_port);
        let server = primary.map(|(server, _)| server.process.clone());
        let server_managed = primary.is_some_and(|(_, managed)| managed);
        let server_listeners =
            primary.map_or_else(Vec::new, |(server, _)| server.listeners.clone());
        let actual_bind = server_listeners
            .iter()
            .find(|listener| endpoint_port(listener) == Some(configured_port))
            .or_else(|| server_listeners.first())
            .cloned()
            .unwrap_or(server_bind);
        let server_port_open = server_listeners
            .iter()
            .any(|listener| endpoint_port(listener) == Some(configured_port))
            || actual_bind
                .parse::<SocketAddr>()
                .ok()
                .is_some_and(|address| port_open(probe_address(address)));
        let database = database_status_from_sources(
            &self.root,
            environment.get("TJXY_DATABASE_URL").map(String::as_str),
            &observed_servers,
        );
        let tool_versions = self.tool_versions.get_or_init(|| ToolVersions {
            rust: command_version("rustc", &["--version"], &self.root),
            node: command_version("node", &["--version"], &self.root),
            npm: command_version("npm", &["--version"], &self.root),
        });
        StatusSnapshot {
            server,
            server_managed,
            server_instances: observed_servers.len(),
            server_listeners,
            server_bind: actual_bind,
            server_port_open,
            admin_port_open: port_open(SocketAddr::from((Ipv4Addr::LOCALHOST, 5173))),
            database,
            build_mode: mode,
            binary_size: file_size(&self.server_binary(mode)),
            rust_version: tool_versions.rust.clone(),
            node_version: tool_versions.node.clone(),
            npm_version: tool_versions.npm.clone(),
            admin_deps: self.admin_dir.join("node_modules").is_dir(),
            admin_dist: self.admin_dir.join("dist/index.html").is_file(),
        }
    }

    #[must_use]
    pub fn environment_rows(&self) -> Vec<(String, String)> {
        let dotenv = fs::read_to_string(self.root.join(".env"))
            .map_or_else(|_| BTreeMap::new(), |contents| parse_env_lines(&contents));
        let values = merge_environment(dotenv, env::vars());
        values
            .into_iter()
            .filter(|(key, _)| key.starts_with("TJXY_"))
            .map(|(key, value)| {
                let masked = mask_env_value(&key, &value);
                (key, masked)
            })
            .collect()
    }

    #[must_use]
    pub fn log_lines(&self, count: usize) -> Vec<String> {
        let mut contents = String::new();
        if let Ok(mut file) = File::open(&self.log_path) {
            let _ = file.read_to_string(&mut contents);
        }
        tail_lines(&contents, count)
    }

    #[must_use]
    pub fn log_path(&self) -> &Path {
        &self.log_path
    }

    /// Resolves the configured `SQLite` file against the project root.
    ///
    /// # Errors
    ///
    /// Returns an error for malformed, non-SQLite, in-memory, or pathless URLs.
    pub fn database_path(&self) -> Result<PathBuf, String> {
        let environment = self.runtime_environment();
        let observed_servers = self.observed_servers();
        let status = database_status_from_sources(
            &self.root,
            environment.get("TJXY_DATABASE_URL").map(String::as_str),
            &observed_servers,
        );
        status.sqlite_path.ok_or_else(|| {
            format!(
                "{} is in use; SQLite-only actions are unavailable",
                status.backend.label()
            )
        })
    }

    #[must_use]
    pub fn run_action(&self, action: Action) -> ActionReport {
        match action {
            Action::StartServer => self.start_server(),
            Action::StopServer => self.stop_server(),
            Action::RestartServer => {
                let stop = self.stop_server();
                if !stop.ok {
                    return stop;
                }
                let start = self.start_server();
                ActionReport::new(
                    start.ok,
                    format!("restart: {}; {}", stop.message, start.message),
                )
            }
            Action::BuildDebug => self.build_server(false),
            Action::BuildRelease => self.build_server(true),
            Action::BuildAdmin => self.build_admin(),
            Action::BuildAll => self.build_all(),
            Action::CheckProject => self.check_project(),
            Action::BackupDatabase => self.backup_database(),
            Action::IntegrityCheck => self.integrity_check(),
            Action::VacuumDatabase => self.sqlite_command("VACUUM;"),
        }
    }

    fn start_server(&self) -> ActionReport {
        let observed_servers = self.observed_servers();
        if !observed_servers.is_empty() {
            let pids = observed_servers
                .iter()
                .map(|server| server.process.pid.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            return ActionReport::error(format!("server already running (PID {pids})"));
        }
        let mode = self.snapshot().build_mode;
        if mode == BuildMode::None {
            return ActionReport::error(
                "server binary not found; run Build Debug or Build Release first",
            );
        }
        if let Err(error) = fs::create_dir_all(self.log_path.parent().unwrap_or(&self.root)) {
            return ActionReport::error(format!("create log directory: {error}"));
        }
        if let Err(error) = fs::create_dir_all(&self.target_dir) {
            return ActionReport::error(format!("create target directory: {error}"));
        }

        let log_file = match OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_path)
        {
            Ok(file) => file,
            Err(error) => return ActionReport::error(format!("open server log: {error}")),
        };
        let mut command = Command::new(self.server_binary(mode));
        command
            .current_dir(&self.root)
            .envs(self.runtime_environment())
            .stdin(Stdio::null())
            .stdout(Stdio::from(match log_file.try_clone() {
                Ok(file) => file,
                Err(error) => return ActionReport::error(format!("clone server log: {error}")),
            }))
            .stderr(Stdio::from(log_file));
        configure_process_group(&mut command);

        match command.spawn() {
            Ok(mut child) => {
                if let Err(error) = fs::write(&self.pid_path, child.id().to_string()) {
                    let _ = child.kill();
                    let _ = child.wait();
                    return ActionReport::error(format!(
                        "write managed PID file for server PID {}: {error}",
                        child.id()
                    ));
                }
                let pid = child.id();
                let pid_path = self.pid_path.clone();
                thread::spawn(move || {
                    let _ = child.wait();
                    let owns_pid_file = fs::read_to_string(&pid_path)
                        .ok()
                        .is_some_and(|contents| contents.trim() == pid.to_string());
                    if owns_pid_file {
                        let _ = fs::remove_file(pid_path);
                    }
                });
                ActionReport::ok(format!("server started (PID {pid})"))
            }
            Err(error) => ActionReport::error(format!("start server: {error}")),
        }
    }

    fn stop_server(&self) -> ActionReport {
        let Some(pid) = self.read_pid() else {
            return ActionReport::error("server is not running or has no managed PID");
        };
        if !process_alive(pid) {
            let _ = fs::remove_file(&self.pid_path);
            return ActionReport::error("managed server PID is stale");
        }
        if !self.pid_matches_server(pid) {
            let _ = fs::remove_file(&self.pid_path);
            return ActionReport::error(format!(
                "managed PID {pid} no longer belongs to this project's tjxy-server"
            ));
        }
        if let Err(error) =
            terminate_process_group(pid, false).or_else(|_| signal_process(pid, false))
        {
            return ActionReport::error(format!("stop server PID {pid}: {error}"));
        }
        for _ in 0..30 {
            if !process_alive(pid) {
                let _ = fs::remove_file(&self.pid_path);
                return ActionReport::ok(format!("server stopped (PID {pid})"));
            }
            thread::sleep(Duration::from_millis(100));
        }
        if let Err(error) =
            terminate_process_group(pid, true).or_else(|_| signal_process(pid, true))
        {
            return ActionReport::error(format!("server did not stop; force stop failed: {error}"));
        }
        let _ = fs::remove_file(&self.pid_path);
        ActionReport::ok(format!("server force-stopped (PID {pid})"))
    }

    fn build_server(&self, release: bool) -> ActionReport {
        let mut args = vec!["build", "-p", "tjxy-server", "--bin", "tjxy-server"];
        if release {
            args.insert(1, "--release");
        }
        self.run_command("cargo", &args, Duration::from_secs(300), "server build")
    }

    fn build_admin(&self) -> ActionReport {
        self.run_admin_command(&["run", "build"], Duration::from_secs(180), "admin build")
    }

    fn build_all(&self) -> ActionReport {
        let server = self.build_server(false);
        if !server.ok {
            return server;
        }
        let admin = self.build_admin();
        ActionReport::new(
            admin.ok,
            format!(
                "server build: {}; admin build: {}",
                server.message, admin.message
            ),
        )
    }

    fn check_project(&self) -> ActionReport {
        let cargo = self.run_command(
            "cargo",
            &["check", "-p", "tjxy-tui"],
            Duration::from_secs(180),
            "TUI check",
        );
        if !cargo.ok {
            return cargo;
        }
        let admin = self.run_admin_command(
            &["run", "typecheck"],
            Duration::from_secs(90),
            "admin typecheck",
        );
        ActionReport::new(
            admin.ok,
            format!(
                "TUI check: {}; admin typecheck: {}",
                cargo.message, admin.message
            ),
        )
    }

    fn backup_database(&self) -> ActionReport {
        let database_path = match self.database_path() {
            Ok(path) => path,
            Err(error) => return ActionReport::error(error),
        };
        if !database_path.is_file() {
            return ActionReport::error(format!(
                "database not found at {}",
                database_path.display()
            ));
        }
        let backup = PathBuf::from(format!("{}.bak", database_path.display()));
        let backup_command = format!(".backup {}", quote_sqlite_cli_argument(&backup));
        let database = database_path.to_string_lossy();
        match run_output_with_timeout(
            "sqlite3",
            &[database.as_ref(), &backup_command],
            &self.root,
            Duration::from_secs(120),
        ) {
            Ok(output) if output.status.success() => ActionReport::ok(format!(
                "database backup written to {} ({})",
                backup.display(),
                file_size(&backup)
            )),
            Ok(output) => ActionReport::error(format!(
                "database backup failed: {}",
                first_line_or_ok(&summarize_output(&output.stdout, &output.stderr))
            )),
            Err(error) => ActionReport::error(format!("database backup: {error}")),
        }
    }

    fn sqlite_command(&self, sql: &str) -> ActionReport {
        let database_path = match self.database_path() {
            Ok(path) => path,
            Err(error) => return ActionReport::error(error),
        };
        if !database_path.is_file() {
            return ActionReport::error("database file does not exist");
        }
        let database = database_path.to_string_lossy();
        let result = self.run_command(
            "sqlite3",
            &[database.as_ref(), sql],
            Duration::from_secs(120),
            "sqlite",
        );
        if result.ok && !result.message.contains("failed") {
            result
        } else {
            ActionReport::error(result.message)
        }
    }

    fn integrity_check(&self) -> ActionReport {
        let database_path = match self.database_path() {
            Ok(path) => path,
            Err(error) => return ActionReport::error(error),
        };
        if !database_path.is_file() {
            return ActionReport::error("database file does not exist");
        }
        let database = database_path.to_string_lossy();
        match run_output_with_timeout(
            "sqlite3",
            &[database.as_ref(), "PRAGMA integrity_check;"],
            &self.root,
            Duration::from_secs(120),
        ) {
            Ok(output) if output.status.success() && integrity_output_is_ok(&output.stdout) => {
                ActionReport::ok("SQLite integrity check: ok")
            }
            Ok(output) => ActionReport::error(format!(
                "SQLite integrity check failed: {}",
                first_line_or_ok(&summarize_output(&output.stdout, &output.stderr))
            )),
            Err(error) => ActionReport::error(format!("SQLite integrity check: {error}")),
        }
    }

    fn run_command(
        &self,
        program: &str,
        args: &[&str],
        timeout: Duration,
        label: &str,
    ) -> ActionReport {
        match run_output_with_timeout(program, args, &self.root, timeout) {
            Ok(output) => report_from_output(label, &output),
            Err(error) => ActionReport::error(format!("{label}: {error}")),
        }
    }

    fn run_admin_command(&self, args: &[&str], timeout: Duration, label: &str) -> ActionReport {
        match run_output_with_timeout("npm", args, &self.admin_dir, timeout) {
            Ok(output) => report_from_output(label, &output),
            Err(error) => ActionReport::error(format!("{label}: {error}")),
        }
    }

    fn runtime_environment(&self) -> BTreeMap<String, String> {
        let dotenv = fs::read_to_string(self.root.join(".env"))
            .map_or_else(|_| BTreeMap::new(), |contents| parse_env_lines(&contents));
        merge_environment(dotenv, env::vars())
    }

    fn read_pid(&self) -> Option<u32> {
        fs::read_to_string(&self.pid_path).ok()?.trim().parse().ok()
    }

    fn observed_servers(&self) -> Vec<ObservedServer> {
        discover_observed_servers(self)
    }

    fn pid_matches_server(&self, pid: u32) -> bool {
        let Some(command_line) = process_command_line(pid) else {
            return false;
        };
        [BuildMode::Release, BuildMode::Debug]
            .into_iter()
            .map(|mode| self.server_binary(mode))
            .any(|binary| command_line_matches_binary(&command_line, &binary))
    }
}

impl ActionReport {
    fn new(ok: bool, message: String) -> Self {
        Self { ok, message }
    }
}

fn file_size(path: &Path) -> String {
    path.metadata().map_or_else(
        |_| "missing".to_owned(),
        |metadata| format_bytes(metadata.len()),
    )
}

fn command_version(program: &str, args: &[&str], current_dir: &Path) -> String {
    run_output_with_timeout(program, args, current_dir, Duration::from_secs(2))
        .ok()
        .filter(|output| output.status.success())
        .map_or_else(
            || "missing".to_owned(),
            |output| first_line_or_ok(&String::from_utf8_lossy(&output.stdout)),
        )
}

fn run_output_with_timeout(
    program: &str,
    args: &[&str],
    current_dir: &Path,
    timeout: Duration,
) -> io::Result<Output> {
    let mut command = Command::new(program);
    command
        .current_dir(current_dir)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    configure_process_group(&mut command);
    let mut child = command.spawn()?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("child stdout pipe is unavailable"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| io::Error::other("child stderr pipe is unavailable"))?;
    let stdout_reader = thread::spawn(move || read_child_stdout(stdout));
    let stderr_reader = thread::spawn(move || read_child_stderr(stderr));
    let started = std::time::Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Output {
                status,
                stdout: join_output_reader(stdout_reader, "stdout")?,
                stderr: join_output_reader(stderr_reader, "stderr")?,
            });
        }
        if started.elapsed() >= timeout {
            let _ = terminate_process_group(child.id(), false);
            for _ in 0..5 {
                if child.try_wait()?.is_some() {
                    break;
                }
                thread::sleep(Duration::from_millis(100));
            }
            if child.try_wait()?.is_none() {
                let _ = terminate_process_group(child.id(), true);
                let _ = child.kill();
            }
            let status = child.wait()?;
            let output = Output {
                status,
                stdout: join_output_reader(stdout_reader, "stdout")?,
                stderr: join_output_reader(stderr_reader, "stderr")?,
            };
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                format!(
                    "timed out after {}s; {}",
                    timeout.as_secs(),
                    first_line_or_ok(&summarize_output(&output.stdout, &output.stderr))
                ),
            ));
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn read_child_stdout(mut stdout: ChildStdout) -> io::Result<Vec<u8>> {
    let mut output = Vec::new();
    stdout.read_to_end(&mut output)?;
    Ok(output)
}

fn read_child_stderr(mut stderr: ChildStderr) -> io::Result<Vec<u8>> {
    let mut output = Vec::new();
    stderr.read_to_end(&mut output)?;
    Ok(output)
}

fn join_output_reader(
    reader: thread::JoinHandle<io::Result<Vec<u8>>>,
    stream: &str,
) -> io::Result<Vec<u8>> {
    reader
        .join()
        .map_err(|_| io::Error::other(format!("child {stream} reader panicked")))?
}

fn report_from_output(label: &str, output: &Output) -> ActionReport {
    let message = summarize_output(&output.stdout, &output.stderr);
    if output.status.success() {
        ActionReport::ok(format!("{label}: {}", first_line_or_ok(&message)))
    } else {
        ActionReport::error(format!("{label} failed: {}", first_line_or_ok(&message)))
    }
}

fn first_line_or_ok(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("ok")
        .chars()
        .take(160)
        .collect()
}

fn summarize_output(stdout: &[u8], stderr: &[u8]) -> String {
    let stdout = String::from_utf8_lossy(stdout);
    let stderr = String::from_utf8_lossy(stderr);
    if stderr.trim().is_empty() {
        first_line_or_ok(&stdout)
    } else {
        first_line_or_ok(&stderr)
    }
}

fn probe_address(address: SocketAddr) -> SocketAddr {
    match address.ip() {
        IpAddr::V4(ip) if ip.is_unspecified() => {
            SocketAddr::from((Ipv4Addr::LOCALHOST, address.port()))
        }
        IpAddr::V6(ip) if ip.is_unspecified() => {
            SocketAddr::from((Ipv6Addr::LOCALHOST, address.port()))
        }
        _ => address,
    }
}

fn port_open(address: SocketAddr) -> bool {
    TcpStream::connect_timeout(&address, Duration::from_millis(80)).is_ok()
}

fn discover_observed_servers(project: &Project) -> Vec<ObservedServer> {
    let Ok(output) = run_output_with_timeout(
        "lsof",
        &["-a", "-c", "tjxy-serv", "-d", "txt", "-Fpn"],
        &project.root,
        Duration::from_secs(2),
    ) else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }

    let expected = [
        project.server_binary(BuildMode::Debug),
        project.server_binary(BuildMode::Release),
    ]
    .map(|path| fs::canonicalize(&path).unwrap_or(path));
    let mut servers = parse_lsof_process_executables(&String::from_utf8_lossy(&output.stdout))
        .into_iter()
        .filter(|(_, executables)| {
            executables
                .iter()
                .map(|path| fs::canonicalize(path).unwrap_or_else(|_| path.clone()))
                .any(|path| expected.contains(&path))
        })
        .map(|(pid, _)| {
            let process = process_info(pid).unwrap_or(ProcessInfo {
                pid,
                elapsed: "-".to_owned(),
                rss: "-".to_owned(),
                cpu: "-".to_owned(),
            });
            let (listeners, database_connections) = process_network(pid, &project.root);
            ObservedServer {
                process,
                listeners,
                database_connections,
            }
        })
        .collect::<Vec<_>>();
    servers.sort_by_key(|server| server.process.pid);
    servers
}

fn parse_lsof_process_executables(output: &str) -> BTreeMap<u32, Vec<PathBuf>> {
    let mut processes = BTreeMap::new();
    let mut pid = None;
    for line in output.lines() {
        if let Some(value) = line.strip_prefix('p') {
            pid = value.parse::<u32>().ok();
        } else if let (Some(pid), Some(path)) = (pid, line.strip_prefix('n')) {
            processes
                .entry(pid)
                .or_insert_with(Vec::new)
                .push(PathBuf::from(path));
        }
    }
    processes
}

fn process_network(pid: u32, root: &Path) -> (Vec<String>, Vec<String>) {
    let pid = pid.to_string();
    let Ok(output) = run_output_with_timeout(
        "lsof",
        &["-a", "-p", &pid, "-nP", "-iTCP", "-FpnT"],
        root,
        Duration::from_secs(2),
    ) else {
        return (Vec::new(), Vec::new());
    };
    if !output.status.success() {
        return (Vec::new(), Vec::new());
    }
    parse_lsof_network(&String::from_utf8_lossy(&output.stdout))
}

fn parse_lsof_network(output: &str) -> (Vec<String>, Vec<String>) {
    let mut listeners = Vec::new();
    let mut connections = Vec::new();
    let mut name = None;

    for line in output.lines() {
        if let Some(value) = line.strip_prefix('n') {
            name = Some(value.to_owned());
        } else if line == "TST=LISTEN" {
            if let Some(value) = name.take() {
                listeners.push(value);
            }
        } else if line == "TST=ESTABLISHED"
            && let Some(remote) = name
                .take()
                .and_then(|value| value.split_once("->").map(|(_, remote)| remote.to_owned()))
        {
            connections.push(remote);
        }
    }
    listeners.sort();
    listeners.dedup();
    connections.sort();
    connections.dedup();
    (listeners, connections)
}

fn select_primary_server(
    servers: &[ObservedServer],
    managed_pid: Option<u32>,
    configured_port: u16,
) -> Option<(&ObservedServer, bool)> {
    if let Some(pid) = managed_pid
        && let Some(server) = servers.iter().find(|server| server.process.pid == pid)
    {
        return Some((server, true));
    }
    servers
        .iter()
        .find(|server| {
            server
                .listeners
                .iter()
                .any(|listener| endpoint_port(listener) == Some(configured_port))
        })
        .or_else(|| servers.first())
        .map(|server| (server, false))
}

fn endpoint_port(endpoint: &str) -> Option<u16> {
    endpoint.rsplit_once(':')?.1.parse().ok()
}

fn database_status_from_sources(
    root: &Path,
    configured_url: Option<&str>,
    servers: &[ObservedServer],
) -> DatabaseStatus {
    if let Some(database_url) = configured_url {
        let Ok(parsed) = Url::parse(database_url) else {
            return DatabaseStatus {
                backend: DatabaseBackend::Unknown,
                target: "invalid TJXY_DATABASE_URL".to_owned(),
                connected: false,
                exists: false,
                size: "n/a".to_owned(),
                sqlite_path: None,
            };
        };
        return match parsed.scheme() {
            "sqlite" => sqlite_database_status(root, database_url),
            "postgres" | "postgresql" => {
                let port = parsed.port().unwrap_or(5432);
                let connection = servers
                    .iter()
                    .flat_map(|server| &server.database_connections)
                    .find(|endpoint| endpoint_port(endpoint) == Some(port));
                DatabaseStatus {
                    backend: DatabaseBackend::PostgreSql,
                    target: redact_database_url(database_url),
                    connected: connection.is_some(),
                    exists: false,
                    size: "n/a".to_owned(),
                    sqlite_path: None,
                }
            }
            _ => DatabaseStatus {
                backend: DatabaseBackend::Unknown,
                target: redact_database_url(database_url),
                connected: false,
                exists: false,
                size: "n/a".to_owned(),
                sqlite_path: None,
            },
        };
    }

    if let Some(endpoint) = servers
        .iter()
        .flat_map(|server| &server.database_connections)
        .find(|endpoint| endpoint_port(endpoint) == Some(5432))
    {
        return DatabaseStatus {
            backend: DatabaseBackend::PostgreSql,
            target: endpoint.clone(),
            connected: true,
            exists: false,
            size: "n/a".to_owned(),
            sqlite_path: None,
        };
    }

    sqlite_database_status(root, "sqlite://tjxy.db?mode=rwc")
}

fn sqlite_database_status(root: &Path, database_url: &str) -> DatabaseStatus {
    match resolve_sqlite_database_path(root, database_url) {
        Ok(path) => DatabaseStatus {
            backend: DatabaseBackend::SQLite,
            target: path.display().to_string(),
            connected: path.is_file(),
            exists: path.is_file(),
            size: file_size(&path),
            sqlite_path: Some(path),
        },
        Err(error) => DatabaseStatus {
            backend: DatabaseBackend::Unknown,
            target: error,
            connected: false,
            exists: false,
            size: "n/a".to_owned(),
            sqlite_path: None,
        },
    }
}

fn redact_database_url(value: &str) -> String {
    let Ok(mut parsed) = Url::parse(value) else {
        return "****".to_owned();
    };
    if parsed.password().is_some() && parsed.set_password(Some("****")).is_err() {
        return "****".to_owned();
    }
    let query = parsed
        .query_pairs()
        .map(|(key, value)| {
            let value = if sensitive_name(&key) {
                "****".to_owned()
            } else {
                value.into_owned()
            };
            (key.into_owned(), value)
        })
        .collect::<Vec<_>>();
    if parsed.query().is_some() {
        parsed.query_pairs_mut().clear().extend_pairs(query);
    }
    parsed.to_string()
}

fn sensitive_name(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    ["password", "secret", "token", "credential", "key"]
        .iter()
        .any(|needle| name.contains(needle))
}

fn resolve_sqlite_database_path(root: &Path, database_url: &str) -> Result<PathBuf, String> {
    let parsed =
        Url::parse(database_url).map_err(|error| format!("invalid TJXY_DATABASE_URL: {error}"))?;
    if parsed.scheme() != "sqlite" {
        return Err("database actions are available only for SQLite URLs".to_owned());
    }
    if database_url == "sqlite::memory:" || parsed.path() == ":memory:" {
        return Err("database actions are unavailable for in-memory SQLite".to_owned());
    }

    let path = if let Some(host) = parsed.host_str() {
        let mut path = PathBuf::from(host);
        let suffix = parsed.path().trim_start_matches('/');
        if !suffix.is_empty() {
            path.push(suffix);
        }
        path
    } else {
        PathBuf::from(parsed.path())
    };
    if path.as_os_str().is_empty() {
        return Err("TJXY_DATABASE_URL does not contain a SQLite file path".to_owned());
    }
    if path.is_absolute() {
        Ok(path)
    } else {
        Ok(root.join(path))
    }
}

fn quote_sqlite_cli_argument(path: &Path) -> String {
    let escaped = path
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    format!("\"{escaped}\"")
}

fn command_line_matches_binary(command_line: &str, binary: &Path) -> bool {
    let expected = binary.to_string_lossy();
    let command_line = command_line.trim();
    command_line == expected
        || command_line
            .strip_prefix(expected.as_ref())
            .is_some_and(|suffix| suffix.starts_with(char::is_whitespace))
}

fn process_command_line(pid: u32) -> Option<String> {
    let output = Command::new("ps")
        .args(["-o", "command=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let command_line = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    (!command_line.is_empty()).then_some(command_line)
}

fn integrity_output_is_ok(output: &[u8]) -> bool {
    let output = String::from_utf8_lossy(output);
    let mut lines = output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty());
    let Some(first) = lines.next() else {
        return false;
    };
    first == "ok" && lines.all(|line| line == "ok")
}

fn configure_process_group(command: &mut Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
}

fn terminate_process_group(pid: u32, force: bool) -> io::Result<()> {
    #[cfg(unix)]
    {
        let signal = if force { "-KILL" } else { "-TERM" };
        let process_group = format!("-{pid}");
        let status = Command::new("kill")
            .args([signal, &process_group])
            .status()?;
        if status.success() {
            return Ok(());
        }
        Err(io::Error::other(format!(
            "kill process group exited with {status}"
        )))
    }
    #[cfg(not(unix))]
    {
        let _ = (pid, force);
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "process-group termination requires Unix",
        ))
    }
}

fn process_alive(pid: u32) -> bool {
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .status()
        .is_ok_and(|status| status.success())
}

fn signal_process(pid: u32, force: bool) -> io::Result<()> {
    let signal = if force { "-KILL" } else { "-TERM" };
    let status = Command::new("kill")
        .args([signal, &pid.to_string()])
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!("kill exited with {status}")))
    }
}

fn process_info(pid: u32) -> Option<ProcessInfo> {
    let output = Command::new("ps")
        .args(["-o", "etime=,rss=,%cpu=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let fields: Vec<_> = String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .map(str::to_owned)
        .collect();
    Some(ProcessInfo {
        pid,
        elapsed: fields.first().cloned().unwrap_or_else(|| "-".to_owned()),
        rss: fields
            .get(1)
            .map_or_else(|| "-".to_owned(), |value| format!("{value} KB")),
        cpu: fields
            .get(2)
            .map_or_else(|| "-".to_owned(), |value| format!("{value}%")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary_project(name: &str) -> Project {
        let root = env::temp_dir().join(format!("tjxy-tui-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        Project::new(root)
    }

    #[test]
    fn formats_bytes_with_human_readable_units() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(1_536), "1.5 KB");
        assert_eq!(format_bytes(2 * 1024 * 1024), "2.0 MB");
    }

    #[test]
    fn masks_sensitive_environment_values() {
        assert_eq!(
            mask_env_value("TJXY_SERVER_NAME", "Living Room"),
            "Living Room"
        );
        assert_eq!(
            mask_env_value("TJXY_BOOTSTRAP_ADMIN_PASSWORD", "secret"),
            "****"
        );
        assert_eq!(mask_env_value("TJXY_TMDB_ACCESS_TOKEN", "token"), "****");
        let database_url = mask_env_value(
            "TJXY_DATABASE_URL",
            "postgres://reader:super-secret@db.example/tjxy?sslmode=require&token=query-secret",
        );
        assert!(!database_url.contains("super-secret"));
        assert!(!database_url.contains("query-secret"));
        assert!(database_url.contains("reader"));
        assert!(database_url.contains("sslmode=require"));
    }

    #[test]
    fn parses_dotenv_lines() {
        let parsed = parse_env_lines(
            r#"
            # comment
            TJXY_BIND="127.0.0.1:8096"
            TJXY_SERVER_NAME='Living Room'
            INVALID
        "#,
        );

        assert_eq!(parsed.get("TJXY_BIND"), Some(&"127.0.0.1:8096".to_owned()));
        assert_eq!(
            parsed.get("TJXY_SERVER_NAME"),
            Some(&"Living Room".to_owned())
        );
        assert!(!parsed.contains_key("INVALID"));
    }

    #[test]
    fn returns_only_the_last_requested_log_lines() {
        assert_eq!(tail_lines("one\ntwo\nthree\n", 2), vec!["two", "three"]);
        assert_eq!(tail_lines("", 3), Vec::<String>::new());
    }

    #[test]
    fn captures_large_command_output_without_blocking_on_full_pipes() {
        let output = run_output_with_timeout(
            "sh",
            &["-c", "dd if=/dev/zero bs=1024 count=256 2>/dev/null"],
            Path::new("."),
            Duration::from_secs(2),
        )
        .unwrap();

        assert!(output.status.success());
        assert_eq!(output.stdout.len(), 256 * 1024);
    }

    #[test]
    fn environment_status_only_exposes_tjxy_configuration() {
        let project = temporary_project("environment-filter");
        fs::write(
            project.root.join(".env"),
            "TJXY_SERVER_NAME=Living Room\nUNRELATED_SECRET=do-not-display\n",
        )
        .unwrap();

        let rows = project.environment_rows();

        assert!(
            rows.iter()
                .any(|(key, value)| { key == "TJXY_SERVER_NAME" && value == "Living Room" })
        );
        assert!(!rows.iter().any(|(key, _)| key == "UNRELATED_SECRET"));
        fs::remove_dir_all(&project.root).unwrap();
    }

    #[test]
    fn live_environment_overrides_dotenv_values() {
        let dotenv = parse_env_lines("TJXY_BIND=127.0.0.1:8096\nTJXY_SERVER_NAME=File\n");

        let merged = merge_environment(
            dotenv,
            [("TJXY_SERVER_NAME".to_owned(), "Process".to_owned())],
        );

        assert_eq!(merged.get("TJXY_BIND"), Some(&"127.0.0.1:8096".to_owned()));
        assert_eq!(merged.get("TJXY_SERVER_NAME"), Some(&"Process".to_owned()));
    }

    #[test]
    fn resolves_the_configured_sqlite_database_path() {
        let root = Path::new("/tmp/tjxy-project");

        assert_eq!(
            resolve_sqlite_database_path(root, "sqlite://data/catalog.db?mode=rwc").unwrap(),
            root.join("data/catalog.db")
        );
        assert_eq!(
            resolve_sqlite_database_path(root, "sqlite:///tmp/catalog.db?mode=rwc").unwrap(),
            PathBuf::from("/tmp/catalog.db")
        );
        assert!(resolve_sqlite_database_path(root, "postgres://localhost/tjxy").is_err());
        assert!(resolve_sqlite_database_path(root, "sqlite::memory:").is_err());
    }

    #[test]
    fn managed_pid_must_match_a_server_binary_from_this_project() {
        let project = temporary_project("pid-owner");
        let debug = project.server_binary(BuildMode::Debug);

        assert!(command_line_matches_binary(
            &debug.display().to_string(),
            &debug
        ));
        assert!(command_line_matches_binary(
            &format!("{} --flag", debug.display()),
            &debug
        ));
        assert!(!command_line_matches_binary(
            "/usr/bin/unrelated-worker",
            &debug
        ));
        assert!(!command_line_matches_binary(
            &format!("{}-helper", debug.display()),
            &debug
        ));

        fs::remove_dir_all(&project.root).unwrap();
    }

    #[test]
    fn integrity_check_requires_every_result_row_to_be_ok() {
        assert!(integrity_output_is_ok(b"ok\n"));
        assert!(integrity_output_is_ok(b"ok\nok\n"));
        assert!(!integrity_output_is_ok(b""));
        assert!(!integrity_output_is_ok(b"row 12 missing from index\n"));
        assert!(!integrity_output_is_ok(b"ok\nrow 4 malformed\n"));
    }

    #[test]
    fn command_timeout_terminates_descendants_holding_output_pipes() {
        let started = std::time::Instant::now();
        let result = run_output_with_timeout(
            "sh",
            &["-c", "sleep 5 & wait"],
            Path::new("."),
            Duration::from_millis(100),
        );

        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::TimedOut);
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn wildcard_bind_addresses_are_probed_through_loopback() {
        assert_eq!(
            probe_address("0.0.0.0:9000".parse().unwrap()),
            "127.0.0.1:9000".parse().unwrap()
        );
        assert_eq!(
            probe_address("[::]:9000".parse().unwrap()),
            "[::1]:9000".parse().unwrap()
        );
    }

    fn observed_server(
        pid: u32,
        listeners: &[&str],
        database_connections: &[&str],
    ) -> ObservedServer {
        ObservedServer {
            process: ProcessInfo {
                pid,
                elapsed: "1:00".to_owned(),
                rss: "1024 KB".to_owned(),
                cpu: "0.1%".to_owned(),
            },
            listeners: listeners.iter().map(ToString::to_string).collect(),
            database_connections: database_connections
                .iter()
                .map(ToString::to_string)
                .collect(),
        }
    }

    #[test]
    fn running_server_is_observed_without_a_managed_pid_file() {
        let servers = vec![
            observed_server(41, &["127.0.0.1:8097"], &[]),
            observed_server(42, &["127.0.0.1:8096"], &["[::1]:5432"]),
        ];

        let (server, managed) = select_primary_server(&servers, None, 8096).unwrap();

        assert_eq!(server.process.pid, 42);
        assert!(!managed);
    }

    #[test]
    fn postgres_is_inferred_from_a_running_servers_connections() {
        let root = Path::new("/tmp/tjxy-project");
        let servers = vec![observed_server(42, &["127.0.0.1:8096"], &["[::1]:5432"])];

        let status = database_status_from_sources(root, None, &servers);

        assert_eq!(status.backend, DatabaseBackend::PostgreSql);
        assert_eq!(status.target, "[::1]:5432");
        assert!(status.connected);
        assert_eq!(status.size, "n/a");
    }

    #[test]
    fn parses_listener_and_database_connection_records_from_lsof() {
        let output = "\
p42\n\
f8\n\
n[::1]:56567->[::1]:5432\n\
TST=ESTABLISHED\n\
f21\n\
n127.0.0.1:8096\n\
TST=LISTEN\n";

        let (listeners, connections) = parse_lsof_network(output);

        assert_eq!(listeners, vec!["127.0.0.1:8096"]);
        assert_eq!(connections, vec!["[::1]:5432"]);
    }

    #[test]
    fn parses_server_pids_and_executables_without_pgrep() {
        let output = "\
p41\n\
ftxt\n\
n/repo/target/debug/tjxy-server\n\
n/usr/lib/dyld\n\
p42\n\
ftxt\n\
n/repo/target/release/tjxy-server\n";

        let processes = parse_lsof_process_executables(output);

        assert_eq!(
            processes.get(&41),
            Some(&vec![
                PathBuf::from("/repo/target/debug/tjxy-server"),
                PathBuf::from("/usr/lib/dyld")
            ])
        );
        assert_eq!(
            processes.get(&42),
            Some(&vec![PathBuf::from("/repo/target/release/tjxy-server")])
        );
    }
}
