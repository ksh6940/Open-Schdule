use chrono::{DateTime, Local, NaiveDate, NaiveDateTime, NaiveTime};
use serde::{Deserialize, Serialize};

/// 할 일 상태.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// 진행예정
    #[default]
    Planned,
    /// 진행중
    InProgress,
    /// 완료
    Done,
    /// 조건부완료
    ConditionalDone,
}

impl Status {
    pub fn current(self) -> Status {
        if self == Status::ConditionalDone { Status::Done } else { self }
    }

    pub const ALL: [Status; 4] = [
        Status::Planned,
        Status::InProgress,
        Status::Done,
        Status::ConditionalDone,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Status::Planned => "진행예정",
            Status::InProgress => "진행중",
            Status::Done => "완료",
            Status::ConditionalDone => "조건부완료",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Status::Planned => "○",
            Status::InProgress => "◐",
            Status::Done => "●",
            Status::ConditionalDone => "◑",
        }
    }

    pub fn as_db(self) -> &'static str {
        match self {
            Status::Planned => "planned",
            Status::InProgress => "in_progress",
            Status::Done => "done",
            Status::ConditionalDone => "conditional_done",
        }
    }

    pub fn from_db(s: &str) -> Status {
        match s {
            "in_progress" => Status::InProgress,
            "done" => Status::Done,
            "conditional_done" => Status::ConditionalDone,
            _ => Status::Planned,
        }
    }

    /// 한국어/영어 등 느슨한 문자열을 상태로 해석 (AI/Discord 입력용).
    pub fn parse_loose(s: &str) -> Option<Status> {
        let s = s.trim().to_lowercase().replace([' ', '_', '-'], "");
        Some(match s.as_str() {
            "진행예정" | "예정" | "planned" | "todo" | "plan" => Status::Planned,
            "진행중" | "진행" | "inprogress" | "doing" | "progress" => Status::InProgress,
            "완료" | "done" | "complete" | "completed" | "finish" => Status::Done,
            "조건부완료" | "조건부" | "conditionaldone" | "conditional" => Status::ConditionalDone,
            _ => return None,
        })
    }

    /// 오버레이에서 클릭 시 순환 순서.
    pub fn next(self) -> Status {
        match self {
            Status::Planned => Status::InProgress,
            Status::InProgress => Status::Done,
            Status::Done => Status::Planned,
            Status::ConditionalDone => Status::Planned,
        }
    }

    pub fn is_finished(self) -> bool {
        matches!(self, Status::Done | Status::ConditionalDone)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Category {
    pub id: i64,
    pub name: String,
    /// 0xRRGGBB
    pub color: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Subtask {
    pub title: String,
    #[serde(default)]
    pub done: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Todo {
    pub id: i64,
    pub title: String,
    pub memo: String,
    pub category_id: Option<i64>,
    pub due_date: NaiveDate,
    #[serde(default)]
    pub end_date: Option<NaiveDate>,
    pub due_time: Option<NaiveTime>,
    #[serde(default)]
    pub reminder_at: Option<NaiveDateTime>,
    #[serde(default)]
    pub subtasks: Vec<Subtask>,
    #[serde(default)]
    pub priority: u8,
    pub status: Status,
    /// 조건부완료일 때의 조건 메모.
    pub condition_note: String,
    pub created_at: DateTime<Local>,
    pub updated_at: DateTime<Local>,
}

impl Todo {
    pub fn is_overdue(&self, today: NaiveDate) -> bool {
        !self.status.is_finished() && self.end_date.unwrap_or(self.due_date) < today
    }

    pub fn to_draft(&self) -> TodoDraft {
        TodoDraft {
            title: self.title.clone(),
            memo: self.memo.clone(),
            category_id: self.category_id,
            due_date: self.due_date,
            end_date: self.end_date,
            due_time: self.due_time,
            reminder_at: self.reminder_at,
            subtasks: self.subtasks.clone(),
            priority: self.priority,
            status: self.status,
            condition_note: self.condition_note.clone(),
        }
    }
}

/// 생성/수정용 입력값.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TodoDraft {
    pub title: String,
    pub memo: String,
    pub category_id: Option<i64>,
    pub due_date: NaiveDate,
    #[serde(default)]
    pub end_date: Option<NaiveDate>,
    pub due_time: Option<NaiveTime>,
    #[serde(default)]
    pub reminder_at: Option<NaiveDateTime>,
    #[serde(default)]
    pub subtasks: Vec<Subtask>,
    #[serde(default)]
    pub priority: u8,
    pub status: Status,
    pub condition_note: String,
}

impl TodoDraft {
    pub fn new(title: impl Into<String>, due_date: NaiveDate) -> Self {
        Self {
            title: title.into(),
            memo: String::new(),
            category_id: None,
            due_date,
            end_date: None,
            due_time: None,
            reminder_at: None,
            subtasks: Vec::new(),
            priority: 0,
            status: Status::Planned,
            condition_note: String::new(),
        }
    }
}
