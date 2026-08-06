use std::{
    env,
    fmt::Write as _,
    io,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::{
    DefaultTerminal, Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, List, ListItem, Paragraph, Row, Table, Tabs, Wrap},
};
use tjxy_tui::{ActionMessage, ActionReport, ConfigState, Project, ServiceAction, StatusSnapshot};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Language {
    Chinese,
    English,
}

impl Language {
    fn from_environment() -> Self {
        match env::var("TJXY_TUI_LANGUAGE").as_deref() {
            Ok("en-US" | "en") => Self::English,
            _ => Self::Chinese,
        }
    }

    const fn toggle(self) -> Self {
        match self {
            Self::Chinese => Self::English,
            Self::English => Self::Chinese,
        }
    }

    const fn text(self, chinese: &'static str, english: &'static str) -> &'static str {
        match self {
            Self::Chinese => chinese,
            Self::English => english,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum View {
    Overview,
    Diagnostics,
    Logs,
}

impl View {
    const fn index(self) -> usize {
        match self {
            Self::Overview => 0,
            Self::Diagnostics => 1,
            Self::Logs => 2,
        }
    }

    const fn next(self) -> Self {
        match self {
            Self::Overview => Self::Diagnostics,
            Self::Diagnostics => Self::Logs,
            Self::Logs => Self::Overview,
        }
    }

    const fn previous(self) -> Self {
        match self {
            Self::Overview => Self::Logs,
            Self::Diagnostics => Self::Overview,
            Self::Logs => Self::Diagnostics,
        }
    }
}

#[derive(Debug)]
struct UiState {
    view: View,
    language: Language,
    log_lines: usize,
    pending: Option<ServiceAction>,
    report: Option<ActionReport>,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            view: View::Overview,
            language: Language::from_environment(),
            log_lines: 30,
            pending: None,
            report: None,
        }
    }
}

impl UiState {
    fn begin_action(&mut self, action: ServiceAction) -> bool {
        if self.pending.is_some() {
            return false;
        }
        self.pending = Some(action);
        self.report = None;
        true
    }

    fn complete_action(&mut self, report: ActionReport) {
        self.pending = None;
        self.report = Some(report);
    }
}

fn main() {
    let project = Project::discover();
    let result = ratatui::run(|terminal| run(terminal, &project));
    if let Err(error) = result {
        eprintln!("tjxy-tui: {error}");
        std::process::exit(1);
    }
}

fn run(terminal: &mut DefaultTerminal, project: &Project) -> io::Result<()> {
    let mut state = UiState::default();
    let mut snapshot = project.snapshot();
    let mut last_refresh = Instant::now();
    let (action_sender, action_receiver) = mpsc::channel::<ActionReport>();

    loop {
        if let Ok(report) = action_receiver.try_recv() {
            state.complete_action(report);
            snapshot = project.snapshot();
            last_refresh = Instant::now();
        }
        if last_refresh.elapsed() >= Duration::from_secs(2) {
            snapshot = project.snapshot();
            last_refresh = Instant::now();
        }
        terminal.draw(|frame| render(frame, project, &snapshot, &state))?;

        if !event::poll(Duration::from_millis(250))? {
            continue;
        }
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc if state.pending.is_none() => break,
            KeyCode::Left | KeyCode::Char('h') => state.view = state.view.previous(),
            KeyCode::Right | KeyCode::Char('l') | KeyCode::Tab => state.view = state.view.next(),
            KeyCode::Char('r') => {
                snapshot = project.snapshot();
                last_refresh = Instant::now();
            }
            KeyCode::Char('g') => state.language = state.language.toggle(),
            KeyCode::Char('+') if state.view == View::Logs => {
                state.log_lines = if state.log_lines == 30 { 100 } else { 30 };
            }
            KeyCode::Char('1') => {
                dispatch_action(ServiceAction::Start, project, &mut state, &action_sender);
            }
            KeyCode::Char('2') => {
                dispatch_action(ServiceAction::Stop, project, &mut state, &action_sender);
            }
            KeyCode::Char('3') => {
                dispatch_action(ServiceAction::Restart, project, &mut state, &action_sender);
            }
            _ => {}
        }
    }
    Ok(())
}

fn dispatch_action(
    action: ServiceAction,
    project: &Project,
    state: &mut UiState,
    sender: &mpsc::Sender<ActionReport>,
) {
    if !state.begin_action(action) {
        return;
    }
    let project = project.clone();
    let sender = sender.clone();
    if let Err(error) = thread::Builder::new()
        .name("tjxy-tui-service-action".to_owned())
        .spawn(move || {
            let report =
                std::panic::catch_unwind(|| project.run_action(action)).unwrap_or(ActionReport {
                    ok: false,
                    message: ActionMessage::StartFailed,
                    pid: None,
                    detail: Some("service action worker panicked".to_owned()),
                });
            let _ = sender.send(report);
        })
    {
        state.complete_action(ActionReport {
            ok: false,
            message: ActionMessage::StartFailed,
            pid: None,
            detail: Some(format!("start action worker: {error}")),
        });
    }
}

fn render(frame: &mut Frame<'_>, project: &Project, snapshot: &StatusSnapshot, state: &UiState) {
    let language = state.language;
    let area = frame.area();
    if area.width < 72 || area.height < 18 {
        let message = language.text(
            "终端窗口过小，最小尺寸为 72 x 18",
            "Terminal too small. Minimum size: 72 x 18",
        );
        frame.render_widget(
            Paragraph::new(message)
                .block(Block::default().borders(Borders::ALL).title("TJXY TUI"))
                .style(Style::default().fg(Color::Yellow))
                .alignment(ratatui::layout::Alignment::Center),
            area,
        );
        return;
    }

    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(6),
            Constraint::Length(2),
        ])
        .split(area);
    let title = Paragraph::new(Line::from(vec![
        Span::styled(
            " TJXY ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(language.text(" 服务诊断控制台", " service diagnostic console")),
        Span::styled(
            format!("  {}", project.root.display()),
            Style::default().fg(Color::DarkGray),
        ),
    ]))
    .block(Block::default().borders(Borders::BOTTOM));
    frame.render_widget(title, root[0]);

    let tabs = Tabs::new(vec![
        language.text("概览", "Overview"),
        language.text("诊断", "Diagnostics"),
        language.text("日志", "Logs"),
    ])
    .select(state.view.index())
    .highlight_style(
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )
    .divider("|")
    .block(Block::default().borders(Borders::BOTTOM));
    frame.render_widget(tabs, root[1]);

    if diagnostics_unlocked(snapshot.configuration.state) {
        match state.view {
            View::Overview => render_overview(frame, root[2], snapshot, language),
            View::Diagnostics => render_diagnostics(frame, root[2], snapshot, language),
            View::Logs => render_logs(frame, root[2], snapshot, state),
        }
    } else {
        render_installation_gate(frame, root[2], snapshot, language);
    }
    frame.render_widget(
        Paragraph::new(footer_line(state, snapshot))
            .style(Style::default().fg(Color::DarkGray))
            .block(Block::default().borders(Borders::TOP)),
        root[3],
    );
}

const fn diagnostics_unlocked(state: ConfigState) -> bool {
    matches!(state, ConfigState::Completed)
}

fn render_installation_gate(
    frame: &mut Frame<'_>,
    area: Rect,
    snapshot: &StatusSnapshot,
    language: Language,
) {
    let (title, message) = match snapshot.configuration.state {
        ConfigState::Missing => (
            language.text("尚未安装", "Not installed"),
            language.text(
                "请按 1 启动服务，然后在桌面端完成安装。安装完成后即可查看状态和日志。",
                "Press 1 to start the service, then complete installation in the desktop app. Status and logs unlock after installation.",
            ),
        ),
        ConfigState::Pending => (
            language.text("安装待恢复", "Installation pending"),
            language.text(
                "请启动服务并在桌面端继续安装恢复。完成后即可查看状态和日志。",
                "Start the service and resume installation in the desktop app. Status and logs unlock after completion.",
            ),
        ),
        ConfigState::Invalid | ConfigState::Unreadable => (
            language.text("安装配置异常", "Installation config issue"),
            language.text(
                "安装配置无效或无法读取，请先在桌面端修复安装后再查看。",
                "The installation config is invalid or unreadable. Repair the installation in the desktop app before viewing status or logs.",
            ),
        ),
        ConfigState::Completed => return,
    };
    let content = vec![
        Line::styled(
            title,
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Line::from(""),
        Line::from(message),
        Line::from(""),
        Line::styled(
            format!(
                "{}: {}",
                language.text("配置位置", "Config path"),
                snapshot.configuration.path.display()
            ),
            Style::default().fg(Color::DarkGray),
        ),
    ];
    frame.render_widget(
        Paragraph::new(content)
            .block(panel(language.text("安装要求", "Installation required")))
            .wrap(Wrap { trim: true })
            .alignment(ratatui::layout::Alignment::Center),
        area,
    );
}

fn render_overview(
    frame: &mut Frame<'_>,
    area: Rect,
    snapshot: &StatusSnapshot,
    language: Language,
) {
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);
    let process = snapshot.server.as_ref();
    let service_rows = vec![
        kv_line(
            language.text("后端", "Backend"),
            server_state(snapshot, language),
        ),
        kv_line(
            language.text("监听地址", "Listen address"),
            snapshot.server_bind.clone(),
        ),
        kv_line(
            language.text("HTTP 端口", "HTTP port"),
            open_label(snapshot.server_port_open, language),
        ),
        kv_line(
            language.text("进程 ID", "Process ID"),
            process.map_or_else(|| "-".to_owned(), |value| value.pid.to_string()),
        ),
        kv_line(
            language.text("运行时间", "Uptime"),
            process.map_or_else(|| "-".to_owned(), |value| value.elapsed.clone()),
        ),
        kv_line(
            language.text("实例数", "Instances"),
            snapshot.server_instances.to_string(),
        ),
    ];
    frame.render_widget(
        List::new(service_rows).block(panel(language.text("服务状态", "Service status"))),
        columns[0],
    );

    let runtime_rows = vec![
        kv_line(
            language.text("CPU", "CPU"),
            process.map_or_else(|| "-".to_owned(), |value| value.cpu.clone()),
        ),
        kv_line(
            language.text("内存", "Memory"),
            process.map_or_else(|| "-".to_owned(), |value| value.rss.clone()),
        ),
        kv_line(
            language.text("配置", "Configuration"),
            config_label(snapshot.configuration.state, language).to_owned(),
        ),
        kv_line(
            language.text("前端资源", "Frontend assets"),
            present_label(snapshot.admin_dist.exists, language),
        ),
        kv_line(
            language.text("日志文件", "Log file"),
            present_label(snapshot.log.exists, language),
        ),
        kv_line(
            language.text("日志大小", "Log size"),
            snapshot.log.size.clone(),
        ),
    ];
    frame.render_widget(
        List::new(runtime_rows).block(panel(language.text("运行诊断", "Runtime diagnostics"))),
        columns[1],
    );
}

fn render_diagnostics(
    frame: &mut Frame<'_>,
    area: Rect,
    snapshot: &StatusSnapshot,
    language: Language,
) {
    let rows = vec![
        diagnostic_row(
            language.text("后端进程", "Backend process"),
            snapshot.server.is_some(),
            server_state(snapshot, language),
            language,
        ),
        diagnostic_row(
            language.text("HTTP 端口", "HTTP port"),
            snapshot.server_port_open,
            snapshot.server_bind.clone(),
            language,
        ),
        Row::new(vec![
            Cell::from(language.text("安装配置", "Installation config")),
            Cell::from(config_label(snapshot.configuration.state, language)),
            Cell::from(snapshot.configuration.path.display().to_string()),
        ])
        .style(Style::default().fg(config_color(snapshot.configuration.state))),
        diagnostic_row(
            language.text("前端静态资源", "Frontend assets"),
            snapshot.admin_dist.exists,
            snapshot.admin_dist.path.display().to_string(),
            language,
        ),
        diagnostic_row(
            language.text("后端日志", "Backend log"),
            snapshot.log.exists,
            snapshot.log.path.display().to_string(),
            language,
        ),
    ];
    frame.render_widget(
        Table::new(
            rows,
            [
                Constraint::Length(20),
                Constraint::Length(18),
                Constraint::Min(24),
            ],
        )
        .header(
            Row::new(vec![
                language.text("检查项", "Check"),
                language.text("状态", "State"),
                language.text("详情", "Details"),
            ])
            .style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        )
        .block(panel(language.text("诊断检查", "Diagnostic checks")))
        .column_spacing(2),
        area,
    );
}

fn render_logs(frame: &mut Frame<'_>, area: Rect, snapshot: &StatusSnapshot, state: &UiState) {
    let language = state.language;
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(4)])
        .split(area);
    let header = format!(
        "{}  |  {} {}  |  [+] {}",
        snapshot.log.path.display(),
        language.text("末尾", "last"),
        state.log_lines,
        language.text("切换 30/100 行", "toggle 30/100 lines"),
    );
    frame.render_widget(
        Paragraph::new(header).block(panel(language.text("日志文件", "Log file"))),
        sections[0],
    );

    let visible_lines = sections[1].height.saturating_sub(2) as usize;
    let content = if let Some(error) = &snapshot.log_error {
        vec![Line::styled(
            format!(
                "{}: {error}",
                language.text("无法读取日志", "Cannot read log")
            ),
            Style::default().fg(Color::Red),
        )]
    } else if snapshot.recent_log_lines.is_empty() {
        vec![Line::styled(
            language.text("（日志文件为空）", "(log file is empty)"),
            Style::default().fg(Color::DarkGray),
        )]
    } else {
        let limit = state.log_lines.min(visible_lines);
        snapshot
            .recent_log_lines
            .iter()
            .rev()
            .take(limit)
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .map(|line| Line::styled(line.clone(), log_style(&line)))
            .collect()
    };
    frame.render_widget(
        Paragraph::new(content)
            .block(panel(language.text("最近输出", "Recent output")))
            .wrap(Wrap { trim: false }),
        sections[1],
    );
}

fn diagnostic_row(
    component: &'static str,
    healthy: bool,
    details: String,
    language: Language,
) -> Row<'static> {
    Row::new(vec![
        Cell::from(component),
        Cell::from(if healthy {
            language.text("正常", "OK")
        } else {
            language.text("异常", "ISSUE")
        }),
        Cell::from(details),
    ])
    .style(Style::default().fg(if healthy { Color::Green } else { Color::Red }))
}

fn panel(title: &'static str) -> Block<'static> {
    Block::default().borders(Borders::ALL).title(title)
}

fn kv_line(key: &str, value: String) -> ListItem<'static> {
    ListItem::new(Line::from(vec![
        Span::styled(format!("{key:<18}"), Style::default().fg(Color::Cyan)),
        Span::raw(value),
    ]))
}

fn footer_line(state: &UiState, snapshot: &StatusSnapshot) -> Line<'static> {
    let language = state.language;
    let mut spans = vec![
        key_hint(" q "),
        Span::raw(language.text(" 退出  ", " quit  ")),
        key_hint(" ←/→ "),
        Span::raw(language.text(" 切页  ", " tabs  ")),
        key_hint(" r "),
        Span::raw(language.text(" 刷新  ", " refresh  ")),
        key_hint(" g "),
        Span::raw(language.text(" English", " 中文")),
    ];
    spans.push(Span::raw("  |  "));
    if let Some(action) = state.pending {
        spans.push(Span::styled(
            pending_label(action, language),
            Style::default().fg(Color::Yellow),
        ));
    } else {
        if snapshot.server.is_some() {
            spans.push(key_hint(" 2 "));
            spans.push(Span::raw(language.text(" 关闭  ", " stop  ")));
            spans.push(key_hint(" 3 "));
            spans.push(Span::raw(language.text(" 重启", " restart")));
        } else {
            spans.push(key_hint(" 1 "));
            spans.push(Span::raw(language.text(" 启动", " start")));
        }
        if let Some(report) = &state.report {
            spans.push(Span::raw("  |  "));
            spans.push(Span::styled(
                action_report_label(report, language),
                Style::default().fg(if report.ok { Color::Green } else { Color::Red }),
            ));
        }
    }
    if state.view == View::Logs {
        spans.push(Span::raw(
            language.text("  |  + 更多/更少日志", "  |  + more/less logs"),
        ));
    }
    Line::from(spans)
}

fn pending_label(action: ServiceAction, language: Language) -> &'static str {
    match action {
        ServiceAction::Start => language.text("正在启动…", "starting..."),
        ServiceAction::Stop => language.text("正在关闭…", "stopping..."),
        ServiceAction::Restart => language.text("正在重启…", "restarting..."),
    }
}

fn action_report_label(report: &ActionReport, language: Language) -> String {
    let message = match report.message {
        ActionMessage::Started => language.text("服务已启动", "service started"),
        ActionMessage::Stopped => language.text("服务已关闭", "service stopped"),
        ActionMessage::Restarted => language.text("服务已重启", "service restarted"),
        ActionMessage::AlreadyRunning => language.text("服务已经运行", "service already running"),
        ActionMessage::NotRunning => language.text("服务未运行", "service is not running"),
        ActionMessage::MultipleInstances => language.text(
            "发现多个实例，操作已拒绝",
            "multiple instances found; action refused",
        ),
        ActionMessage::BinaryMissing => language.text(
            "服务程序不存在，请先安装",
            "server binary missing; install it first",
        ),
        ActionMessage::LogUnavailable => language.text("日志文件不可用", "log file unavailable"),
        ActionMessage::StartFailed => language.text("启动失败", "start failed"),
        ActionMessage::StopFailed => language.text("关闭失败", "stop failed"),
        ActionMessage::StopTimedOut => language.text(
            "关闭超时，未强制终止",
            "stop timed out; process was not killed",
        ),
    };
    let mut value = message.to_owned();
    if let Some(pid) = report.pid {
        let _ = write!(value, " (PID {pid})");
    }
    if let Some(detail) = &report.detail {
        value.push_str(": ");
        value.push_str(detail);
    }
    value
}

fn key_hint(value: &'static str) -> Span<'static> {
    Span::styled(
        value,
        Style::default()
            .fg(Color::Black)
            .bg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )
}

fn server_state(snapshot: &StatusSnapshot, language: Language) -> String {
    if snapshot.server.is_some() {
        language.text("运行中", "RUNNING").to_owned()
    } else {
        language.text("未运行", "STOPPED").to_owned()
    }
}

fn open_label(open: bool, language: Language) -> String {
    if open {
        language.text("开放", "OPEN").to_owned()
    } else {
        language.text("关闭", "CLOSED").to_owned()
    }
}

fn present_label(present: bool, language: Language) -> String {
    if present {
        language.text("存在", "PRESENT").to_owned()
    } else {
        language.text("缺失", "MISSING").to_owned()
    }
}

const fn config_label(state: ConfigState, language: Language) -> &'static str {
    match state {
        ConfigState::Missing => language.text("未配置", "MISSING"),
        ConfigState::Pending => language.text("待恢复", "PENDING"),
        ConfigState::Completed => language.text("已完成", "COMPLETED"),
        ConfigState::Invalid => language.text("无效", "INVALID"),
        ConfigState::Unreadable => language.text("不可读取", "UNREADABLE"),
    }
}

const fn config_color(state: ConfigState) -> Color {
    match state {
        ConfigState::Completed => Color::Green,
        ConfigState::Missing | ConfigState::Pending => Color::Yellow,
        ConfigState::Invalid | ConfigState::Unreadable => Color::Red,
    }
}

fn log_style(line: &str) -> Style {
    let lower = line.to_lowercase();
    if ["error", "fail", "fatal", "panic", "错误", "失败"]
        .iter()
        .any(|needle| lower.contains(needle))
    {
        Style::default().fg(Color::Red)
    } else if lower.contains("warn") || lower.contains("警告") {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::Gray)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn language_toggle_is_reversible() {
        assert_eq!(Language::Chinese.toggle(), Language::English);
        assert_eq!(Language::English.toggle(), Language::Chinese);
    }

    #[test]
    fn views_cycle_in_both_directions() {
        assert_eq!(View::Overview.next(), View::Diagnostics);
        assert_eq!(View::Overview.previous(), View::Logs);
        assert_eq!(View::Logs.next(), View::Overview);
    }

    #[test]
    fn errors_and_warnings_receive_diagnostic_colors() {
        assert_eq!(log_style("ERROR backend failed").fg, Some(Color::Red));
        assert_eq!(log_style("警告: retrying").fg, Some(Color::Yellow));
        assert_eq!(log_style("request completed").fg, Some(Color::Gray));
    }

    #[test]
    fn ui_allows_only_one_service_action_at_a_time() {
        let mut state = UiState::default();
        assert!(state.begin_action(ServiceAction::Start));
        assert!(!state.begin_action(ServiceAction::Restart));

        state.complete_action(ActionReport {
            ok: true,
            message: ActionMessage::Started,
            pid: Some(42),
            detail: None,
        });

        assert_eq!(state.pending, None);
        assert!(state.report.as_ref().is_some_and(|report| report.ok));
    }

    #[test]
    fn action_reports_are_localized() {
        let report = ActionReport {
            ok: false,
            message: ActionMessage::BinaryMissing,
            pid: None,
            detail: None,
        };
        assert!(action_report_label(&report, Language::Chinese).contains("请先安装"));
        assert!(action_report_label(&report, Language::English).contains("install it first"));
    }

    #[test]
    fn diagnostics_unlock_only_after_installation_is_completed() {
        assert!(diagnostics_unlocked(ConfigState::Completed));
        for state in [
            ConfigState::Missing,
            ConfigState::Pending,
            ConfigState::Invalid,
            ConfigState::Unreadable,
        ] {
            assert!(!diagnostics_unlocked(state));
        }
    }
}
