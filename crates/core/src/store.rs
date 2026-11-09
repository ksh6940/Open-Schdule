use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, NaiveTime};
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use crate::model::{Category, Status, Todo, TodoDraft};

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

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    pub due_date: NaiveDate,
    #[serde(default)]
    pub due_time: Option<NaiveTime>,
    #[serde(default)]
    pub status: Status,
    #[serde(default)]
    pub condition_note: String,
    #[serde(default = "Local::now")]
    pub created_at: DateTime<Local>,
    #[serde(default = "Local::now")]
    pub updated_at: DateTime<Local>,
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
        let _ = store.export_all_dates_to_json();
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

        let todos = self.query(&TodoQuery {
            date: DateFilter::Day(date),
            ..Default::default()
        })?;
        let cats = self.categories()?;
        let appdata_path = crate::paths::appdata_date_schedule_json_path(date);

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
                    due_time: t.due_time,
                    status: t.status,
                    condition_note: t.condition_note,
                    created_at: t.created_at,
                    updated_at: t.updated_at,
                }
            })
            .collect();

        if entries.is_empty() {
            // 해당 날짜에 할 일이 모두 삭제된 경우 빈 JSON 파일도 정리
            let _ = std::fs::remove_file(&primary_path);
            if appdata_path != primary_path {
                let _ = std::fs::remove_file(&appdata_path);
            }
            return Ok(primary_path);
        }

        let file_data = DailyScheduleFile {
            date: date.format("%Y-%m-%d").to_string(),
            updated_at: Local::now().to_rfc3339(),
            count: entries.len(),
            todos: entries,
        };

        let json_str = serde_json::to_string_pretty(&file_data)?;
        std::fs::write(&primary_path, &json_str)?;
        if appdata_path != primary_path {
            let _ = std::fs::write(&appdata_path, &json_str);
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

        let entries: Vec<TodoJsonEntry> =
            if let Ok(daily) = serde_json::from_str::<DailyScheduleFile>(&raw) {
                daily.todos
            } else {
                serde_json::from_str::<Vec<TodoJsonEntry>>(&raw)
                    .with_context(|| format!("JSON 파싱 실패: {}", path.display()))?
            };

        let mut imported = 0;
        for entry in entries {
            if entry.title.trim().is_empty() {
                continue;
            }
            let cat_id = if let Some(cid) = entry.category_id {
                Some(cid)
            } else if !entry.category_name.trim().is_empty() && entry.category_name != "미분류" {
                self.find_category(&entry.category_name, true)?
            } else {
                None
            };

            // 동일한 ID가 있거나 (같은 날짜 + 같은 제목)이 있는지 확인
            let exists_by_id = if entry.id > 0 {
                self.get(entry.id)?.is_some()
            } else {
                false
            };

            let draft = TodoDraft {
                title: entry.title.trim().to_string(),
                memo: entry.memo,
                category_id: cat_id,
                due_date: entry.due_date,
                due_time: entry.due_time,
                status: entry.status,
                condition_note: entry.condition_note,
            };

            if exists_by_id {
                self.conn.execute(
                    "UPDATE todos SET title=?1, memo=?2, category_id=?3, due_date=?4, due_time=?5, status=?6,
                         condition_note=?7, updated_at=?8 WHERE id=?9",
                    params![
                        draft.title,
                        draft.memo,
                        draft.category_id,
                        draft.due_date.format("%Y-%m-%d").to_string(),
                        draft.due_time.map(|t| t.format("%H:%M").to_string()),
                        draft.status.as_db(),
                        draft.condition_note,
                        entry.updated_at.to_rfc3339(),
                        entry.id
                    ],
                )?;
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
                        "INSERT INTO todos(title, memo, category_id, due_date, due_time, status, condition_note, created_at, updated_at)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
                        params![
                            draft.title,
                            draft.memo,
                            draft.category_id,
                            draft.due_date.format("%Y-%m-%d").to_string(),
                            draft.due_time.map(|t| t.format("%H:%M").to_string()),
                            draft.status.as_db(),
                            draft.condition_note,
                            now
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
        let dir = crate::paths::schedules_dir();
        let mut total_imported = 0;
        if let Ok(read_dir) = std::fs::read_dir(&dir) {
            for entry in read_dir.flatten() {
                let path = entry.path();
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.ends_with("_schedule.json") {
                        if let Ok(n) = self.import_date_json_file(&path) {
                            total_imported += n;
                        }
                    }
                }
            }
        }
        self.export_all_dates_to_json()?;
        Ok(total_imported)
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
        let _ = self.export_all_dates_to_json();
        Ok(())
    }

    pub fn delete_category(&self, id: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE todos SET category_id=NULL WHERE category_id=?1",
            params![id],
        )?;
        self.conn
            .execute("DELETE FROM categories WHERE id=?1", params![id])?;
        let _ = self.export_all_dates_to_json();
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
        Ok(Todo {
            id: r.get(0)?,
            title: r.get(1)?,
            memo: r.get(2)?,
            category_id: r.get(3)?,
            due_date: NaiveDate::parse_from_str(&due_date, "%Y-%m-%d")
                .unwrap_or_else(|_| Local::now().date_naive()),
            due_time: due_time.and_then(|t| {
                NaiveTime::parse_from_str(&t, "%H:%M")
                    .or_else(|_| NaiveTime::parse_from_str(&t, "%H:%M:%S"))
                    .ok()
            }),
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
        "id, title, memo, category_id, due_date, due_time, status, condition_note, created_at, updated_at";

    pub fn get(&self, id: i64) -> Result<Option<Todo>> {
        let sql = format!("SELECT {} FROM todos WHERE id=?1", Self::COLS);
        Ok(self
            .conn
            .query_row(&sql, params![id], Self::row_to_todo)
            .optional()?)
    }

    pub fn add(&self, d: &TodoDraft) -> Result<i64> {
        let now = Local::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO todos(title, memo, category_id, due_date, due_time, status, condition_note, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
            params![
                d.title.trim(),
                d.memo,
                d.category_id,
                d.due_date.format("%Y-%m-%d").to_string(),
                d.due_time.map(|t| t.format("%H:%M").to_string()),
                d.status.as_db(),
                d.condition_note,
                now
            ],
        )?;
        let id = self.conn.last_insert_rowid();
        let _ = self.save_date_json(d.due_date);
        Ok(id)
    }

    pub fn update(&self, id: i64, d: &TodoDraft) -> Result<()> {
        let old_date = self.get(id)?.map(|t| t.due_date);
        self.conn.execute(
            "UPDATE todos SET title=?1, memo=?2, category_id=?3, due_date=?4, due_time=?5, status=?6,
                 condition_note=?7, updated_at=?8 WHERE id=?9",
            params![
                d.title.trim(),
                d.memo,
                d.category_id,
                d.due_date.format("%Y-%m-%d").to_string(),
                d.due_time.map(|t| t.format("%H:%M").to_string()),
                d.status.as_db(),
                d.condition_note,
                Local::now().to_rfc3339(),
                id
            ],
        )?;
        let _ = self.save_date_json(d.due_date);
        if let Some(old_d) = old_date {
            if old_d != d.due_date {
                let _ = self.save_date_json(old_d);
            }
        }
        Ok(())
    }

    pub fn set_status(&self, id: i64, status: Status) -> Result<()> {
        let target_date = self.get(id)?.map(|t| t.due_date);
        self.conn.execute(
            "UPDATE todos SET status=?1, updated_at=?2 WHERE id=?3",
            params![status.as_db(), Local::now().to_rfc3339(), id],
        )?;
        if let Some(d) = target_date {
            let _ = self.save_date_json(d);
        }
        Ok(())
    }

    pub fn delete(&self, id: i64) -> Result<()> {
        let target_date = self.get(id)?.map(|t| t.due_date);
        self.conn
            .execute("DELETE FROM todos WHERE id=?1", params![id])?;
        if let Some(d) = target_date {
            let _ = self.save_date_json(d);
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
                sql += " AND due_date = ?";
                args.push(Box::new(fmt(today)));
            }
            DateFilter::TodayAndOverdue => {
                sql += " AND (due_date = ? OR (due_date < ? AND status NOT IN ('done','conditional_done')))";
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
                sql += " AND due_date BETWEEN ? AND ?";
                args.push(Box::new(fmt(start)));
                args.push(Box::new(fmt(end)));
            }
            DateFilter::Completed => {
                sql += " AND status IN ('done','conditional_done')";
            }
            DateFilter::Day(d) => {
                sql += " AND due_date = ?";
                args.push(Box::new(fmt(d)));
            }
            DateFilter::Range(a, b) => {
                sql += " AND due_date BETWEEN ? AND ?";
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
            map.entry(t.due_date).or_default().push(t);
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
            "SELECT COUNT(*) FROM todos WHERE (due_date = ?1 OR (due_date < ?1 AND status NOT IN ('done', 'conditional_done')))",
            params![today],
            |r| r.get(0),
        )?;
        let scheduled_count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM todos WHERE due_date > ?1 AND status NOT IN ('done', 'conditional_done')",
            params![today],
            |r| r.get(0),
        )?;
        let all_count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM todos WHERE status NOT IN ('done', 'conditional_done')",
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

    /// 각 카테고리별 미완료 할 일 카운트.
    pub fn category_counts(&self) -> Result<HashMap<i64, usize>> {
        let mut map = HashMap::new();
        let mut st = self.conn.prepare(
            "SELECT category_id, COUNT(*) FROM todos WHERE category_id IS NOT NULL AND status NOT IN ('done', 'conditional_done') GROUP BY category_id",
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
    fn status_parse() {
        assert_eq!(
            Status::parse_loose("조건부 완료"),
            Some(Status::ConditionalDone)
        );
        assert_eq!(Status::parse_loose("in_progress"), Some(Status::InProgress));
        assert_eq!(Status::parse_loose("??"), None);
    }
}
