//! OpenSchedule / OpenLab 메인 데스크톱 UI
//! - [투두리스트 / 캘린더 뷰] 모드 전환 지원
//! - 일자별 `{date}_schedule.json` 로컬 파일 저장 및 SQLite(`rusqlite`) 양방향 동기화
//! - 저장, 불러오기, 수정, 상태 변경, 삭제 완벽 지원

use std::collections::HashMap;
use std::time::{Duration, Instant};

use chrono::{Datelike, Duration as ChronoDuration, Local, NaiveDate};
use eframe::egui::{self, Color32, CornerRadius, Key, RichText, Stroke};
use openschedule_core::{
    cli::{parse_flexible_date, parse_flexible_time},
    model::{Category, Status, Todo, TodoDraft},
    paths,
    theme::{
        self, apple_button_danger, apple_button_outline, apple_button_primary, category_chip,
        circular_checkbox, icons, quick_chip_button, smart_card, status_pill,
        status_pill_selectable, BORDER, CANVAS, DANGER, FOREGROUND, MUTED, PRIMARY, PURPLE,
        SECONDARY, SUCCESS, SURFACE, WARNING,
    },
    DateFilter, Settings, SmartCounts, Store, TodoQuery,
};

/// 상단 메인 뷰 모드 ([투두리스트 / 캘린더 뷰 / 설정 & CLI])
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    TodoList,
    Calendar,
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SmartListFilter {
    Today,
    Scheduled,
    All,
    Completed,
}

const APPLE_PALETTE: &[(&str, u32)] = &[
    ("Blue", 0x0071E3),
    ("Green", 0x34C759),
    ("Orange", 0xFF9500),
    ("Red", 0xFF3B30),
    ("Purple", 0xAF52DE),
    ("Teal", 0x30B0C7),
];

pub struct MainView {
    pub view_mode: ViewMode,
    pub smart_filter: SmartListFilter,
    pub selected_category_id: Option<i64>,
    pub status_filter: Option<Status>,
    pub search_query: String,

    // 캘린더 뷰 상태
    pub cal_year: i32,
    pub cal_month: u32,
    pub cal_selected_date: NaiveDate,
    pub month_todos: HashMap<NaiveDate, Vec<Todo>>,
    pub cal_quick_title: String,
    pub cal_quick_time: String,
    pub cal_quick_category_id: Option<i64>,
    pub cal_quick_memo: String,

    // 캐시된 스토어 데이터
    pub todos: Vec<Todo>,
    pub categories: Vec<Category>,
    pub smart_counts: SmartCounts,
    pub category_counts: HashMap<i64, usize>,
    pub json_files: Vec<(String, std::path::PathBuf, usize)>,
    last_data_version: i64,
    last_refresh_at: Instant,

    // 인라인 일정 빠른 추가 입력값
    pub quick_title: String,
    pub quick_date_str: String,
    pub quick_time_str: String,
    pub quick_category_id: Option<i64>,
    pub quick_memo: String,

    // 우측 세부사항 인스펙터 (수정/삭제)
    pub selected_todo_id: Option<i64>,
    pub inspect_title: String,
    pub inspect_memo: String,
    pub inspect_date_str: String,
    pub inspect_time_str: String,
    pub inspect_category_id: Option<i64>,
    pub inspect_status: Status,
    pub inspect_condition: String,

    // 새 일정 추가 모달 (Ctrl+N)
    pub show_new_todo_modal: bool,
    pub modal_title: String,
    pub modal_date_str: String,
    pub modal_time_str: String,
    pub modal_category_id: Option<i64>,
    pub modal_status: Status,
    pub modal_condition: String,
    pub modal_memo: String,

    // 새 목록(카테고리) 추가 모달
    pub show_add_category_modal: bool,
    pub new_cat_name: String,
    pub new_cat_color: u32,

    // 피드백 토스트 메시지
    pub toast_message: Option<(String, Instant, bool)>,
}

impl MainView {
    pub fn new(store: &Store) -> Self {
        let today = Local::now().date_naive();
        let today_str = today.format("%Y-%m-%d").to_string();
        let _ = store.sync_all_json_files();

        let mut s = Self {
            view_mode: ViewMode::TodoList,
            smart_filter: SmartListFilter::Today,
            selected_category_id: None,
            status_filter: None,
            search_query: String::new(),

            cal_year: today.year(),
            cal_month: today.month(),
            cal_selected_date: today,
            month_todos: HashMap::new(),
            cal_quick_title: String::new(),
            cal_quick_time: String::new(),
            cal_quick_category_id: None,
            cal_quick_memo: String::new(),

            todos: vec![],
            categories: vec![],
            smart_counts: SmartCounts::default(),
            category_counts: HashMap::new(),
            json_files: vec![],
            last_data_version: -1,
            last_refresh_at: Instant::now() - Duration::from_secs(10),

            quick_title: String::new(),
            quick_date_str: today_str.clone(),
            quick_time_str: String::new(),
            quick_category_id: None,
            quick_memo: String::new(),

            selected_todo_id: None,
            inspect_title: String::new(),
            inspect_memo: String::new(),
            inspect_date_str: today_str.clone(),
            inspect_time_str: String::new(),
            inspect_category_id: None,
            inspect_status: Status::Planned,
            inspect_condition: String::new(),

            show_new_todo_modal: false,
            modal_title: String::new(),
            modal_date_str: today_str,
            modal_time_str: String::new(),
            modal_category_id: None,
            modal_status: Status::Planned,
            modal_condition: String::new(),
            modal_memo: String::new(),

            show_add_category_modal: false,
            new_cat_name: String::new(),
            new_cat_color: 0x0071E3,

            toast_message: None,
        };
        s.force_refresh(store);
        s
    }

    pub fn show_toast(&mut self, msg: impl Into<String>, is_error: bool) {
        self.toast_message = Some((msg.into(), Instant::now(), is_error));
    }

    pub fn force_refresh(&mut self, store: &Store) {
        self.last_data_version = -1;
        self.refresh_if_needed(store);
    }

    pub fn refresh_if_needed(&mut self, store: &Store) {
        let ver = store.data_version();
        let elapsed = self.last_refresh_at.elapsed() >= Duration::from_millis(1200);
        if self.last_data_version == -1 || ver != self.last_data_version || elapsed {
            self.last_data_version = ver;
            self.last_refresh_at = Instant::now();

            self.categories = store.categories().unwrap_or_default();
            self.smart_counts = store.smart_counts().unwrap_or_default();
            self.category_counts = store.category_counts().unwrap_or_default();
            self.month_todos = store
                .todos_by_month(self.cal_year, self.cal_month)
                .unwrap_or_default();
            self.json_files = store.list_json_schedule_files().unwrap_or_default();

            let date_filter = if self.selected_category_id.is_some() {
                DateFilter::All
            } else {
                match self.smart_filter {
                    SmartListFilter::Today => DateFilter::TodayAndOverdue,
                    SmartListFilter::Scheduled => DateFilter::Scheduled,
                    SmartListFilter::All => DateFilter::All,
                    SmartListFilter::Completed => DateFilter::Completed,
                }
            };

            let q = TodoQuery {
                date: date_filter,
                category: self.selected_category_id.map(Some),
                statuses: self.status_filter.into_iter().collect(),
                search: self.search_query.clone(),
            };
            self.todos = store.query(&q).unwrap_or_default();
        }
    }

    fn open_new_todo_modal(&mut self, default_date: Option<NaiveDate>) {
        let d = default_date.unwrap_or_else(|| Local::now().date_naive());
        self.modal_title.clear();
        self.modal_date_str = d.format("%Y-%m-%d").to_string();
        self.modal_time_str.clear();
        self.modal_category_id = self.selected_category_id;
        self.modal_status = Status::Planned;
        self.modal_condition.clear();
        self.modal_memo.clear();
        self.show_new_todo_modal = true;
    }

    fn select_todo_for_inspector(&mut self, todo: &Todo) {
        self.selected_todo_id = Some(todo.id);
        self.inspect_title = todo.title.clone();
        self.inspect_memo = todo.memo.clone();
        self.inspect_date_str = todo.due_date.format("%Y-%m-%d").to_string();
        self.inspect_time_str = todo
            .due_time
            .map(|t| t.format("%H:%M").to_string())
            .unwrap_or_default();
        self.inspect_category_id = todo.category_id;
        self.inspect_status = todo.status;
        self.inspect_condition = todo.condition_note.clone();
    }

    pub fn show(
        &mut self,
        ctx: &egui::Context,
        store: &Store,
        settings: &mut Settings,
        mut on_toggle_overlay: impl FnMut(),
    ) {
        self.refresh_if_needed(store);

        // Ctrl + N 단축키로 새 일정 추가 모달 열기
        if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(Key::N)) {
            let d = if self.view_mode == ViewMode::Calendar {
                Some(self.cal_selected_date)
            } else {
                None
            };
            self.open_new_todo_modal(d);
        }

        // ── 상단 네비게이션 바 (모드 변경: [투두리스트 / 캘린더 뷰 / 설정 & CLI]) ──
        egui::TopBottomPanel::top("top_navbar")
            .exact_height(56.0)
            .frame(
                egui::Frame::new()
                    .fill(SURFACE)
                    .inner_margin(egui::Margin::symmetric(22, 10))
                    .stroke(Stroke::new(1.0_f32, BORDER)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    // 브랜드 로고
                    let (logo_rect, _) =
                        ui.allocate_exact_size(egui::vec2(24.0, 24.0), egui::Sense::hover());
                    ui.painter().rect_filled(logo_rect, CornerRadius::same(6), PRIMARY);
                    let check_g = ui.painter().layout_no_wrap(
                        icons::CHECK.into(),
                        egui::FontId::proportional(13.0),
                        Color32::WHITE,
                    );
                    ui.painter().galley(
                        logo_rect.center() - check_g.size() / 2.0,
                        check_g,
                        Color32::WHITE,
                    );

                    ui.label(
                        RichText::new("OpenLab Schedule")
                            .size(16.5)
                            .strong()
                            .color(FOREGROUND),
                    );

                    ui.add_space(18.0);

                    // 모드 전환 세그먼트 버튼: [투두리스트] / [캘린더 뷰] / [설정 & JSON/CLI]
                    if quick_chip_button(
                        ui,
                        &format!("{} 투두리스트", icons::LIST_CHECKS),
                        self.view_mode == ViewMode::TodoList,
                    )
                    .clicked()
                    {
                        self.view_mode = ViewMode::TodoList;
                    }

                    if quick_chip_button(
                        ui,
                        &format!("{} 캘린더 뷰", icons::CALENDAR),
                        self.view_mode == ViewMode::Calendar,
                    )
                    .clicked()
                    {
                        self.view_mode = ViewMode::Calendar;
                        self.force_refresh(store);
                    }

                    if quick_chip_button(
                        ui,
                        &format!("{} 데이터 & CLI", icons::GEAR),
                        self.view_mode == ViewMode::Settings,
                    )
                    .clicked()
                    {
                        self.view_mode = ViewMode::Settings;
                        self.force_refresh(store);
                    }

                    // 토스트 메시지 표시
                    if let Some((ref msg, ts, is_err)) = self.toast_message {
                        if ts.elapsed() < Duration::from_secs(4) {
                            ui.add_space(10.0);
                            let col = if is_err { DANGER } else { SUCCESS };
                            ui.label(RichText::new(msg).size(12.5).strong().color(col));
                        } else {
                            self.toast_message = None;
                        }
                    }

                    // 우측 액션 버튼
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if apple_button_primary(
                            ui,
                            &format!("{} 새 일정 추가 (Ctrl+N)", icons::CALENDAR_PLUS),
                        )
                        .clicked()
                        {
                            let d = if self.view_mode == ViewMode::Calendar {
                                Some(self.cal_selected_date)
                            } else {
                                None
                            };
                            self.open_new_todo_modal(d);
                        }

                        if apple_button_outline(ui, "JSON 불러오기").clicked() {
                            match store.sync_all_json_files() {
                                Ok(_) => {
                                    self.force_refresh(store);
                                    self.show_toast(
                                        "✓ 로컬 {date}_schedule.json 파일들과 동기화되었습니다.",
                                        false,
                                    );
                                }
                                Err(e) => {
                                    self.show_toast(format!("동기화 실패: {e}"), true);
                                }
                            }
                        }

                        if apple_button_outline(
                            ui,
                            &format!("{} 상단 오버레이", icons::WEBCAM),
                        )
                        .clicked()
                        {
                            on_toggle_overlay();
                        }
                    });
                });
            });

        // ── 메인 컨텐츠 영역 ──
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(CANVAS)
                    .inner_margin(egui::Margin::symmetric(18, 14)),
            )
            .show(ctx, |ui| match self.view_mode {
                ViewMode::TodoList => self.show_todolist_mode(ui, store),
                ViewMode::Calendar => self.show_calendar_mode(ui, store),
                ViewMode::Settings => self.show_settings_mode(ui, store, settings, &mut on_toggle_overlay),
            });

        // ── 모달 다이얼로그 렌더링 ──
        if self.show_new_todo_modal {
            self.render_new_todo_modal(ctx, store);
        }
        if self.show_add_category_modal {
            self.render_add_category_modal(ctx, store);
        }
    }

    // ═══════════════════════════════════════════════════════════════════════
    //  모드 1: 투두리스트 뷰 (3-Column Apple Reminders View)
    // ═══════════════════════════════════════════════════════════════════════
    fn show_todolist_mode(&mut self, ui: &mut egui::Ui, store: &Store) {
        let avail_h = ui.available_height();

        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 14.0;

            // 1. 좌측 사이드바 카드 (240px)
            egui::Frame::new()
                .fill(SURFACE)
                .corner_radius(CornerRadius::same(18))
                .stroke(Stroke::new(1.0_f32, BORDER))
                .inner_margin(egui::Margin::same(16))
                .show(ui, |ui| {
                    ui.set_width(236.0);
                    ui.set_min_height(avail_h - 32.0);
                    self.render_sidebar(ui, store);
                });

            let inspector_open = self.selected_todo_id.is_some();
            let center_w = if inspector_open {
                (ui.available_width() - 324.0).max(340.0)
            } else {
                (ui.available_width() - 32.0).max(340.0)
            };

            // 2. 중앙 메인 목록 카드
            egui::Frame::new()
                .fill(SURFACE)
                .corner_radius(CornerRadius::same(18))
                .stroke(Stroke::new(1.0_f32, BORDER))
                .inner_margin(egui::Margin::same(20))
                .show(ui, |ui| {
                    ui.set_width(center_w);
                    ui.set_min_height(avail_h - 40.0);
                    self.render_center_column(ui, store);
                });

            // 3. 우측 세부사항 인스펙터 카드 (선택 시 표시)
            if inspector_open {
                egui::Frame::new()
                    .fill(SURFACE)
                    .corner_radius(CornerRadius::same(18))
                    .stroke(Stroke::new(1.0_f32, BORDER))
                    .inner_margin(egui::Margin::same(18))
                    .show(ui, |ui| {
                        ui.set_width(280.0);
                        ui.set_min_height(avail_h - 36.0);
                        self.render_inspector(ui, store);
                    });
            }
        });
    }

    // ═══════════════════════════════════════════════════════════════════════
    //  모드 2: 캘린더 뷰 (월간 달력 그리드 + 선택 일자 `{date}_schedule.json` 투두리스트)
    // ═══════════════════════════════════════════════════════════════════════
    fn show_calendar_mode(&mut self, ui: &mut egui::Ui, store: &Store) {
        let avail_h = ui.available_height();
        let today = Local::now().date_naive();

        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 14.0;

            let right_panel_w = 360.0;
            let cal_card_w = (ui.available_width() - right_panel_w - 60.0).max(460.0);

            // 1. 좌측/중앙 월간 캘린더 그리드 카드
            egui::Frame::new()
                .fill(SURFACE)
                .corner_radius(CornerRadius::same(18))
                .stroke(Stroke::new(1.0_f32, BORDER))
                .inner_margin(egui::Margin::same(18))
                .show(ui, |ui| {
                    ui.set_width(cal_card_w);
                    ui.set_min_height(avail_h - 36.0);

                    // 캘린더 상단 헤더 (이전 달 / 연월 / 다음 달 / 오늘)
                    ui.horizontal(|ui| {
                        if quick_chip_button(ui, "◀ 이전 달", false).clicked() {
                            if self.cal_month == 1 {
                                self.cal_year -= 1;
                                self.cal_month = 12;
                            } else {
                                self.cal_month -= 1;
                            }
                            self.force_refresh(store);
                        }

                        ui.add_space(6.0);
                        ui.label(
                            RichText::new(format!("{}년 {:02}월", self.cal_year, self.cal_month))
                                .size(22.0)
                                .strong()
                                .color(FOREGROUND),
                        );
                        ui.add_space(6.0);

                        if quick_chip_button(ui, "다음 달 ▶", false).clicked() {
                            if self.cal_month == 12 {
                                self.cal_year += 1;
                                self.cal_month = 1;
                            } else {
                                self.cal_month += 1;
                            }
                            self.force_refresh(store);
                        }

                        ui.add_space(8.0);
                        if quick_chip_button(ui, "오늘로 이동", self.cal_selected_date == today)
                            .clicked()
                        {
                            self.cal_year = today.year();
                            self.cal_month = today.month();
                            self.cal_selected_date = today;
                            self.force_refresh(store);
                        }

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let json_name = format!(
                                "{} {}_schedule.json",
                                icons::NOTE,
                                self.cal_selected_date.format("%Y-%m-%d")
                            );
                            ui.label(RichText::new(json_name).size(12.5).strong().color(PRIMARY));
                        });
                    });

                    ui.add_space(10.0);

                    let cell_w = ((cal_card_w - 6.0 * 6.0) / 7.0).max(58.0);
                    let cell_h = ((avail_h - 140.0) / 6.0).max(66.0);

                    // 요일 헤더 (일 ~ 토)
                    let weekdays = ["일", "월", "화", "수", "목", "금", "토"];
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        for (idx, wd) in weekdays.iter().enumerate() {
                            let col = if idx == 0 {
                                DANGER
                            } else if idx == 6 {
                                PRIMARY
                            } else {
                                MUTED
                            };
                            let (r, _) =
                                ui.allocate_exact_size(egui::vec2(cell_w, 22.0), egui::Sense::hover());
                            let g = ui.painter().layout_no_wrap(
                                (*wd).to_string(),
                                egui::FontId::proportional(13.0),
                                col,
                            );
                            ui.painter()
                                .galley(r.center() - g.size() / 2.0, g, col);
                        }
                    });

                    ui.add_space(4.0);

                    // 월간 날짜 계산
                    let first_day = NaiveDate::from_ymd_opt(self.cal_year, self.cal_month, 1)
                        .unwrap_or(today);
                    let start_offset = first_day.weekday().num_days_from_sunday() as i64;
                    let grid_start = first_day - ChronoDuration::days(start_offset);

                    let mut clicked_date: Option<NaiveDate> = None;

                    for week in 0..6 {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 6.0;
                            for dow in 0..7 {
                                let d = grid_start + ChronoDuration::days(week * 7 + dow);
                                let in_month = d.month() == self.cal_month;
                                let is_today = d == today;
                                let is_selected = d == self.cal_selected_date;

                                let (rect, resp) = ui.allocate_exact_size(
                                    egui::vec2(cell_w, cell_h),
                                    egui::Sense::click(),
                                );

                                let bg = if is_selected {
                                    Color32::from_rgb(0xEA, 0xF2, 0xFD)
                                } else if resp.hovered() {
                                    theme::SURFACE_HOVER
                                } else if in_month {
                                    CANVAS
                                } else {
                                    Color32::from_rgb(0xFA, 0xFA, 0xFC)
                                };

                                let stroke = if is_selected {
                                    Stroke::new(2.0_f32, PRIMARY)
                                } else if is_today {
                                    Stroke::new(1.5_f32, PRIMARY.linear_multiply(0.6))
                                } else {
                                    Stroke::new(1.0_f32, BORDER)
                                };

                                ui.painter().rect(
                                    rect,
                                    CornerRadius::same(10),
                                    bg,
                                    stroke,
                                    egui::StrokeKind::Inside,
                                );

                                // 날짜 숫자 표시
                                let day_color = if !in_month {
                                    MUTED.linear_multiply(0.45)
                                } else if is_today {
                                    PRIMARY
                                } else if dow == 0 {
                                    DANGER
                                } else if dow == 6 {
                                    PRIMARY
                                } else {
                                    FOREGROUND
                                };

                                let day_galley = ui.painter().layout_no_wrap(
                                    d.day().to_string(),
                                    egui::FontId::proportional(13.0),
                                    day_color,
                                );
                                ui.painter().galley(
                                    egui::pos2(rect.min.x + 8.0, rect.min.y + 6.0),
                                    day_galley,
                                    day_color,
                                );

                                // 해당 날짜의 할 일 표시
                                if let Some(day_todos) = self.month_todos.get(&d) {
                                    if !day_todos.is_empty() {
                                        let done_cnt = day_todos
                                            .iter()
                                            .filter(|t| t.status.is_finished())
                                            .count();
                                        let badge_col = if done_cnt == day_todos.len() {
                                            SUCCESS
                                        } else {
                                            PRIMARY
                                        };
                                        let cnt_galley = ui.painter().layout_no_wrap(
                                            format!("{}/{}", done_cnt, day_todos.len()),
                                            egui::FontId::proportional(10.5),
                                            badge_col,
                                        );
                                        ui.painter().galley(
                                            egui::pos2(
                                                rect.max.x - cnt_galley.size().x - 6.0,
                                                rect.min.y + 6.0,
                                            ),
                                            cnt_galley,
                                            badge_col,
                                        );

                                        // 최대 2개 할 일 제목 미니 프리뷰
                                        for (idx, t) in day_todos.iter().take(2).enumerate() {
                                            let y_pos = rect.min.y + 24.0 + (idx as f32) * 16.0;
                                            if y_pos + 14.0 <= rect.max.y {
                                                let item_col = if t.status.is_finished() {
                                                    MUTED
                                                } else {
                                                    FOREGROUND
                                                };
                                                let mut short_title: String =
                                                    t.title.chars().take(8).collect();
                                                if t.title.chars().count() > 8 {
                                                    short_title.push('…');
                                                }
                                                let line = format!("{} {}", t.status.icon(), short_title);
                                                let tg = ui.painter().layout_no_wrap(
                                                    line,
                                                    egui::FontId::proportional(11.0),
                                                    item_col,
                                                );
                                                ui.painter().galley(
                                                    egui::pos2(rect.min.x + 6.0, y_pos),
                                                    tg,
                                                    item_col,
                                                );
                                            }
                                        }
                                    }
                                }

                                if resp.clicked() {
                                    clicked_date = Some(d);
                                }
                            }
                        });
                        ui.add_space(5.0);
                    }

                    if let Some(d) = clicked_date {
                        self.cal_selected_date = d;
                        if d.month() != self.cal_month || d.year() != self.cal_year {
                            self.cal_year = d.year();
                            self.cal_month = d.month();
                            self.force_refresh(store);
                        }
                    }
                });

            // 2. 우측: 선택된 날짜(`{date}_schedule.json`) 전용 투두리스트 & 편집 패널
            egui::Frame::new()
                .fill(SURFACE)
                .corner_radius(CornerRadius::same(18))
                .stroke(Stroke::new(1.0_f32, BORDER))
                .inner_margin(egui::Margin::same(18))
                .show(ui, |ui| {
                    ui.set_width(right_panel_w);
                    ui.set_min_height(avail_h - 36.0);
                    self.render_calendar_day_panel(ui, store);
                });
        });
    }

    fn render_calendar_day_panel(&mut self, ui: &mut egui::Ui, store: &Store) {
        let sel_date = self.cal_selected_date;
        let date_str = sel_date.format("%Y-%m-%d").to_string();
        let json_path = paths::date_schedule_json_path(sel_date);

        let day_todos = store
            .query(&TodoQuery {
                date: DateFilter::Day(sel_date),
                ..Default::default()
            })
            .unwrap_or_default();

        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("{} 일정 ({}개)", date_str, day_todos.len()))
                    .size(18.5)
                    .strong()
                    .color(PRIMARY),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if quick_chip_button(ui, "+ 상세 추가", false).clicked() {
                    self.open_new_todo_modal(Some(sel_date));
                }
            });
        });

        ui.label(
            RichText::new(format!(
                "저장 파일: {}_schedule.json {}",
                date_str,
                if json_path.exists() { "(저장됨)" } else { "(비어있음)" }
            ))
            .size(11.5)
            .color(MUTED),
        );

        ui.add_space(8.0);

        // 선택한 날짜에 바로 일정 추가
        let mut add_to_day = false;
        egui::Frame::new()
            .fill(CANVAS)
            .corner_radius(CornerRadius::same(10))
            .stroke(Stroke::new(1.0_f32, BORDER))
            .inner_margin(egui::Margin::same(10))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut self.cal_quick_title)
                            .hint_text(format!("{date_str} 할 일 제목 입력 (Enter)..."))
                            .desired_width(ui.available_width() - 74.0),
                    );
                    if (resp.lost_focus() || resp.has_focus())
                        && ui.input(|i| i.key_pressed(Key::Enter))
                    {
                        add_to_day = true;
                    }
                    if apple_button_primary(ui, "추가").clicked() {
                        add_to_day = true;
                    }
                });

                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.cal_quick_time)
                            .hint_text("시간(14:00)")
                            .desired_width(72.0),
                    );

                    let cat_name = self
                        .cal_quick_category_id
                        .and_then(|cid| self.categories.iter().find(|c| c.id == cid))
                        .map(|c| c.name.as_str())
                        .unwrap_or("미분류");
                    egui::ComboBox::from_id_salt("cal_day_cat_combo")
                        .selected_text(cat_name)
                        .width(85.0)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.cal_quick_category_id, None, "미분류");
                            for c in &self.categories {
                                ui.selectable_value(
                                    &mut self.cal_quick_category_id,
                                    Some(c.id),
                                    &c.name,
                                );
                            }
                        });

                    let m_resp = ui.add(
                        egui::TextEdit::singleline(&mut self.cal_quick_memo)
                            .hint_text("메모...")
                            .desired_width(ui.available_width()),
                    );
                    if (m_resp.lost_focus() || m_resp.has_focus())
                        && ui.input(|i| i.key_pressed(Key::Enter))
                    {
                        add_to_day = true;
                    }
                });
            });

        if add_to_day && !self.cal_quick_title.trim().is_empty() {
            let due_time = parse_flexible_time(&self.cal_quick_time).ok().flatten();
            let draft = TodoDraft {
                title: self.cal_quick_title.trim().to_string(),
                memo: self.cal_quick_memo.trim().to_string(),
                category_id: self.cal_quick_category_id,
                due_date: sel_date,
                due_time,
                status: Status::Planned,
                condition_note: String::new(),
            };
            if store.add(&draft).is_ok() {
                self.cal_quick_title.clear();
                self.cal_quick_time.clear();
                self.cal_quick_memo.clear();
                self.force_refresh(store);
                self.show_toast(
                    format!("✓ {}_schedule.json에 저장되었습니다.", date_str),
                    false,
                );
            }
        }

        ui.add_space(8.0);

        // 선택된 할 일이 있으면 바로 아래에 수정/삭제 인스펙터 표시, 없으면 목록 전체 높이 사용
        let list_max_h = if self.selected_todo_id.is_some() {
            180.0
        } else {
            ui.available_height() - 10.0
        };

        let mut toggle_action: Option<(i64, Status)> = None;
        let mut select_action: Option<Todo> = None;
        let mut delete_action: Option<i64> = None;

        egui::ScrollArea::vertical()
            .id_salt("cal_day_todo_scroll")
            .max_height(list_max_h)
            .show(ui, |ui| {
                if day_todos.is_empty() {
                    ui.vertical_centered(|ui| {
                        ui.add_space(24.0);
                        ui.label(
                            RichText::new("이 날짜에 등록된 할 일이 없습니다.")
                                .size(13.5)
                                .color(MUTED),
                        );
                    });
                }

                for todo in &day_todos {
                    let is_sel = self.selected_todo_id == Some(todo.id);
                    let cat_info = todo
                        .category_id
                        .and_then(|cid| self.categories.iter().find(|c| c.id == cid));
                    let cat_color = cat_info.map(|c| theme::rgb(c.color)).unwrap_or(PRIMARY);

                    egui::Frame::new()
                        .fill(if is_sel {
                            Color32::from_rgb(0xF2, 0xF7, 0xFE)
                        } else {
                            SURFACE
                        })
                        .corner_radius(CornerRadius::same(9))
                        .stroke(Stroke::new(
                            if is_sel { 1.5_f32 } else { 1.0_f32 },
                            if is_sel { PRIMARY } else { BORDER },
                        ))
                        .inner_margin(egui::Margin::symmetric(10, 8))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                if circular_checkbox(ui, todo.status, cat_color).clicked() {
                                    let next = if todo.status == Status::Done {
                                        Status::InProgress
                                    } else {
                                        Status::Done
                                    };
                                    toggle_action = Some((todo.id, next));
                                }

                                ui.vertical(|ui| {
                                    let mut rt = RichText::new(&todo.title).size(14.0).color(
                                        if todo.status.is_finished() {
                                            MUTED
                                        } else {
                                            FOREGROUND
                                        },
                                    );
                                    if todo.status.is_finished() {
                                        rt = rt.strikethrough();
                                    }
                                    if ui
                                        .add(egui::Label::new(rt).sense(egui::Sense::click()))
                                        .clicked()
                                    {
                                        select_action = Some(todo.clone());
                                    }

                                    ui.horizontal_wrapped(|ui| {
                                        if let Some(cat) = cat_info {
                                            category_chip(ui, &cat.name, cat_color);
                                        }
                                        if let Some(tm) = todo.due_time {
                                            ui.label(
                                                RichText::new(tm.format("%H:%M").to_string())
                                                    .small()
                                                    .color(MUTED),
                                            );
                                        }
                                        if !todo.memo.is_empty() {
                                            ui.label(
                                                RichText::new(&todo.memo).small().color(MUTED),
                                            );
                                        }
                                    });
                                });

                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui
                                            .small_button(RichText::new(icons::TRASH).color(DANGER))
                                            .on_hover_text("삭제")
                                            .clicked()
                                        {
                                            delete_action = Some(todo.id);
                                        }
                                        if status_pill(ui, todo.status).clicked() {
                                            toggle_action = Some((todo.id, todo.status.next()));
                                        }
                                    },
                                );
                            });
                        });
                    ui.add_space(4.0);
                }
            });

        if let Some((id, st)) = toggle_action {
            let _ = store.set_status(id, st);
            self.force_refresh(store);
        }
        if let Some(id) = delete_action {
            let _ = store.delete(id);
            if self.selected_todo_id == Some(id) {
                self.selected_todo_id = None;
            }
            self.force_refresh(store);
            self.show_toast("✓ 할 일이 삭제되었습니다.", false);
        }
        if let Some(todo) = select_action {
            self.select_todo_for_inspector(&todo);
        }

        // 선택된 할 일이 있으면 하단에 수정 패널 렌더링
        if self.selected_todo_id.is_some() {
            ui.separator();
            egui::ScrollArea::vertical()
                .id_salt("cal_inspector_scroll")
                .show(ui, |ui| {
                    self.render_inspector(ui, store);
                });
        }
    }

    // ═══════════════════════════════════════════════════════════════════════
    //  좌측 사이드바 / 중앙 컬럼 / 우측 인스펙터 (투두리스트 뷰)
    // ═══════════════════════════════════════════════════════════════════════
    fn render_sidebar(&mut self, ui: &mut egui::Ui, store: &Store) {
        let today_day = Local::now().format("%d").to_string();
        let card_w = 112.0;

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let sel_today =
                self.selected_category_id.is_none() && self.smart_filter == SmartListFilter::Today;
            if smart_card(
                ui,
                &today_day,
                PRIMARY,
                "오늘",
                self.smart_counts.today,
                sel_today,
                card_w,
            )
            .clicked()
            {
                self.selected_category_id = None;
                self.smart_filter = SmartListFilter::Today;
                self.force_refresh(store);
            }

            let sel_sched = self.selected_category_id.is_none()
                && self.smart_filter == SmartListFilter::Scheduled;
            if smart_card(
                ui,
                icons::CLOCK,
                WARNING,
                "예정",
                self.smart_counts.scheduled,
                sel_sched,
                card_w,
            )
            .clicked()
            {
                self.selected_category_id = None;
                self.smart_filter = SmartListFilter::Scheduled;
                self.force_refresh(store);
            }
        });

        ui.add_space(6.0);

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let sel_all =
                self.selected_category_id.is_none() && self.smart_filter == SmartListFilter::All;
            if smart_card(
                ui,
                icons::TRAY,
                SECONDARY,
                "전체",
                self.smart_counts.all,
                sel_all,
                card_w,
            )
            .clicked()
            {
                self.selected_category_id = None;
                self.smart_filter = SmartListFilter::All;
                self.force_refresh(store);
            }

            let sel_comp = self.selected_category_id.is_none()
                && self.smart_filter == SmartListFilter::Completed;
            if smart_card(
                ui,
                icons::CHECK,
                SUCCESS,
                "완료됨",
                self.smart_counts.completed,
                sel_comp,
                card_w,
            )
            .clicked()
            {
                self.selected_category_id = None;
                self.smart_filter = SmartListFilter::Completed;
                self.force_refresh(store);
            }
        });

        ui.add_space(16.0);

        ui.horizontal(|ui| {
            ui.label(RichText::new("내 목록").size(13.0).strong().color(MUTED));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button(RichText::new(format!("{} 목록 추가", icons::PLUS)).color(PRIMARY))
                    .clicked()
                {
                    self.new_cat_name.clear();
                    self.new_cat_color = 0x0071E3;
                    self.show_add_category_modal = true;
                }
            });
        });

        ui.add_space(4.0);

        let mut select_cat = None;
        let mut delete_cat = None;

        egui::ScrollArea::vertical()
            .id_salt("sidebar_categories")
            .show(ui, |ui| {
                for cat in &self.categories {
                    let is_sel = self.selected_category_id == Some(cat.id);
                    let cnt = self.category_counts.get(&cat.id).copied().unwrap_or(0);
                    let bg = if is_sel {
                        Color32::from_rgb(0xEA, 0xF2, 0xFD)
                    } else {
                        Color32::TRANSPARENT
                    };

                    egui::Frame::new()
                        .fill(bg)
                        .corner_radius(CornerRadius::same(8))
                        .inner_margin(egui::Margin::symmetric(10, 7))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let (dot_r, _) = ui.allocate_exact_size(
                                    egui::vec2(10.0, 10.0),
                                    egui::Sense::hover(),
                                );
                                ui.painter()
                                    .circle_filled(dot_r.center(), 4.5, theme::rgb(cat.color));

                                let name_col = if is_sel { PRIMARY } else { FOREGROUND };
                                let resp = ui.add(
                                    egui::Label::new(
                                        RichText::new(&cat.name).size(14.0).color(name_col),
                                    )
                                    .sense(egui::Sense::click()),
                                );
                                if resp.clicked() {
                                    select_cat = Some(cat.id);
                                }

                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui
                                            .small_button(RichText::new(icons::X).color(MUTED))
                                            .on_hover_text("목록 삭제")
                                            .clicked()
                                        {
                                            delete_cat = Some(cat.id);
                                        }
                                        ui.label(
                                            RichText::new(cnt.to_string()).size(12.5).color(MUTED),
                                        );
                                    },
                                );
                            });
                        });
                }
            });

        if let Some(cid) = select_cat {
            self.selected_category_id = Some(cid);
            self.quick_category_id = Some(cid);
            self.force_refresh(store);
        }
        if let Some(cid) = delete_cat {
            let _ = store.delete_category(cid);
            if self.selected_category_id == Some(cid) {
                self.selected_category_id = None;
            }
            self.force_refresh(store);
        }
    }

    fn render_center_column(&mut self, ui: &mut egui::Ui, store: &Store) {
        let today = Local::now().date_naive();

        let (title_text, title_color) = if let Some(cid) = self.selected_category_id {
            if let Some(c) = self.categories.iter().find(|c| c.id == cid) {
                (c.name.clone(), theme::rgb(c.color))
            } else {
                ("목록".to_string(), PRIMARY)
            }
        } else {
            match self.smart_filter {
                SmartListFilter::Today => ("오늘".to_string(), PRIMARY),
                SmartListFilter::Scheduled => ("예정".to_string(), WARNING),
                SmartListFilter::All => ("전체".to_string(), FOREGROUND),
                SmartListFilter::Completed => ("완료됨".to_string(), SUCCESS),
            }
        };

        ui.horizontal(|ui| {
            ui.label(
                RichText::new(&title_text)
                    .size(28.0)
                    .strong()
                    .color(title_color),
            );
            ui.label(
                RichText::new(format!("{}개", self.todos.len()))
                    .size(16.0)
                    .color(MUTED),
            );

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut self.search_query)
                        .hint_text(format!("{} 일정 검색...", icons::MAGNIFYING_GLASS))
                        .desired_width(180.0),
                );
                if resp.changed() {
                    self.force_refresh(store);
                }
            });
        });

        ui.add_space(10.0);

        // 인라인 일정 추가 박스
        let mut submit_quick_add = false;
        egui::Frame::new()
            .fill(CANVAS)
            .corner_radius(CornerRadius::same(12))
            .stroke(Stroke::new(1.0_f32, BORDER))
            .inner_margin(egui::Margin::symmetric(14, 10))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let (r, _) =
                        ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::hover());
                    ui.painter()
                        .circle_stroke(r.center(), 8.5, Stroke::new(1.5_f32, PRIMARY));
                    let plus_g = ui.painter().layout_no_wrap(
                        icons::PLUS.into(),
                        egui::FontId::proportional(11.0),
                        PRIMARY,
                    );
                    ui.painter()
                        .galley(r.center() - plus_g.size() / 2.0, plus_g, PRIMARY);

                    let title_w = (ui.available_width() - 115.0).max(150.0);
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut self.quick_title)
                            .hint_text("새로운 할 일 제목 입력 (Enter 또는 [일정 추가]로 {date}_schedule.json에 저장)...")
                            .desired_width(title_w),
                    );
                    if (resp.lost_focus() || resp.has_focus())
                        && ui.input(|i| i.key_pressed(Key::Enter))
                    {
                        submit_quick_add = true;
                    }

                    if apple_button_primary(ui, &format!("{} 일정 추가", icons::PLUS)).clicked() {
                        submit_quick_add = true;
                    }
                });

                ui.add_space(4.0);

                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    ui.label(RichText::new(format!("{} 날짜:", icons::CALENDAR)).small().color(MUTED));

                    let today_s = today.format("%Y-%m-%d").to_string();
                    let tmr_s = (today + ChronoDuration::days(1)).format("%Y-%m-%d").to_string();
                    let nextw_s = (today + ChronoDuration::days(7)).format("%Y-%m-%d").to_string();

                    if quick_chip_button(ui, "오늘", self.quick_date_str == today_s).clicked() {
                        self.quick_date_str = today_s;
                    }
                    if quick_chip_button(ui, "내일", self.quick_date_str == tmr_s).clicked() {
                        self.quick_date_str = tmr_s;
                    }
                    if quick_chip_button(ui, "다음주", self.quick_date_str == nextw_s).clicked() {
                        self.quick_date_str = nextw_s;
                    }

                    ui.add(
                        egui::TextEdit::singleline(&mut self.quick_date_str)
                            .hint_text("YYYY-MM-DD")
                            .desired_width(86.0),
                    );

                    ui.label(RichText::new(format!("{} 시간:", icons::CLOCK)).small().color(MUTED));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.quick_time_str)
                            .hint_text("예: 14:00")
                            .desired_width(64.0),
                    );

                    ui.label(RichText::new(format!("{} 목록:", icons::TAG)).small().color(MUTED));
                    let selected_cat_name = self
                        .quick_category_id
                        .and_then(|cid| self.categories.iter().find(|c| c.id == cid))
                        .map(|c| c.name.as_str())
                        .unwrap_or("미분류");

                    egui::ComboBox::from_id_salt("quick_add_cat_combo")
                        .selected_text(selected_cat_name)
                        .width(85.0)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.quick_category_id, None, "미분류");
                            for c in &self.categories {
                                ui.selectable_value(
                                    &mut self.quick_category_id,
                                    Some(c.id),
                                    &c.name,
                                );
                            }
                        });

                    let memo_resp = ui.add(
                        egui::TextEdit::singleline(&mut self.quick_memo)
                            .hint_text("메모 (선택)...")
                            .desired_width(120.0),
                    );
                    if (memo_resp.lost_focus() || memo_resp.has_focus())
                        && ui.input(|i| i.key_pressed(Key::Enter))
                    {
                        submit_quick_add = true;
                    }
                });
            });

        if submit_quick_add {
            let trimmed = self.quick_title.trim().to_string();
            if trimmed.is_empty() {
                self.show_toast("일정 제목을 입력해주세요.", true);
            } else {
                let due_date = parse_flexible_date(&self.quick_date_str).unwrap_or(today);
                let due_time = parse_flexible_time(&self.quick_time_str).ok().flatten();
                let cat_id = self.quick_category_id.or(self.selected_category_id);

                let draft = TodoDraft {
                    title: trimmed.clone(),
                    memo: self.quick_memo.trim().to_string(),
                    category_id: cat_id,
                    due_date,
                    due_time,
                    status: Status::Planned,
                    condition_note: String::new(),
                };

                match store.add(&draft) {
                    Ok(_id) => {
                        self.quick_title.clear();
                        self.quick_time_str.clear();
                        self.quick_memo.clear();

                        if self.selected_category_id.is_none()
                            && self.smart_filter == SmartListFilter::Today
                            && due_date > today
                        {
                            self.smart_filter = SmartListFilter::Scheduled;
                        }
                        self.force_refresh(store);
                        self.show_toast(
                            format!(
                                "✓ \"{trimmed}\" 저장됨 ({}_schedule.json)",
                                due_date.format("%Y-%m-%d")
                            ),
                            false,
                        );
                    }
                    Err(e) => {
                        self.show_toast(format!("일정 추가 실패: {e}"), true);
                    }
                }
            }
        }

        ui.add_space(8.0);

        // 4대 세부 상태 필터 알약
        ui.horizontal(|ui| {
            for st in Status::ALL {
                let is_active = self.status_filter == Some(st);
                if status_pill_selectable(ui, st, is_active).clicked() {
                    if is_active {
                        self.status_filter = None;
                    } else {
                        self.status_filter = Some(st);
                    }
                    self.force_refresh(store);
                }
            }
        });

        ui.add_space(6.0);
        ui.separator();
        ui.add_space(6.0);

        if self.todos.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(50.0);
                ui.label(RichText::new(icons::CHECK_CIRCLE).size(36.0).color(PRIMARY));
                ui.add_space(6.0);
                ui.label(
                    RichText::new("등록된 일정이 없습니다")
                        .size(16.0)
                        .strong()
                        .color(MUTED),
                );
                ui.label(
                    RichText::new(
                        "위의 입력란에서 바로 추가하거나 [캘린더 뷰]에서 날짜를 클릭해 일정을 등록하세요.",
                    )
                    .size(13.0)
                    .color(MUTED),
                );
            });
            return;
        }

        let mut toggle_status_action: Option<(i64, Status)> = None;
        let mut select_todo_action: Option<Todo> = None;
        let mut delete_todo_action: Option<i64> = None;

        egui::ScrollArea::vertical()
            .id_salt("center_todo_checklist")
            .show(ui, |ui| {
                for todo in &self.todos {
                    let is_selected = self.selected_todo_id == Some(todo.id);
                    let is_finished = todo.status.is_finished();
                    let is_overdue = todo.is_overdue(today);

                    let cat_info = todo
                        .category_id
                        .and_then(|cid| self.categories.iter().find(|c| c.id == cid));
                    let cat_color = cat_info.map(|c| theme::rgb(c.color)).unwrap_or(PRIMARY);

                    let row_bg = if is_selected {
                        Color32::from_rgb(0xF2, 0xF7, 0xFE)
                    } else {
                        SURFACE
                    };
                    let row_stroke = if is_selected {
                        Stroke::new(1.5_f32, PRIMARY)
                    } else {
                        Stroke::new(1.0_f32, BORDER)
                    };

                    egui::Frame::new()
                        .fill(row_bg)
                        .corner_radius(CornerRadius::same(10))
                        .stroke(row_stroke)
                        .inner_margin(egui::Margin::symmetric(12, 10))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                if circular_checkbox(ui, todo.status, cat_color).clicked() {
                                    let next = if todo.status == Status::Done {
                                        Status::InProgress
                                    } else {
                                        Status::Done
                                    };
                                    toggle_status_action = Some((todo.id, next));
                                }

                                ui.vertical(|ui| {
                                    ui.horizontal(|ui| {
                                        let title_col = if is_finished {
                                            MUTED
                                        } else if is_overdue {
                                            DANGER
                                        } else {
                                            FOREGROUND
                                        };
                                        let mut rt =
                                            RichText::new(&todo.title).size(15.0).color(title_col);
                                        if is_finished {
                                            rt = rt.strikethrough();
                                        }
                                        if ui
                                            .add(egui::Label::new(rt).sense(egui::Sense::click()))
                                            .clicked()
                                        {
                                            select_todo_action = Some(todo.clone());
                                        }

                                        if let Some(cat) = cat_info {
                                            category_chip(ui, &cat.name, cat_color);
                                        }
                                    });

                                    ui.horizontal_wrapped(|ui| {
                                        ui.label(
                                            RichText::new(format!(
                                                "{} {} ({}_schedule.json)",
                                                icons::CALENDAR,
                                                todo.due_date.format("%Y-%m-%d"),
                                                todo.due_date.format("%Y-%m-%d")
                                            ))
                                            .size(11.5)
                                            .color(MUTED),
                                        );
                                        if let Some(tm) = todo.due_time {
                                            ui.label(
                                                RichText::new(format!(
                                                    "{} {}",
                                                    icons::CLOCK,
                                                    tm.format("%H:%M")
                                                ))
                                                .size(12.0)
                                                .color(MUTED),
                                            );
                                        }
                                        if is_overdue {
                                            ui.label(
                                                RichText::new(format!(
                                                    "{} 기한 초과",
                                                    icons::WARNING
                                                ))
                                                .size(12.0)
                                                .strong()
                                                .color(DANGER),
                                            );
                                        }
                                        if !todo.condition_note.is_empty() {
                                            ui.label(
                                                RichText::new(format!(
                                                    "{} 조건: {}",
                                                    icons::TAG,
                                                    todo.condition_note
                                                ))
                                                .size(12.0)
                                                .color(PURPLE),
                                            );
                                        }
                                        if !todo.memo.is_empty() {
                                            ui.label(
                                                RichText::new(format!(
                                                    "{} {}",
                                                    icons::NOTE,
                                                    todo.memo
                                                ))
                                                .size(12.0)
                                                .color(MUTED),
                                            );
                                        }
                                    });
                                });

                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui
                                            .small_button(RichText::new(icons::TRASH).color(DANGER))
                                            .on_hover_text("삭제")
                                            .clicked()
                                        {
                                            delete_todo_action = Some(todo.id);
                                        }

                                        if ui
                                            .small_button(RichText::new("수정").color(PRIMARY))
                                            .on_hover_text("세부사항 수정")
                                            .clicked()
                                        {
                                            select_todo_action = Some(todo.clone());
                                        }

                                        if status_pill(ui, todo.status).clicked() {
                                            toggle_status_action =
                                                Some((todo.id, todo.status.next()));
                                        }
                                    },
                                );
                            });
                        });

                    ui.add_space(5.0);
                }
            });

        if let Some((id, next_st)) = toggle_status_action {
            let _ = store.set_status(id, next_st);
            self.force_refresh(store);
        }
        if let Some(id) = delete_todo_action {
            let _ = store.delete(id);
            if self.selected_todo_id == Some(id) {
                self.selected_todo_id = None;
            }
            self.force_refresh(store);
            self.show_toast("✓ 할 일이 삭제되었습니다.", false);
        }
        if let Some(todo) = select_todo_action {
            self.select_todo_for_inspector(&todo);
        }
    }

    fn render_inspector(&mut self, ui: &mut egui::Ui, store: &Store) {
        let Some(todo_id) = self.selected_todo_id else {
            return;
        };
        let today = Local::now().date_naive();

        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("할 일 수정 (#{todo_id})"))
                    .size(16.0)
                    .strong()
                    .color(FOREGROUND),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button(RichText::new(icons::X).size(13.0).color(MUTED))
                    .clicked()
                {
                    self.selected_todo_id = None;
                }
            });
        });

        ui.add_space(8.0);

        ui.label(RichText::new("제목").size(12.0).strong().color(MUTED));
        ui.add(
            egui::TextEdit::singleline(&mut self.inspect_title)
                .desired_width(ui.available_width()),
        );

        ui.add_space(6.0);
        ui.label(RichText::new("메모").size(12.0).strong().color(MUTED));
        ui.add(
            egui::TextEdit::multiline(&mut self.inspect_memo)
                .desired_rows(2)
                .desired_width(ui.available_width()),
        );

        ui.add_space(6.0);
        ui.label(RichText::new("기한 일자 ({date}_schedule.json)").size(12.0).strong().color(MUTED));
        ui.horizontal(|ui| {
            if quick_chip_button(ui, "오늘", false).clicked() {
                self.inspect_date_str = today.format("%Y-%m-%d").to_string();
            }
            if quick_chip_button(ui, "내일", false).clicked() {
                self.inspect_date_str = (today + ChronoDuration::days(1))
                    .format("%Y-%m-%d")
                    .to_string();
            }
            if quick_chip_button(ui, "다음 주", false).clicked() {
                self.inspect_date_str = (today + ChronoDuration::days(7))
                    .format("%Y-%m-%d")
                    .to_string();
            }
        });
        ui.add(
            egui::TextEdit::singleline(&mut self.inspect_date_str)
                .hint_text("YYYY-MM-DD")
                .desired_width(ui.available_width()),
        );

        ui.add_space(6.0);
        ui.label(RichText::new("기한 시간").size(12.0).strong().color(MUTED));
        ui.horizontal_wrapped(|ui| {
            if quick_chip_button(ui, "09:00", self.inspect_time_str == "09:00").clicked() {
                self.inspect_time_str = "09:00".into();
            }
            if quick_chip_button(ui, "13:00", self.inspect_time_str == "13:00").clicked() {
                self.inspect_time_str = "13:00".into();
            }
            if quick_chip_button(ui, "18:00", self.inspect_time_str == "18:00").clicked() {
                self.inspect_time_str = "18:00".into();
            }
            if quick_chip_button(ui, "삭제", self.inspect_time_str.is_empty()).clicked() {
                self.inspect_time_str.clear();
            }
        });
        ui.add(
            egui::TextEdit::singleline(&mut self.inspect_time_str)
                .hint_text("HH:MM (예: 14:30)")
                .desired_width(ui.available_width()),
        );

        ui.add_space(6.0);
        ui.label(RichText::new("목록 (카테고리)").size(12.0).strong().color(MUTED));
        let cat_label = self
            .inspect_category_id
            .and_then(|cid| self.categories.iter().find(|c| c.id == cid))
            .map(|c| c.name.as_str())
            .unwrap_or("미분류");
        egui::ComboBox::from_id_salt("inspect_category_combo")
            .selected_text(cat_label)
            .width(ui.available_width())
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut self.inspect_category_id, None, "미분류");
                for c in &self.categories {
                    ui.selectable_value(&mut self.inspect_category_id, Some(c.id), &c.name);
                }
            });

        ui.add_space(6.0);
        ui.label(RichText::new("진행 상태").size(12.0).strong().color(MUTED));
        egui::ComboBox::from_id_salt("inspect_status_combo")
            .selected_text(format!(
                "{} {}",
                self.inspect_status.icon(),
                self.inspect_status.label()
            ))
            .width(ui.available_width())
            .show_ui(ui, |ui| {
                for st in Status::ALL {
                    ui.selectable_value(
                        &mut self.inspect_status,
                        st,
                        format!("{} {}", st.icon(), st.label()),
                    );
                }
            });

        if self.inspect_status == Status::ConditionalDone {
            ui.add_space(6.0);
            ui.label(
                RichText::new("조건부 완료 조건")
                    .size(12.0)
                    .strong()
                    .color(PURPLE),
            );
            ui.add(
                egui::TextEdit::singleline(&mut self.inspect_condition)
                    .hint_text("예: 상사 승인 후")
                    .desired_width(ui.available_width()),
            );
        }

        ui.add_space(12.0);
        ui.horizontal(|ui| {
            if apple_button_primary(ui, &format!("{} 수정 저장", icons::FLOPPY_DISK)).clicked() {
                let due_date = parse_flexible_date(&self.inspect_date_str).unwrap_or(today);
                let due_time = parse_flexible_time(&self.inspect_time_str).ok().flatten();
                let draft = TodoDraft {
                    title: self.inspect_title.trim().to_string(),
                    memo: self.inspect_memo.trim().to_string(),
                    category_id: self.inspect_category_id,
                    due_date,
                    due_time,
                    status: self.inspect_status,
                    condition_note: self.inspect_condition.trim().to_string(),
                };
                if !draft.title.is_empty() {
                    let _ = store.update(todo_id, &draft);
                    self.selected_todo_id = None;
                    self.force_refresh(store);
                    self.show_toast(
                        format!(
                            "✓ 수정 완료 ({}_schedule.json 갱신됨)",
                            due_date.format("%Y-%m-%d")
                        ),
                        false,
                    );
                }
            }

            if apple_button_danger(ui, &format!("{} 삭제", icons::TRASH)).clicked() {
                let _ = store.delete(todo_id);
                self.selected_todo_id = None;
                self.force_refresh(store);
                self.show_toast("✓ 할 일이 삭제되었습니다.", false);
            }
        });
    }

    fn render_new_todo_modal(&mut self, ctx: &egui::Context, store: &Store) {
        let today = Local::now().date_naive();
        let mut close_modal = false;
        let mut submit_modal = false;

        egui::Window::new(format!("{} 새 일정 추가", icons::CALENDAR_PLUS))
            .collapsible(false)
            .resizable(false)
            .fixed_size(egui::vec2(430.0, 420.0))
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.add_space(4.0);
                ui.label(RichText::new("일정 제목 *").size(13.0).strong().color(MUTED));
                let title_resp = ui.add(
                    egui::TextEdit::singleline(&mut self.modal_title)
                        .hint_text("무엇을 해야 하나요? (예: 프로젝트 기획안 발표)")
                        .desired_width(ui.available_width()),
                );
                if (title_resp.lost_focus() || title_resp.has_focus())
                    && ui.input(|i| i.key_pressed(Key::Enter))
                {
                    submit_modal = true;
                }

                ui.add_space(8.0);
                ui.label(
                    RichText::new("기한 날짜 ({date}_schedule.json 파일로 저장)")
                        .size(13.0)
                        .strong()
                        .color(MUTED),
                );
                ui.horizontal(|ui| {
                    let d0 = today.format("%Y-%m-%d").to_string();
                    let d1 = (today + ChronoDuration::days(1))
                        .format("%Y-%m-%d")
                        .to_string();
                    let d2 = (today + ChronoDuration::days(2))
                        .format("%Y-%m-%d")
                        .to_string();
                    let d7 = (today + ChronoDuration::days(7))
                        .format("%Y-%m-%d")
                        .to_string();

                    if quick_chip_button(ui, "오늘", self.modal_date_str == d0).clicked() {
                        self.modal_date_str = d0;
                    }
                    if quick_chip_button(ui, "내일", self.modal_date_str == d1).clicked() {
                        self.modal_date_str = d1;
                    }
                    if quick_chip_button(ui, "모레", self.modal_date_str == d2).clicked() {
                        self.modal_date_str = d2;
                    }
                    if quick_chip_button(ui, "다음 주", self.modal_date_str == d7).clicked() {
                        self.modal_date_str = d7;
                    }
                    ui.add(
                        egui::TextEdit::singleline(&mut self.modal_date_str)
                            .hint_text("YYYY-MM-DD")
                            .desired_width(110.0),
                    );
                });

                ui.add_space(8.0);
                ui.label(
                    RichText::new("기한 시간 (선택)")
                        .size(13.0)
                        .strong()
                        .color(MUTED),
                );
                ui.horizontal(|ui| {
                    for t in ["09:00", "12:00", "14:00", "18:00"] {
                        if quick_chip_button(ui, t, self.modal_time_str == t).clicked() {
                            self.modal_time_str = t.to_string();
                        }
                    }
                    if quick_chip_button(ui, "없음", self.modal_time_str.is_empty()).clicked() {
                        self.modal_time_str.clear();
                    }
                    ui.add(
                        egui::TextEdit::singleline(&mut self.modal_time_str)
                            .hint_text("HH:MM")
                            .desired_width(80.0),
                    );
                });

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new("목록 (카테고리)").size(13.0).strong().color(MUTED));
                        let cat_name = self
                            .modal_category_id
                            .and_then(|cid| self.categories.iter().find(|c| c.id == cid))
                            .map(|c| c.name.as_str())
                            .unwrap_or("미분류");
                        egui::ComboBox::from_id_salt("modal_todo_cat_combo")
                            .selected_text(cat_name)
                            .width(180.0)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut self.modal_category_id, None, "미분류");
                                for c in &self.categories {
                                    ui.selectable_value(
                                        &mut self.modal_category_id,
                                        Some(c.id),
                                        &c.name,
                                    );
                                }
                            });
                    });

                    ui.add_space(12.0);

                    ui.vertical(|ui| {
                        ui.label(RichText::new("초기 상태").size(13.0).strong().color(MUTED));
                        egui::ComboBox::from_id_salt("modal_todo_status_combo")
                            .selected_text(format!(
                                "{} {}",
                                self.modal_status.icon(),
                                self.modal_status.label()
                            ))
                            .width(180.0)
                            .show_ui(ui, |ui| {
                                for st in Status::ALL {
                                    ui.selectable_value(
                                        &mut self.modal_status,
                                        st,
                                        format!("{} {}", st.icon(), st.label()),
                                    );
                                }
                            });
                    });
                });

                ui.add_space(8.0);
                ui.label(RichText::new("메모").size(13.0).strong().color(MUTED));
                ui.add(
                    egui::TextEdit::multiline(&mut self.modal_memo)
                        .hint_text("관련 링크, 준비물, 세부 내용을 적어주세요...")
                        .desired_rows(3)
                        .desired_width(ui.available_width()),
                );

                ui.add_space(14.0);
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if apple_button_primary(ui, &format!("{} 일정 등록", icons::CHECK))
                            .clicked()
                        {
                            submit_modal = true;
                        }
                        if apple_button_outline(ui, "취소").clicked() {
                            close_modal = true;
                        }
                    });
                });
            });

        if submit_modal {
            let trimmed = self.modal_title.trim().to_string();
            if trimmed.is_empty() {
                self.show_toast("일정 제목을 입력해주세요.", true);
            } else {
                let due_date = parse_flexible_date(&self.modal_date_str).unwrap_or(today);
                let due_time = parse_flexible_time(&self.modal_time_str).ok().flatten();
                let draft = TodoDraft {
                    title: trimmed.clone(),
                    memo: self.modal_memo.trim().to_string(),
                    category_id: self.modal_category_id,
                    due_date,
                    due_time,
                    status: self.modal_status,
                    condition_note: self.modal_condition.trim().to_string(),
                };
                match store.add(&draft) {
                    Ok(_) => {
                        self.cal_selected_date = due_date;
                        self.cal_year = due_date.year();
                        self.cal_month = due_date.month();
                        if self.selected_category_id.is_none()
                            && self.smart_filter == SmartListFilter::Today
                            && due_date > today
                        {
                            self.smart_filter = SmartListFilter::Scheduled;
                        }
                        self.force_refresh(store);
                        self.show_toast(
                            format!(
                                "✓ \"{trimmed}\" 등록 완료 ({}_schedule.json)",
                                due_date.format("%Y-%m-%d")
                            ),
                            false,
                        );
                        close_modal = true;
                    }
                    Err(e) => {
                        self.show_toast(format!("일정 등록 실패: {e}"), true);
                    }
                }
            }
        }

        if close_modal {
            self.show_new_todo_modal = false;
        }
    }

    fn render_add_category_modal(&mut self, ctx: &egui::Context, store: &Store) {
        let mut close_modal = false;
        let mut submit_cat = false;

        egui::Window::new(format!("{} 새 목록 만들기", icons::FOLDER_PLUS))
            .collapsible(false)
            .resizable(false)
            .fixed_size(egui::vec2(340.0, 200.0))
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.label(RichText::new("목록 이름").size(13.0).strong().color(MUTED));
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut self.new_cat_name)
                        .hint_text("예: 프로젝트, 쇼핑, 독서")
                        .desired_width(ui.available_width()),
                );
                if (resp.lost_focus() || resp.has_focus())
                    && ui.input(|i| i.key_pressed(Key::Enter))
                {
                    submit_cat = true;
                }

                ui.add_space(8.0);
                ui.label(RichText::new("색상 선택").size(13.0).strong().color(MUTED));
                ui.horizontal(|ui| {
                    for (_label, hex) in APPLE_PALETTE {
                        let (r, resp) =
                            ui.allocate_exact_size(egui::vec2(26.0, 26.0), egui::Sense::click());
                        ui.painter().circle_filled(r.center(), 10.5, theme::rgb(*hex));
                        if self.new_cat_color == *hex {
                            ui.painter()
                                .circle_stroke(r.center(), 12.5, Stroke::new(2.0_f32, PRIMARY));
                        }
                        if resp.clicked() {
                            self.new_cat_color = *hex;
                        }
                    }
                });

                ui.add_space(12.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if apple_button_primary(ui, &format!("{} 추가", icons::PLUS)).clicked() {
                        submit_cat = true;
                    }
                    if apple_button_outline(ui, "취소").clicked() {
                        close_modal = true;
                    }
                });
            });

        if submit_cat && !self.new_cat_name.trim().is_empty() {
            let _ = store.add_category(self.new_cat_name.trim(), self.new_cat_color);
            self.force_refresh(store);
            close_modal = true;
        }
        if close_modal {
            self.show_add_category_modal = false;
        }
    }

    // ═══════════════════════════════════════════════════════════════════════
    //  모드 3: 데이터(JSON & SQLite) 관리 및 CLI 가이드 탭
    // ═══════════════════════════════════════════════════════════════════════
    fn show_settings_mode(
        &mut self,
        ui: &mut egui::Ui,
        store: &Store,
        settings: &mut Settings,
        on_toggle_overlay: &mut impl FnMut(),
    ) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            // 1. 일자별 JSON 파일 (`{date}_schedule.json`) & SQLite 스토어 관리 카드
            egui::Frame::new()
                .fill(SURFACE)
                .corner_radius(CornerRadius::same(18))
                .stroke(Stroke::new(1.0_f32, BORDER))
                .inner_margin(egui::Margin::same(22))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(
                        RichText::new(format!(
                            "{} 일자별 {{date}}_schedule.json & SQLite 데이터 관리",
                            icons::FLOPPY_DISK
                        ))
                        .size(18.0)
                        .strong()
                        .color(FOREGROUND),
                    );
                    ui.label(
                        RichText::new(format!(
                            "로컬 JSON 저장 경로: {}  |  SQLite DB 경로: {}",
                            paths::schedules_dir().display(),
                            paths::db_path().display()
                        ))
                        .size(13.0)
                        .color(MUTED),
                    );
                    ui.add_space(10.0);

                    ui.horizontal(|ui| {
                        if apple_button_primary(ui, "로컬 JSON 전체 동기화 (불러오기 & 저장)")
                            .clicked()
                        {
                            let _ = store.sync_all_json_files();
                            self.force_refresh(store);
                            self.show_toast("✓ 모든 {date}_schedule.json 파일과 SQLite DB가 동기화되었습니다.", false);
                        }

                        if apple_button_outline(ui, "JSON 저장 폴더 열기").clicked() {
                            let _ = std::process::Command::new("explorer")
                                .arg(paths::schedules_dir())
                                .spawn();
                        }
                    });

                    ui.add_space(10.0);
                    ui.label(
                        RichText::new(format!(
                            "저장된 일자별 JSON 파일 ({}개):",
                            self.json_files.len()
                        ))
                        .size(13.0)
                        .strong()
                        .color(FOREGROUND),
                    );

                    if self.json_files.is_empty() {
                        ui.label(
                            RichText::new("아직 생성된 {date}_schedule.json 파일이 없습니다. 할 일을 추가하면 즉시 생성됩니다.")
                                .size(12.5)
                                .color(MUTED),
                        );
                    } else {
                        let mut jump_to_date: Option<NaiveDate> = None;
                        for (date_str, path, cnt) in &self.json_files {
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(format!("• {}_schedule.json ({}건)", date_str, cnt))
                                        .monospace()
                                        .color(PRIMARY),
                                );
                                ui.label(
                                    RichText::new(path.display().to_string())
                                        .size(11.5)
                                        .color(MUTED),
                                );
                                if ui.small_button("캘린더에서 보기").clicked() {
                                    if let Ok(d) = NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
                                        jump_to_date = Some(d);
                                    }
                                }
                            });
                        }
                        if let Some(d) = jump_to_date {
                            self.cal_selected_date = d;
                            self.cal_year = d.year();
                            self.cal_month = d.month();
                            self.view_mode = ViewMode::Calendar;
                            self.force_refresh(store);
                        }
                    }
                });

            ui.add_space(16.0);

            // 2. 웹캠 핫존 드롭다운 설정 카드
            egui::Frame::new()
                .fill(SURFACE)
                .corner_radius(CornerRadius::same(18))
                .stroke(Stroke::new(1.0_f32, BORDER))
                .inner_margin(egui::Margin::same(22))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(
                        RichText::new(format!(
                            "{} 웹캠 핫존 드롭다운 (Dynamic Island)",
                            icons::WEBCAM
                        ))
                        .size(18.0)
                        .strong()
                        .color(FOREGROUND),
                    );
                    if ui
                        .checkbox(
                            &mut settings.hotzone_enabled,
                            "화면 상단 중앙 웹캠 위치 마우스 호버 시 오늘의 할 일 자동 열기",
                        )
                        .changed()
                    {
                        let _ = settings.save();
                    }
                    ui.add_space(6.0);
                    if apple_button_outline(
                        ui,
                        &format!("{} 상단 오버레이 지금 열기/닫기", icons::WEBCAM),
                    )
                    .clicked()
                    {
                        on_toggle_overlay();
                    }
                });

            ui.add_space(16.0);

            // 3. OpenLab 터미널 CLI 명령어 가이드
            egui::Frame::new()
                .fill(SURFACE)
                .corner_radius(CornerRadius::same(18))
                .stroke(Stroke::new(1.0_f32, BORDER))
                .inner_margin(egui::Margin::same(22))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(
                        RichText::new(format!(
                            "{} 일반 터미널 & AI 연동 CLI 명령어 가이드",
                            icons::TERMINAL_WINDOW
                        ))
                        .size(18.0)
                        .strong()
                        .color(FOREGROUND),
                    );
                    ui.add_space(10.0);

                    let examples = [
                        ("1. 일반 터미널에서 대화형 CLI 실행", "openlab start -s"),
                        ("2. 일반 터미널에서 GUI 프로그램 실행", "openlab start -s program"),
                        (
                            "3. CLI로 일정 직접 추가 ({date}_schedule.json 자동 생성)",
                            r#"openlab start -s add "팀 회의" --date 2026-11-10 --time 14:00 --category 업무"#,
                        ),
                        (
                            "4. 일자별 일정 조회 및 JSON 동기화",
                            "openlab start -s list --date 2026-11-10",
                        ),
                    ];

                    for (title, cmd) in examples {
                        ui.label(RichText::new(title).size(13.0).strong().color(FOREGROUND));
                        egui::Frame::new()
                            .fill(CANVAS)
                            .corner_radius(CornerRadius::same(8))
                            .stroke(Stroke::new(1.0_f32, BORDER))
                            .inner_margin(egui::Margin::symmetric(12, 8))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(cmd).monospace().color(PRIMARY));
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| {
                                            if ui.small_button("복사").clicked() {
                                                ui.ctx().copy_text(cmd.to_string());
                                                self.show_toast(
                                                    "✓ 명령어가 클립보드에 복사되었습니다.",
                                                    false,
                                                );
                                            }
                                        },
                                    );
                                });
                            });
                        ui.add_space(6.0);
                    }
                });
        });
    }
}
