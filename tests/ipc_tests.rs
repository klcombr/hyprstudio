//! IPC tests against a mock Unix socket, plus JSON parsing fixtures.

mod common;

use std::path::PathBuf;

use hyprstudio::core::models::Value;
use hyprstudio::hyprland::ipc::{HyprIpc, parse_option};
use hyprstudio::hyprland::version::Version;
use serde_json::Value as Json;

#[test]
fn requests_are_written_and_responses_read() {
    let mock = common::mock_ipc(|command| match command {
        "reload" => "ok".to_string(),
        "j/version" => r#"{"version":"0.57.2","branch":"v0.57.2","commit":"abc"}"#.to_string(),
        other => format!("unknown:{other}"),
    });

    mock.ipc.reload().expect("reload should succeed");
    let version = mock.ipc.version().expect("version should parse");
    assert_eq!(version.version, "0.57.2");
    assert_eq!(Version::parse(&version.version).unwrap().minor, 57);
}

#[test]
fn missing_socket_is_reported_as_not_running() {
    let ipc = HyprIpc::at(
        PathBuf::from("/nonexistent/hypr/.socket.sock"),
        PathBuf::from("/nonexistent/hypr/.socket2.sock"),
        "none",
        "/tmp",
    );
    let err = ipc.request("reload").unwrap_err();
    assert!(err.to_string().contains("not running") || err.to_string().contains("does not exist"));
}

#[test]
fn reload_failure_is_surfaced() {
    let mock = common::mock_ipc(|_| "err: unknown request".to_string());
    assert!(mock.ipc.reload().is_err());
}

#[test]
fn config_errors_ignores_success_marker() {
    let mock = common::mock_ipc(|command| match command {
        "configerrors" => "config ok\n".to_string(),
        _ => String::new(),
    });
    assert!(mock.ipc.config_errors().unwrap().is_empty());

    let mock = common::mock_ipc(|command| match command {
        "configerrors" => "error: invalid value in decoration:rounding\n".to_string(),
        _ => String::new(),
    });
    let errors = mock.ipc.config_errors().unwrap();
    assert_eq!(errors.len(), 1);
    assert!(errors[0].contains("invalid value"));
}

#[test]
fn getoption_is_parsed_into_typed_values() {
    let mock = common::mock_ipc(|command| match command {
        "j/getoption decoration:rounding" => {
            r#"{"option":"decoration:rounding","int":20,"set":true}"#.to_string()
        }
        "j/getoption general:col.active_border" => {
            r#"{"option":"general:col.active_border","gradient":"ffffffff cc000000 45deg","set":true}"#.to_string()
        }
        "j/getoption input:accel_profile" => r#"{"option":"input:accel_profile","str":"flat"}"#.to_string(),
        _ => "null".to_string(),
    });
    assert_eq!(
        mock.ipc.option("decoration:rounding").unwrap(),
        Some(Value::Int(20))
    );
    assert_eq!(
        mock.ipc.option("general:col.active_border").unwrap(),
        Some(Value::Text("rgba(ffffffff) rgba(cc000000) 45deg".into()))
    );
    assert_eq!(
        mock.ipc.option("input:accel_profile").unwrap(),
        Some(Value::Text("flat".into()))
    );
    assert_eq!(mock.ipc.option("missing:option").unwrap(), None);
}

#[test]
fn monitors_and_binds_parse_from_live_shaped_json() {
    let mock = common::mock_ipc(|command| match command {
        "j/monitors" => include_str!("fixtures/monitors.json").to_string(),
        "j/binds" => include_str!("fixtures/binds.json").to_string(),
        _ => "[]".to_string(),
    });
    let monitors = mock.ipc.monitors().unwrap();
    assert_eq!(monitors[0].name, "eDP-1");
    assert_eq!(monitors[0].current_mode(), "1920x1080@60");
    assert_eq!(monitors[0].transform_label(), "normal");
    let binds = mock.ipc.binds().unwrap();
    assert_eq!(binds.len(), 2);
    assert_eq!(binds[0].shortcut_label(), "SUPER Escape");
}

#[test]
fn animations_parse_curves_and_rules() {
    let mock = common::mock_ipc(|command| match command {
        "j/animations" => include_str!("fixtures/animations.json").to_string(),
        _ => "[]".to_string(),
    });
    let (curves, rules) = mock.ipc.animations().unwrap();
    assert_eq!(curves.len(), 2);
    assert_eq!(curves[0].hyprland_value(), "0.15 0 0.1 1");
    assert_eq!(rules[1].leaf, "workspaces");
    assert_eq!(rules[1].style, "slide");
}

#[test]
fn option_parser_handles_all_documented_shapes() {
    let cases: &[(&str, Value)] = &[
        (r#"{"int":3}"#, Value::Int(3)),
        (r#"{"float":0.5}"#, Value::Float(0.5)),
        (r#"{"bool":false}"#, Value::Bool(false)),
        (r#"{"str":"br"}"#, Value::Text("br".into())),
        (r#"{"gradient":"ff0000ff 0deg"}"#, Value::Text("rgba(ff0000ff)".into())),
    ];
    for (raw, expected) in cases {
        let json: Json = serde_json::from_str(raw).unwrap();
        assert_eq!(parse_option(&json).as_ref(), Some(expected));
    }
}

#[test]
fn version_detection_recognizes_prefixes() {
    assert_eq!(Version::parse("Hyprland 0.57.2").map(|v| v.to_string()), Some("0.57.2".to_string()));
    assert_eq!(Version::parse("v0.55").map(|v| v.to_string()), Some("0.55.0".to_string()));
}
