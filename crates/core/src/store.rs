use std::path::Path;

use anyhow::{Context, Result};
use chrono::{Datelike, Duration, Local, NaiveDate, NaiveTime};
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
        Self { date: DateFilter::All, category: None, statuses: vec![], search: String::new() }
    }
}

pub struct Store {
    conn: Connection,
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
        Self::open(&crate::paths::db_path())
    }

    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path).with_context(|| format!("DB 열기 실패: {}", path.display()))?;
        Self::init(conn)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.execute_batch(SCHEMA)?;
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM categories", [], |r| r.get(0))?;
        if count == 0 {
            for (name, color) in DEFAULT_CATEGORIES {
                conn.execute("INSERT INTO categories(name, color) VALUES (?1, ?2)", params![name, color])?;
            }
        }
        Ok(Self { conn })
    }

    /// 다른 커넥션(다른 앱)이 DB를 변경하면 값이 바뀜.
    pub fn data_version(&self) -> i64 {
        self.conn.query_row("PRAGMA data_version", [], |r| r.get(0)).unwrap_or(0)
    }

    // ───────────── categories ─────────────

    pub fn categories(&self) -> Result<Vec<Category>> {
        let mut st = self.conn.prepare("SELECT id, name, color FROM categories ORDER BY id")?;
        let rows = st.query_map([], |r| Ok(Category { id: r.get(0)?, name: r.get(1)?, color: r.get(2)? }))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn add_category(&self, name: &str, color: u32) -> Result<i64> {
        self.conn.execute("INSERT INTO categories(name, color) VALUES (?1, ?2)", params![name.trim(), color])?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn update_category(&self, id: i64, name: &str, color: u32) -> Result<()> {
        self.conn.execute("UPDATE categories SET name=?1, color=?2 WHERE id=?3", params![name.trim(), color, id])?;
        Ok(())
    }

    pub fn delete_category(&self, id: i64) -> Result<()> {
        self.conn.execute("UPDATE todos SET category_id=NULL WHERE category_id=?1", params![id])?;
        self.conn.execute("DELETE FROM categories WHERE id=?1", params![id])?;
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
            due_date: NaiveDate::parse_from_str(&due_date, "%Y-%m-%d").unwrap_or_else(|_| Local::now().date_naive()),
            due_time: due_time.and_then(|t| NaiveTime::parse_from_str(&t, "%H:%M").ok()),
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
        Ok(self.conn.query_row(&sql, params![id], Self::row_to_todo).optional()?)
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
        Ok(self.conn.last_insert_rowid())
    }

    pub fn update(&self, id: i64, d: &TodoDraft) -> Result<()> {
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
        Ok(())
    }

    pub fn set_status(&self, id: i64, status: Status) -> Result<()> {
        self.conn.execute(
            "UPDATE todos SET status=?1, updated_at=?2 WHERE id=?3",
            params![status.as_db(), Local::now().to_rfc3339(), id],
        )?;
        Ok(())
    }

    pub fn delete(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM todos WHERE id=?1", params![id])?;
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

    /// AI 컨텍스트용 간단한 텍스트 요약.
    pub fn context_text(&self, filter: DateFilter, limit: usize) -> Result<String> {
        let cats = self.categories()?;
        let todos = self.query(&TodoQuery { date: filter, ..Default::default() })?;
        let mut out = String::new();
        for t in todos.iter().take(limit) {
            let cat = t
                .category_id
                .and_then(|id| cats.iter().find(|c| c.id == id))
                .map(|c| c.name.as_str())
                .unwrap_or("미분류");
            let time = t.due_time.map(|t| t.format(" %H:%M").to_string()).unwrap_or_default();
            out += &format!(
                "- [#{}] {} | {}{} | {} | {}{}\n",
                t.id,
                t.title,
                t.due_date.format("%Y-%m-%d"),
                time,
                cat,
                t.status.label(),
                if t.condition_note.is_empty() { String::new() } else { format!(" (조건: {})", t.condition_note) }
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
    pub fn category_counts(&self) -> Result<std::collections::HashMap<i64, usize>> {
        let mut map = std::collections::HashMap::new();
        let mut st = self.conn.prepare(
            "SELECT category_id, COUNT(*) FROM todos WHERE category_id IS NOT NULL AND status NOT IN ('done', 'conditional_done') GROUP BY category_id",
        )?;
        let rows = st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)? as usize)))?;
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

        let q = TodoQuery { date: DateFilter::TodayAndOverdue, ..Default::default() };
        assert_eq!(s.query(&q).unwrap().len(), 2);

        s.set_status(old_id, Status::ConditionalDone).unwrap();
        assert_eq!(s.query(&q).unwrap().len(), 1);

        let q = TodoQuery { category: Some(Some(cats[0].id)), ..Default::default() };
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
        assert_eq!(Status::parse_loose("조건부 완료"), Some(Status::ConditionalDone));
        assert_eq!(Status::parse_loose("in_progress"), Some(Status::InProgress));
        assert_eq!(Status::parse_loose("??"), None);
    }
}
