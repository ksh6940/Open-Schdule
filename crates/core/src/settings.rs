use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    // ── 핫존 오버레이 ──
    pub hotzone_enabled: bool,
    /// 논리 픽셀
    pub hotzone_width: f32,
    pub hotzone_height: f32,
    pub hotzone_dwell_ms: u64,
    pub hide_delay_ms: u64,
    pub panel_width: f32,

    // ── 기타 ──
    pub autostart: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            hotzone_enabled: true,
            hotzone_width: 240.0,
            hotzone_height: 4.0,
            hotzone_dwell_ms: 150,
            hide_delay_ms: 400,
            panel_width: 420.0,
            autostart: false,
        }
    }
}

impl Settings {
    pub fn load() -> Self {
        let path = crate::paths::settings_path();
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> Result<()> {
        let path = crate::paths::settings_path();
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self)?)?;
        std::fs::rename(tmp, path)?;
        Ok(())
    }

    /// 설정 파일 수정 시각 (다른 프로세스 변경 감지용).
    pub fn modified() -> Option<std::time::SystemTime> {
        std::fs::metadata(crate::paths::settings_path()).and_then(|m| m.modified()).ok()
    }
}

/// 에이전트 로컬 API 접속 정보 (agent.json / agent_info.json).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInfo {
    pub port: u16,
    pub token: String,
    pub pid: u32,
}

impl AgentInfo {
    pub fn load() -> Option<Self> {
        // agent.json 또는 agent_info.json 둘 중 존재하는 것 로드
        std::fs::read_to_string(crate::paths::agent_info_path())
            .or_else(|_| std::fs::read_to_string(crate::paths::agent_info_alt_path()))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
    }

    pub fn save(&self) -> Result<()> {
        let json = serde_json::to_string(self)?;
        // Electron이나 외부 툴이 어느 경로를 찾든 읽을 수 있도록 두 곳 모두에 저장
        let _ = std::fs::write(crate::paths::agent_info_path(), &json);
        let _ = std::fs::write(crate::paths::agent_info_alt_path(), &json);
        Ok(())
    }

    pub fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }
}
