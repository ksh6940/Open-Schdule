use std::path::PathBuf;
use chrono::NaiveDate;

/// `%APPDATA%\OpenSchedule` (Windows) 등 앱 데이터 루트.
pub fn data_dir() -> PathBuf {
    let dir = directories::BaseDirs::new()
        .map(|b| b.config_dir().join("OpenSchedule"))
        .unwrap_or_else(|| PathBuf::from(".openschedule"));
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// 프로젝트 워크스페이스 루트 (`d:\Rust_Workspace\openschdule`) 경로.
pub fn workspace_root() -> Option<PathBuf> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // crates/core -> 상위 2단계가 프로젝트 루트
    let root = manifest_dir.parent()?.parent()?.to_path_buf();
    if root.exists() {
        Some(root)
    } else {
        None
    }
}

/// 일자별 `{date}_schedule.json` 파일이 저장되는 기본 로컬 디렉토리.
/// 프로젝트 폴더(`d:\Rust_Workspace\openschdule\schedules`)가 존재하면 우선 사용하며,
/// 없을 경우 `%APPDATA%\OpenSchedule\schedules`를 사용합니다.
pub fn schedules_dir() -> PathBuf {
    let dir = if let Some(root) = workspace_root() {
        root.join("schedules")
    } else {
        data_dir().join("schedules")
    };
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// `%APPDATA%\OpenSchedule\schedules` 백업 디렉토리.
pub fn appdata_schedules_dir() -> PathBuf {
    let p = data_dir().join("schedules");
    let _ = std::fs::create_dir_all(&p);
    p
}

/// 특정 날짜의 `{date}_schedule.json` 파일 경로 반환 (예: `2026-11-10_schedule.json`).
pub fn date_schedule_json_path(date: NaiveDate) -> PathBuf {
    let filename = format!("{}_schedule.json", date.format("%Y-%m-%d"));
    schedules_dir().join(filename)
}

/// AppData 내 특정 날짜의 `{date}_schedule.json` 파일 경로 반환.
pub fn appdata_date_schedule_json_path(date: NaiveDate) -> PathBuf {
    let filename = format!("{}_schedule.json", date.format("%Y-%m-%d"));
    appdata_schedules_dir().join(filename)
}

pub fn db_path() -> PathBuf {
    data_dir().join("todos.db")
}

pub fn settings_path() -> PathBuf {
    data_dir().join("settings.json")
}

pub fn agent_info_path() -> PathBuf {
    data_dir().join("agent.json")
}

pub fn agent_info_alt_path() -> PathBuf {
    data_dir().join("agent_info.json")
}

pub fn logs_dir() -> PathBuf {
    let p = data_dir().join("logs");
    let _ = std::fs::create_dir_all(&p);
    p
}

/// 현재 실행 파일과 같은 폴더에 있는 형제 실행 파일 경로.
pub fn sibling_exe(name: &str) -> PathBuf {
    let file = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    };
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join(&file)))
        .unwrap_or_else(|| PathBuf::from(file))
}
