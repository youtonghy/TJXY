use std::{
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
use tjxy_tui::{Action, ActionReport, DatabaseBackend, Project, StatusSnapshot};

const TABS: [&str; 5] = ["Overview", "Service", "Build", "Database", "Logs / Config"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum View {
    Overview,
    Service,
    Build,
    Database,
    Logs,
}

impl View {
    const fn index(self) -> usize {
        match self {
            Self::Overview => 0,
            Self::Service => 1,
            Self::Build => 2,
            Self::Database => 3,
            Self::Logs => 4,
        }
    }

    fn next(self) -> Self {
        match self {
            Self::Overview => Self::Service,
            Self::Service => Self::Build,
            Self::Build => Self::Database,
            Self::Database => Self::Logs,
            Self::Logs => Self::Overview,
        }
    }

    fn previous(self) -> Self {
        match self {
            Self::Overview => Self::Logs,
            Self::Service => Self::Overview,
            Self::Build => Self::Service,
            Self::Database => Self::Build,
            Self::Logs => Self::Database,
        }
    }
}

#[derive(Debug)]
struct UiState {
    view: View,
    status: Option<ActionReport>,
    log_lines: usize,
    pending: Option<Action>,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            view: View::Overview,
            status: None,
            log_lines: 30,
            pending: None,
        }
    }
}

impl UiState {
    fn begin_action(&mut self, action: Action) -> bool {
        if self.pending.is_some() {
            return false;
        }
        self.pending = Some(action);
        self.status = Some(ActionReport::ok(format!("{action:?} is running")));
        true
    }

    fn complete_action(&mut self, report: ActionReport) {
        self.pending = None;
        self.status = Some(report);
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
            KeyCode::Char('q') | KeyCode::Esc => {
                if state.pending.is_some() {
                    state.status = Some(ActionReport::error(
                        "wait for the running management action before quitting",
                    ));
                } else {
                    break;
                }
            }
            KeyCode::Left | KeyCode::Char('h') => state.view = state.view.previous(),
            KeyCode::Right | KeyCode::Char('l') | KeyCode::Tab => state.view = state.view.next(),
            KeyCode::Char('r') => {
                snapshot = project.snapshot();
                last_refresh = Instant::now();
                state.status = Some(ActionReport::ok("status refreshed"));
            }
            KeyCode::Char(key) => {
                if let Some(action) = action_for_key(state.view, key, snapshot.database.backend) {
                    if state.begin_action(action) {
                        let worker_project = project.clone();
                        let worker_sender = action_sender.clone();
                        if let Err(error) = thread::Builder::new()
                            .name("tjxy-tui-action".to_owned())
                            .spawn(move || {
                                let report =
                                    std::panic::catch_unwind(|| worker_project.run_action(action))
                                        .unwrap_or_else(|_| {
                                            ActionReport::error(format!(
                                                "{action:?} worker panicked"
                                            ))
                                        });
                                let _ = worker_sender.send(report);
                            })
                        {
                            state.complete_action(ActionReport::error(format!(
                                "start management action: {error}"
                            )));
                        }
                    } else {
                        state.status = Some(ActionReport::error(
                            "another management action is already running",
                        ));
                    }
                } else if state.view == View::Logs && key == '+' {
                    state.log_lines = if state.log_lines == 30 { 100 } else { 30 };
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn action_for_key(view: View, key: char, database_backend: DatabaseBackend) -> Option<Action> {
    match view {
        View::Overview | View::Service => match key {
            '1' => Some(Action::StartServer),
            '2' => Some(Action::StopServer),
            '3' => Some(Action::RestartServer),
            'b' => Some(Action::BuildDebug),
            'a' => Some(Action::BuildAdmin),
            'c' => Some(Action::CheckProject),
            _ => None,
        },
        View::Build => match key {
            'd' => Some(Action::BuildDebug),
            'R' => Some(Action::BuildRelease),
            'a' => Some(Action::BuildAdmin),
            'A' => Some(Action::BuildAll),
            'c' => Some(Action::CheckProject),
            _ => None,
        },
        View::Database if database_backend == DatabaseBackend::SQLite => match key {
            'b' => Some(Action::BackupDatabase),
            'i' => Some(Action::IntegrityCheck),
            'v' => Some(Action::VacuumDatabase),
            _ => None,
        },
        View::Database | View::Logs => None,
    }
}

fn render(frame: &mut Frame<'_>, project: &Project, snapshot: &StatusSnapshot, state: &UiState) {
    let area = frame.area();
    if area.width < 78 || area.height < 20 {
        let message = Paragraph::new("Terminal too small. Minimum size: 78 x 20")
            .block(Block::default().borders(Borders::ALL).title("TJXY TUI"))
            .style(Style::default().fg(Color::Yellow))
            .alignment(ratatui::layout::Alignment::Center);
        frame.render_widget(message, area);
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
        Span::raw(" server management console"),
        Span::styled(
            format!("  {}", project.root.display()),
            Style::default().fg(Color::DarkGray),
        ),
    ]))
    .block(Block::default().borders(Borders::BOTTOM));
    frame.render_widget(title, root[0]);

    let tabs = Tabs::new(TABS)
        .select(state.view.index())
        .highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .divider("|")
        .block(Block::default().borders(Borders::BOTTOM));
    frame.render_widget(tabs, root[1]);

    match state.view {
        View::Overview => render_overview(frame, root[2], snapshot),
        View::Service => render_service(frame, root[2], project, snapshot),
        View::Build => render_build(frame, root[2], snapshot),
        View::Database => render_database(frame, root[2], snapshot),
        View::Logs => render_logs(frame, root[2], project, state),
    }

    let footer = footer_line(state, snapshot);
    frame.render_widget(
        Paragraph::new(footer)
            .style(Style::default().fg(Color::DarkGray))
            .block(Block::default().borders(Borders::TOP)),
        root[3],
    );
}

fn render_overview(frame: &mut Frame<'_>, area: Rect, snapshot: &StatusSnapshot) {
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    let service_rows = vec![
        kv_line("Server", server_state_label(snapshot)),
        kv_line(&snapshot.server_bind, open_label(snapshot.server_port_open)),
        kv_line("Admin port 5173", open_label(snapshot.admin_port_open)),
        kv_line(
            "PID",
            snapshot
                .server
                .as_ref()
                .map_or_else(|| "-".to_owned(), |process| process.pid.to_string()),
        ),
        kv_line(
            "Uptime",
            snapshot
                .server
                .as_ref()
                .map_or_else(|| "-".to_owned(), |process| process.elapsed.clone()),
        ),
    ];
    frame.render_widget(
        List::new(service_rows)
            .block(panel("Service status"))
            .highlight_style(Style::default().fg(Color::Cyan)),
        columns[0],
    );

    let build_rows = vec![
        kv_line("Server binary", snapshot.build_mode.label().to_owned()),
        kv_line("Binary size", snapshot.binary_size.clone()),
        kv_line(
            "Database",
            format!(
                "{} {}",
                snapshot.database.backend.label(),
                connection_label(snapshot.database.connected)
            ),
        ),
        kv_line("Admin deps", yes_no(snapshot.admin_deps)),
        kv_line("Admin dist", yes_no(snapshot.admin_dist)),
        kv_line("Rust", snapshot.rust_version.clone()),
        kv_line("Node", snapshot.node_version.clone()),
    ];
    frame.render_widget(
        List::new(build_rows).block(panel("Build and toolchain")),
        columns[1],
    );
}

fn render_service(frame: &mut Frame<'_>, area: Rect, project: &Project, snapshot: &StatusSnapshot) {
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(8), Constraint::Min(5)])
        .split(area);

    let rows = vec![
        Row::new(vec![
            Cell::from("Server"),
            Cell::from(server_state_label(snapshot)),
            Cell::from(format!("PID {}", pid_label(snapshot))),
        ]),
        Row::new(vec![
            Cell::from("HTTP"),
            Cell::from(open_label(snapshot.server_port_open)),
            Cell::from(snapshot.server_bind.clone()),
        ]),
        Row::new(vec![
            Cell::from("Admin dev"),
            Cell::from(open_label(snapshot.admin_port_open)),
            Cell::from("127.0.0.1:5173"),
        ]),
        Row::new(vec![
            Cell::from("Instances"),
            Cell::from(snapshot.server_instances.to_string()),
            Cell::from(snapshot.server_listeners.join(", ")),
        ]),
    ];
    frame.render_widget(
        Table::new(
            rows,
            [
                Constraint::Length(14),
                Constraint::Length(20),
                Constraint::Min(20),
            ],
        )
        .header(Row::new(vec!["Component", "State", "Details"]))
        .block(panel("Managed services"))
        .column_spacing(2),
        sections[0],
    );

    let config = project
        .environment_rows()
        .into_iter()
        .take(8)
        .map(|(key, value)| ListItem::new(format!("{key} = {value}")))
        .collect::<Vec<_>>();
    frame.render_widget(
        List::new(config).block(panel("Runtime configuration (secrets masked)")),
        sections[1],
    );
}

fn render_build(frame: &mut Frame<'_>, area: Rect, snapshot: &StatusSnapshot) {
    let sections = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    let actions = [
        "[d] Build server debug",
        "[R] Build server release",
        "[a] Build admin production",
        "[A] Build all",
        "[c] Run checks",
    ]
    .into_iter()
    .map(ListItem::new)
    .collect::<Vec<_>>();
    frame.render_widget(
        List::new(actions).block(panel("Build actions")),
        sections[0],
    );

    let artifacts = vec![
        kv_line("Server", snapshot.build_mode.label().to_owned()),
        kv_line("Binary", snapshot.binary_size.clone()),
        kv_line("Admin deps", yes_no(snapshot.admin_deps)),
        kv_line("Admin dist", yes_no(snapshot.admin_dist)),
        kv_line("Rust", snapshot.rust_version.clone()),
        kv_line(
            "Node / npm",
            format!("{} / {}", snapshot.node_version, snapshot.npm_version),
        ),
    ];
    frame.render_widget(
        List::new(artifacts).block(panel("Current artifacts")),
        sections[1],
    );
}

fn render_database(frame: &mut Frame<'_>, area: Rect, snapshot: &StatusSnapshot) {
    let sections = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    let status = vec![
        kv_line("Backend", snapshot.database.backend.label().to_owned()),
        kv_line("Target", snapshot.database.target.clone()),
        kv_line("Connection", connection_label(snapshot.database.connected)),
        kv_line("Size", snapshot.database.size.clone()),
    ];
    frame.render_widget(
        List::new(status).block(panel("Database status")),
        sections[0],
    );

    let actions = if snapshot.database.backend == DatabaseBackend::SQLite {
        vec![
            ListItem::new("[b] Backup database"),
            ListItem::new("[i] PRAGMA integrity_check"),
            ListItem::new("[v] VACUUM"),
        ]
    } else {
        vec![
            ListItem::new("Status monitoring only"),
            ListItem::new("Maintenance: external database tooling"),
        ]
    };
    frame.render_widget(
        List::new(actions).block(panel("Database actions")),
        sections[1],
    );
}

fn render_logs(frame: &mut Frame<'_>, area: Rect, project: &Project, state: &UiState) {
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(4)])
        .split(area);

    let header = Paragraph::new(format!(
        "{}  |  showing last {} lines  |  [+] toggle 30/100",
        project.log_path().display(),
        state.log_lines
    ))
    .block(panel("Log file"));
    frame.render_widget(header, sections[0]);

    let visible_lines = sections[1].height.saturating_sub(2) as usize;
    let lines = project
        .log_lines(state.log_lines)
        .into_iter()
        .rev()
        .take(visible_lines)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(|line| {
            let style = if line.to_ascii_lowercase().contains("error")
                || line.to_ascii_lowercase().contains("fail")
            {
                Style::default().fg(Color::Red)
            } else if line.to_ascii_lowercase().contains("warn") {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::Gray)
            };
            Line::from(Span::styled(line, style))
        })
        .collect::<Vec<_>>();
    let content = if lines.is_empty() {
        vec![Line::from(Span::styled(
            "(no log content)",
            Style::default().fg(Color::DarkGray),
        ))]
    } else {
        lines
    };
    frame.render_widget(
        Paragraph::new(content)
            .block(panel("Recent output"))
            .wrap(Wrap { trim: false }),
        sections[1],
    );
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
    let mut spans = vec![
        Span::styled(
            " q ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" quit  "),
        Span::styled(
            " ←/→ ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" tabs  "),
        Span::styled(
            " r ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" refresh"),
    ];
    let action_hint = match state.view {
        View::Overview | View::Service if snapshot.server.is_some() && !snapshot.server_managed => {
            "  observed server; management disabled"
        }
        View::Overview | View::Service => "  1/2/3 server  b build  a admin  c check",
        View::Build => "  d debug  R release  a admin  A all  c check",
        View::Database if snapshot.database.backend == DatabaseBackend::SQLite => {
            "  b backup  i integrity  v vacuum"
        }
        View::Database => "  database status",
        View::Logs => "  + more/less logs",
    };
    spans.push(Span::styled(
        action_hint,
        Style::default().fg(Color::DarkGray),
    ));
    if let Some(status) = &state.status {
        spans.push(Span::raw("  |  "));
        spans.push(Span::styled(
            status.message.clone(),
            Style::default().fg(if status.ok { Color::Green } else { Color::Red }),
        ));
    }
    Line::from(spans)
}

fn server_state_label(snapshot: &StatusSnapshot) -> String {
    match (&snapshot.server, snapshot.server_managed) {
        (Some(_), true) => "RUNNING (managed)".to_owned(),
        (Some(_), false) => "RUNNING (observed)".to_owned(),
        (None, _) => "STOPPED".to_owned(),
    }
}

fn open_label(open: bool) -> String {
    if open {
        "OPEN".to_owned()
    } else {
        "CLOSED".to_owned()
    }
}

fn yes_no(value: bool) -> String {
    if value {
        "yes".to_owned()
    } else {
        "no".to_owned()
    }
}

fn connection_label(connected: bool) -> String {
    if connected {
        "CONNECTED".to_owned()
    } else {
        "NOT CONNECTED".to_owned()
    }
}

fn pid_label(snapshot: &StatusSnapshot) -> String {
    snapshot
        .server
        .as_ref()
        .map_or_else(|| "-".to_owned(), |process| process.pid.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_state_allows_only_one_management_action_at_a_time() {
        let mut state = UiState::default();

        assert!(state.begin_action(Action::BuildDebug));
        assert!(!state.begin_action(Action::BuildAdmin));
        assert_eq!(state.pending, Some(Action::BuildDebug));
    }

    #[test]
    fn completing_an_action_clears_pending_state_and_keeps_the_report() {
        let mut state = UiState::default();
        assert!(state.begin_action(Action::IntegrityCheck));

        state.complete_action(ActionReport::ok("integrity: ok"));

        assert_eq!(state.pending, None);
        assert_eq!(state.status, Some(ActionReport::ok("integrity: ok")));
    }
}
