use tjxy_tui::{format_bytes, parse_env_lines, tail_lines};

#[test]
fn public_helpers_support_service_diagnostics() {
    assert_eq!(format_bytes(1_536), "1.5 KB");
    assert_eq!(tail_lines("one\ntwo\nthree\n", 2), vec!["two", "three"]);
}

#[test]
fn dotenv_parser_finds_diagnostic_overrides() {
    let values = parse_env_lines("TJXY_BIND=127.0.0.1:8096\nTJXY_LOG_FILE=logs/server.log\n");
    assert_eq!(
        values.get("TJXY_LOG_FILE"),
        Some(&"logs/server.log".to_owned())
    );
}
