use std::{
    collections::BTreeMap,
    env,
    fs::{self, File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpStream},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const DEFAULT_BIND: &str = "127.0.0.1:8096";
const DEFAULT_CONFIG_SUFFIX: &str = ".config/tjxy/tjxy.toml";
const DEFAULT_LOG_PATH: &str = "data/server.log";
const MAX_CONFIG_BYTES: u64 = 64 * 1024;
const MAX_LOG_TAIL_BYTES: u64 = 256 * 1024;
const STARTUP_CONFIRM_ATTEMPTS: usize = 30;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessInfo {
    pub pid: u32,
    pub elapsed: String,
    pub rss: String,
    pub cpu: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigState {
    Missing,
    Pending,
    Completed,
    Invalid,
    Unreadable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceAction {
    Start,
    Stop,
    Restart,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionMessage {
    Started,
    Stopped,
    Restarted,
    AlreadyRunning,
    NotRunning,
    MultipleInstances,
    BinaryMissing,
    LogUnavailable,
    StartFailed,
    StopFailed,
    StopTimedOut,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionReport {
    pub ok: bool,
    pub message: ActionMessage,
    pub pid: Option<u32>,
    pub detail: Option<String>,
}

impl ActionReport {
    fn success(message: ActionMessage, pid: u32, detail: Option<String>) -> Self {
        Self {
            ok: true,
            message,
            pid: Some(pid),
            detail,
        }
    }

    fn error(message: ActionMessage, pid: Option<u32>, detail: Option<String>) -> Self {
        Self {
            ok: false,
            message,
            pid,
            detail,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileStatus {
    pub path: PathBuf,
    pub exists: bool,
    pub size: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigStatus {
    pub path: PathBuf,
    pub state: ConfigState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct StatusSnapshot {
    pub server: Option<ProcessInfo>,
    pub server_instances: usize,
    pub server_listeners: Vec<String>,
    pub server_bind: String,
    pub server_port_open: bool,
    pub configuration: ConfigStatus,
    pub admin_dist: FileStatus,
    pub log: FileStatus,
    pub recent_log_lines: Vec<String>,
    pub log_error: Option<String>,
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

#[must_use]
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

#[derive(Clone, Debug)]
pub struct Project {
    pub root: PathBuf,
    target_dir: PathBuf,
    pid_path: PathBuf,
}

impl Project {
    #[must_use]
    pub fn discover() -> Self {
        if let Some(home) = env::var_os("TJXY_HOME") {
            return Self::new(PathBuf::from(home));
        }
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let development_root = manifest_dir.parent().unwrap_or(&manifest_dir);
        let root = env::current_exe()
            .ok()
            .and_then(|path| path.parent().map(Path::to_path_buf))
            .filter(|path| !is_cargo_target_directory(path))
            .unwrap_or_else(|| development_root.to_path_buf());
        Self::new(root)
    }

    #[must_use]
    pub fn new(root: PathBuf) -> Self {
        Self {
            target_dir: root.join("target"),
            pid_path: root.join("data/tjxy-server.pid"),
            root,
        }
    }

    #[must_use]
    pub fn snapshot(&self) -> StatusSnapshot {
        let environment = self.runtime_environment();
        let configured_bind = environment
            .get("TJXY_BIND")
            .cloned()
            .unwrap_or_else(|| DEFAULT_BIND.to_owned());
        let configured_port = configured_bind
            .parse::<SocketAddr>()
            .map_or(8096, |address| address.port());
        let observed_servers = discover_observed_servers(self);
        let primary = select_primary_server(&observed_servers, configured_port);
        let server = primary.map(|server| server.process.clone());
        let server_listeners = primary.map_or_else(Vec::new, |server| server.listeners.clone());
        let server_bind = server_listeners
            .iter()
            .find(|listener| endpoint_port(listener) == Some(configured_port))
            .or_else(|| server_listeners.first())
            .cloned()
            .unwrap_or(configured_bind);
        let server_port_open = server_listeners
            .iter()
            .any(|listener| endpoint_port(listener) == Some(configured_port))
            || server_bind
                .parse::<SocketAddr>()
                .ok()
                .is_some_and(|address| port_open(probe_address(address)));
        let configuration = configuration_status(&self.root, &environment);
        let admin_dist_path = resolve_path(
            &self.root,
            environment
                .get("TJXY_ADMIN_DIST_DIR")
                .map_or("admin/dist", String::as_str),
        )
        .join("index.html");
        let log_path = self.log_path(&environment);

        let (recent_log_lines, log_error) = match read_log_tail(&log_path, 100) {
            Ok(lines) => (lines, None),
            Err(error) => (Vec::new(), Some(error.to_string())),
        };

        StatusSnapshot {
            server,
            server_instances: observed_servers.len(),
            server_listeners,
            server_bind,
            server_port_open,
            configuration,
            admin_dist: file_status(admin_dist_path),
            log: file_status(log_path),
            recent_log_lines,
            log_error,
        }
    }

    #[must_use]
    pub fn run_action(&self, action: ServiceAction) -> ActionReport {
        match action {
            ServiceAction::Start => self.start_server(ActionMessage::Started),
            ServiceAction::Stop => self.stop_server(),
            ServiceAction::Restart => {
                let stopped = self.stop_server();
                if !stopped.ok {
                    return stopped;
                }
                let started = self.start_server(ActionMessage::Restarted);
                if started.ok {
                    started
                } else {
                    ActionReport {
                        detail: Some(format!(
                            "server stopped, but restart failed: {}",
                            started.detail.as_deref().unwrap_or("unknown error")
                        )),
                        ..started
                    }
                }
            }
        }
    }

    fn start_server(&self, success_message: ActionMessage) -> ActionReport {
        let servers = discover_observed_servers(self);
        if !servers.is_empty() {
            return ActionReport::error(
                ActionMessage::AlreadyRunning,
                servers.first().map(|server| server.process.pid),
                Some(pid_list(&servers)),
            );
        }
        let environment = self.runtime_environment();
        let configured_bind = environment
            .get("TJXY_BIND")
            .map_or(DEFAULT_BIND, String::as_str);
        if configured_bind
            .parse::<SocketAddr>()
            .ok()
            .is_some_and(|address| port_open(probe_address(address)))
        {
            return ActionReport::error(
                ActionMessage::AlreadyRunning,
                None,
                Some(format!(
                    "configured endpoint {configured_bind} is already open"
                )),
            );
        }
        let Some(binary) = self.preferred_server_binary() else {
            return ActionReport::error(ActionMessage::BinaryMissing, None, None);
        };
        let log_path = self.log_path(&environment);
        let mut log_file = match open_log_for_append(&log_path) {
            Ok(file) => file,
            Err(error) => {
                return ActionReport::error(
                    ActionMessage::LogUnavailable,
                    None,
                    Some(error.to_string()),
                );
            }
        };
        if let Err(error) = write_lifecycle_event(&mut log_file, "start requested") {
            return ActionReport::error(
                ActionMessage::LogUnavailable,
                None,
                Some(error.to_string()),
            );
        }
        let stdout = match log_file.try_clone() {
            Ok(file) => file,
            Err(error) => {
                return ActionReport::error(
                    ActionMessage::LogUnavailable,
                    None,
                    Some(error.to_string()),
                );
            }
        };
        self.spawn_server(
            &binary,
            environment,
            log_path,
            stdout,
            log_file,
            success_message,
        )
    }

    fn spawn_server(
        &self,
        binary: &Path,
        environment: BTreeMap<String, String>,
        log_path: PathBuf,
        stdout: File,
        log_file: File,
        success_message: ActionMessage,
    ) -> ActionReport {
        let mut command = Command::new(binary);
        command
            .current_dir(&self.root)
            .envs(environment)
            .stdin(Stdio::null())
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(log_file));
        configure_process_group(&mut command);

        match command.spawn() {
            Ok(mut child) => {
                let pid = child.id();
                if let Err(error) = fs::write(&self.pid_path, pid.to_string()) {
                    let _ = child.kill();
                    let _ = child.wait();
                    return ActionReport::error(
                        ActionMessage::StartFailed,
                        Some(pid),
                        Some(format!("write PID file: {error}")),
                    );
                }
                for _ in 0..STARTUP_CONFIRM_ATTEMPTS {
                    thread::sleep(Duration::from_millis(100));
                    match child.try_wait() {
                        Ok(Some(status)) => {
                            remove_owned_pid_file(&self.pid_path, pid);
                            let event =
                                format!("server PID {pid} exited during startup with {status}");
                            let _ = append_lifecycle_event(&log_path, &event);
                            return ActionReport::error(
                                ActionMessage::StartFailed,
                                Some(pid),
                                Some(format!("server exited with {status}")),
                            );
                        }
                        Ok(None) => {}
                        Err(error) => {
                            remove_owned_pid_file(&self.pid_path, pid);
                            let _ = child.kill();
                            let _ = child.wait();
                            return ActionReport::error(
                                ActionMessage::StartFailed,
                                Some(pid),
                                Some(format!("confirm server startup: {error}")),
                            );
                        }
                    }
                }
                let pid_path = self.pid_path.clone();
                thread::spawn(move || {
                    let status = child.wait();
                    remove_owned_pid_file(&pid_path, pid);
                    if let Ok(mut log) = open_log_for_append(&log_path) {
                        let event = status.map_or_else(
                            |error| format!("server wait failed for PID {pid}: {error}"),
                            |status| format!("server PID {pid} exited with {status}"),
                        );
                        let _ = write_lifecycle_event(&mut log, &event);
                    }
                });
                ActionReport::success(success_message, pid, None)
            }
            Err(error) => {
                ActionReport::error(ActionMessage::StartFailed, None, Some(error.to_string()))
            }
        }
    }

    fn stop_server(&self) -> ActionReport {
        let servers = discover_observed_servers(self);
        let pid = match servers.as_slice() {
            [server] => server.process.pid,
            [] => match self
                .managed_pid()
                .filter(|pid| self.pid_matches_server(*pid))
            {
                Some(pid) => pid,
                None => return ActionReport::error(ActionMessage::NotRunning, None, None),
            },
            _ => {
                return ActionReport::error(
                    ActionMessage::MultipleInstances,
                    None,
                    Some(pid_list(&servers)),
                );
            }
        };
        let environment = self.runtime_environment();
        let log_path = self.log_path(&environment);
        let log_warning =
            append_lifecycle_event(&log_path, &format!("stop requested for PID {pid}"))
                .err()
                .map(|error| format!("lifecycle log: {error}"));

        if let Err(error) = signal_process(pid) {
            return ActionReport::error(
                ActionMessage::StopFailed,
                Some(pid),
                Some(error.to_string()),
            );
        }
        for _ in 0..30 {
            if !process_alive(pid) {
                remove_owned_pid_file(&self.pid_path, pid);
                let event_warning =
                    append_lifecycle_event(&log_path, &format!("server PID {pid} stopped"))
                        .err()
                        .map(|error| format!("lifecycle log: {error}"));
                return ActionReport::success(
                    ActionMessage::Stopped,
                    pid,
                    event_warning.or(log_warning),
                );
            }
            thread::sleep(Duration::from_millis(100));
        }
        ActionReport::error(ActionMessage::StopTimedOut, Some(pid), log_warning)
    }

    fn runtime_environment(&self) -> BTreeMap<String, String> {
        let dotenv = fs::read_to_string(self.root.join(".env"))
            .map_or_else(|_| BTreeMap::new(), |contents| parse_env_lines(&contents));
        let mut values = dotenv;
        values.extend(env::vars());
        values
    }

    fn log_path(&self, environment: &BTreeMap<String, String>) -> PathBuf {
        resolve_path(
            &self.root,
            environment
                .get("TJXY_LOG_FILE")
                .map_or(DEFAULT_LOG_PATH, String::as_str),
        )
    }

    fn preferred_server_binary(&self) -> Option<PathBuf> {
        [
            self.root.join("tjxy-server"),
            self.server_binary("release"),
            self.server_binary("debug"),
        ]
        .into_iter()
        .find(|path| path.is_file())
    }

    fn managed_pid(&self) -> Option<u32> {
        fs::read_to_string(&self.pid_path).ok()?.trim().parse().ok()
    }

    fn pid_matches_server(&self, pid: u32) -> bool {
        let Some(command_line) = process_command_line(pid) else {
            return false;
        };
        [
            self.root.join("tjxy-server"),
            self.server_binary("release"),
            self.server_binary("debug"),
        ]
        .into_iter()
        .any(|binary| command_line_matches_binary(&command_line, &binary))
    }

    fn server_binary(&self, profile: &str) -> PathBuf {
        self.target_dir.join(profile).join("tjxy-server")
    }
}

fn is_cargo_target_directory(path: &Path) -> bool {
    matches!(
        (
            path.file_name().and_then(|value| value.to_str()),
            path.parent()
                .and_then(Path::file_name)
                .and_then(|value| value.to_str())
        ),
        (Some("debug" | "release" | "dist"), Some("target"))
    )
}

fn resolve_path(root: &Path, value: &str) -> PathBuf {
    let path = PathBuf::from(value);
    if path.is_absolute() {
        path
    } else {
        root.join(path)
    }
}

fn open_log_for_append(path: &Path) -> io::Result<File> {
    if let Ok(metadata) = fs::symlink_metadata(path)
        && (metadata.file_type().is_symlink() || !metadata.is_file())
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "log path is not a regular file",
        ));
    }
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty());
    if let Some(parent) = parent {
        fs::create_dir_all(parent)?;
    }
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

fn append_lifecycle_event(path: &Path, event: &str) -> io::Result<()> {
    let mut log = open_log_for_append(path)?;
    write_lifecycle_event(&mut log, event)
}

fn write_lifecycle_event(log: &mut File, event: &str) -> io::Result<()> {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    writeln!(log, "[tjxy-tui {timestamp}] {event}")?;
    log.flush()
}

fn pid_list(servers: &[ObservedServer]) -> String {
    servers
        .iter()
        .map(|server| server.process.pid.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

fn remove_owned_pid_file(path: &Path, pid: u32) {
    if fs::read_to_string(path)
        .ok()
        .is_some_and(|contents| contents.trim() == pid.to_string())
    {
        let _ = fs::remove_file(path);
    }
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

fn process_alive(pid: u32) -> bool {
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn signal_process(pid: u32) -> io::Result<()> {
    let status = Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!("kill exited with {status}")))
    }
}

fn configure_process_group(command: &mut Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
}

fn file_status(path: PathBuf) -> FileStatus {
    let metadata = fs::symlink_metadata(&path)
        .ok()
        .filter(std::fs::Metadata::is_file);
    FileStatus {
        exists: metadata.is_some(),
        size: metadata.map_or_else(|| "-".to_owned(), |value| format_bytes(value.len())),
        path,
    }
}

fn configuration_status(root: &Path, environment: &BTreeMap<String, String>) -> ConfigStatus {
    let path = configuration_path(
        root,
        environment.get("TJXY_CONFIG_FILE").map(String::as_str),
        environment.get("HOME").map(String::as_str),
    );
    let state = inspect_configuration(&path);
    ConfigStatus { path, state }
}

fn configuration_path(root: &Path, override_path: Option<&str>, home: Option<&str>) -> PathBuf {
    if let Some(value) = override_path {
        return resolve_path(root, value);
    }
    home.map_or_else(
        || root.join(DEFAULT_CONFIG_SUFFIX),
        |path| Path::new(path).join(DEFAULT_CONFIG_SUFFIX),
    )
}

fn inspect_configuration(path: &Path) -> ConfigState {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            return ConfigState::Invalid;
        }
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return ConfigState::Missing,
        Err(_) => return ConfigState::Unreadable,
    };
    if metadata.len() > MAX_CONFIG_BYTES {
        return ConfigState::Invalid;
    }
    let Ok(contents) = fs::read_to_string(path) else {
        return ConfigState::Unreadable;
    };
    let Ok(config) = contents.parse::<toml::Table>() else {
        return ConfigState::Invalid;
    };
    if config
        .get("format_version")
        .and_then(toml::Value::as_integer)
        != Some(1)
    {
        return ConfigState::Invalid;
    }
    match config.get("state").and_then(toml::Value::as_str) {
        Some("pending") => ConfigState::Pending,
        Some("completed") => ConfigState::Completed,
        _ => ConfigState::Invalid,
    }
}

fn read_log_tail(path: &Path, count: usize) -> io::Result<Vec<String>> {
    if count == 0 {
        return Ok(Vec::new());
    }
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "log path is not a regular file",
        ));
    }
    let mut file = File::open(path)?;
    let length = file.metadata()?.len();
    let start = length.saturating_sub(MAX_LOG_TAIL_BYTES);
    file.seek(SeekFrom::Start(start))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    let mut contents = String::from_utf8_lossy(&bytes).into_owned();
    if start > 0
        && let Some(first_newline) = contents.find('\n')
    {
        contents.drain(..=first_newline);
    }
    Ok(tail_lines(&contents, count))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ObservedServer {
    process: ProcessInfo,
    listeners: Vec<String>,
}

fn discover_observed_servers(project: &Project) -> Vec<ObservedServer> {
    let Ok(output) = Command::new("lsof")
        .args(["-a", "-c", "tjxy-serv", "-d", "txt", "-Fpn"])
        .current_dir(&project.root)
        .output()
    else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }

    let expected = [
        project.root.join("tjxy-server"),
        project.server_binary("debug"),
        project.server_binary("release"),
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
        .map(|(pid, _)| ObservedServer {
            process: process_info(pid).unwrap_or(ProcessInfo {
                pid,
                elapsed: "-".to_owned(),
                rss: "-".to_owned(),
                cpu: "-".to_owned(),
            }),
            listeners: process_listeners(pid, &project.root),
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

fn process_listeners(pid: u32, root: &Path) -> Vec<String> {
    let output = Command::new("lsof")
        .args([
            "-a",
            "-p",
            &pid.to_string(),
            "-nP",
            "-iTCP",
            "-sTCP:LISTEN",
            "-Fpn",
        ])
        .current_dir(root)
        .output();
    let Ok(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    let mut listeners = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.strip_prefix('n').map(str::to_owned))
        .collect::<Vec<_>>();
    listeners.sort();
    listeners.dedup();
    listeners
}

fn select_primary_server(
    servers: &[ObservedServer],
    configured_port: u16,
) -> Option<&ObservedServer> {
    servers
        .iter()
        .find(|server| {
            server
                .listeners
                .iter()
                .any(|listener| endpoint_port(listener) == Some(configured_port))
        })
        .or_else(|| servers.first())
}

fn endpoint_port(endpoint: &str) -> Option<u16> {
    endpoint.rsplit_once(':')?.1.parse().ok()
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

fn process_info(pid: u32) -> Option<ProcessInfo> {
    let output = Command::new("ps")
        .args(["-o", "etime=,rss=,%cpu=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let fields = String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .map(str::to_owned)
        .collect::<Vec<_>>();
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
    fn parses_dotenv_lines() {
        let parsed = parse_env_lines(
            "# comment\nTJXY_BIND=\"127.0.0.1:8096\"\nTJXY_LOG_FILE='logs/tjxy.log'\nINVALID\n",
        );
        assert_eq!(parsed.get("TJXY_BIND"), Some(&"127.0.0.1:8096".to_owned()));
        assert_eq!(
            parsed.get("TJXY_LOG_FILE"),
            Some(&"logs/tjxy.log".to_owned())
        );
        assert!(!parsed.contains_key("INVALID"));
    }

    #[test]
    fn returns_only_the_last_requested_log_lines() {
        assert_eq!(tail_lines("one\ntwo\nthree\n", 2), vec!["two", "three"]);
        assert_eq!(tail_lines("", 3), Vec::<String>::new());
    }

    #[test]
    fn reads_only_a_bounded_log_tail() {
        let project = temporary_project("log-tail");
        let log_path = project.root.join(DEFAULT_LOG_PATH);
        fs::create_dir_all(log_path.parent().unwrap()).unwrap();
        let prefix = "x".repeat(usize::try_from(MAX_LOG_TAIL_BYTES).unwrap());
        fs::write(&log_path, format!("{prefix}\nfirst\nsecond\nthird\n")).unwrap();

        assert_eq!(
            read_log_tail(&log_path, 2).unwrap(),
            vec!["second", "third"]
        );
        fs::remove_dir_all(&project.root).unwrap();
    }

    #[test]
    fn release_binary_is_preferred_over_debug() {
        let project = temporary_project("binary-preference");
        let debug = project.server_binary("debug");
        let release = project.server_binary("release");
        fs::create_dir_all(debug.parent().unwrap()).unwrap();
        fs::create_dir_all(release.parent().unwrap()).unwrap();
        fs::write(&debug, "debug").unwrap();
        assert_eq!(project.preferred_server_binary(), Some(debug));
        fs::write(&release, "release").unwrap();
        assert_eq!(project.preferred_server_binary(), Some(release));
        let packaged = project.root.join("tjxy-server");
        fs::write(&packaged, "packaged").unwrap();
        assert_eq!(project.preferred_server_binary(), Some(packaged));
        fs::remove_dir_all(&project.root).unwrap();
    }

    #[test]
    fn identifies_cargo_output_directories() {
        assert!(is_cargo_target_directory(Path::new("/repo/target/debug")));
        assert!(is_cargo_target_directory(Path::new("/repo/target/release")));
        assert!(is_cargo_target_directory(Path::new("/repo/target/dist")));
        assert!(!is_cargo_target_directory(Path::new("/opt/tjxy")));
    }

    #[test]
    fn lifecycle_events_are_appended_without_overwriting_server_logs() {
        let project = temporary_project("lifecycle-log");
        let log_path = project.root.join(DEFAULT_LOG_PATH);
        fs::create_dir_all(log_path.parent().unwrap()).unwrap();
        fs::write(&log_path, "existing server output\n").unwrap();

        append_lifecycle_event(&log_path, "start requested").unwrap();
        let contents = fs::read_to_string(&log_path).unwrap();

        assert!(contents.starts_with("existing server output\n"));
        assert!(contents.contains("] start requested\n"));
        fs::remove_dir_all(&project.root).unwrap();
    }

    #[test]
    fn command_line_matching_does_not_accept_similarly_named_programs() {
        let binary = Path::new("/repo/target/release/tjxy-server");
        assert!(command_line_matches_binary(
            "/repo/target/release/tjxy-server",
            binary
        ));
        assert!(command_line_matches_binary(
            "/repo/target/release/tjxy-server --flag",
            binary
        ));
        assert!(!command_line_matches_binary(
            "/repo/target/release/tjxy-server-helper",
            binary
        ));
    }

    #[cfg(unix)]
    #[test]
    fn termination_signal_stops_a_child_process() {
        let mut child = Command::new("sleep").arg("10").spawn().unwrap();
        let pid = child.id();

        signal_process(pid).unwrap();
        let status = child.wait().unwrap();

        assert!(!status.success());
        assert!(!process_alive(pid));
    }

    #[test]
    fn configuration_status_is_read_only_and_recognizes_state() {
        let project = temporary_project("configuration");
        let path = project.root.join("tjxy.toml");
        assert_eq!(inspect_configuration(&path), ConfigState::Missing);

        fs::write(&path, "format_version = 1\nstate = \"pending\"\n").unwrap();
        assert_eq!(inspect_configuration(&path), ConfigState::Pending);
        fs::write(&path, "format_version = 1\nstate = \"completed\"\n").unwrap();
        assert_eq!(inspect_configuration(&path), ConfigState::Completed);
        fs::write(&path, "not valid toml = [").unwrap();
        assert_eq!(inspect_configuration(&path), ConfigState::Invalid);
        fs::remove_dir_all(&project.root).unwrap();
    }

    #[test]
    fn configuration_path_uses_system_default_and_resolves_explicit_overrides() {
        let root = Path::new("/srv/tjxy");

        assert_eq!(
            configuration_path(root, None, Some("/home/media")),
            PathBuf::from("/home/media/.config/tjxy/tjxy.toml")
        );
        assert_eq!(
            configuration_path(root, Some("config/tjxy.toml"), None),
            root.join("config/tjxy.toml")
        );
        assert_eq!(
            configuration_path(root, Some("/config/tjxy.toml"), None),
            PathBuf::from("/config/tjxy.toml")
        );
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

    #[test]
    fn parses_server_pids_and_executables_without_pgrep() {
        let output = "p41\nftxt\nn/repo/target/debug/tjxy-server\nn/usr/lib/dyld\np42\nftxt\nn/repo/target/release/tjxy-server\n";
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
