//! OpenSchedule 메인 데스크톱 UI (Apple Reminders 3-Column 디자인 + Phosphor Icons).
//! 순수 Rust (egui) 기반으로 SQLite Store와 직접 연동되어 지연이나 통신 오류 없이 작동합니다.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use chrono::{Datelike, Duration as ChronoDuration, Local, NaiveDate};
use eframe::egui::{self, Color32, CornerRadius, Key, RichText, Stroke};
use openschedule_core::{
    cli::{parse_flexible_date, parse_flexible_time},
    model::{Category, Status, Todo, TodoDraft},
    theme::{
        self, apple_button_danger, apple_button_outline, apple_button_primary, category_chip,
        circular_checkbox, icons, quick_chip_button, smart_card, status_pill,
        status_pill_selectable, BORDER, CANVAS, DANGER, FOREGROUND, MUTED, PRIMARY, PURPLE,
        SECONDARY, SUCCESS, SURFACE, WARNING,
    },
    DateFilter, Settings, SmartCounts, Store, TodoQuery,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveTab {
    Reminders,
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemindersViewMode {
    TodoList,
    Calendar,
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
    pub active_tab: ActiveTab,
    pub view_mode: RemindersViewMode,
    pub smart_filter: SmartListFilter,
    pub selected_category_id: Option<i64>,
    pub status_filter: Option<Status>,
    pub search_query: String,

    // 캘린더 뷰 상태
    pub cal_year: i32,
    pub cal_month: u32,
    pub cal_selected_date: NaiveDate,
    pub month_todos: HashMap<NaiveDate, Vec<Todo>>,

    // 캐시된 스토어 데이터
    pub todos: Vec<Todo>,
    pub categories: Vec<Category>,
    pub smart_counts: SmartCounts,
    pub category_counts: HashMap<i64, usize>,
    last_data_version: i64,
    last_refresh_at: Instant,

    // 인라인 일정 빠른 추가 입력값
    pub quick_title: String,
    pub quick_date_str: String,
    pub quick_time_str: String,
    pub quick_category_id: Option<i64>,
    pub quick_memo: String,

    // 우측 세부사항 인스펙터
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
        let mut s = Self {
            active_tab: ActiveTab::Reminders,
            view_mode: RemindersViewMode::TodoList,
            smart_filter: SmartListFilter::Today,
            selected_category_id: None,
            status_filter: None,
            search_query: String::new(),

            cal_year: today.year(),
            cal_month: today.month(),
            cal_selected_date: today,
            month_todos: HashMap::new(),

            todos: vec![],
            categories: vec![],
            smart_counts: SmartCounts::default(),
            category_counts: HashMap::new(),
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

            let date_filter = if self.view_mode == RemindersViewMode::Calendar {
                DateFilter::Day(self.cal_selected_date)
            } else if self.selected_category_id.is_some() {
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
                category: if self.view_mode == RemindersViewMode::Calendar {
                    self.selected_category_id.map(Some)
                } else {
                    self.selected_category_id.map(Some)
                },
                statuses: self.status_filter.into_iter().collect(),
                search: self.search_query.clone(),
            };
            self.todos = store.query(&q).unwrap_or_default();
        }
    }

    fn open_new_todo_modal(&mut self) {
        let default_date = if self.view_mode == RemindersViewMode::Calendar {
            self.cal_selected_date.format("%Y-%m-%d").to_string()
        } else {
            Local::now().date_naive().format("%Y-%m-%d").to_string()
        };
        self.modal_title.clear();
        self.modal_date_str = default_date;
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
            self.open_new_todo_modal();
        }

        // ── 상단 네비게이션 바 ──
        egui::TopBottomPanel::top("top_navbar")
            .exact_height(54.0)
            .frame(
                egui::Frame::new()
                    .fill(SURFACE)
                    .inner_margin(egui::Margin::symmetric(22, 8))
                    .stroke(Stroke::new(1.0_f32, BORDER)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    // 브랜드 로고
                    let (logo_rect, _) =
                        ui.allocate_exact_size(egui::vec2(24.0, 24.0), egui::Sense::hover());
                    ui.painter()
                        .rect_filled(logo_rect, CornerRadius::same(6), PRIMARY);
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
                        RichText::new("OpenSchedule")
                            .size(16.5)
                            .strong()
                            .color(FOREGROUND),
                    );

                    ui.add_space(24.0);

                    // 네비게이션 탭
                    let tab_reminders = self.active_tab == ActiveTab::Reminders;
                    if quick_chip_button(
                        ui,
                        &format!("{} 미리알림", icons::LIST_CHECKS),
                        tab_reminders,
                    )
                    .clicked()
                    {
                        self.active_tab = ActiveTab::Reminders;
                    }

                    let tab_settings = self.active_tab == ActiveTab::Settings;
                    if quick_chip_button(
                        ui,
                        &format!("{} 설정 & CLI", icons::GEAR),
                        tab_settings,
                    )
                    .clicked()
                    {
                        self.active_tab = ActiveTab::Settings;
                    }

                    // 토스트 메시지 표시
                    if let Some((ref msg, ts, is_err)) = self.toast_message {
                        if ts.elapsed() < Duration::from_secs(4) {
                            ui.add_space(12.0);
                            let col = if is_err { DANGER } else { SUCCESS };
                            ui.label(RichText::new(msg).size(13.0).strong().color(col));
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
                            self.open_new_todo_modal();
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
                    .inner_margin(egui::Margin::symmetric(18, 16)),
            )
            .show(ctx, |ui| match self.active_tab {
                ActiveTab::Reminders => self.show_reminders_tab(ui, store),
                ActiveTab::Settings => {
                    self.show_settings_tab(ui, store, settings, &mut on_toggle_overlay)
                }
            });

        // ── 모달 다이얼로그 렌더링 ──
        if self.show_new_todo_modal {
            self.render_new_todo_modal(ctx, store);
        }
        if self.show_add_category_modal {
            self.render_add_category_modal(ctx, store);
        }
    }

    fn show_reminders_tab(&mut self, ui: &mut egui::Ui, store: &Store) {
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
                    ui.set_width(240.0);
                    ui.set_min_height(avail_h - 32.0);
                    self.render_sidebar(ui, store);
                });

            // 인스펙터가 열려 있으면 우측 폭 확보
            let inspector_open = self.selected_todo_id.is_some();
            let center_w = if inspector_open {
                (ui.available_width() - 326.0).max(340.0)
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

            // 3. 우측 세부사항 인스펙터 카드 (열려 있을 때만)
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

    fn render_sidebar(&mut self, ui: &mut egui::Ui, store: &Store) {
        let today_day = Local::now().format("%d").to_string();
        let card_w = 114.0;

        // 2x2 스마트 카드 그리드
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let sel_today = self.view_mode == RemindersViewMode::TodoList
                && self.selected_category_id.is_none()
                && self.smart_filter == SmartListFilter::Today;
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
                self.view_mode = RemindersViewMode::TodoList;
                self.selected_category_id = None;
                self.smart_filter = SmartListFilter::Today;
                self.force_refresh(store);
            }

            let sel_sched = self.view_mode == RemindersViewMode::TodoList
                && self.selected_category_id.is_none()
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
                self.view_mode = RemindersViewMode::TodoList;
                self.selected_category_id = None;
                self.smart_filter = SmartListFilter::Scheduled;
                self.force_refresh(store);
            }
        });

        ui.add_space(6.0);

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let sel_all = self.view_mode == RemindersViewMode::TodoList
                && self.selected_category_id.is_none()
                && self.smart_filter == SmartListFilter::All;
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
                self.view_mode = RemindersViewMode::TodoList;
                self.selected_category_id = None;
                self.smart_filter = SmartListFilter::All;
                self.force_refresh(store);
            }

            let sel_comp = self.view_mode == RemindersViewMode::TodoList
                && self.selected_category_id.is_none()
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
                self.view_mode = RemindersViewMode::TodoList;
                self.selected_category_id = None;
                self.smart_filter = SmartListFilter::Completed;
                self.force_refresh(store);
            }
        });

        ui.add_space(16.0);

        // 내 목록 헤더
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

        // 카테고리 목록
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

        // 1. 상단 타이틀 & [투두리스트 / 캘린더 뷰] 모드 전환 & 검색창
        let (title_text, title_color) = if self.view_mode == RemindersViewMode::Calendar {
            (
                format!(
                    "{} ({}_schedule.json)",
                    self.cal_selected_date.format("%Y-%m-%d"),
                    self.cal_selected_date.format("%Y-%m-%d")
                ),
                PRIMARY,
            )
        } else if let Some(cid) = self.selected_category_id {
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
                    .size(if self.view_mode == RemindersViewMode::Calendar {
                        22.0
                    } else {
                        28.0
                    })
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
                        .desired_width(160.0),
                );
                if resp.changed() {
                    self.force_refresh(store);
                }

                ui.add_space(6.0);

                // [캘린더 뷰 / 투두리스트] 모드 전환
                let is_cal = self.view_mode == RemindersViewMode::Calendar;
                if quick_chip_button(ui, &format!("{} 캘린더 뷰", icons::CALENDAR), is_cal)
                    .clicked()
                {
                    self.view_mode = RemindersViewMode::Calendar;
                    self.quick_date_str = self.cal_selected_date.format("%Y-%m-%d").to_string();
                    self.force_refresh(store);
                }

                let is_todo = self.view_mode == RemindersViewMode::TodoList;
                if quick_chip_button(ui, &format!("{} 투두리스트", icons::LIST_CHECKS), is_todo)
                    .clicked()
                {
                    self.view_mode = RemindersViewMode::TodoList;
                    self.force_refresh(store);
                }
            });
        });

        ui.add_space(10.0);

        // 캘린더 뷰 모드일 때 월간 달력 그리드 표시
        if self.view_mode == RemindersViewMode::Calendar {
            self.render_calendar_grid(ui, store, today);
            ui.add_space(10.0);
        }

        // 2. 인라인 일정 추가 박스 (제목 + 날짜 + 시간 + 카테고리 + 메모 + 즉시 등록 버튼)
        let mut submit_quick_add = false;
        egui::Frame::new()
            .fill(CANVAS)
            .corner_radius(CornerRadius::same(12))
            .stroke(Stroke::new(1.0_f32, BORDER))
            .inner_margin(egui::Margin::symmetric(14, 10))
            .show(ui, |ui| {
                // 1행: 제목 입력 + [+ 일정 추가] 버튼
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
                            .hint_text("새로운 일정 제목을 입력하고 Enter 또는 [일정 추가]를 누르세요...")
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

                // 2행: 날짜 칩 & 입력 + 시간 + 카테고리 + 메모
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    ui.label(
                        RichText::new(format!("{} 날짜:", icons::CALENDAR))
                            .small()
                            .color(MUTED),
                    );

                    let today_s = today.format("%Y-%m-%d").to_string();
                    let tmr_s = (today + ChronoDuration::days(1))
                        .format("%Y-%m-%d")
                        .to_string();
                    let nextw_s = (today + ChronoDuration::days(7))
                        .format("%Y-%m-%d")
                        .to_string();

                    if quick_chip_button(ui, "오늘", self.quick_date_str == today_s).clicked() {
                        self.quick_date_str = today_s;
                        if self.view_mode == RemindersViewMode::Calendar {
                            self.cal_selected_date = today;
                            self.cal_year = today.year();
                            self.cal_month = today.month();
                            self.force_refresh(store);
                        }
                    }
                    if quick_chip_button(ui, "내일", self.quick_date_str == tmr_s).clicked() {
                        self.quick_date_str = tmr_s;
                        if self.view_mode == RemindersViewMode::Calendar {
                            let d = today + ChronoDuration::days(1);
                            self.cal_selected_date = d;
                            self.cal_year = d.year();
                            self.cal_month = d.month();
                            self.force_refresh(store);
                        }
                    }
                    if quick_chip_button(ui, "다음주", self.quick_date_str == nextw_s).clicked() {
                        self.quick_date_str = nextw_s;
                        if self.view_mode == RemindersViewMode::Calendar {
                            let d = today + ChronoDuration::days(7);
                            self.cal_selected_date = d;
                            self.cal_year = d.year();
                            self.cal_month = d.month();
                            self.force_refresh(store);
                        }
                    }

                    ui.add(
                        egui::TextEdit::singleline(&mut self.quick_date_str)
                            .hint_text("YYYY-MM-DD")
                            .desired_width(86.0),
                    );

                    ui.label(
                        RichText::new(format!("{} 시간:", icons::CLOCK))
                            .small()
                            .color(MUTED),
                    );
                    ui.add(
                        egui::TextEdit::singleline(&mut self.quick_time_str)
                            .hint_text("예: 14:00")
                            .desired_width(64.0),
                    );

                    ui.label(
                        RichText::new(format!("{} 목록:", icons::TAG))
                            .small()
                            .color(MUTED),
                    );
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

                        if self.view_mode == RemindersViewMode::Calendar {
                            self.cal_selected_date = due_date;
                            self.cal_year = due_date.year();
                            self.cal_month = due_date.month();
                        } else if self.selected_category_id.is_none()
                            && self.smart_filter == SmartListFilter::Today
                            && due_date > today
                        {
                            self.smart_filter = SmartListFilter::Scheduled;
                        }
                        self.force_refresh(store);
                        self.show_toast(format!("✓ \"{trimmed}\" 일정이 추가되었습니다."), false);
                    }
                    Err(e) => {
                        self.show_toast(format!("일정 추가 실패: {e}"), true);
                    }
                }
            }
        }

        ui.add_space(8.0);

        // 3. 4대 세부 상태 필터 알약
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

        // 4. 할 일 체크리스트 스크롤 영역
        if self.todos.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(36.0);
                ui.label(
                    RichText::new(icons::CHECK_CIRCLE)
                        .size(36.0)
                        .color(PRIMARY),
                );
                ui.add_space(6.0);
                ui.label(
                    RichText::new("등록된 일정이 없습니다")
                        .size(16.0)
                        .strong()
                        .color(MUTED),
                );
                ui.label(
                    RichText::new(
                        "위의 입력란에서 바로 추가하거나 우측 상단 [+ 새 일정 추가] 버튼을 눌러 등록하세요.",
                    )
                    .size(13.0)
                    .color(MUTED),
                );
            });
            return;
        }

        let mut toggle_status_action: Option<(i64, Status)> = None;
        let mut select_todo_action: Option<Todo> = None;

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
                                // 원형 체크박스
                                if circular_checkbox(ui, todo.status, cat_color).clicked() {
                                    let next = if todo.status == Status::Done {
                                        Status::InProgress
                                    } else {
                                        Status::Done
                                    };
                                    toggle_status_action = Some((todo.id, next));
                                }

                                // 본문 클릭 영역
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

                                    // 메타데이터 (기한, 시간, 조건, 메모)
                                    ui.horizontal_wrapped(|ui| {
                                        ui.label(
                                            RichText::new(format!(
                                                "{} {}",
                                                icons::CALENDAR,
                                                todo.due_date.format("%Y-%m-%d")
                                            ))
                                            .size(12.0)
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

                                // 우측 상태 배지 & 상세 아이콘
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui
                                            .add(
                                                egui::Button::new(
                                                    RichText::new(icons::INFO)
                                                        .size(14.0)
                                                        .color(PRIMARY),
                                                )
                                                .frame(false),
                                            )
                                            .on_hover_text("세부사항 편집")
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
        if let Some(todo) = select_todo_action {
            self.select_todo_for_inspector(&todo);
        }
    }

    fn render_calendar_grid(&mut self, ui: &mut egui::Ui, store: &Store, today: NaiveDate) {
        egui::Frame::new()
            .fill(CANVAS)
            .corner_radius(CornerRadius::same(12))
            .stroke(Stroke::new(1.0_f32, BORDER))
            .inner_margin(egui::Margin::symmetric(12, 10))
            .show(ui, |ui| {
                // 월 이동 헤더
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
                    ui.label(
                        RichText::new(format!("{}년 {:02}월", self.cal_year, self.cal_month))
                            .size(16.0)
                            .strong()
                            .color(FOREGROUND),
                    );
                    if quick_chip_button(ui, "다음 달 ▶", false).clicked() {
                        if self.cal_month == 12 {
                            self.cal_year += 1;
                            self.cal_month = 1;
                        } else {
                            self.cal_month += 1;
                        }
                        self.force_refresh(store);
                    }
                    if quick_chip_button(ui, "오늘", self.cal_selected_date == today).clicked() {
                        self.cal_year = today.year();
                        self.cal_month = today.month();
                        self.cal_selected_date = today;
                        self.quick_date_str = today.format("%Y-%m-%d").to_string();
                        self.force_refresh(store);
                    }
                });

                ui.add_space(6.0);

                let first_day =
                    NaiveDate::from_ymd_opt(self.cal_year, self.cal_month, 1).unwrap_or(today);
                let start_offset = first_day.weekday().num_days_from_sunday() as i64;
                let grid_start = first_day - ChronoDuration::days(start_offset);

                let cell_w = ((ui.available_width() - 36.0) / 7.0).max(42.0);
                let cell_h = 44.0;

                // 요일 헤더
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    for (idx, wd) in ["일", "월", "화", "수", "목", "금", "토"].iter().enumerate() {
                        let col = if idx == 0 {
                            DANGER
                        } else if idx == 6 {
                            PRIMARY
                        } else {
                            MUTED
                        };
                        ui.allocate_ui(egui::vec2(cell_w, 18.0), |ui| {
                            ui.centered_and_justified(|ui| {
                                ui.label(RichText::new(*wd).size(12.0).strong().color(col));
                            });
                        });
                    }
                });

                let mut clicked_date: Option<NaiveDate> = None;

                for week in 0..6 {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        for dow in 0..7 {
                            let d = grid_start + ChronoDuration::days(week * 7 + dow);
                            let in_month = d.month() == self.cal_month;
                            let is_sel = d == self.cal_selected_date;
                            let is_today = d == today;
                            let day_todos = self.month_todos.get(&d);
                            let count = day_todos.map(|v| v.len()).unwrap_or(0);

                            let fill = if is_sel {
                                Color32::from_rgb(0xEA, 0xF2, 0xFD)
                            } else if in_month {
                                SURFACE
                            } else {
                                CANVAS
                            };
                            let stroke = if is_sel {
                                Stroke::new(1.8_f32, PRIMARY)
                            } else if is_today {
                                Stroke::new(1.2_f32, PRIMARY)
                            } else {
                                Stroke::new(1.0_f32, BORDER)
                            };

                            let (rect, resp) = ui.allocate_exact_size(
                                egui::vec2(cell_w, cell_h),
                                egui::Sense::click(),
                            );
                            ui.painter()
                                .rect(rect, CornerRadius::same(8), fill, stroke, egui::StrokeKind::Inside);

                            let day_col = if !in_month {
                                MUTED
                            } else if is_sel || is_today {
                                PRIMARY
                            } else if dow == 0 {
                                DANGER
                            } else {
                                FOREGROUND
                            };

                            let day_g = ui.painter().layout_no_wrap(
                                d.day().to_string(),
                                egui::FontId::proportional(13.0),
                                day_col,
                            );
                            ui.painter().galley(
                                egui::pos2(rect.left() + 7.0, rect.top() + 5.0),
                                day_g,
                                day_col,
                            );

                            if count > 0 {
                                let badge_g = ui.painter().layout_no_wrap(
                                    format!("{count}개"),
                                    egui::FontId::proportional(10.5),
                                    PRIMARY,
                                );
                                ui.painter().galley(
                                    egui::pos2(rect.left() + 7.0, rect.bottom() - 16.0),
                                    badge_g,
                                    PRIMARY,
                                );
                            }

                            if resp.clicked() {
                                clicked_date = Some(d);
                            }
                        }
                    });
                    ui.add_space(4.0);
                }

                if let Some(d) = clicked_date {
                    self.cal_selected_date = d;
                    self.cal_year = d.year();
                    self.cal_month = d.month();
                    self.quick_date_str = d.format("%Y-%m-%d").to_string();
                    self.force_refresh(store);
                }
            });
    }

    fn render_inspector(&mut self, ui: &mut egui::Ui, store: &Store) {
        let Some(todo_id) = self.selected_todo_id else {
            return;
        };
        let today = Local::now().date_naive();

        ui.horizontal(|ui| {
            ui.label(
                RichText::new("일정 세부사항")
                    .size(17.0)
                    .strong()
                    .color(FOREGROUND),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .button(RichText::new(icons::X).size(14.0).color(MUTED))
                    .clicked()
                {
                    self.selected_todo_id = None;
                }
            });
        });

        ui.add_space(10.0);

        ui.label(RichText::new("제목").size(12.5).strong().color(MUTED));
        ui.add(
            egui::TextEdit::singleline(&mut self.inspect_title)
                .desired_width(ui.available_width()),
        );

        ui.add_space(8.0);
        ui.label(RichText::new("메모").size(12.5).strong().color(MUTED));
        ui.add(
            egui::TextEdit::multiline(&mut self.inspect_memo)
                .desired_rows(3)
                .desired_width(ui.available_width()),
        );

        ui.add_space(8.0);
        ui.label(RichText::new("기한 일자").size(12.5).strong().color(MUTED));
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

        ui.add_space(8.0);
        ui.label(RichText::new("기한 시간").size(12.5).strong().color(MUTED));
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
            if quick_chip_button(ui, "시간 삭제", self.inspect_time_str.is_empty()).clicked() {
                self.inspect_time_str.clear();
            }
        });
        ui.add(
            egui::TextEdit::singleline(&mut self.inspect_time_str)
                .hint_text("HH:MM (예: 14:30)")
                .desired_width(ui.available_width()),
        );

        ui.add_space(8.0);
        ui.label(RichText::new("목록 (카테고리)").size(12.5).strong().color(MUTED));
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

        ui.add_space(8.0);
        ui.label(RichText::new("진행 상태").size(12.5).strong().color(MUTED));
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
            ui.add_space(8.0);
            ui.label(
                RichText::new("조건부 완료 조건")
                    .size(12.5)
                    .strong()
                    .color(PURPLE),
            );
            ui.add(
                egui::TextEdit::singleline(&mut self.inspect_condition)
                    .hint_text("예: 상사 승인 후, 검수 통과 시")
                    .desired_width(ui.available_width()),
            );
        }

        ui.add_space(16.0);
        ui.horizontal(|ui| {
            if apple_button_primary(ui, &format!("{} 저장", icons::FLOPPY_DISK)).clicked() {
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
                    self.show_toast("✓ 일정이 수정되었습니다.", false);
                }
            }

            if apple_button_danger(ui, &format!("{} 삭제", icons::TRASH)).clicked() {
                let _ = store.delete(todo_id);
                self.selected_todo_id = None;
                self.force_refresh(store);
                self.show_toast("✓ 일정이 삭제되었습니다.", false);
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
                ui.label(RichText::new("기한 날짜").size(13.0).strong().color(MUTED));
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
                        if self.view_mode == RemindersViewMode::Calendar {
                            self.cal_selected_date = due_date;
                            self.cal_year = due_date.year();
                            self.cal_month = due_date.month();
                        } else if self.selected_category_id.is_none()
                            && self.smart_filter == SmartListFilter::Today
                            && due_date > today
                        {
                            self.smart_filter = SmartListFilter::Scheduled;
                        }
                        self.force_refresh(store);
                        self.show_toast(format!("✓ \"{trimmed}\" 일정이 등록되었습니다."), false);
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
                            ui.painter().circle_stroke(
                                r.center(),
                                12.5,
                                Stroke::new(2.0_f32, PRIMARY),
                            );
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

    fn show_settings_tab(
        &mut self,
        ui: &mut egui::Ui,
        store: &Store,
        settings: &mut Settings,
        on_toggle_overlay: &mut impl FnMut(),
    ) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            // 1. 웹캠 핫존 드롭다운 설정 카드
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
                    ui.label(
                        RichText::new(
                            "화면 최상단 중앙(웹캠 위치)으로 마우스를 가져가면 오늘의 미리알림 카드가 자동으로 열립니다.",
                        )
                        .size(13.5)
                        .color(MUTED),
                    );
                    ui.add_space(10.0);

                    if ui
                        .checkbox(
                            &mut settings.hotzone_enabled,
                            "상단 웹캠 핫존 마우스 호버 시 자동 열기 활성화",
                        )
                        .changed()
                    {
                        let _ = settings.save();
                    }

                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if apple_button_outline(
                            ui,
                            &format!("{} 상단 오버레이 지금 열기/닫기", icons::WEBCAM),
                        )
                        .clicked()
                        {
                            on_toggle_overlay();
                        }

                        if apple_button_outline(
                            ui,
                            &format!("{} 로컬 JSON 새로고침", icons::FLOPPY_DISK),
                        )
                        .clicked()
                        {
                            if let Ok(cnt) = store.sync_all_json_files() {
                                self.force_refresh(store);
                                self.show_toast(
                                    format!("✓ {cnt}개 {{date}}_schedule.json 파일이 동기화되었습니다."),
                                    false,
                                );
                            }
                        }
                    });
                });

            ui.add_space(16.0);

            // 2. CLI 명령줄 도구 가이드 카드
            egui::Frame::new()
                .fill(SURFACE)
                .corner_radius(CornerRadius::same(18))
                .stroke(Stroke::new(1.0_f32, BORDER))
                .inner_margin(egui::Margin::same(22))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(
                        RichText::new(format!(
                            "{} CLI 명령줄 도구 (외부 AI 에이전트 연동)",
                            icons::TERMINAL_WINDOW
                        ))
                        .size(18.0)
                        .strong()
                        .color(FOREGROUND),
                    );
                    ui.label(
                        RichText::new(
                            "Claude Code, Antigravity, Cursor 등 AI 에이전트나 터미널에서 CLI로 일정을 즉시 추가·조회·완료할 수 있으며, 이 화면과 상단 오버레이에 실시간으로 반영됩니다.",
                        )
                        .size(13.5)
                        .color(MUTED),
                    );
                    ui.add_space(12.0);

                    let examples = [
                        (
                            "1. CLI 실행 / GUI 프로그램 실행",
                            r#"openlab start -s  /  openlab start -s program"#,
                        ),
                        (
                            "2. 새 일정 추가 (날짜, 시간, 카테고리, 메모 지정)",
                            r#"openschedule add "팀 주간 회의" --date tomorrow --time 14:00 --category 업무 --memo "분기 목표 점검""#,
                        ),
                        (
                            "3. 일정 목록 조회 및 JSON 출력 (AI 파싱용)",
                            r#"openschedule list --filter today --json"#,
                        ),
                        (
                            "4. 일정 완료 / 상태 변경 / 삭제",
                            r#"openschedule done 1"#,
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
