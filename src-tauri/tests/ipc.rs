//! IPC boundary integration test.
//!
//! Unit tests inside `commands/*.rs` call handler functions directly (Rust →
//! Rust), which never exercises the piece most likely to break silently: the
//! actual `invoke_handler` dispatch a JS `invoke("cmd", args)` call goes
//! through — command-name routing, `tauri::State` extraction, and
//! JSON (de)serialization of the request/response body. This drives real
//! `InvokeRequest`s through `tauri::test::get_ipc_response` against a
//! `MockRuntime` app built with the same state and the same
//! `tauri::generate_handler!` registrations as `run()`, so a renamed command,
//! a broken `State` extractor, or a non-serializable response type fails a
//! test here instead of shipping silently broken to the frontend.
//!
//! Skipped on Windows (`#![cfg(not(windows))]` below): linking *any* binary
//! — including a standalone `tests/` integration-test binary, not just the
//! `--lib` unit-test harness — against this crate's `rlib` output fails at
//! process start with `STATUS_ENTRYPOINT_NOT_FOUND` on the Windows CI
//! runner. Root cause is `[lib] crate-type = ["staticlib", "cdylib",
//! "rlib"]` (required for the Tauri app bundle across all three platforms):
//! rustc/link.exe produce a PE object exporting a DLL entry point
//! alongside the rlib metadata, and something in that combination confuses
//! the Windows loader for a *dependent* binary (this doesn't reproduce on
//! macOS or Linux, where the equivalent ELF/Mach-O outputs coexist fine).
//! Narrowing `crate-type` isn't an option without risking the shipped
//! Windows installer build. Windows already gets full coverage from the
//! `--lib` unit tests in `src/**/*.rs` (which don't link this crate as a
//! dependency) and from `cargo tauri build` in the release workflow, which
//! builds and packages the real Windows `.exe`/`.msi`/`.nsis` artifacts.

#![cfg(not(windows))]

use std::sync::Arc;

use serde_json::json;
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{get_ipc_response, mock_builder, mock_context, noop_assets, INVOKE_KEY};
use tauri::webview::InvokeRequest;

use supersearch_runtime::kernel::runtime::KernelConfig;
use supersearch_runtime::kernel::RuntimeKernel;

use supersearch_app_lib::commands;
use supersearch_app_lib::settings::SettingsStore;
use supersearch_app_lib::state::AppState;

/// Boot a real (in-memory-backed) kernel + app, wired with the exact command
/// set `run()` registers, running against `MockRuntime` (no actual
/// WebView/window system needed — safe for headless CI).
fn build_test_app(
    journal_dir: &std::path::Path,
    settings_dir: &std::path::Path,
) -> tauri::App<tauri::test::MockRuntime> {
    let config = KernelConfig {
        journal_dir: journal_dir.to_string_lossy().into_owned(),
        ..KernelConfig::default()
    };
    let kernel = RuntimeKernel::boot(config);
    let app_state = AppState::from_kernel(&kernel, 0);
    let settings_store = Arc::new(SettingsStore::load(settings_dir.to_path_buf()));

    // Kernel's async run loop isn't started here — these commands don't need
    // it, and skipping it keeps the test synchronous and fast.
    std::mem::forget(kernel);

    mock_builder()
        .manage(app_state)
        .manage(settings_store)
        .invoke_handler(tauri::generate_handler![
            commands::telemetry::get_telemetry,
            commands::agent::agent_check,
            commands::settings::get_settings,
        ])
        .build(mock_context(noop_assets()))
        .expect("failed to build mock app")
}

fn invoke_request(cmd: &str, body: serde_json::Value) -> InvokeRequest {
    InvokeRequest {
        cmd: cmd.into(),
        callback: CallbackFn(0),
        error: CallbackFn(1),
        // "tauri://localhost" resolves as the app's Local origin under the
        // real capabilities ACL (mirrors the webview's actual origin);
        // "http://tauri.localhost" resolves as Remote and gets denied.
        url: "tauri://localhost".parse().unwrap(),
        body: InvokeBody::Json(body),
        headers: Default::default(),
        invoke_key: INVOKE_KEY.to_string(),
    }
}

/// A real `invoke("get_telemetry")` round-trip: dispatch through the
/// registered command table, not a direct function call. Would fail if the
/// command were renamed, unregistered, or `AppState` weren't managed.
#[test]
fn get_telemetry_ipc_roundtrip() {
    let journal_dir = tempfile::tempdir().unwrap();
    let settings_dir = tempfile::tempdir().unwrap();
    let app = build_test_app(journal_dir.path(), settings_dir.path());
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();

    let res = get_ipc_response(&webview, invoke_request("get_telemetry", json!({})))
        .map(|b| b.deserialize::<commands::telemetry::TelemetrySnapshot>().unwrap())
        .expect("get_telemetry IPC call failed");

    // Freshly booted kernel: no ticks yet, queue idle, capabilities granted
    // (the agent token from boot) but none revoked.
    assert!(res.scheduler_idle);
    assert!(res.capabilities_total >= 1, "boot should grant at least the agent capability token");
    assert!(res.capabilities_active <= res.capabilities_total);
}

/// `agent_check` classifies a natural-language launch command as
/// agent-routable — proves argument (de)serialization (`query: String`)
/// survives the real IPC JSON body, not just a native `String` passed
/// in-process.
#[test]
fn agent_check_ipc_roundtrip() {
    let journal_dir = tempfile::tempdir().unwrap();
    let settings_dir = tempfile::tempdir().unwrap();
    let app = build_test_app(journal_dir.path(), settings_dir.path());
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();

    let res = get_ipc_response(
        &webview,
        invoke_request("agent_check", json!({ "query": "open chrome in incognito" })),
    )
    .map(|b| b.deserialize::<bool>().unwrap())
    .expect("agent_check IPC call failed");
    assert!(res, "a natural-language launch command should be agent-routable");

    let res = get_ipc_response(&webview, invoke_request("agent_check", json!({ "query": "xyz" })))
        .map(|b| b.deserialize::<bool>().unwrap())
        .expect("agent_check IPC call failed");
    assert!(!res, "a plain fuzzy-search token should not be agent-routable");
}

/// `get_settings` round-trips through IPC against a `SettingsStore` loaded
/// from an on-disk fixture dir, proving `State<Arc<SettingsStore>>`
/// extraction works through the real dispatch path (it's a distinct
/// `.manage()` call from `AppState`, easy to miss if it were dropped from
/// `run()`'s builder).
#[test]
fn get_settings_ipc_roundtrip() {
    let journal_dir = tempfile::tempdir().unwrap();
    let settings_dir = tempfile::tempdir().unwrap();
    let app = build_test_app(journal_dir.path(), settings_dir.path());
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();

    let res = get_ipc_response(&webview, invoke_request("get_settings", json!({})))
        .map(|b| b.deserialize::<supersearch_app_lib::settings::Settings>().unwrap())
        .expect("get_settings IPC call failed");
    assert_eq!(res.toggle_shortcut, "Alt+Space", "fresh settings dir should yield defaults");
}
