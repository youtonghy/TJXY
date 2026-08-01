#!/usr/bin/env python3
"""
TJXY Management TUI
A comprehensive text-based management interface for TJXY Media Catalog Server.

Usage:
  ./scripts/tjxy-tui.py
  python3 scripts/tjxy-tui.py

Supports macOS (launchd) and Linux (systemd) service management.
"""

import curses
import json
import os
import pathlib
import signal
import subprocess
import sys
import time
import threading
from datetime import datetime
from typing import Optional

# ─── Constants ────────────────────────────────────────────────────────

PROJECT_ROOT = pathlib.Path(__file__).resolve().parent.parent
ADMIN_DIR = PROJECT_ROOT / "admin"
SERVER_BINARY = "tjxy-server"
CARGO_PKG = "-p tjxy-server --bin tjxy-server"
DB_PATH = PROJECT_ROOT / "tjxy.db"
PID_DIR = pathlib.Path("/tmp")
PID_FILE = PID_DIR / "tjxy-server.pid"
LOG_DIR = PROJECT_ROOT / "data"
LOG_FILE = LOG_DIR / "server.log"
ENV_FILE = PROJECT_ROOT / ".env"

# Color pairs
C_DEFAULT = 0
C_TITLE = 1       # white on blue
C_MENU = 2         # white on default
C_MENU_ACTIVE = 3  # white on blue (selected)
C_STATUS_OK = 4    # green on default
C_STATUS_WARN = 5  # yellow on default
C_STATUS_ERR = 6   # red on default
C_HEADER = 7       # bold white on default
C_BORDER = 8       # blue on default
C_LABEL = 9        # cyan on default
C_VALUE = 10       # white on default
C_HIGHLIGHT = 11   # black on cyan
C_DIM = 12         # dark gray on default
C_ACTIVE_BG = 13   # black on white

# ─── Helpers ──────────────────────────────────────────────────────────

def run(cmd, **kwargs):
    """Run a shell command, return (rc, stdout, stderr)."""
    try:
        r = subprocess.run(
            cmd, capture_output=True, text=True, timeout=kwargs.pop("timeout", 30), **kwargs
        )
        return r.returncode, r.stdout.strip(), r.stderr.strip()
    except subprocess.TimeoutExpired:
        return -1, "", "Timeout"
    except FileNotFoundError:
        return -1, "", "Command not found"
    except Exception as e:
        return -1, "", str(e)


def pid_alive(pid):
    """Check if a PID is alive."""
    try:
        os.kill(pid, 0)
        return True
    except (OSError, ProcessLookupError):
        return False


def find_server_pid():
    """Find the server PID from PID file or pgrep."""
    if PID_FILE.exists():
        try:
            pid = int(PID_FILE.read_text().strip())
            if pid_alive(pid):
                return pid
        except (ValueError, OSError):
            pass
    _, out, _ = run(["pgrep", "-f", "tjxy-server"])
    if out:
        try:
            pid = int(out.split("\n")[0])
            if pid_alive(pid):
                return pid
        except (ValueError, IndexError):
            pass
    return None


def find_admin_dev_pid():
    """Find the admin Vite dev server PID."""
    _, out, _ = run(["pgrep", "-f", "vite"])
    if out:
        for line in out.split("\n"):
            try:
                pid = int(line.strip())
                if pid_alive(pid):
                    # Check if it's the admin's vite
                    rc, _, _ = run(["lsof", "-p", str(pid), "-Fn"], timeout=5)
                    if rc == 0:
                        return pid
            except ValueError:
                continue
    return None


def check_port(port=8096):
    """Check if port is in use using lsof."""
    rc, out, _ = run(["lsof", "-i", f"tcp:{port}", "-P", "-n"], timeout=5)
    if rc == 0 and "LISTEN" in out:
        for line in out.split("\n"):
            if "LISTEN" in line and "tjxy-server" in line:
                return True
    return False


def check_admin_port(port=5173):
    """Check if admin dev port is in use."""
    rc, out, _ = run(["lsof", "-i", f"tcp:{port}", "-P", "-n"], timeout=5)
    return rc == 0 and "LISTEN" in out


def get_db_size():
    """Get database file size in human-readable format."""
    if not DB_PATH.exists():
        return "N/A"
    size = DB_PATH.stat().st_size
    for unit in ("B", "KB", "MB", "GB"):
        if size < 1024:
            return f"{size:.1f} {unit}"
        size /= 1024
    return f"{size:.1f} TB"


def get_server_uptime(pid):
    """Get server uptime string."""
    try:
        rc, out, _ = run(["ps", "-o", "etime=", "-p", str(pid)], timeout=5)
        if rc == 0 and out:
            return out.strip()
    except Exception:
        pass
    return "N/A"


def get_rust_version():
    """Get Rust toolchain version."""
    _, out, _ = run(["rustc", "--version"])
    return out or "N/A"


def get_node_version():
    """Get Node.js version."""
    _, out, _ = run(["node", "--version"])
    return out or "N/A"


def get_npm_version():
    """Get npm version."""
    _, out, _ = run(["npm", "--version"])
    return out or "N/A"


def get_build_mode():
    """Detect debug/release build."""
    release = PROJECT_ROOT / "target" / "release" / SERVER_BINARY
    debug = PROJECT_ROOT / "target" / "debug" / SERVER_BINARY
    if release.exists():
        return "release"
    if debug.exists():
        return "debug"
    return "none"


def get_binary_size():
    """Get server binary size."""
    mode = get_build_mode()
    if mode == "none":
        return "N/A"
    path = PROJECT_ROOT / "target" / mode / SERVER_BINARY
    if path.exists():
        size = path.stat().st_size
        for unit in ("B", "KB", "MB"):
            if size < 1024:
                return f"{size:.1f} {unit}"
            size /= 1024
        return f"{size:.1f} GB"
    return "N/A"


def admin_dist_exists():
    """Check if admin dist is built."""
    dist_index = ADMIN_DIR / "dist" / "index.html"
    return dist_index.exists()


def has_admin_binary():
    """Check if admin npm dependencies are installed."""
    return (ADMIN_DIR / "node_modules" / ".package-lock.json").exists() or (
        ADMIN_DIR / "node_modules" / ".modules.yaml"
    ).exists()


def get_env_config():
    """Read TJXY config from environment or .env file."""
    config = {}
    # Read from .env file
    if ENV_FILE.exists():
        for line in ENV_FILE.read_text().splitlines():
            line = line.strip()
            if line and not line.startswith("#") and "=" in line:
                k, v = line.split("=", 1)
                config[k.strip()] = v.strip().strip("\"'")
    # Also read from live environment
    env_prefix = "TJXY_"
    for k, v in os.environ.items():
        if k.startswith(env_prefix):
            config[k] = v
    return config


def format_env_summary(config):
    """Format environment config for display, masking secrets."""
    lines = []
    for k in sorted(config.keys()):
        v = config[k]
        if any(secret in k.lower() for secret in ("password", "secret", "key", "token", "credential")):
            v = "****" if v else "(empty)"
        elif len(v) > 60:
            v = v[:57] + "..."
        lines.append((k, v))
    return lines


# ─── Service Actions ──────────────────────────────────────────────────

def start_server(stdscr=None):
    """Start the TJXY server."""
    pid = find_server_pid()
    if pid:
        return False, f"Server already running (PID {pid})"

    mode = get_build_mode()
    if mode == "none":
        return False, "Server binary not found. Build first (option 4)."

    log_dir = LOG_DIR
    log_dir.mkdir(parents=True, exist_ok=True)

    # Build env from .env if present
    env = os.environ.copy()
    if ENV_FILE.exists():
        for line in ENV_FILE.read_text().splitlines():
            line = line.strip()
            if line and not line.startswith("#") and "=" in line:
                k, v = line.split("=", 1)
                env[k.strip()] = v.strip().strip("\"'")

    cmd = [str(PROJECT_ROOT / "target" / mode / SERVER_BINARY)]
    log_fh = open(LOG_FILE, "a")
    log_fh.write(f"\n--- Server started at {datetime.now().isoformat()} ---\n")
    log_fh.flush()

    try:
        proc = subprocess.Popen(
            cmd,
            cwd=str(PROJECT_ROOT),
            env=env,
            stdout=log_fh,
            stderr=subprocess.STDOUT,
            preexec_fn=os.setsid if sys.platform != "darwin" else None,
        )
        PID_FILE.write_text(str(proc.pid))
        return True, f"Server started (PID {proc.pid})"
    except Exception as e:
        return False, f"Failed to start: {e}"


def stop_server():
    """Stop the TJXY server."""
    pid = find_server_pid()
    if not pid:
        return False, "Server is not running"

    try:
        os.kill(pid, signal.SIGTERM)
        # Wait for graceful shutdown
        for _ in range(30):
            if not pid_alive(pid):
                break
            time.sleep(0.5)
        else:
            os.kill(pid, signal.SIGKILL)
            time.sleep(0.5)
        if PID_FILE.exists():
            PID_FILE.unlink()
        return True, f"Server stopped (PID {pid})"
    except Exception as e:
        return False, f"Failed to stop: {e}"


def restart_server(stdscr=None):
    """Restart the TJXY server."""
    stopped, msg = stop_server()
    time.sleep(1)
    started, msg2 = start_server(stdscr)
    return started, f"Restart: {msg} | {msg2}"


def build_server(mode="debug"):
    """Build the Rust server."""
    profile = "" if mode == "debug" else "--release"
    cmd = f"cargo build {profile} -p tjxy-server --bin tjxy-server"
    rc, out, err = run(cmd.split(), cwd=str(PROJECT_ROOT), timeout=300)
    return rc == 0, out if rc == 0 else err


def build_admin():
    """Build the admin frontend."""
    rc, out, err = run(
        ["npm", "--prefix", str(ADMIN_DIR), "run", "build"],
        timeout=120,
    )
    return rc == 0, out if rc == 0 else err


def install_deps():
    """Install all project dependencies."""
    results = []
    # Rust check
    rc, _, _ = run(["cargo", "--version"])
    results.append(("Rust toolchain", "available" if rc == 0 else "MISSING"))

    # Admin npm install
    rc, out, err = run(
        ["npm", "--prefix", str(ADMIN_DIR), "ci"],
        timeout=120,
    )
    results.append(("Admin deps (npm ci)", "OK" if rc == 0 else f"FAILED: {err[:80]}"))

    return results


# ─── TUI ──────────────────────────────────────────────────────────────

MENU_ITEMS = [
    ("Dashboard", "D"),
    ("Server", "S"),
    ("Admin", "A"),
    ("Build", "B"),
    ("Database", "T"),
    ("Install", "I"),
    ("Logs", "L"),
    ("Config", "C"),
    ("Quit", "Q"),
]

MENU_LABELS = [item[0] for item in MENU_ITEMS]


def init_colors():
    """Initialize curses color pairs."""
    curses.start_color()
    curses.use_default_colors()
    #             pair   fg           bg
    curses.init_pair(C_TITLE, curses.COLOR_WHITE, curses.COLOR_BLUE)
    curses.init_pair(C_MENU, curses.COLOR_WHITE, -1)
    curses.init_pair(C_MENU_ACTIVE, curses.COLOR_WHITE, curses.COLOR_BLUE)
    curses.init_pair(C_STATUS_OK, curses.COLOR_GREEN, -1)
    curses.init_pair(C_STATUS_WARN, curses.COLOR_YELLOW, -1)
    curses.init_pair(C_STATUS_ERR, curses.COLOR_RED, -1)
    curses.init_pair(C_HEADER, curses.COLOR_WHITE, -1)
    curses.init_pair(C_BORDER, curses.COLOR_BLUE, -1)
    curses.init_pair(C_LABEL, curses.COLOR_CYAN, -1)
    curses.init_pair(C_VALUE, curses.COLOR_WHITE, -1)
    curses.init_pair(C_HIGHLIGHT, curses.COLOR_BLACK, curses.COLOR_CYAN)
    curses.init_pair(C_DIM, curses.COLOR_DARK_GRAY if hasattr(curses, "COLOR_DARK_GRAY") else 8, -1)
    curses.init_pair(C_ACTIVE_BG, curses.COLOR_BLACK, curses.COLOR_WHITE)


def draw_header(stdscr, title, width):
    """Draw a colored header bar."""
    stdscr.attron(curses.color_pair(C_TITLE) | curses.A_BOLD)
    bar = " " + title + " " * (width - len(title) - 2)
    stdscr.addstr(0, 0, bar[:width])
    stdscr.attroff(curses.color_pair(C_TITLE) | curses.A_BOLD)


def draw_menu_bar(stdscr, active_idx, width):
    """Draw horizontal menu bar."""
    x = 1
    y = 1
    stdscr.attron(curses.color_pair(C_DIM))
    stdscr.addstr(y, 0, "─" * width)
    stdscr.attroff(curses.color_pair(C_DIM))

    for i, (label, key) in enumerate(MENU_ITEMS):
        if i == active_idx:
            stdscr.attron(curses.color_pair(C_MENU_ACTIVE) | curses.A_BOLD)
        else:
            stdscr.attron(curses.color_pair(C_MENU))

        text = f" {label} "
        if x + len(text) >= width - 2:
            break
        stdscr.addstr(y, x, text)
        stdscr.attroff(curses.color_pair(C_MENU_ACTIVE) | curses.A_BOLD)
        stdscr.attroff(curses.color_pair(C_MENU))
        x += len(text) + 1


def draw_status_bar(stdscr, width, message=""):
    """Draw status bar at bottom."""
    h, _ = stdscr.getmaxyx()
    y = h - 1
    stdscr.attron(curses.color_pair(C_DIM))
    stdscr.addstr(y, 0, "─" * width)
    stdscr.attroff(curses.color_pair(C_DIM))

    ts = datetime.now().strftime("%H:%M:%S")
    if message:
        text = f" {message} "
    else:
        text = f" [{ts}] TJXY Management Console | ↑↓ Navigate | Enter Select | Q Quit "
    stdscr.attron(curses.A_REVERSE if message else curses.A_DIM)
    stdscr.addstr(y, 0, text[:width])
    stdscr.attroff(curses.A_REVERSE if message else curses.A_DIM)


def draw_border_box(stdscr, y, x, h, w, title=""):
    """Draw a bordered box with optional title."""
    if h < 2 or w < 4:
        return
    stdscr.attron(curses.color_pair(C_BORDER))
    stdscr.addstr(y, x, "┌" + "─" * (w - 2) + "┐")
    for row in range(1, h - 1):
        if y + row < stdscr.getmaxyx()[0] - 1:
            stdscr.addstr(y + row, x, "│")
            stdscr.addstr(y + row, x + w - 1, "│")
    stdscr.addstr(y + h - 1, x, "└" + "─" * (w - 2) + "┘")
    stdscr.attroff(curses.color_pair(C_BORDER))
    if title:
        stdscr.attron(curses.A_BOLD)
        stdscr.addstr(y, x + 2, f" {title} ")
        stdscr.attroff(curses.A_BOLD)


def draw_label_value(stdscr, y, x, label, value, max_w=None, label_color=C_LABEL, value_color=C_VALUE):
    """Draw a label: value pair."""
    stdscr.attron(curses.color_pair(label_color))
    stdscr.addstr(y, x, label)
    stdscr.attroff(curses.color_pair(label_color))
    stdscr.attron(curses.color_pair(value_color) | curses.A_BOLD)
    remaining = max_w - len(label) - 1 if max_w else None
    if remaining and len(value) > remaining:
        value = value[:remaining - 3] + "..."
    stdscr.addstr(y, x + len(label) + 1, value)
    stdscr.attroff(curses.color_pair(value_color) | curses.A_BOLD)


def status_tag(status_str):
    """Return (colored_status, color_pair)."""
    s = status_str.lower()
    if s in ("running", "ok", "yes", "available", "connected"):
        return ("● RUNNING", C_STATUS_OK)
    elif s in ("stopped", "no", "missing", "disconnected", "none"):
        return ("○ STOPPED", C_STATUS_ERR)
    elif s in ("warning", "degraded"):
        return ("◐ WARN", C_STATUS_WARN)
    else:
        return (f"? {status_str}", C_DIM)


# ─── View: Dashboard ──────────────────────────────────────────────────

def render_dashboard(stdscr, y_start, x_start, max_y, max_w, status_msg):
    """Render the main dashboard view."""
    pid = find_server_pid()
    server_running = pid is not None
    port_open = check_port()
    admin_dev = find_admin_dev_pid()
    admin_dev_running = admin_dev is not None
    admin_dev_port = check_admin_port()
    db_exists = DB_PATH.exists()
    build_mode = get_build_mode()
    binary_size = get_binary_size()
    rust_ver = get_rust_version()
    node_ver = get_node_version()
    npm_ver = get_npm_version()
    dist_ok = admin_dist_exists()
    admin_deps = has_admin_binary()

    y = y_start

    # ── Service Status Box ──
    box_h = 6
    draw_border_box(stdscr, y, x_start, box_h, max_w, "Service Status")
    y += 1
    srv_tag, srv_color = status_tag("running" if server_running else "stopped")
    stdscr.attron(curses.color_pair(srv_color) | curses.A_BOLD)
    stdscr.addstr(y, x_start + 2, srv_tag)
    stdscr.attroff(curses.color_pair(srv_color) | curses.A_BOLD)

    uptime = get_server_uptime(pid) if server_running else "-"
    stdscr.addstr(y, x_start + 16, f"PID: {pid if server_running else '-'}  |  Port: {'8096 (open)' if port_open else '8096 (closed)'}  |  Uptime: {uptime}")
    y += 1

    adm_tag, adm_color = status_tag("running" if admin_dev_running else "stopped")
    stdscr.attron(curses.color_pair(adm_color) | curses.A_BOLD)
    stdscr.addstr(y, x_start + 2, adm_tag)
    stdscr.attroff(curses.color_pair(adm_color) | curses.A_BOLD)
    stdscr.addstr(y, x_start + 16, f"Admin Dev Server  |  Port: {'5173 (open)' if admin_dev_port else '5173 (closed)'}  |  PID: {admin_dev if admin_dev_running else '-'}")
    y += 1

    db_tag, db_color = status_tag("ok" if db_exists else "missing")
    stdscr.attron(curses.color_pair(db_color) | curses.A_BOLD)
    stdscr.addstr(y, x_start + 2, db_tag)
    stdscr.attroff(curses.color_pair(db_color) | curses.A_BOLD)
    stdscr.addstr(y, x_start + 16, f"Database: {DB_PATH.name}  |  Size: {get_db_size()}")
    y += 1

    dist_tag, dist_color = status_tag("ok" if dist_ok else "missing")
    stdscr.attron(curses.color_pair(dist_color) | curses.A_BOLD)
    stdscr.addstr(y, x_start + 2, dist_tag)
    stdscr.attroff(curses.color_pair(dist_color) | curses.A_BOLD)
    stdscr.addstr(y, x_start + 16, f"Admin Dist: {'built' if dist_ok else 'not built'}")
    y += 1
    y += 1  # box bottom

    # ── Build Info Box ──
    box_h = 5
    draw_border_box(stdscr, y, x_start, box_h, max_w, "Build Info")
    y += 1
    draw_label_value(stdscr, y, x_start + 2, "Rust:", rust_ver, max_w - 10)
    y += 1
    draw_label_value(stdscr, y, x_start + 2, "Node:", f"{node_ver} / npm {npm_ver}", max_w - 10)
    y += 1
    draw_label_value(stdscr, y, x_start + 2, "Binary:", f"{build_mode} ({binary_size})", max_w - 10)
    y += 1
    deps_tag, deps_color = status_tag("ok" if admin_deps else "missing")
    stdscr.attron(curses.color_pair(deps_color) | curses.A_BOLD)
    stdscr.addstr(y, x_start + 2, deps_tag)
    stdscr.attroff(curses.color_pair(deps_color) | curses.A_BOLD)
    stdscr.addstr(y, x_start + 16, "Admin deps installed")
    y += 1
    y += 1

    # ── Quick Actions ──
    box_h = 4
    draw_border_box(stdscr, y, x_start, box_h, max_w, "Quick Actions")
    y += 1
    actions = [
        ("1", "Start Server", "2", "Stop Server", "3", "Restart Server"),
        ("4", "Build Server", "5", "Build Admin", "6", "Open Dashboard"),
    ]
    for row_actions in actions:
        col = x_start + 2
        for i in range(0, 6, 2):
            key, label = row_actions[i], row_actions[i + 1]
            stdscr.attron(curses.color_pair(C_HIGHLIGHT))
            stdscr.addstr(y, col, f" [{key}] ")
            stdscr.attroff(curses.color_pair(C_HIGHLIGHT))
            stdscr.addstr(f" {label}  ")
            col += 20
        y += 1

    return y


# ─── View: Server Management ──────────────────────────────────────────

def render_server(stdscr, y_start, x_start, max_y, max_w, status_msg):
    """Render server management view."""
    pid = find_server_pid()
    server_running = pid is not None
    port_open = check_port()

    y = y_start

    draw_border_box(stdscr, y, x_start, 8, max_w, "Server Control")
    y += 1

    srv_tag, srv_color = status_tag("running" if server_running else "stopped")
    stdscr.attron(curses.color_pair(srv_color) | curses.A_BOLD)
    stdscr.addstr(y, x_start + 2, srv_tag)
    stdscr.attroff(curses.color_pair(srv_color) | curses.A_BOLD)
    stdscr.addstr(y, x_start + 16, f"PID: {pid if server_running else '-'}  |  Port: {'8096 (open)' if port_open else '8096 (closed)'}")
    y += 1

    if server_running:
        uptime = get_server_uptime(pid)
        stdscr.addstr(y, x_start + 2, f"Uptime: {uptime}")
        # Memory info
        rc, out, _ = run(["ps", "-o", "rss=", "-p", str(pid)], timeout=5)
        if rc == 0 and out:
            try:
                rss_kb = int(out.strip())
                rss_mb = rss_kb / 1024
                stdscr.addstr(y, x_start + 30, f"Memory: {rss_mb:.0f} MB")
            except ValueError:
                pass
        y += 1
        # CPU info
        rc, out, _ = run(["ps", "-o", "%cpu=", "-p", str(pid)], timeout=5)
        if rc == 0 and out:
            stdscr.addstr(y, x_start + 2, f"CPU: {out.strip()}%")
        y += 1
    y += 1

    # Action buttons
    actions = [
        ("[1] Start Server", start_server),
        ("[2] Stop Server", stop_server),
        ("[3] Restart Server", restart_server),
    ]
    if not server_running:
        actions[0] = (actions[0][0], actions[0][1])
        actions[1] = ("[2] Stop Server (N/A)", None)
    else:
        actions[0] = ("[1] Start Server (N/A)", None)
        actions[1] = (actions[1][0], actions[1][1])

    for i, (label, action) in enumerate(actions):
        if action:
            stdscr.attron(curses.color_pair(C_HIGHLIGHT))
            stdscr.addstr(y, x_start + 2, label)
            stdscr.attroff(curses.color_pair(C_HIGHLIGHT))
        else:
            stdscr.attron(curses.color_pair(C_DIM))
            stdscr.addstr(y, x_start + 2, label)
            stdscr.attroff(curses.color_pair(C_DIM))
        y += 1
    y += 1
    y += 1  # box bottom

    # ── Server Config Box ──
    box_h = 6
    draw_border_box(stdscr, y, x_start, box_h, max_w, "Server Configuration")
    y += 1
    config = get_env_config()
    key_items = ["TJXY_SERVER_ID", "TJXY_SERVER_NAME", "TJXY_BIND", "TJXY_DATABASE_URL", "TJXY_ASSETS_DIR"]
    for key in key_items:
        val = config.get(key, "(not set)")
        if key == "TJXY_DATABASE_URL":
            # Mask path
            val = val.split("?")[0] if "?" in val else val
        draw_label_value(stdscr, y, x_start + 2, f"{key}:", val, max_w - 10, value_color=C_DIM)
        y += 1

    return y


# ─── View: Admin ──────────────────────────────────────────────────────

def render_admin(stdscr, y_start, x_start, max_y, max_w, status_msg):
    """Render admin frontend management view."""
    admin_dev = find_admin_dev_pid()
    admin_dev_running = admin_dev is not None
    admin_dev_port = check_admin_port()
    dist_ok = admin_dist_exists()
    deps_ok = has_admin_binary()

    y = y_start

    draw_border_box(stdscr, y, x_start, 7, max_w, "Admin Frontend")
    y += 1

    dev_tag, dev_color = status_tag("running" if admin_dev_running else "stopped")
    stdscr.attron(curses.color_pair(dev_color) | curses.A_BOLD)
    stdscr.addstr(y, x_start + 2, dev_tag)
    stdscr.attroff(curses.color_pair(dev_color) | curses.A_BOLD)
    stdscr.addstr(y, x_start + 16, f"Dev Server  |  Port: {'5173 (open)' if admin_dev_port else '5173 (closed)'}  |  PID: {admin_dev if admin_dev_running else '-'}")
    y += 1

    dist_tag, dist_color = status_tag("ok" if dist_ok else "missing")
    stdscr.attron(curses.color_pair(dist_color) | curses.A_BOLD)
    stdscr.addstr(y, x_start + 2, dist_tag)
    stdscr.attroff(curses.color_pair(dist_color) | curses.A_BOLD)
    stdscr.addstr(y, x_start + 16, f"Production Build  |  {('admin/dist/index.html' if dist_ok else 'not built')}")
    y += 1

    deps_tag, deps_color = status_tag("ok" if deps_ok else "missing")
    stdscr.attron(curses.color_pair(deps_color) | curses.A_BOLD)
    stdscr.addstr(y, x_start + 2, deps_tag)
    stdscr.attroff(curses.color_pair(deps_color) | curses.A_BOLD)
    stdscr.addstr(y, x_start + 16, "Dependencies installed")
    y += 1
    y += 1

    # Actions
    actions = [
        ("[1]  Start Dev Server", "npm --prefix admin run dev"),
        ("[2]  Stop Dev Server", "pkill -f 'vite'"),
        ("[3]  Build Production", "npm --prefix admin run build"),
        ("[4]  Install Dependencies", "npm --prefix admin ci"),
        ("[5]  Type Check", "npm --prefix admin run typecheck"),
        ("[6]  Run Tests", "npm --prefix admin run test -- --run"),
    ]
    for i, (label, _) in enumerate(actions):
        stdscr.attron(curses.color_pair(C_HIGHLIGHT))
        stdscr.addstr(y, x_start + 2, label)
        stdscr.attroff(curses.color_pair(C_HIGHLIGHT))
        y += 1

    return y


# ─── View: Build ──────────────────────────────────────────────────────

def render_build(stdscr, y_start, x_start, max_y, max_w, status_msg):
    """Render build management view."""
    y = y_start

    draw_border_box(stdscr, y, x_start, 5, max_w, "Build Actions")
    y += 1

    actions = [
        ("[1]  Build Server (debug)", "cargo build -p tjxy-server --bin tjxy-server"),
        ("[2]  Build Server (release)", "cargo build --release -p tjxy-server --bin tjxy-server"),
        ("[3]  Build Admin Frontend", "npm --prefix admin run build"),
        ("[4]  Build All (server + admin)", "full build"),
        ("[5]  Clean Build Artifacts", "cargo clean"),
    ]
    for i, (label, _) in enumerate(actions):
        stdscr.attron(curses.color_pair(C_HIGHLIGHT))
        stdscr.addstr(y, x_start + 2, label)
        stdscr.attroff(curses.color_pair(C_HIGHLIGHT))
        y += 1
    y += 1
    y += 1  # box bottom

    # Build status
    box_h = 5
    draw_border_box(stdscr, y, x_start, box_h, max_w, "Current Build Artifacts")
    y += 1
    mode = get_build_mode()
    bsize = get_binary_size()
    draw_label_value(stdscr, y, x_start + 2, "Server binary:", f"{mode} ({bsize})", max_w - 10)
    y += 1
    dist_ok = admin_dist_exists()
    draw_label_value(stdscr, y, x_start + 2, "Admin dist:", f"{'built' if dist_ok else 'not built'}", max_w - 10)
    y += 1
    # Check if admin dist is stale vs source
    if dist_ok:
        dist_time = (ADMIN_DIR / "dist" / "index.html").stat().st_mtime
        src_time = (ADMIN_DIR / "src").stat().st_mtime
        if dist_time < src_time:
            stdscr.attron(curses.color_pair(C_STATUS_WARN))
            stdscr.addstr(y, x_start + 2, "⚠ Admin dist may be stale (source newer than build)")
            stdscr.attroff(curses.color_pair(C_STATUS_WARN))

    return y


# ─── View: Database ───────────────────────────────────────────────────

def render_database(stdscr, y_start, x_start, max_y, max_w, status_msg):
    """Render database management view."""
    y = y_start

    db_exists = DB_PATH.exists()
    db_size = get_db_size()

    draw_border_box(stdscr, y, x_start, 5, max_w, "Database Status")
    y += 1
    db_tag, db_color = status_tag("ok" if db_exists else "missing")
    stdscr.attron(curses.color_pair(db_color) | curses.A_BOLD)
    stdscr.addstr(y, x_start + 2, db_tag)
    stdscr.attroff(curses.color_pair(db_color) | curses.A_BOLD)
    stdscr.addstr(y, x_start + 16, f"Path: {DB_PATH}")
    y += 1
    draw_label_value(stdscr, y, x_start + 2, "Size:", db_size, max_w - 10)
    y += 1
    if db_exists:
        mod_time = datetime.fromtimestamp(DB_PATH.stat().st_mtime).strftime("%Y-%m-%d %H:%M:%S")
        draw_label_value(stdscr, y, x_start + 2, "Last modified:", mod_time, max_w - 10)
    y += 1
    y += 1  # box bottom

    # Actions
    draw_border_box(stdscr, y, x_start, 5, max_w, "Database Actions")
    y += 1
    actions = [
        ("[1]  Backup Database", "cp tjxy.db tjxy.db.bak"),
        ("[2]  Show Table Stats", "sqlite3 tjxy.db \".tables\""),
        ("[3]  Database Integrity Check", "sqlite3 tjxy.db \"PRAGMA integrity_check;\""),
        ("[4]  Vacuum (Compact)", "sqlite3 tjxy.db \"VACUUM;\""),
    ]
    for i, (label, _) in enumerate(actions):
        stdscr.attron(curses.color_pair(C_HIGHLIGHT))
        stdscr.addstr(y, x_start + 2, label)
        stdscr.attroff(curses.color_pair(C_HIGHLIGHT))
        y += 1

    return y


# ─── View: Install ────────────────────────────────────────────────────

def render_install(stdscr, y_start, x_start, max_y, max_w, status_msg):
    """Render install/dependency management view."""
    y = y_start

    # Check dependencies
    deps = [
        ("Rust", "cargo", get_rust_version()),
        ("Node.js", "node", get_node_version()),
        ("npm", "npm", get_npm_version()),
        ("Admin deps", "npm ci", "installed" if has_admin_binary() else "not installed"),
        ("Server binary", "cargo build", get_build_mode()),
        ("Admin dist", "npm run build", "built" if admin_dist_exists() else "not built"),
    ]

    draw_border_box(stdscr, y, x_start, len(deps) + 2, max_w, "Dependency Status")
    y += 1
    for name, check, version in deps:
        ok = version not in ("N/A", "not installed", "not built", "none")
        tag, color = status_tag("ok" if ok else "missing")
        stdscr.attron(curses.color_pair(color) | curses.A_BOLD)
        stdscr.addstr(y, x_start + 2, tag)
        stdscr.attroff(curses.color_pair(color) | curses.A_BOLD)
        stdscr.addstr(y, x_start + 16, f"{name}: {version}")
        y += 1
    y += 1  # box bottom

    draw_border_box(stdscr, y, x_start, 6, max_w, "Install / Setup Actions")
    y += 1
    actions = [
        ("[1]  Install All Dependencies", "npm ci + cargo check"),
        ("[2]  Install Admin Dependencies", "npm --prefix admin ci"),
        ("[3]  Verify Rust Toolchain", "rustc --version && cargo --version"),
        ("[4]  Full Setup (deps + build)", "install all + build all"),
        ("[5]  Create .env Template", "create default .env file"),
    ]
    for i, (label, _) in enumerate(actions):
        stdscr.attron(curses.color_pair(C_HIGHLIGHT))
        stdscr.addstr(y, x_start + 2, label)
        stdscr.attroff(curses.color_pair(C_HIGHLIGHT))
        y += 1

    return y


# ─── View: Logs ───────────────────────────────────────────────────────

def render_logs(stdscr, y_start, x_start, max_y, max_w, status_msg):
    """Render log viewer."""
    y = y_start
    box_h = 3
    draw_border_box(stdscr, y, x_start, box_h, max_w, "Server Logs")
    y += 1

    actions = [
        ("[1]  Tail Server Logs (live)", "tail -f"),
        ("[2]  View Last 50 Lines", "tail -50"),
        ("[3]  View Last 200 Lines", "tail -200"),
        ("[4]  Clear Log File", "truncate"),
    ]
    for i, (label, _) in enumerate(actions):
        stdscr.attron(curses.color_pair(C_HIGHLIGHT))
        stdscr.addstr(y, x_start + 2, label)
        stdscr.attroff(curses.color_pair(C_HIGHLIGHT))
        y += 1
    y += 1  # box bottom

    # Show recent log content
    log_content = ""
    if LOG_FILE.exists():
        try:
            rc, out, _ = run(["tail", "-30", str(LOG_FILE)], timeout=5)
            if rc == 0:
                log_content = out
        except Exception:
            log_content = "(error reading log)"
    else:
        log_content = "(no log file found)"

    log_lines = log_content.split("\n")
    view_h = max_y - y - 2
    log_lines = log_lines[-view_h:]

    draw_border_box(stdscr, y, x_start, len(log_lines) + 2, max_w, "Recent Log Output")
    y += 1
    for line in log_lines:
        line = line[:max_w - 4]
        if "error" in line.lower() or "fail" in line.lower():
            stdscr.attron(curses.color_pair(C_STATUS_ERR))
        elif "warn" in line.lower():
            stdscr.attron(curses.color_pair(C_STATUS_WARN))
        else:
            stdscr.attron(curses.color_pair(C_DIM))
        stdscr.addstr(y, x_start + 2, line)
        stdscr.attroff(curses.color_pair(C_STATUS_ERR) | curses.COLOR_RED)
        stdscr.attroff(curses.color_pair(C_STATUS_WARN))
        stdscr.attroff(curses.color_pair(C_DIM))
        y += 1
        if y >= max_y - 1:
            break

    return y


# ─── View: Config ─────────────────────────────────────────────────────

def render_config(stdscr, y_start, x_start, max_y, max_w, status_msg):
    """Render configuration view."""
    y = y_start
    config = get_env_config()

    box_h = 4
    draw_border_box(stdscr, y, x_start, box_h, max_w, "Environment Configuration")
    y += 1
    stdscr.addstr(y, x_start + 2, f"Source: {' .env file + environment' if ENV_FILE.exists() else ' environment only'}")
    y += 1
    stdscr.addstr(y, x_start + 2, f"Total TJXY_* variables: {len(config)}")
    y += 1
    stdscr.addstr(y, x_start + 2, "Values ending in PASSWORD/SECRET/KEY/TOKEN are masked")
    y += 1
    y += 1  # box bottom

    # Config listing
    entries = format_env_summary(config)
    view_h = max_y - y - 2
    entries = entries[:view_h]

    draw_border_box(stdscr, y, x_start, len(entries) + 2, max_w, "Configuration Values")
    y += 1
    for k, v in entries:
        draw_label_value(stdscr, y, x_start + 2, k + ":", v, max_w - 10, value_color=C_DIM)
        y += 1

    return y


# ─── Action Handling ──────────────────────────────────────────────────

def run_action(stdscr, view, key, status_msg):
    """Execute an action based on current view and keypress."""
    results = []
    if view == 0:  # Dashboard
        if key == ord("1"):
            ok, msg = start_server(stdscr)
            return msg
        elif key == ord("2"):
            ok, msg = stop_server()
            return msg
        elif key == ord("3"):
            ok, msg = restart_server(stdscr)
            return msg
        elif key == ord("4"):
            return run_build_task(stdscr, "debug")
        elif key == ord("5"):
            ok, msg = build_admin()
            return "Build admin: " + ("OK" if ok else msg)
    elif view == 1:  # Server
        if key == ord("1"):
            ok, msg = start_server(stdscr)
            return msg
        elif key == ord("2"):
            ok, msg = stop_server()
            return msg
        elif key == ord("3"):
            ok, msg = restart_server(stdscr)
            return msg
    elif view == 2:  # Admin
        if key == ord("1"):
            return run_admin_dev(stdscr)
        elif key == ord("2"):
            run(["pkill", "-f", "vite"], timeout=5)
            return "Admin dev server stopped"
        elif key == ord("3"):
            status_msg[0] = "Building admin frontend..."
            ok, msg = build_admin()
            return "Build admin: " + ("OK" if ok else msg)
        elif key == ord("4"):
            status_msg[0] = "Installing admin dependencies..."
            rc, out, err = run(["npm", "--prefix", str(ADMIN_DIR), "ci"], timeout=120)
            return "npm ci: " + ("OK" if rc == 0 else err[:100])
        elif key == ord("5"):
            status_msg[0] = "Running type check..."
            rc, out, err = run(["npm", "--prefix", str(ADMIN_DIR), "run", "typecheck"], timeout=60)
            return "TypeCheck: " + ("OK" if rc == 0 else err[:200])
        elif key == ord("6"):
            status_msg[0] = "Running tests..."
            rc, out, err = run(["npm", "--prefix", str(ADMIN_DIR), "run", "test", "--", "--run"], timeout=120)
            return "Tests: " + ("OK" if rc == 0 else err[:200])
    elif view == 3:  # Build
        if key == ord("1"):
            return run_build_task(stdscr, "debug")
        elif key == ord("2"):
            return run_build_task(stdscr, "release")
        elif key == ord("3"):
            status_msg[0] = "Building admin frontend..."
            ok, msg = build_admin()
            return "Build admin: " + ("OK" if ok else msg)
        elif key == ord("4"):
            return run_build_all(stdscr)
        elif key == ord("5"):
            status_msg[0] = "Cleaning build artifacts..."
            rc, out, err = run(["cargo", "clean"], cwd=str(PROJECT_ROOT), timeout=60)
            return "Clean: " + ("OK" if rc == 0 else err[:100])
    elif view == 4:  # Database
        if key == ord("1"):
            status_msg[0] = "Backing up database..."
            rc, out, err = run(["cp", str(DB_PATH), str(DB_PATH) + ".bak"], timeout=10)
            return "Backup: " + ("OK" if rc == 0 else err[:100])
        elif key == ord("2"):
            rc, out, err = run(["sqlite3", str(DB_PATH), ".tables"], timeout=10)
            if rc == 0:
                tables = out.replace("\n", ", ")
                return f"Tables: {tables}"
            return f"Error: {err[:100]}"
        elif key == ord("3"):
            status_msg[0] = "Running integrity check..."
            rc, out, err = run(["sqlite3", str(DB_PATH), "PRAGMA integrity_check;"], timeout=30)
            return f"Integrity: {out[:200]}"
        elif key == ord("4"):
            status_msg[0] = "Running vacuum..."
            rc, out, err = run(["sqlite3", str(DB_PATH), "VACUUM;"], timeout=120)
            return "Vacuum: " + ("OK" if rc == 0 else err[:100])
    elif view == 5:  # Install
        if key == ord("1"):
            status_msg[0] = "Installing all dependencies..."
            results = install_deps()
            return "; ".join(f"{k}: {v}" for k, v in results)
        elif key == ord("2"):
            status_msg[0] = "Installing admin deps..."
            rc, out, err = run(["npm", "--prefix", str(ADMIN_DIR), "ci"], timeout=120)
            return "Admin deps: " + ("OK" if rc == 0 else err[:100])
        elif key == ord("3"):
            return f"Rust: {get_rust_version()}"
        elif key == ord("4"):
            return run_full_setup(stdscr)
        elif key == ord("5"):
            return create_env_template()
    elif view == 6:  # Logs
        if key == ord("1"):
            return run_tail_logs(stdscr)
        elif key == ord("2"):
            rc, out, _ = run(["tail", "-50", str(LOG_FILE)], timeout=5)
            return out[-2000:] if out else "(no log content)"
        elif key == ord("3"):
            rc, out, _ = run(["tail", "-200", str(LOG_FILE)], timeout=5)
            return out[-2000:] if out else "(no log content)"
        elif key == ord("4"):
            if LOG_FILE.exists():
                LOG_FILE.write_text("")
                return "Log file cleared"
            return "(no log file)"

    return None


def run_build_task(stdscr, mode):
    """Run a build with progress display."""
    msg = ["Building..."]
    result = [None]

    def build_thread():
        ok, out = build_server(mode)
        result[0] = (ok, out)

    thread = threading.Thread(target=build_thread, daemon=True)
    thread.start()

    spinner = "|/-\\"
    i = 0
    while thread.is_alive():
        if stdscr:
            h, w = stdscr.getmaxyx()
            stdscr.attron(curses.color_pair(C_STATUS_WARN))
            stdscr.addstr(h - 2, 2, f"  Building server ({mode})... {spinner[i % 4]}  ")
            stdscr.attroff(curses.color_pair(C_STATUS_WARN))
            stdscr.refresh()
        i += 1
        time.sleep(0.1)

    ok, out = result[0]
    if ok:
        return f"Build ({mode}): OK"
    else:
        return f"Build failed: {out[:200]}"


def run_build_all(stdscr):
    """Build both server and admin."""
    status_msg = ["Building server (debug)..."]
    ok, msg = build_server("debug")
    if not ok:
        return f"Server build failed: {msg[:100]}"
    status_msg[0] = "Building admin frontend..."
    ok, msg = build_admin()
    if not ok:
        return f"Admin build failed: {msg[:100]}"
    return "Full build: OK"


def run_full_setup(stdscr):
    """Run full setup: deps + build."""
    status_msg = ["Installing dependencies..."]
    results = install_deps()
    status_msg[0] = "Building server..."
    ok, msg = build_server("debug")
    if not ok:
        return f"Setup failed at server build: {msg[:100]}"
    status_msg[0] = "Building admin..."
    ok, msg = build_admin()
    if not ok:
        return f"Setup failed at admin build: {msg[:100]}"
    deps_str = "; ".join(f"{k}: {v}" for k, v in results)
    return f"Full setup complete. {deps_str}"


def run_admin_dev(stdscr):
    """Start the admin dev server in background."""
    msg = ["Starting admin dev server..."]
    result = [None]

    def dev_thread():
        env = os.environ.copy()
        env["TJXY_DEV_SERVER"] = "http://127.0.0.1:8096"
        try:
            proc = subprocess.Popen(
                ["npm", "--prefix", str(ADMIN_DIR), "run", "dev"],
                cwd=str(ADMIN_DIR),
                env=env,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
            time.sleep(3)
            if pid_alive(proc.pid):
                result[0] = f"Admin dev server started (PID {proc.pid})"
            else:
                result[0] = "Admin dev server failed to start"
        except Exception as e:
            result[0] = f"Error: {e}"

    thread = threading.Thread(target=dev_thread, daemon=True)
    thread.start()
    thread.join(timeout=10)
    return result[0] or "Timeout starting dev server"


def run_tail_logs(stdscr):
    """Tail logs in a separate terminal."""
    if not LOG_FILE.exists():
        return "(no log file to tail)"

    # Try to open in a new terminal window
    terminal_cmds = [
        ["open", "-a", "Terminal", str(LOG_FILE)],
        ["open", "-a", "iTerm", str(LOG_FILE)],
    ]
    for cmd in terminal_cmds:
        rc, _, _ = run(cmd, timeout=5)
        if rc == 0:
            return "Logs opened in Terminal"

    # Fallback: show last lines
    rc, out, _ = run(["tail", "-100", str(LOG_FILE)], timeout=5)
    return out[-2000:] if out else "(no content)"


def create_env_template():
    """Create a .env template file."""
    template = """# TJXY Server Configuration
# Copy this to .env and adjust values

# Required: Server identity
TJXY_SERVER_ID="00000000-0000-0000-0000-000000000001"
TJXY_SERVER_NAME="TJXY Media Server"
TJXY_BIND="127.0.0.1:8096"

# Database
TJXY_DATABASE_URL="sqlite://tjxy.db?mode=rwc"

# Admin credentials (first run only)
TJXY_BOOTSTRAP_ADMIN_USERNAME="Admin"
TJXY_BOOTSTRAP_ADMIN_PASSWORD="change-me"

# Asset storage
TJXY_ASSETS_DIR="./data/assets"

# Optional: Credential keyring (base64 32-byte keys)
# TJXY_CREDENTIAL_KEYRING='{"active_version":1,"keys":{"1":"..."}}'

# Optional: Redis cache
# TJXY_REDIS_MODE="auto"
# TJXY_REDIS_URL="redis://127.0.0.1:6379"

# Optional: TMDb metadata provider
# TJXY_ENABLE_REMOTE_PROVIDERS="true"
# TJXY_TMDB_ACCESS_TOKEN="your-tmdb-token"

# Optional: Google Drive OAuth
# TJXY_GOOGLE_OAUTH_CLIENT_ID="..."
# TJXY_GOOGLE_OAUTH_CLIENT_SECRET="..."
# TJXY_GOOGLE_OAUTH_REDIRECT_URI="http://localhost:8096/..."

# Optional: OneDrive OAuth
# TJXY_ONEDRIVE_OAUTH_CLIENT_ID="..."
# TJXY_ONEDRIVE_OAUTH_REDIRECT_URI="http://localhost:8096/..."
"""
    if not ENV_FILE.exists():
        ENV_FILE.write_text(template)
        return f"Created {ENV_FILE}"
    else:
        return f"{ENV_FILE} already exists (not overwritten)"


# ─── Main TUI Loop ────────────────────────────────────────────────────

RENDERERS = [
    render_dashboard,
    render_server,
    render_admin,
    render_build,
    render_database,
    render_install,
    render_logs,
    render_config,
]


def main(stdscr):
    """Main TUI loop."""
    curses.curs_set(0)  # Hide cursor
    stdscr.timeout(500)  # 500ms refresh for updates
    init_colors()

    current_view = 0
    status_message = [""]  # Mutable for threaded updates
    message_timer = 0

    while True:
        h, w = stdscr.getmaxyx()
        if h < 18 or w < 60:
            stdscr.clear()
            stdscr.addstr(0, 0, f"Terminal too small ({h}x{w}). Minimum: 18x60")
            stdscr.refresh()
            time.sleep(1)
            continue

        stdscr.clear()

        # ── Header ──
        draw_header(stdscr, " TJXY Management Console ", w)

        # ── Menu Bar ──
        draw_menu_bar(stdscr, current_view, w)

        # ── Content Area ──
        content_y = 3
        content_h = h - 5

        # Draw view-specific content
        if current_view < len(RENDERERS):
            RENDERERS[current_view](stdscr, content_y, 1, content_h, w - 2, status_message)

        # ── Status Bar ──
        if status_message[0] and message_timer > 0:
            draw_status_bar(stdscr, w, status_message[0])
            message_timer -= 1
        else:
            status_message[0] = ""
            draw_status_bar(stdscr, w)

        stdscr.refresh()

        # ── Input ──
        key = stdscr.getch()

        if key == ord("q") or key == ord("Q"):
            break

        # Navigation
        if key == curses.KEY_LEFT or key == ord("h"):
            current_view = (current_view - 1) % len(MENU_ITEMS)
        elif key == curses.KEY_RIGHT or key == ord("l"):
            current_view = (current_view + 1) % len(MENU_ITEMS)
        elif key == curses.KEY_UP or key == ord("k"):
            current_view = (current_view - 1) % (len(MENU_ITEMS) - 1)  # Exclude Quit
        elif key == curses.KEY_DOWN or key == ord("j"):
            current_view = (current_view + 1) % (len(MENU_ITEMS) - 1)
        elif key == curses.KEY_ENTER or key == 10 or key == 13:
            if current_view == len(MENU_ITEMS) - 1:  # Quit
                break
        elif key == ord("d") or key == ord("D"):
            current_view = 0
        elif key == ord("s") or key == ord("S"):
            current_view = 1
        elif key == ord("a") or key == ord("A"):
            current_view = 2
        elif key == ord("b") or key == ord("B"):
            current_view = 3
        elif key == ord("t") or key == ord("T"):
            current_view = 4
        elif key == ord("i") or key == ord("I"):
            current_view = 5
        elif key == ord("l") or key == ord("L"):
            current_view = 6
        elif key == ord("c") or key == ord("C"):
            current_view = 7

        # Numeric actions (1-9)
        if ord("1") <= key <= ord("9"):
            result = run_action(stdscr, current_view, key, status_message)
            if result:
                status_message[0] = result
                message_timer = 8  # Show for ~4 seconds

        # Resize
        if key == curses.KEY_RESIZE:
            stdscr.clear()


def main_wrapper():
    """Wrapper for curses with error handling."""
    try:
        curses.wrapper(main)
    except KeyboardInterrupt:
        pass
    except Exception as e:
        # Restore terminal
        curses.endwin()
        print(f"\nError: {e}", file=sys.stderr)
        import traceback
        traceback.print_exc()
        sys.exit(1)


if __name__ == "__main__":
    main_wrapper()