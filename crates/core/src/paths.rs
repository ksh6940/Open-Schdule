use std::path::PathBuf;

/// `%APPDATA%\OpenSchedule` (Windows) 등 앱 데이터 루트.
pub fn data_dir() -> PathBuf {
    let dir = directories::BaseDirs::new()
        .map(|b| b.config_dir().join("OpenSchedule"))
        .unwrap_or_else(|| PathBuf::from(".openschedule"));
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn sub(name: &str) -> PathBuf {
    let p = data_dir().join(name);
    let _ = std::fs::create_dir_all(&p);
    p
}

pub fn db_path() -> PathBuf {
    data_dir().join("todos.db")
}
pub fn settings_path() -> PathBuf {
    data_dir().join("settings.json")
}
/// 에이전트가 실행 중일 때 포트/토큰을 기록하는 파일.
pub fn agent_info_path() -> PathBuf {
    data_dir().join("agent.json")
}
pub fn agent_info_alt_path() -> PathBuf {
    data_dir().join("agent_info.json")
}
pub fn logs_dir() -> PathBuf {
    sub("logs")
}

/// 현재 실행 파일과 같은 폴더에 있는 형제 실행 파일 경로.
pub fn sibling_exe(name: &str) -> PathBuf {
    let file = if cfg!(windows) { format!("{name}.exe") } else { name.to_string() };
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join(&file)))
        .unwrap_or_else(|| PathBuf::from(file))
}
