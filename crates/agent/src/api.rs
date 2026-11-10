//! 에이전트 로컬 HTTP API (127.0.0.1 루프백).
//! Electron 프론트엔드와 연동하여 투두 CRUD, 카테고리, 스마트 카운트, JSON 동기화, 설정 제공.

use std::collections::HashMap;

use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap, Method, StatusCode},
    middleware::{self, Next},
    response::Response,
    routing::{get, post, put},
    Json, Router,
};
use chrono::{Local, NaiveDate, NaiveTime};
use openschedule_core::{
    model::{Category, Status, Subtask, Todo, TodoDraft},
    AgentInfo, DateFilter, Settings, SmartCounts, Store, TodoQuery,
};
use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct ApiState {
    pub token: String,
}

pub async fn start_api_server() -> anyhow::Result<(u16, String, tokio::task::JoinHandle<()>)> {
    use rand::RngCore;
    let mut tok_bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut tok_bytes);
    let token = hex::encode(tok_bytes);

    let state = ApiState {
        token: token.clone(),
    };

    let app = Router::new()
        .route("/health", get(health_handler))
        .route("/api/capabilities", get(capabilities_handler))
        .route("/api/sync", post(sync_handler))
        .route("/api/overlay", get(overlay_handler))
        .route("/api/dashboard", get(dashboard_handler))
        // 투두 CRUD
        .route("/api/todos", get(list_todos_handler).post(create_todo_handler))
        .route(
            "/api/todos/:id",
            get(get_todo_handler)
                .put(update_todo_handler)
                .delete(delete_todo_handler),
        )
        .route("/api/todos/:id/status", post(toggle_status_handler))
        // 카테고리 CRUD & 카운트
        .route(
            "/api/categories",
            get(list_categories_handler).post(create_category_handler),
        )
        .route(
            "/api/categories/:id",
            put(update_category_handler).delete(delete_category_handler),
        )
        .route("/api/smart_counts", get(smart_counts_handler))
        .route("/api/smart-counts", get(smart_counts_handler))
        .route("/api/category_counts", get(category_counts_handler))
        .route("/api/category-counts", get(category_counts_handler))
        // 설정
        .route(
            "/api/settings",
            get(get_settings_handler).put(save_settings_handler),
        )
        .layer(middleware::from_fn(cors_middleware))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();

    let info = AgentInfo {
        port,
        token: token.clone(),
        pid: std::process::id(),
        schedules_dir: Some(openschedule_core::paths::schedules_dir()),
    };
    info.save()?;
    log::info!(
        "OpenSchedule Agent API running at http://127.0.0.1:{port} (PID {})",
        info.pid
    );

    let handle = tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            log::error!("API server error: {e}");
        }
    });

    Ok((port, token, handle))
}

async fn cors_middleware(req: axum::extract::Request, next: Next) -> Response {
    if req.method() == Method::OPTIONS {
        return Response::builder()
            .status(StatusCode::NO_CONTENT)
            .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
            .header(
                header::ACCESS_CONTROL_ALLOW_METHODS,
                "GET, POST, PUT, DELETE, OPTIONS",
            )
            .header(header::ACCESS_CONTROL_ALLOW_HEADERS, "*")
            .body(axum::body::Body::empty())
            .unwrap();
    }

    let mut res = next.run(req).await;
    let headers = res.headers_mut();
    headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*".parse().unwrap());
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        "GET, POST, PUT, DELETE, OPTIONS".parse().unwrap(),
    );
    headers.insert(header::ACCESS_CONTROL_ALLOW_HEADERS, "*".parse().unwrap());
    res
}

fn auth_check(headers: &HeaderMap, state: &ApiState) -> Result<(), (StatusCode, &'static str)> {
    let auth = headers
        .get("Authorization")
        .and_then(|h| h.to_str().ok())
        .unwrap_or_default();
    let expected = format!("Bearer {}", state.token);
    if auth == expected || auth == state.token {
        Ok(())
    } else {
        Err((StatusCode::UNAUTHORIZED, "Unauthorized"))
    }
}

async fn health_handler() -> &'static str {
    "ok"
}

async fn capabilities_handler(
    State(state): State<ApiState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, &'static str)> {
    auth_check(&headers, &state)?;
    Ok(Json(serde_json::json!({ "reminders_and_subtasks": true, "priority": true })))
}

#[derive(Serialize)]
struct OverlayData {
    todos: Vec<Todo>,
    categories: Vec<Category>,
}

#[derive(Serialize)]
struct DashboardData {
    smart_counts: SmartCounts,
    category_counts: HashMap<i64, usize>,
    categories: Vec<Category>,
    todos: Vec<Todo>,
    filtered_todos: Vec<Todo>,
}

fn dashboard_data(store: &Store, filter: &TodoQuery) -> anyhow::Result<DashboardData> {
    Ok(DashboardData {
        smart_counts: store.smart_counts()?,
        category_counts: store.category_counts()?,
        categories: store.categories()?,
        todos: store.query(&TodoQuery::default())?,
        filtered_todos: store.query(filter)?,
    })
}

async fn dashboard_handler(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Query(params): Query<TodoQueryParams>,
) -> Result<Json<DashboardData>, (StatusCode, String)> {
    auth_check(&headers, &state).map_err(|(sc, msg)| (sc, msg.to_string()))?;
    let data = tokio::task::spawn_blocking(move || {
        let store = Store::open_default()?;
        dashboard_data(&store, &todo_query(params))
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(data))
}

fn overlay_data(store: &Store) -> anyhow::Result<OverlayData> {
    Ok(OverlayData {
        todos: store.query(&TodoQuery {
            date: DateFilter::TodayAndOverdue,
            ..Default::default()
        })?,
        categories: store.categories()?,
    })
}

async fn overlay_handler(
    State(state): State<ApiState>,
    headers: HeaderMap,
) -> Result<Json<OverlayData>, (StatusCode, String)> {
    auth_check(&headers, &state).map_err(|(sc, msg)| (sc, msg.to_string()))?;
    let data = tokio::task::spawn_blocking(|| {
        let store = Store::open_default()?;
        overlay_data(&store)
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(data))
}

async fn sync_handler(
    State(state): State<ApiState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    auth_check(&headers, &state).map_err(|(sc, msg)| (sc, msg.to_string()))?;
    tokio::task::spawn_blocking(Store::open_default)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(serde_json::json!({ "status": "ok" })))
}

// ──────────────── 투두 REST API ────────────────

#[derive(Deserialize)]
pub struct TodoQueryParams {
    pub date: Option<String>,
    pub category_id: Option<i64>,
    pub status: Option<String>,
    pub search: Option<String>,
}

/// 프론트엔드의 `<input type="date">` / `<input type="time">` ("14:00" 또는 "")을 모두 허용하는 페이로드
#[derive(Deserialize)]
pub struct TodoPayload {
    pub title: String,
    #[serde(default)]
    pub memo: Option<String>,
    pub category_id: Option<i64>,
    pub due_date: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_end_date")]
    pub end_date: Option<Option<String>>,
    pub due_time: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_end_date")]
    pub reminder_at: Option<Option<String>>,
    #[serde(default)]
    pub subtasks: Option<Vec<Subtask>>,
    #[serde(default)]
    pub priority: Option<u8>,
    pub status: Option<Status>,
    #[serde(default)]
    pub condition_note: Option<String>,
}

fn deserialize_optional_end_date<'de, D>(deserializer: D) -> Result<Option<Option<String>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Some(Option::<String>::deserialize(deserializer)?))
}

impl TodoPayload {
    fn into_draft(self, existing: Option<&Todo>) -> Result<TodoDraft, String> {
        let due_date = match self.due_date.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            Some(value) => NaiveDate::parse_from_str(value, "%Y-%m-%d")
                .map_err(|_| "시작일 형식이 올바르지 않습니다.".to_string())?,
            None => Local::now().date_naive(),
        };

        let due_time = match self.due_time.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            Some(value) => Some(
                NaiveTime::parse_from_str(value, "%H:%M")
                    .or_else(|_| NaiveTime::parse_from_str(value, "%H:%M:%S"))
                    .map_err(|_| "일정 시간 형식이 올바르지 않습니다.".to_string())?
            ),
            None => None,
        };

        let end_date = match self.end_date {
            Some(Some(value)) if !value.trim().is_empty() => Some(
                NaiveDate::parse_from_str(value.trim(), "%Y-%m-%d")
                    .map_err(|_| "종료일 형식이 올바르지 않습니다.".to_string())?
            ),
            Some(_) => None,
            None => match existing {
                Some(todo) => match todo.end_date {
                    Some(end) => Some(end.checked_add_signed(due_date - todo.due_date)
                        .ok_or_else(|| "일정 종료일 범위를 벗어났습니다.".to_string())?),
                    None => None,
                },
                None => None,
            },
        };
        if end_date.is_some_and(|end| end < due_date) {
            return Err("종료일은 시작일보다 빠를 수 없습니다.".to_string());
        }

        let reminder_at = match self.reminder_at {
            Some(Some(value)) if !value.trim().is_empty() =>
                return Err("일정 알림 기능은 더 이상 지원되지 않습니다.".to_string()),
            Some(_) => None,
            None => existing.and_then(|todo| todo.reminder_at),
        };
        let subtasks = self.subtasks
            .unwrap_or_else(|| existing.map(|todo| todo.subtasks.clone()).unwrap_or_default());
        if subtasks.iter().any(|item| item.title.trim().is_empty()) {
            return Err("세부 할 일 제목을 입력해주세요.".to_string());
        }
        let priority = self.priority.unwrap_or_else(|| existing.map_or(0, |todo| todo.priority));
        if priority > 3 {
            return Err("긴급도는 0부터 3까지 설정할 수 있습니다.".to_string());
        }

        Ok(TodoDraft {
            title: self.title,
            memo: self.memo.unwrap_or_default(),
            category_id: self.category_id,
            due_date,
            end_date,
            due_time,
            reminder_at,
            subtasks,
            priority,
            status: self.status.unwrap_or(Status::Planned),
            condition_note: self.condition_note.unwrap_or_default(),
        })
    }
}

async fn list_todos_handler(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Query(params): Query<TodoQueryParams>,
) -> Result<Json<Vec<Todo>>, (StatusCode, String)> {
    auth_check(&headers, &state).map_err(|(sc, msg)| (sc, msg.to_string()))?;
    let store =
        Store::open_default().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let q = todo_query(params);
    let todos = store
        .query(&q)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(todos))
}

fn todo_query(params: TodoQueryParams) -> TodoQuery {
    let date_filter = match params.date.as_deref() {
        Some("today") => DateFilter::Today,
        Some("scheduled") => DateFilter::Scheduled,
        Some("completed") => DateFilter::Completed,
        Some("today_and_overdue") => DateFilter::TodayAndOverdue,
        Some("this_week") => DateFilter::ThisWeek,
        Some("all") | None => DateFilter::All,
        Some(other) => {
            if let Ok(d) = NaiveDate::parse_from_str(other.trim(), "%Y-%m-%d") {
                DateFilter::Day(d)
            } else {
                DateFilter::All
            }
        }
    };

    let statuses = params
        .status
        .and_then(|s| Status::parse_loose(&s))
        .map(|s| vec![s])
        .unwrap_or_default();

    TodoQuery {
        date: date_filter,
        category: params.category_id.map(Some),
        statuses,
        search: params.search.unwrap_or_default(),
    }
}

async fn create_todo_handler(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Json(payload): Json<TodoPayload>,
) -> Result<Json<Todo>, (StatusCode, String)> {
    auth_check(&headers, &state).map_err(|(sc, msg)| (sc, msg.to_string()))?;
    let draft = payload.into_draft(None).map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    let store =
        Store::open_default().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let id = store
        .add(&draft)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let todo = store
        .get(id)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Todo not found".into()))?;
    Ok(Json(todo))
}

async fn get_todo_handler(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> Result<Json<Todo>, (StatusCode, String)> {
    auth_check(&headers, &state).map_err(|(sc, msg)| (sc, msg.to_string()))?;
    let store =
        Store::open_default().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let todo = store
        .get(id)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Todo not found".into()))?;
    Ok(Json(todo))
}

async fn update_todo_handler(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(payload): Json<TodoPayload>,
) -> Result<Json<Todo>, (StatusCode, String)> {
    auth_check(&headers, &state).map_err(|(sc, msg)| (sc, msg.to_string()))?;
    let store =
        Store::open_default().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let existing = store.get(id)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Todo not found".into()))?;
    let draft = payload.into_draft(Some(&existing)).map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    store
        .update(id, &draft)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let todo = store
        .get(id)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Todo not found".into()))?;
    Ok(Json(todo))
}

async fn delete_todo_handler(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> Result<StatusCode, (StatusCode, String)> {
    auth_check(&headers, &state).map_err(|(sc, msg)| (sc, msg.to_string()))?;
    let store =
        Store::open_default().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    store
        .delete(id)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct StatusUpdatePayload {
    pub status: Option<String>,
}

async fn toggle_status_handler(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(payload): Json<StatusUpdatePayload>,
) -> Result<Json<Todo>, (StatusCode, String)> {
    auth_check(&headers, &state).map_err(|(sc, msg)| (sc, msg.to_string()))?;
    let store =
        Store::open_default().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let cur = store
        .get(id)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Todo not found".into()))?;

    let next = match payload.status.as_deref() {
        Some(value) => Status::parse_loose(value)
            .ok_or((StatusCode::BAD_REQUEST, "알 수 없는 상태입니다.".into()))?.current(),
        None => cur.status.next(),
    };

    store
        .set_status(id, next)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let updated = store
        .get(id)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "Todo not found".into()))?;
    Ok(Json(updated))
}

// ──────────────── 카테고리 REST API ────────────────

#[derive(Deserialize)]
pub struct CategoryPayload {
    pub name: String,
    pub color: Option<u32>,
}

async fn list_categories_handler(
    State(state): State<ApiState>,
    headers: HeaderMap,
) -> Result<Json<Vec<Category>>, (StatusCode, String)> {
    auth_check(&headers, &state).map_err(|(sc, msg)| (sc, msg.to_string()))?;
    let store =
        Store::open_default().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let cats = store
        .categories()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(cats))
}

async fn create_category_handler(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Json(payload): Json<CategoryPayload>,
) -> Result<Json<Category>, (StatusCode, String)> {
    auth_check(&headers, &state).map_err(|(sc, msg)| (sc, msg.to_string()))?;
    let store =
        Store::open_default().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let color = payload.color.unwrap_or(0x0071E3);
    let id = store
        .add_category(&payload.name, color)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(Category {
        id,
        name: payload.name,
        color,
    }))
}

async fn update_category_handler(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(payload): Json<CategoryPayload>,
) -> Result<StatusCode, (StatusCode, String)> {
    auth_check(&headers, &state).map_err(|(sc, msg)| (sc, msg.to_string()))?;
    let store =
        Store::open_default().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let color = payload.color.unwrap_or(0x0071E3);
    store
        .update_category(id, &payload.name, color)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(StatusCode::OK)
}

async fn delete_category_handler(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> Result<StatusCode, (StatusCode, String)> {
    auth_check(&headers, &state).map_err(|(sc, msg)| (sc, msg.to_string()))?;
    let store =
        Store::open_default().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    store
        .delete_category(id)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}

async fn smart_counts_handler(
    State(state): State<ApiState>,
    headers: HeaderMap,
) -> Result<Json<SmartCounts>, (StatusCode, String)> {
    auth_check(&headers, &state).map_err(|(sc, msg)| (sc, msg.to_string()))?;
    let store =
        Store::open_default().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let counts = store
        .smart_counts()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(counts))
}

async fn category_counts_handler(
    State(state): State<ApiState>,
    headers: HeaderMap,
) -> Result<Json<HashMap<i64, usize>>, (StatusCode, String)> {
    auth_check(&headers, &state).map_err(|(sc, msg)| (sc, msg.to_string()))?;
    let store =
        Store::open_default().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let counts = store
        .category_counts()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(counts))
}

// ──────────────── 설정 REST API ────────────────

async fn get_settings_handler(
    State(state): State<ApiState>,
    headers: HeaderMap,
) -> Result<Json<Settings>, (StatusCode, String)> {
    auth_check(&headers, &state).map_err(|(sc, msg)| (sc, msg.to_string()))?;
    Ok(Json(Settings::load()))
}

async fn save_settings_handler(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Json(settings): Json<Settings>,
) -> Result<StatusCode, (StatusCode, String)> {
    auth_check(&headers, &state).map_err(|(sc, msg)| (sc, msg.to_string()))?;
    settings
        .save()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(StatusCode::OK)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn compose_payload_persists_time_memo_subtasks_and_priority() {
        let store = Store::open_in_memory().unwrap();
        let payload: TodoPayload = serde_json::from_value(serde_json::json!({
            "title": "task", "due_date": "2030-01-01", "end_date": "2030-01-03",
            "due_time": "14:30", "reminder_at": null, "memo": "first\nsecond",
            "subtasks": [{ "title": "step", "done": true }], "priority": 2
        })).unwrap();
        let draft = payload.into_draft(None).unwrap();
        let id = store.add(&draft).unwrap();
        assert_eq!(store.get(id).unwrap().unwrap().to_draft(), draft);
        assert_eq!(draft.due_time, NaiveTime::from_hms_opt(14, 30, 0));
        assert_eq!(draft.reminder_at, None);
        let invalid: TodoPayload = serde_json::from_value(serde_json::json!({
            "title": "task", "due_time": "25:61"
        })).unwrap();
        assert!(invalid.into_draft(None).is_err());
    }

    #[test]
    fn reminder_and_subtasks_validate_and_preserve_legacy_updates() {
        let store = Store::open_in_memory().unwrap();
        let date = Local::now().date_naive();
        let mut draft = TodoDraft::new("task", date);
        draft.reminder_at = date.and_hms_opt(9, 0, 0);
        draft.subtasks = vec![Subtask { title: "step".into(), done: true }];
        draft.priority = 3;
        let id = store.add(&draft).unwrap();
        let todo = store.get(id).unwrap().unwrap();
        let payload: TodoPayload = serde_json::from_value(serde_json::json!({ "title": "task" })).unwrap();
        let updated = payload.into_draft(Some(&todo)).unwrap();
        assert_eq!(updated.reminder_at, draft.reminder_at);
        assert_eq!(updated.subtasks, draft.subtasks);
        assert_eq!(updated.priority, 3);
        let payload: TodoPayload = serde_json::from_value(serde_json::json!({
            "title": "task", "reminder_at": null, "subtasks": []
        })).unwrap();
        let cleared = payload.into_draft(Some(&todo)).unwrap();
        assert_eq!(cleared.reminder_at, None);
        assert!(cleared.subtasks.is_empty());
        for value in [serde_json::json!({ "reminder_at": "bad" }),
                      serde_json::json!({ "reminder_at": "2030-01-01T10:00" }),
                      serde_json::json!({ "subtasks": [{ "title": " " }] }),
                      serde_json::json!({ "priority": 4 })] {
            let mut value = value;
            value["title"] = serde_json::json!("task");
            let payload: TodoPayload = serde_json::from_value(value).unwrap();
            assert!(payload.into_draft(None).is_err());
        }
    }

    #[test]
    fn payload_validates_periods_and_preserves_legacy_updates() {
        let store = Store::open_in_memory().unwrap();
        let today = Local::now().date_naive();
        let mut draft = TodoDraft::new("period", today);
        draft.end_date = Some(today + Duration::days(2));
        let id = store.add(&draft).unwrap();
        let todo = store.get(id).unwrap().unwrap();
        let next = today + Duration::days(1);
        let payload: TodoPayload = serde_json::from_value(serde_json::json!({
            "title": "edited", "due_date": next.to_string()
        })).unwrap();
        assert_eq!(payload.into_draft(Some(&todo)).unwrap().end_date, Some(next + Duration::days(2)));
        let payload: TodoPayload = serde_json::from_value(serde_json::json!({
            "title": "edited", "due_date": today.to_string(), "end_date": null
        })).unwrap();
        assert_eq!(payload.into_draft(Some(&todo)).unwrap().end_date, None);
        let payload: TodoPayload = serde_json::from_value(serde_json::json!({
            "title": "bad", "due_date": today.to_string(), "end_date": (today - Duration::days(1)).to_string()
        })).unwrap();
        assert!(payload.into_draft(None).is_err());
    }

    #[test]
    fn overlay_data_matches_today_and_overdue_and_includes_categories() {
        let store = Store::open_in_memory().unwrap();
        let today = Local::now().date_naive();
        store.add(&TodoDraft::new("today", today)).unwrap();
        store.add(&TodoDraft::new("overdue", today - Duration::days(1))).unwrap();
        store.add(&TodoDraft::new("future", today + Duration::days(1))).unwrap();
        let data = overlay_data(&store).unwrap();
        assert_eq!(data.todos.len(), 2);
        assert_eq!(data.categories, store.categories().unwrap());
        let json = serde_json::to_value(data).unwrap();
        assert!(json["todos"].is_array());
        assert!(json["categories"].is_array());
    }

    #[tokio::test]
    async fn overlay_endpoint_requires_authentication() {
        let result = overlay_handler(
            State(ApiState { token: "test".into() }),
            HeaderMap::new(),
        ).await;
        assert!(matches!(result, Err((StatusCode::UNAUTHORIZED, _))));
    }

    #[test]
    fn dashboard_data_matches_individual_queries() {
        let store = Store::open_in_memory().unwrap();
        store.add(&TodoDraft::new("test", Local::now().date_naive())).unwrap();
        let data = dashboard_data(&store, &TodoQuery::default()).unwrap();
        assert_eq!(data.smart_counts, store.smart_counts().unwrap());
        assert_eq!(data.category_counts, store.category_counts().unwrap());
        assert_eq!(data.categories, store.categories().unwrap());
        assert_eq!(data.todos, store.query(&TodoQuery::default()).unwrap());
        let query = todo_query(TodoQueryParams {
            date: Some("completed".into()), category_id: None, status: None, search: None,
        });
        let filtered = dashboard_data(&store, &query).unwrap();
        assert_eq!(filtered.filtered_todos, store.query(&query).unwrap());
        assert_eq!(filtered.filtered_todos.len(), 0);
        assert_eq!(filtered.todos.len(), 1);
    }
}
