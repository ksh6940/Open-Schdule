//! OpenLab / OpenSchedule 메인 진입점 (`openlab.exe`, `openschedule-agent.exe`).
//! - `openlab start -s`         : Claude Code 스타일 대화형 CLI 일정 관리 프로그램 실행
//! - `openlab start -s program` : Electron GUI 데스크톱 프로그램 ([투두리스트 / 캘린더 뷰] + 상단 웹캠 핫존 오버레이) 실행
//! - `openlab start -s <명령>`  : 단일 CLI 명령 실행 (AI 에이전트 및 스크립트 연동)
//! - `openschedule-agent --headless` : Electron 백그라운드 로컬 HTTP API 서버 기동

mod api;

use std::path::PathBuf;
use std::process::Command;

use openschedule_core::cli::{parse_launch_action, LaunchAction};

fn find_workspace_root() -> PathBuf {
    let compile_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));

    if compile_root.join("electron").join("main.js").exists() {
        return compile_root;
    }

    if let Ok(cwd) = std::env::current_dir() {
        if cwd.join("electron").join("main.js").exists() {
            return cwd;
        }
    }

    compile_root
}

fn launch_electron_program() -> anyhow::Result<()> {
    let root = find_workspace_root();
    let main_js = root.join("electron").join("main.js");

    let electron_bin = if cfg!(windows) {
        let local_exe = root
            .join("node_modules")
            .join("electron")
            .join("dist")
            .join("electron.exe");
        if local_exe.exists() {
            Some(local_exe)
        } else {
            None
        }
    } else {
        let local_bin = root.join("node_modules").join(".bin").join("electron");
        if local_bin.exists() {
            Some(local_bin)
        } else {
            None
        }
    };

    println!("🚀 OpenLab Schedule (Electron GUI 프로그램)을 실행합니다...");

    if let Some(exe) = electron_bin {
        Command::new(exe)
            .arg(&main_js)
            .current_dir(&root)
            .spawn()?;
    } else {
        Command::new("npx")
            .args(["electron", "electron/main.js"])
            .current_dir(&root)
            .spawn()?;
    }

    Ok(())
}

async fn run_headless_api_server() -> anyhow::Result<()> {
    let (port, _token, _handle) = api::start_api_server().await?;
    println!("OpenSchedule Rust Engine running on http://127.0.0.1:{port}");
    std::future::pending::<()>().await;
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let raw_args: Vec<String> = std::env::args().skip(1).collect();

    // Electron 메인 프로세스가 백그라운드 엔진으로 기동할 때 (`--headless`)
    if raw_args.iter().any(|a| a == "--headless" || a == "--api-only") {
        let _ = env_logger::try_init();
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?;
        return rt.block_on(run_headless_api_server());
    }

    match parse_launch_action(&raw_args) {
        LaunchAction::RunInteractiveCli => openschedule_core::cli::run_interactive_cli(),
        LaunchAction::RunCliCommand(cmd_args) => openschedule_core::cli::run_cli(&cmd_args),
        LaunchAction::RunGui => launch_electron_program(),
    }
}
