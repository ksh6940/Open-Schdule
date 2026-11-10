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
    model::{Category, Status, Todo, TodoDraft},
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
        .route("/api/sync", post(sync_handler))
        .route("/api/overlay", get(overlay_handler))
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

#[derive(Serialize)]
struct OverlayData {
    todos: Vec<Todo>,
    categories: Vec<Category>,
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
    let store =
        Store::open_default().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    store
        .sync_all_json_files()
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
    pub due_time: Option<String>,
    pub status: Option<Status>,
    #[serde(default)]
    pub condition_note: Option<String>,
}

impl TodoPayload {
    fn into_draft(self) -> TodoDraft {
        let due_date = self
            .due_date
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
            .unwrap_or_else(|| Local::now().date_naive());

        let due_time = self
            .due_time
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .and_then(|s| {
                NaiveTime::parse_from_str(s, "%H:%M")
                    .or_else(|_| NaiveTime::parse_from_str(s, "%H:%M:%S"))
                    .ok()
            });

        TodoDraft {
            title: self.title,
            memo: self.memo.unwrap_or_default(),
            category_id: self.category_id,
            due_date,
            due_time,
            status: self.status.unwrap_or(Status::Planned),
            condition_note: self.condition_note.unwrap_or_default(),
        }
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

    let q = TodoQuery {
        date: date_filter,
        category: params.category_id.map(Some),
        statuses,
        search: params.search.unwrap_or_default(),
    };

    let todos = store
        .query(&q)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(todos))
}

async fn create_todo_handler(
    State(state): State<ApiState>,
    headers: HeaderMap,
    Json(payload): Json<TodoPayload>,
) -> Result<Json<Todo>, (StatusCode, String)> {
    auth_check(&headers, &state).map_err(|(sc, msg)| (sc, msg.to_string()))?;
    let draft = payload.into_draft();
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
    let draft = payload.into_draft();
    let store =
        Store::open_default().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
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

    let next = match payload.status.as_deref().and_then(Status::parse_loose) {
        Some(s) => s,
        None => {
            if cur.status == Status::Done {
                Status::InProgress
            } else {
                Status::Done
            }
        }
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
}
