use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, NaiveDateTime, NaiveTime};
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use crate::model::{Category, Status, Subtask, Todo, TodoDraft};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DateFilter {
    All,
    Today,
    /// 오늘 + 지난 미완료 (오버레이 기본)
    TodayAndOverdue,
    /// 예정 (오늘 이후)
    Scheduled,
    ThisWeek,
    /// 완료된 항목 전체
    Completed,
    Day(NaiveDate),
    Range(NaiveDate, NaiveDate),
}

#[derive(Debug, Clone, PartialEq)]
pub struct TodoQuery {
    pub date: DateFilter,
    /// None = 전체, Some(None) = 미분류, Some(Some(id)) = 해당 카테고리
    pub category: Option<Option<i64>>,
    pub statuses: Vec<Status>,
    pub search: String,
}

impl Default for TodoQuery {
    fn default() -> Self {
        Self {
            date: DateFilter::All,
            category: None,
            statuses: vec![],
            search: String::new(),
        }
    }
}

/// 일자별 `{date}_schedule.json` 파일 포맷
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyScheduleFile {
    pub date: String,
    pub updated_at: String,
    pub count: usize,
    pub todos: Vec<TodoJsonEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TodoJsonEntry {
    #[serde(default)]
    pub id: i64,
    pub title: String,
    #[serde(default)]
    pub memo: String,
    #[serde(default)]
    pub category_id: Option<i64>,
    #[serde(default)]
    pub category_name: String,
    #[serde(default = "default_due_date")]
    pub due_date: NaiveDate,
    #[serde(default, deserialize_with = "deserialize_optional_end_date", skip_serializing_if = "Option::is_none")]
    pub end_date: Option<Option<NaiveDate>>,
    #[serde(default)]
    pub due_time: Option<NaiveTime>,
    #[serde(default, deserialize_with = "deserialize_optional_reminder", skip_serializing_if = "Option::is_none")]
    pub reminder_at: Option<Option<NaiveDateTime>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtasks: Option<Vec<Subtask>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<u8>,
    #[serde(default)]
    pub status: Status,
    #[serde(default)]
    pub condition_note: String,
    #[serde(default = "Local::now")]
    pub created_at: DateTime<Local>,
    #[serde(default = "Local::now")]
    pub updated_at: DateTime<Local>,
}

fn default_due_date() -> NaiveDate {
    Local::now().date_naive()
}

fn deserialize_optional_end_date<'de, D>(deserializer: D) -> Result<Option<Option<NaiveDate>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Some(Option::<NaiveDate>::deserialize(deserializer)?))
}

fn deserialize_optional_reminder<'de, D>(deserializer: D) -> Result<Option<Option<NaiveDateTime>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Some(Option::<NaiveDateTime>::deserialize(deserializer)?))
}

fn schedule_matches(path: &Path, date: &str, entries: &[TodoJsonEntry]) -> Result<bool> {
    let raw = match std::fs::read(path) {
        Ok(raw) => raw,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e).with_context(|| format!("JSON 파일 읽기 실패: {}", path.display())),
    };
    let value: serde_json::Value = serde_json::from_slice(&raw)
        .with_context(|| format!("JSON 파싱 실패: {}", path.display()))?;
    let existing = match serde_json::from_value::<DailyScheduleFile>(value) {
        Ok(existing) => existing,
        Err(_) => return Ok(false),
    };
    Ok(existing.date == date && existing.count == entries.len() && existing.todos == entries)
}

fn latest_schedule_files(dirs: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut files = std::collections::BTreeMap::new();
    for dir in dirs {
        let entries = std::fs::read_dir(dir)
            .with_context(|| format!("일정 폴더 읽기 실패: {}", dir.display()))?;
        for entry in entries {
            let entry = entry?;
            let name = entry.file_name();
            if !name.to_string_lossy().ends_with("_schedule.json") {
                continue;
            }
            let modified = entry.metadata()?.modified()?;
            match files.get(&name) {
                Some((old_modified, _)) if *old_modified >= modified => {}
                _ => { files.insert(name, (modified, entry.path())); }
            }
        }
    }
    Ok(files.into_values().map(|(_, path)| path).collect())
}

pub struct Store {
    conn: Connection,
    sync_json: bool,
}

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS categories (
    id    INTEGER PRIMARY KEY AUTOINCREMENT,
    name  TEXT NOT NULL UNIQUE,
    color INTEGER NOT NULL DEFAULT 7311359
);
CREATE TABLE IF NOT EXISTS todos (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    title          TEXT NOT NULL,
    memo           TEXT NOT NULL DEFAULT '',
    category_id    INTEGER REFERENCES categories(id) ON DELETE SET NULL,
    due_date       TEXT NOT NULL,
    due_time       TEXT,
    status         TEXT NOT NULL DEFAULT 'planned',
    condition_note TEXT NOT NULL DEFAULT '',
    created_at     TEXT NOT NULL,
    updated_at     TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_todos_due ON todos(due_date);
CREATE INDEX IF NOT EXISTS idx_todos_cat ON todos(category_id);
"#;

const DEFAULT_CATEGORIES: &[(&str, u32)] = &[
    ("업무", 0x6C8CFF),
    ("개인", 0x4FD1A5),
    ("공부", 0xF6B44D),
    ("건강", 0xF26D85),
];

impl Store {
    pub fn open_default() -> Result<Self> {
        let store = Self::open(&crate::paths::db_path())?;
        store.sync_all_json_files()?;
        Ok(store)
    }

    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)
            .with_context(|| format!("DB 열기 실패: {}", path.display()))?;
        Self::init(conn, true)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?, false)
    }

    fn init(conn: Connection, sync_json: bool) -> Result<Self> {
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.execute_batch(SCHEMA)?;
        conn.execute_batch("BEGIN IMMEDIATE")?;
        let migration = (|| -> Result<()> {
            let mut statement = conn.prepare("PRAGMA table_info(todos)")?;
            let columns = statement.query_map([], |row| row.get::<_, String>(1))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            if !columns.iter().any(|column| column == "end_date") {
                conn.execute("ALTER TABLE todos ADD COLUMN end_date TEXT", [])?;
            }
            if !columns.iter().any(|column| column == "reminder_at") {
                conn.execute("ALTER TABLE todos ADD COLUMN reminder_at TEXT", [])?;
            }
            if !columns.iter().any(|column| column == "subtasks") {
                conn.execute("ALTER TABLE todos ADD COLUMN subtasks TEXT NOT NULL DEFAULT '[]'", [])?;
            }
            if !columns.iter().any(|column| column == "priority") {
                conn.execute("ALTER TABLE todos ADD COLUMN priority INTEGER NOT NULL DEFAULT 0", [])?;
            }
            conn.execute("UPDATE todos SET status = 'done' WHERE status = 'conditional_done'", [])?;
            Ok(())
        })();
        match migration {
            Ok(()) => conn.execute_batch("COMMIT")?,
            Err(error) => {
                conn.execute_batch("ROLLBACK")?;
                return Err(error);
            }
        }
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM categories", [], |r| r.get(0))?;
        if count == 0 {
            for (name, color) in DEFAULT_CATEGORIES {
                conn.execute(
                    "INSERT INTO categories(name, color) VALUES (?1, ?2)",
                    params![name, color],
                )?;
            }
        }
        Ok(Self { conn, sync_json })
    }

    /// 다른 커넥션(다른 앱)이 DB를 변경하면 값이 바뀜.
    pub fn data_version(&self) -> i64 {
        self.conn
            .query_row("PRAGMA data_version", [], |r| r.get(0))
            .unwrap_or(0)
    }

    // ───────────── 일자별 JSON (`{date}_schedule.json`) 저장 & 불러오기 ─────────────

    /// 특정 날짜(`date`)의 모든 할 일을 조회하여 `{date}_schedule.json`으로 로컬 저장합니다.
    pub fn save_date_json(&self, date: NaiveDate) -> Result<PathBuf> {
        let primary_path = crate::paths::date_schedule_json_path(date);
        if !self.sync_json {
            return Ok(primary_path);
        }
        let appdata_path = crate::paths::appdata_date_schedule_json_path(date);
        self.save_date_json_to(date, primary_path, appdata_path)
    }

    fn save_date_json_to(&self, date: NaiveDate, primary_path: PathBuf, appdata_path: PathBuf) -> Result<PathBuf> {
        let sql = format!("SELECT {} FROM todos WHERE due_date = ?1 ORDER BY due_date, due_time IS NULL, due_time, id", Self::COLS);
        let mut statement = self.conn.prepare(&sql)?;
        let todos = statement.query_map(params![date.to_string()], Self::row_to_todo)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let cats = self.categories()?;

        let entries: Vec<TodoJsonEntry> = todos
            .into_iter()
            .map(|t| {
                let cat_name = t
                    .category_id
                    .and_then(|cid| cats.iter().find(|c| c.id == cid).map(|c| c.name.clone()))
                    .unwrap_or_else(|| "미분류".to_string());
                TodoJsonEntry {
                    id: t.id,
                    title: t.title,
                    memo: t.memo,
                    category_id: t.category_id,
                    category_name: cat_name,
                    due_date: t.due_date,
                    end_date: Some(t.end_date),
                    due_time: t.due_time,
                    reminder_at: Some(t.reminder_at),
                    subtasks: Some(t.subtasks),
                    priority: Some(t.priority),
                    status: t.status,
                    condition_note: t.condition_note,
                    created_at: t.created_at,
                    updated_at: t.updated_at,
                }
            })
            .collect();

        if entries.is_empty() {
            // 해당 날짜에 할 일이 모두 삭제된 경우 빈 JSON 파일도 정리
            for path in [&primary_path, &appdata_path] {
                match std::fs::remove_file(path) {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error).with_context(|| format!("JSON 삭제 실패: {}", path.display())),
                }
            }
            return Ok(primary_path);
        }

        let date_string = date.format("%Y-%m-%d").to_string();
        let write_primary = !schedule_matches(&primary_path, &date_string, &entries)?;
        let write_backup = appdata_path != primary_path
            && !schedule_matches(&appdata_path, &date_string, &entries)?;
        if !write_primary && !write_backup {
            return Ok(primary_path);
        }

        let file_data = DailyScheduleFile {
            date: date_string,
            updated_at: Local::now().to_rfc3339(),
            count: entries.len(),
            todos: entries,
        };

        let json_str = serde_json::to_string_pretty(&file_data)?;
        if write_primary {
            std::fs::write(&primary_path, &json_str)?;
        }
        if write_backup {
            std::fs::write(&appdata_path, &json_str)?;
        }
        Ok(primary_path)
    }

    /// DB에 존재하는 모든 날짜의 일정을 `{date}_schedule.json` 파일들로 내보냅니다.
    pub fn export_all_dates_to_json(&self) -> Result<usize> {
        if !self.sync_json {
            return Ok(0);
        }
        let mut st = self
            .conn
            .prepare("SELECT DISTINCT due_date FROM todos ORDER BY due_date")?;
        let date_strs: Vec<String> = st
            .query_map([], |r| r.get::<_, String>(0))?
            .filter_map(|r| r.ok())
            .collect();

        let mut exported = 0;
        for ds in date_strs {
            if let Ok(d) = NaiveDate::parse_from_str(&ds, "%Y-%m-%d") {
                self.save_date_json(d)?;
                exported += 1;
            }
        }
        Ok(exported)
    }

    /// 특정 `{date}_schedule.json` 파일을 읽어서 SQLite DB로 불러옵니다 (Import/Sync).
    pub fn import_date_json_file(&self, path: &Path) -> Result<usize> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("JSON 파일 읽기 실패: {}", path.display()))?;

        let file_date = path
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.strip_suffix("_schedule.json"))
            .and_then(|ds| NaiveDate::parse_from_str(ds, "%Y-%m-%d").ok());

        let (header_date, entries): (Option<NaiveDate>, Vec<TodoJsonEntry>) =
            if let Ok(daily) = serde_json::from_str::<DailyScheduleFile>(&raw) {
                let hd = NaiveDate::parse_from_str(&daily.date, "%Y-%m-%d").ok();
                (hd, daily.todos)
            } else {
                let list = serde_json::from_str::<Vec<TodoJsonEntry>>(&raw)
                    .with_context(|| format!("JSON 파싱 실패: {}", path.display()))?;
                (None, list)
            };

        let existing_cats = self.categories()?;
        let mut imported = 0;
        for entry in entries {
            if entry.title.trim().is_empty() {
                continue;
            }
            // category_id가 실제 DB에 존재하는지 검증하고, 없으면 category_name으로 매핑
            let valid_cid = entry
                .category_id
                .filter(|cid| existing_cats.iter().any(|c| c.id == *cid));
            let cat_id = if let Some(cid) = valid_cid {
                Some(cid)
            } else if !entry.category_name.trim().is_empty() && entry.category_name != "미분류" {
                self.find_category(&entry.category_name, true)?
            } else {
                None
            };

            let resolved_date = file_date.or(header_date).unwrap_or(entry.due_date);

            // 동일한 ID가 있거나 (같은 날짜 + 같은 제목)이 있는지 확인
            let existing = if entry.id > 0 {
                self.get(entry.id)?
            } else {
                None
            };
            let existing = match existing {
                Some(todo) => Some(todo),
                None => {
                    let id = self.conn.query_row(
                        "SELECT id FROM todos WHERE due_date=?1 AND title=?2 LIMIT 1",
                        params![resolved_date.to_string(), entry.title.trim()],
                        |r| r.get::<_, i64>(0),
                    ).optional()?;
                    match id {
                        Some(id) => self.get(id)?,
                        None => None,
                    }
                }
            };

            let end_date = match entry.end_date {
                Some(end) => end,
                None => match existing.as_ref() {
                    Some(todo) => match todo.end_date {
                        Some(end) => Some(end.checked_add_signed(resolved_date - todo.due_date)
                            .context("일정 종료일 범위를 벗어났습니다.")?),
                        None => None,
                    },
                    None => None,
                },
            };
            let draft = TodoDraft {
                title: entry.title.trim().to_string(),
                memo: entry.memo,
                category_id: cat_id,
                due_date: resolved_date,
                end_date,
                due_time: entry.due_time,
                reminder_at: entry.reminder_at.unwrap_or_else(|| existing.as_ref().and_then(|todo| todo.reminder_at)),
                subtasks: entry.subtasks.unwrap_or_else(|| existing.as_ref().map(|todo| todo.subtasks.clone()).unwrap_or_default()),
                priority: entry.priority.unwrap_or_else(|| existing.as_ref().map_or(0, |todo| todo.priority)),
                status: if entry.status == Status::ConditionalDone { Status::Done } else { entry.status },
                condition_note: entry.condition_note,
            };

            Self::validate_draft(&draft)?;
            if let Some(existing) = existing {
                if existing.to_draft() != draft || existing.updated_at != entry.updated_at {
                    self.conn.execute(
                        "UPDATE todos SET title=?1, memo=?2, category_id=?3, due_date=?4, due_time=?5, status=?6,
                         condition_note=?7, updated_at=?8, end_date=?10, reminder_at=?11, subtasks=?12, priority=?13 WHERE id=?9",
                        params![
                            draft.title,
                            draft.memo,
                            draft.category_id,
                            draft.due_date.format("%Y-%m-%d").to_string(),
                            draft.due_time.map(|t| t.format("%H:%M").to_string()),
                            draft.status.as_db(),
                            draft.condition_note,
                            entry.updated_at.to_rfc3339(),
                            existing.id,
                            draft.end_date.map(|d| d.to_string()),
                            draft.reminder_at.map(|d| d.format("%Y-%m-%dT%H:%M:%S").to_string()),
                            serde_json::to_string(&draft.subtasks)?,
                            draft.priority
                        ],
                    )?;
                }
            } else {
                // 같은 날짜 + 같은 제목이 없을 때만 새로 추가
                let dup: Option<i64> = self
                    .conn
                    .query_row(
                        "SELECT id FROM todos WHERE due_date=?1 AND title=?2 LIMIT 1",
                        params![
                            draft.due_date.format("%Y-%m-%d").to_string(),
                            draft.title
                        ],
                        |r| r.get(0),
                    )
                    .optional()?;
                if let Some(existing_id) = dup {
                    self.update(existing_id, &draft)?;
                } else {
                    let now = Local::now().to_rfc3339();
                    self.conn.execute(
                        "INSERT INTO todos(title, memo, category_id, due_date, due_time, status, condition_note, created_at, updated_at, end_date, reminder_at, subtasks, priority)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8, ?9, ?10, ?11, ?12)",
                        params![
                            draft.title,
                            draft.memo,
                            draft.category_id,
                            draft.due_date.format("%Y-%m-%d").to_string(),
                            draft.due_time.map(|t| t.format("%H:%M").to_string()),
                            draft.status.as_db(),
                            draft.condition_note,
                            now,
                            draft.end_date.map(|d| d.to_string()),
                            draft.reminder_at.map(|d| d.format("%Y-%m-%dT%H:%M:%S").to_string()),
                            serde_json::to_string(&draft.subtasks)?,
                            draft.priority
                        ],
                    )?;
                }
            }
            imported += 1;
        }

        Ok(imported)
    }

    /// `schedules/` 디렉토리의 모든 `{date}_schedule.json` 파일을 스캔하여 SQLite DB와 양방향 동기화합니다.
    pub fn sync_all_json_files(&self) -> Result<usize> {
        let mut total_imported = 0;
        let primary_dir = crate::paths::schedules_dir();
        let appdata_dir = crate::paths::appdata_schedules_dir();

        let mut dirs = vec![primary_dir.clone()];
        if appdata_dir != primary_dir {
            dirs.push(appdata_dir);
        }

        for path in latest_schedule_files(&dirs)? {
            total_imported += self.import_date_json_file(&path)?;
        }
        self.export_all_dates_to_json()?;
        Ok(total_imported)
    }

    /// `schedules/` 디렉토리 내 `{date}_schedule.json` 파일들의 변경 감지용 해시 시그니처.
    pub fn schedules_dir_signature(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        let dir = crate::paths::schedules_dir();
        if let Ok(read_dir) = std::fs::read_dir(&dir) {
            let mut entries: Vec<_> = read_dir.flatten().collect();
            entries.sort_by_key(|e| e.file_name());
            for entry in entries {
                let path = entry.path();
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.ends_with("_schedule.json") {
                        name.hash(&mut hasher);
                        if let Ok(meta) = entry.metadata() {
                            meta.len().hash(&mut hasher);
                            if let Ok(mtime) = meta.modified() {
                                mtime.hash(&mut hasher);
                            }
                        }
                    }
                }
            }
        }
        hasher.finish()
    }

    /// 현재 로컬에 저장된 모든 `{date}_schedule.json` 파일 목록을 반환합니다.
    pub fn list_json_schedule_files(&self) -> Result<Vec<(String, PathBuf, usize)>> {
        let dir = crate::paths::schedules_dir();
        let mut list = Vec::new();
        if let Ok(read_dir) = std::fs::read_dir(&dir) {
            for entry in read_dir.flatten() {
                let path = entry.path();
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if let Some(date_part) = name.strip_suffix("_schedule.json") {
                        let count = std::fs::read_to_string(&path)
                            .ok()
                            .and_then(|s| serde_json::from_str::<DailyScheduleFile>(&s).ok())
                            .map(|f| f.todos.len())
                            .unwrap_or(0);
                        list.push((date_part.to_string(), path, count));
                    }
                }
            }
        }
        list.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(list)
    }

    // ───────────── categories ─────────────

    pub fn categories(&self) -> Result<Vec<Category>> {
        let mut st = self
            .conn
            .prepare("SELECT id, name, color FROM categories ORDER BY id")?;
        let rows = st.query_map([], |r| {
            Ok(Category {
                id: r.get(0)?,
                name: r.get(1)?,
                color: r.get(2)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn add_category(&self, name: &str, color: u32) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO categories(name, color) VALUES (?1, ?2)",
            params![name.trim(), color],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn update_category(&self, id: i64, name: &str, color: u32) -> Result<()> {
        self.conn.execute(
            "UPDATE categories SET name=?1, color=?2 WHERE id=?3",
            params![name.trim(), color, id],
        )?;
        self.export_all_dates_to_json().context("DB 변경 후 JSON 동기화 실패")?;
        Ok(())
    }

    pub fn delete_category(&self, id: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE todos SET category_id=NULL WHERE category_id=?1",
            params![id],
        )?;
        self.conn
            .execute("DELETE FROM categories WHERE id=?1", params![id])?;
        self.export_all_dates_to_json().context("DB 변경 후 JSON 동기화 실패")?;
        Ok(())
    }

    /// 이름으로 카테고리 찾기(대소문자/공백 무시). 없으면 `create`가 true일 때 생성.
    pub fn find_category(&self, name: &str, create: bool) -> Result<Option<i64>> {
        let name = name.trim();
        if name.is_empty() {
            return Ok(None);
        }
        let found = self
            .categories()?
            .into_iter()
            .find(|c| c.name.trim().eq_ignore_ascii_case(name))
            .map(|c| c.id);
        match found {
            Some(id) => Ok(Some(id)),
            None if create => Ok(Some(self.add_category(name, 0x9A8CFF)?)),
            None => Ok(None),
        }
    }

    // ───────────── todos ─────────────

    fn row_to_todo(r: &Row) -> rusqlite::Result<Todo> {
        let due_date: String = r.get(4)?;
        let due_time: Option<String> = r.get(5)?;
        let status: String = r.get(6)?;
        let created: String = r.get(8)?;
        let updated: String = r.get(9)?;
        let end_date: Option<String> = r.get(10)?;
        let reminder_at: Option<String> = r.get(11)?;
        let subtasks: String = r.get(12)?;
        Ok(Todo {
            id: r.get(0)?,
            title: r.get(1)?,
            memo: r.get(2)?,
            category_id: r.get(3)?,
            due_date: NaiveDate::parse_from_str(&due_date, "%Y-%m-%d")
                .unwrap_or_else(|_| Local::now().date_naive()),
            end_date: end_date.map(|date| NaiveDate::parse_from_str(&date, "%Y-%m-%d")
                .map_err(|error| rusqlite::Error::FromSqlConversionFailure(10, rusqlite::types::Type::Text, Box::new(error)))).transpose()?,
            due_time: due_time.and_then(|t| {
                NaiveTime::parse_from_str(&t, "%H:%M")
                    .or_else(|_| NaiveTime::parse_from_str(&t, "%H:%M:%S"))
                    .ok()
            }),
            reminder_at: reminder_at.map(|date| NaiveDateTime::parse_from_str(&date, "%Y-%m-%dT%H:%M:%S")
                .map_err(|error| rusqlite::Error::FromSqlConversionFailure(11, rusqlite::types::Type::Text, Box::new(error)))).transpose()?,
            subtasks: serde_json::from_str(&subtasks)
                .map_err(|error| rusqlite::Error::FromSqlConversionFailure(12, rusqlite::types::Type::Text, Box::new(error)))?,
            priority: r.get(13)?,
            status: Status::from_db(&status),
            condition_note: r.get(7)?,
            created_at: chrono::DateTime::parse_from_rfc3339(&created)
                .map(|d| d.with_timezone(&Local))
                .unwrap_or_else(|_| Local::now()),
            updated_at: chrono::DateTime::parse_from_rfc3339(&updated)
                .map(|d| d.with_timezone(&Local))
                .unwrap_or_else(|_| Local::now()),
        })
    }

    const COLS: &'static str =
        "id, title, memo, category_id, due_date, due_time, status, condition_note, created_at, updated_at, end_date, reminder_at, subtasks, priority";

    fn validate_draft(d: &TodoDraft) -> Result<()> {
        anyhow::ensure!(d.end_date.map_or(true, |end| end >= d.due_date), "종료일은 시작일보다 빠를 수 없습니다.");
        anyhow::ensure!(d.subtasks.iter().all(|item| !item.title.trim().is_empty()), "세부 할 일 제목을 입력해주세요.");
        anyhow::ensure!(d.priority <= 3, "긴급도는 0부터 3까지 설정할 수 있습니다.");
        Ok(())
    }

    pub fn get(&self, id: i64) -> Result<Option<Todo>> {
        let sql = format!("SELECT {} FROM todos WHERE id=?1", Self::COLS);
        Ok(self
            .conn
            .query_row(&sql, params![id], Self::row_to_todo)
            .optional()?)
    }

    pub fn add(&self, d: &TodoDraft) -> Result<i64> {
        Self::validate_draft(d)?;
        let now = Local::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO todos(title, memo, category_id, due_date, due_time, status, condition_note, created_at, updated_at, end_date, reminder_at, subtasks, priority)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8, ?9, ?10, ?11, ?12)",
            params![
                d.title.trim(),
                d.memo,
                d.category_id,
                d.due_date.format("%Y-%m-%d").to_string(),
                d.due_time.map(|t| t.format("%H:%M").to_string()),
                d.status.current().as_db(),
                d.condition_note,
                now,
                d.end_date.map(|date| date.to_string()),
                d.reminder_at.map(|date| date.format("%Y-%m-%dT%H:%M:%S").to_string()),
                serde_json::to_string(&d.subtasks)?,
                d.priority
            ],
        )?;
        let id = self.conn.last_insert_rowid();
        self.save_date_json(d.due_date).context("DB 변경 후 JSON 동기화 실패")?;
        Ok(id)
    }

    pub fn update(&self, id: i64, d: &TodoDraft) -> Result<()> {
        Self::validate_draft(d)?;
        let old_date = self.get(id)?.map(|t| t.due_date);
        self.conn.execute(
            "UPDATE todos SET title=?1, memo=?2, category_id=?3, due_date=?4, due_time=?5, status=?6,
                 condition_note=?7, updated_at=?8, end_date=?10, reminder_at=?11, subtasks=?12, priority=?13 WHERE id=?9",
            params![
                d.title.trim(),
                d.memo,
                d.category_id,
                d.due_date.format("%Y-%m-%d").to_string(),
                d.due_time.map(|t| t.format("%H:%M").to_string()),
                d.status.current().as_db(),
                d.condition_note,
                Local::now().to_rfc3339(),
                id,
                d.end_date.map(|date| date.to_string()),
                d.reminder_at.map(|date| date.format("%Y-%m-%dT%H:%M:%S").to_string()),
                serde_json::to_string(&d.subtasks)?,
                d.priority
            ],
        )?;
        self.save_date_json(d.due_date).context("DB 변경 후 JSON 동기화 실패")?;
        if let Some(old_d) = old_date {
            if old_d != d.due_date {
                self.save_date_json(old_d).context("DB 변경 후 JSON 동기화 실패")?;
            }
        }
        Ok(())
    }

    pub fn set_status(&self, id: i64, status: Status) -> Result<()> {
        let target_date = self.get(id)?.map(|t| t.due_date);
        self.conn.execute(
            "UPDATE todos SET status=?1, updated_at=?2 WHERE id=?3",
            params![status.current().as_db(), Local::now().to_rfc3339(), id],
        )?;
        if let Some(d) = target_date {
            self.save_date_json(d).context("DB 변경 후 JSON 동기화 실패")?;
        }
        Ok(())
    }

    pub fn delete(&self, id: i64) -> Result<()> {
        let target_date = self.get(id)?.map(|t| t.due_date);
        self.conn
            .execute("DELETE FROM todos WHERE id=?1", params![id])?;
        if let Some(d) = target_date {
            self.save_date_json(d).context("DB 변경 후 JSON 동기화 실패")?;
        }
        Ok(())
    }

    /// 날짜(오름차순) → 시간(없으면 뒤) → id 순 정렬.
    pub fn query(&self, q: &TodoQuery) -> Result<Vec<Todo>> {
        let today = Local::now().date_naive();
        let mut sql = format!("SELECT {} FROM todos WHERE 1=1", Self::COLS);
        let mut args: Vec<Box<dyn rusqlite::ToSql>> = vec![];
        let fmt = |d: NaiveDate| d.format("%Y-%m-%d").to_string();

        match q.date {
            DateFilter::All => {}
            DateFilter::Today => {
                sql += " AND due_date <= ? AND COALESCE(end_date, due_date) >= ?";
                args.push(Box::new(fmt(today)));
                args.push(Box::new(fmt(today)));
            }
            DateFilter::TodayAndOverdue => {
                sql += " AND due_date <= ? AND (COALESCE(end_date, due_date) >= ? OR status NOT IN ('done','conditional_done'))";
                args.push(Box::new(fmt(today)));
                args.push(Box::new(fmt(today)));
            }
            DateFilter::Scheduled => {
                sql += " AND due_date > ?";
                args.push(Box::new(fmt(today)));
            }
            DateFilter::ThisWeek => {
                let start = today - Duration::days(today.weekday().num_days_from_monday() as i64);
                let end = start + Duration::days(6);
                sql += " AND COALESCE(end_date, due_date) >= ? AND due_date <= ?";
                args.push(Box::new(fmt(start)));
                args.push(Box::new(fmt(end)));
            }
            DateFilter::Completed => {
                sql += " AND status IN ('done','conditional_done')";
            }
            DateFilter::Day(d) => {
                sql += " AND due_date <= ? AND COALESCE(end_date, due_date) >= ?";
                args.push(Box::new(fmt(d)));
                args.push(Box::new(fmt(d)));
            }
            DateFilter::Range(a, b) => {
                sql += " AND COALESCE(end_date, due_date) >= ? AND due_date <= ?";
                args.push(Box::new(fmt(a)));
                args.push(Box::new(fmt(b)));
            }
        }
        match q.category {
            None => {}
            Some(None) => sql += " AND category_id IS NULL",
            Some(Some(id)) => {
                sql += " AND category_id = ?";
                args.push(Box::new(id));
            }
        }
        if !q.statuses.is_empty() {
            let ph = vec!["?"; q.statuses.len()].join(",");
            sql += &format!(" AND status IN ({ph})");
            for s in &q.statuses {
                args.push(Box::new(s.as_db()));
            }
        }
        if !q.search.trim().is_empty() {
            sql += " AND (title LIKE ? OR memo LIKE ?)";
            let pat = format!("%{}%", q.search.trim());
            args.push(Box::new(pat.clone()));
            args.push(Box::new(pat));
        }
        sql += " ORDER BY due_date ASC, due_time IS NULL, due_time ASC, id ASC";

        let mut st = self.conn.prepare(&sql)?;
        let params: Vec<&dyn rusqlite::ToSql> = args.iter().map(|b| b.as_ref()).collect();
        let rows = st.query_map(params.as_slice(), Self::row_to_todo)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// 특정 연/월의 일자별 할 일 목록 맵 반환 (캘린더 뷰 렌더링용).
    pub fn todos_by_month(&self, year: i32, month: u32) -> Result<HashMap<NaiveDate, Vec<Todo>>> {
        let Some(first_day) = NaiveDate::from_ymd_opt(year, month, 1) else {
            return Ok(HashMap::new());
        };
        let next_month = if month == 12 {
            NaiveDate::from_ymd_opt(year + 1, 1, 1).unwrap()
        } else {
            NaiveDate::from_ymd_opt(year, month + 1, 1).unwrap()
        };
        let last_day = next_month - Duration::days(1);

        let todos = self.query(&TodoQuery {
            date: DateFilter::Range(first_day, last_day),
            ..Default::default()
        })?;

        let mut map: HashMap<NaiveDate, Vec<Todo>> = HashMap::new();
        for t in todos {
            let mut day = t.due_date.max(first_day);
            let end = t.end_date.unwrap_or(t.due_date).min(last_day);
            while day <= end {
                map.entry(day).or_default().push(t.clone());
                if day == end { break; }
                day = day.succ_opt().expect("day before end has a successor");
            }
        }
        Ok(map)
    }

    /// 요약 텍스트 출력.
    pub fn context_text(&self, filter: DateFilter, limit: usize) -> Result<String> {
        let cats = self.categories()?;
        let todos = self.query(&TodoQuery {
            date: filter,
            ..Default::default()
        })?;
        let mut out = String::new();
        for t in todos.iter().take(limit) {
            let cat = t
                .category_id
                .and_then(|id| cats.iter().find(|c| c.id == id))
                .map(|c| c.name.as_str())
                .unwrap_or("미분류");
            let time = t
                .due_time
                .map(|t| t.format(" %H:%M").to_string())
                .unwrap_or_default();
            out += &format!(
                "- [#{}] {} | {}{} | {} | {}{}\n",
                t.id,
                t.title,
                t.due_date.format("%Y-%m-%d"),
                time,
                cat,
                t.status.label(),
                if t.condition_note.is_empty() {
                    String::new()
                } else {
                    format!(" (조건: {})", t.condition_note)
                }
            );
        }
        if out.is_empty() {
            out = "(할 일 없음)\n".into();
        }
        Ok(out)
    }

    /// Apple 미리알림 4대 스마트 목록 (오늘, 예정, 전체, 완료됨) 카운트.
    pub fn smart_counts(&self) -> Result<SmartCounts> {
        let today = Local::now().date_naive().format("%Y-%m-%d").to_string();
        let today_count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM todos WHERE due_date <= ?1 AND (COALESCE(end_date, due_date) >= ?1 OR status NOT IN ('done', 'conditional_done'))",
            params![today],
            |r| r.get(0),
        )?;
        let scheduled_count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM todos WHERE due_date > ?1",
            params![today],
            |r| r.get(0),
        )?;
        let all_count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM todos",
            [],
            |r| r.get(0),
        )?;
        let completed_count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM todos WHERE status IN ('done', 'conditional_done')",
            [],
            |r| r.get(0),
        )?;
        Ok(SmartCounts {
            today: today_count as usize,
            scheduled: scheduled_count as usize,
            all: all_count as usize,
            completed: completed_count as usize,
        })
    }

    /// 각 카테고리별 할 일 카운트 (DateFilter::All 기준과 일치하도록 전체 카운트).
    pub fn category_counts(&self) -> Result<HashMap<i64, usize>> {
        let mut map = HashMap::new();
        let mut st = self.conn.prepare(
            "SELECT category_id, COUNT(*) FROM todos WHERE category_id IS NOT NULL GROUP BY category_id",
        )?;
        let rows = st.query_map([], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)? as usize))
        })?;
        for r in rows {
            let (cid, count) = r?;
            map.insert(cid, count);
        }
        Ok(map)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SmartCounts {
    pub today: usize,
    pub scheduled: usize,
    pub all: usize,
    pub completed: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unchanged_json_import_does_not_write_to_database() {
        let s = Store::open_in_memory().unwrap();
        let date = Local::now().date_naive();
        let id = s.add(&TodoDraft::new("test", date)).unwrap();
        let todo = s.get(id).unwrap().unwrap();
        let entry = TodoJsonEntry {
            id,
            title: todo.title,
            memo: todo.memo,
            category_id: todo.category_id,
            category_name: "미분류".into(),
            due_date: todo.due_date,
            end_date: Some(todo.end_date),
            due_time: todo.due_time,
            reminder_at: Some(todo.reminder_at),
            subtasks: Some(todo.subtasks),
            priority: Some(todo.priority),
            status: todo.status,
            condition_note: todo.condition_note,
            created_at: todo.created_at,
            updated_at: todo.updated_at,
        };
        let file = DailyScheduleFile {
            date: date.to_string(),
            updated_at: Local::now().to_rfc3339(),
            count: 1,
            todos: vec![entry],
        };
        let path = std::env::temp_dir().join(format!(
            "openschedule-test-{}-{}.json",
            std::process::id(),
            Local::now().timestamp_nanos_opt().unwrap()
        ));
        std::fs::write(&path, serde_json::to_string(&file).unwrap()).unwrap();

        assert!(schedule_matches(&path, &file.date, &file.todos).unwrap());
        let changes = s.conn.total_changes();
        s.import_date_json_file(&path).unwrap();
        assert_eq!(s.conn.total_changes(), changes);

        let mut modified = file.clone();
        modified.todos[0].title = "changed".into();
        assert!(!schedule_matches(&path, &modified.date, &modified.todos).unwrap());
        std::fs::write(&path, serde_json::to_string(&modified).unwrap()).unwrap();
        s.import_date_json_file(&path).unwrap();
        assert_eq!(s.get(id).unwrap().unwrap().title, "changed");
        std::fs::write(&path, "{ invalid").unwrap();
        assert!(schedule_matches(&path, &file.date, &file.todos).is_err());
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn crud_and_filters() {
        let s = Store::open_in_memory().unwrap();
        let cats = s.categories().unwrap();
        assert_eq!(cats.len(), 4);

        let today = Local::now().date_naive();
        let mut d = TodoDraft::new("보고서 작성", today);
        d.category_id = Some(cats[0].id);
        let id = s.add(&d).unwrap();

        let old = TodoDraft::new("지난 일", today - Duration::days(3));
        let old_id = s.add(&old).unwrap();
        let future = TodoDraft::new("다음주 일", today + Duration::days(10));
        s.add(&future).unwrap();

        let q = TodoQuery {
            date: DateFilter::TodayAndOverdue,
            ..Default::default()
        };
        assert_eq!(s.query(&q).unwrap().len(), 2);

        s.set_status(old_id, Status::ConditionalDone).unwrap();
        assert_eq!(s.query(&q).unwrap().len(), 1);

        let q = TodoQuery {
            category: Some(Some(cats[0].id)),
            ..Default::default()
        };
        assert_eq!(s.query(&q).unwrap().len(), 1);

        let mut upd = s.get(id).unwrap().unwrap().to_draft();
        upd.title = "보고서 완성".into();
        upd.status = Status::InProgress;
        upd.due_time = NaiveTime::from_hms_opt(15, 0, 0);
        s.update(id, &upd).unwrap();
        let t = s.get(id).unwrap().unwrap();
        assert_eq!(t.title, "보고서 완성");
        assert_eq!(t.status, Status::InProgress);
        assert_eq!(t.due_time, NaiveTime::from_hms_opt(15, 0, 0));

        s.delete_category(cats[0].id).unwrap();
        assert_eq!(s.get(id).unwrap().unwrap().category_id, None);

        s.delete(id).unwrap();
        assert!(s.get(id).unwrap().is_none());
    }

    #[test]
    fn reminders_and_subtasks_persist_and_survive_legacy_json() {
        let store = Store::open_in_memory().unwrap();
        let date = NaiveDate::from_ymd_opt(2030, 1, 1).unwrap();
        let mut draft = TodoDraft::new("task", date);
        draft.reminder_at = Some(date.and_hms_opt(9, 30, 0).unwrap());
        draft.subtasks = vec![Subtask { title: "step".into(), done: true }];
        draft.priority = 3;
        let id = store.add(&draft).unwrap();
        assert_eq!(store.get(id).unwrap().unwrap().to_draft(), draft);
        let path = std::env::temp_dir().join(format!(
            "openschedule-details-{}-{}.json", std::process::id(),
            Local::now().timestamp_nanos_opt().unwrap()
        ));
        let backup = path.with_extension("backup");
        store.save_date_json_to(date, path.clone(), backup.clone()).unwrap();
        let copy = Store::open_in_memory().unwrap();
        copy.import_date_json_file(&path).unwrap();
        assert_eq!(copy.query(&TodoQuery::default()).unwrap()[0].to_draft(), draft);
        std::fs::write(&path, serde_json::json!([{
            "id": id, "title": "task", "due_date": date.to_string()
        }]).to_string()).unwrap();
        store.import_date_json_file(&path).unwrap();
        let restored = store.get(id).unwrap().unwrap();
        assert_eq!(restored.reminder_at, draft.reminder_at);
        assert_eq!(restored.subtasks, draft.subtasks);
        assert_eq!(restored.priority, 3);
        std::fs::write(&path, serde_json::json!([{
            "id": id, "title": "task", "due_date": date.to_string(),
            "reminder_at": null, "subtasks": []
        }]).to_string()).unwrap();
        store.import_date_json_file(&path).unwrap();
        let cleared = store.get(id).unwrap().unwrap();
        assert_eq!(cleared.reminder_at, None);
        assert!(cleared.subtasks.is_empty());
        std::fs::remove_file(path).unwrap();
        std::fs::remove_file(backup).unwrap();
    }

    #[test]
    fn legacy_database_migrates_priority_without_losing_todos() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(SCHEMA).unwrap();
        let now = Local::now().to_rfc3339();
        conn.execute(
            "INSERT INTO todos(title, due_date, created_at, updated_at, status) VALUES (?1, ?2, ?3, ?3, 'conditional_done')",
            params!["legacy", "2030-01-01", now],
        ).unwrap();
        let store = Store::init(conn, false).unwrap();
        let todo = store.get(1).unwrap().unwrap();
        assert_eq!(todo.title, "legacy");
        assert_eq!(todo.status, Status::Done);
        assert_eq!(todo.priority, 0);
        assert!(todo.subtasks.is_empty());
        assert_eq!(todo.reminder_at, None);
        let mut draft = todo.to_draft();
        draft.priority = 4;
        assert!(store.update(todo.id, &draft).is_err());
        assert_eq!(store.get(todo.id).unwrap().unwrap().priority, 0);
    }

    #[test]
    fn importing_legacy_conditional_status_converts_to_done() {
        let store = Store::open_in_memory().unwrap();
        let path = std::env::temp_dir().join(format!(
            "openschedule-legacy-status-{}-{}.json", std::process::id(),
            Local::now().timestamp_nanos_opt().unwrap()
        ));
        std::fs::write(&path, serde_json::json!([{
            "title": "legacy", "due_date": "2030-01-01",
            "status": "conditional_done", "condition_note": "keep note"
        }]).to_string()).unwrap();
        store.import_date_json_file(&path).unwrap();
        let todo = store.query(&TodoQuery::default()).unwrap().remove(0);
        assert_eq!(todo.status, Status::Done);
        assert_eq!(todo.condition_note, "keep note");
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn period_events_migrate_persist_query_and_import() {
        let path = std::env::temp_dir().join(format!(
            "openschedule-period-{}-{}.db", std::process::id(),
            Local::now().timestamp_nanos_opt().unwrap()
        ));
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(SCHEMA).unwrap();
        let store = Store::init(conn, false).unwrap();
        let today = Local::now().date_naive();
        let mut draft = TodoDraft::new("period", today - Duration::days(1));
        draft.end_date = Some(today + Duration::days(2));
        let id = store.add(&draft).unwrap();
        drop(store);

        let store = Store::init(Connection::open(&path).unwrap(), false).unwrap();
        let event = store.get(id).unwrap().unwrap();
        assert_eq!(event.end_date, draft.end_date);
        assert!(!event.is_overdue(today));
        for day in [today - Duration::days(1), today, today + Duration::days(2)] {
            let query = TodoQuery { date: DateFilter::Day(day), ..Default::default() };
            assert_eq!(store.query(&query).unwrap().len(), 1);
        }
        let query = TodoQuery { date: DateFilter::Day(today + Duration::days(3)), ..Default::default() };
        assert!(store.query(&query).unwrap().is_empty());
        assert_eq!(store.smart_counts().unwrap().today, 1);
        assert_eq!(store.todos_by_month(today.year(), today.month()).unwrap().get(&today).unwrap().len(), 1);
        let mut invalid = draft.clone();
        invalid.end_date = Some(draft.due_date - Duration::days(1));
        assert!(store.update(id, &invalid).is_err());
        assert_eq!(store.get(id).unwrap().unwrap().end_date, draft.end_date);

        let json_path = path.with_extension("json");
        let legacy = serde_json::json!([{
            "id": id, "title": "period edited", "due_date": draft.due_date.to_string(),
            "updated_at": event.updated_at
        }]);
        std::fs::write(&json_path, legacy.to_string()).unwrap();
        store.import_date_json_file(&json_path).unwrap();
        assert_eq!(store.get(id).unwrap().unwrap().end_date, draft.end_date);
        let mut with_end = legacy;
        with_end[0]["end_date"] = serde_json::json!(today.to_string());
        std::fs::write(&json_path, with_end.to_string()).unwrap();
        store.import_date_json_file(&json_path).unwrap();
        assert_eq!(store.get(id).unwrap().unwrap().end_date, Some(today));
        with_end[0]["end_date"] = serde_json::Value::Null;
        std::fs::write(&json_path, with_end.to_string()).unwrap();
        store.import_date_json_file(&json_path).unwrap();
        assert_eq!(store.get(id).unwrap().unwrap().end_date, None);
        store.delete(id).unwrap();
        assert!(store.get(id).unwrap().is_none());
        drop(store);
        std::fs::remove_file(json_path).unwrap();
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn period_json_is_written_only_for_start_and_round_trips() {
        let dir = std::env::temp_dir().join(format!(
            "openschedule-period-json-{}-{}", std::process::id(),
            Local::now().timestamp_nanos_opt().unwrap()
        ));
        std::fs::create_dir(&dir).unwrap();
        let store = Store::open_in_memory().unwrap();
        let start = NaiveDate::from_ymd_opt(2026, 11, 12).unwrap();
        let end = start + Duration::days(2);
        let mut draft = TodoDraft::new("period", start);
        draft.end_date = Some(end);
        let id = store.add(&draft).unwrap();
        let primary = dir.join(format!("{start}_schedule.json"));
        let backup = dir.join("backup.json");
        let later = dir.join(format!("{end}_schedule.json"));
        let later_backup = dir.join("later-backup.json");
        store.save_date_json_to(start, primary.clone(), backup.clone()).unwrap();
        store.save_date_json_to(end, later.clone(), later_backup).unwrap();
        assert!(!later.exists());
        let data: DailyScheduleFile = serde_json::from_slice(&std::fs::read(&primary).unwrap()).unwrap();
        assert_eq!(data.todos.len(), 1);
        assert_eq!(data.todos[0].end_date, Some(Some(end)));
        let imported = Store::open_in_memory().unwrap();
        imported.import_date_json_file(&primary).unwrap();
        assert_eq!(imported.query(&TodoQuery::default()).unwrap()[0].end_date, Some(end));
        store.delete(id).unwrap();
        store.save_date_json_to(start, primary.clone(), backup.clone()).unwrap();
        assert!(!primary.exists());
        assert!(!backup.exists());
        std::fs::remove_dir(dir).unwrap();
    }

    #[test]
    fn status_parse() {
        assert_eq!(
            Status::parse_loose("조건부 완료"),
            Some(Status::ConditionalDone)
        );
        assert_eq!(Status::parse_loose("in_progress"), Some(Status::InProgress));
        assert_eq!(Status::parse_loose("??"), None);
    }

    #[test]
    fn newest_schedule_replica_is_the_only_one_imported() {
        let root = std::env::temp_dir().join(format!("openschedule-replica-test-{}", std::process::id()));
        let primary = root.join("primary");
        let backup = root.join("backup");
        std::fs::create_dir_all(&primary).unwrap();
        std::fs::create_dir_all(&backup).unwrap();
        let filename = "2026-01-01_schedule.json";
        let store = Store::open_in_memory().unwrap();
        let older = backup.join(filename);
        let newer = primary.join(filename);
        std::fs::write(&older, r#"[{"title":"old"}]"#).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(100));
        std::fs::write(&newer, r#"[{"title":"new"}]"#).unwrap();
        let paths = latest_schedule_files(&[primary.clone(), backup.clone()]).unwrap();
        assert_eq!(paths, vec![newer.clone()]);
        for path in paths { store.import_date_json_file(&path).unwrap(); }
        let todos = store.query(&TodoQuery::default()).unwrap();
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].title, "new");
        std::fs::remove_file(older).unwrap();
        std::fs::remove_file(newer).unwrap();
        std::fs::remove_dir(primary).unwrap();
        std::fs::remove_dir(backup).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
}
