//! 핫존 진입 시 상단에서 슬라이드 다운되는 투두 오버레이 패널 렌더링.
//! Apple 미리알림 (Apple Reminders) 및 다이내믹 아일랜드 감성의 라이트 글래스 카드.

use chrono::Local;
use eframe::egui::{self, Key};
use openschedule_core::{
    cli::parse_flexible_time,
    model::{Category, Status, Todo, TodoDraft},
    theme::{
        self, apple_button_outline, apple_button_primary, category_chip, circular_checkbox,
        icons, status_pill, BORDER, FOREGROUND, MUTED, PRIMARY, SUCCESS, SURFACE,
    },
    DateFilter, Settings, Store, TodoQuery,
};

pub struct TodoOverlayView {
    pub todos: Vec<Todo>,
    pub categories: Vec<Category>,
    pub quick_add_input: String,
    pub quick_add_time: String,
    last_version: i64,
}

impl TodoOverlayView {
    pub fn new() -> Self {
        Self {
            todos: vec![],
            categories: vec![],
            quick_add_input: String::new(),
            quick_add_time: String::new(),
            last_version: -1,
        }
    }

    pub fn force_refresh(&mut self, store: &Store) {
        self.last_version = -1;
        self.refresh_if_needed(store);
    }

    pub fn refresh_if_needed(&mut self, store: &Store) {
        let v = store.data_version();
        if v != self.last_version || self.last_version == -1 {
            self.last_version = v;
            self.categories = store.categories().unwrap_or_default();
            let q = TodoQuery {
                date: DateFilter::TodayAndOverdue,
                ..Default::default()
            };
            self.todos = store.query(&q).unwrap_or_default();
        }
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        store: &Store,
        _settings: &Settings,
        on_open_manager: impl FnOnce(),
    ) {
        self.refresh_if_needed(store);

        let now = Local::now();
        let today = now.date_naive();
        let total_count = self.todos.len();
        let done_count = self.todos.iter().filter(|t| t.status.is_finished()).count();

        ui.vertical(|ui| {
            // ── 헤더: Apple Reminders 데이트 칩 + 진척도 + 관리 앱 바로가기 ──
            ui.horizontal(|ui| {
                // 파란색 캘린더 원형 배지
                let (r, _) = ui.allocate_exact_size(egui::vec2(28.0, 28.0), egui::Sense::hover());
                ui.painter().circle_filled(r.center(), 14.0, PRIMARY);
                let day_str = now.format("%d").to_string();
                let day_galley = ui.painter().layout_no_wrap(
                    day_str,
                    egui::FontId::proportional(13.0),
                    egui::Color32::WHITE,
                );
                ui.painter().galley(
                    r.center() - day_galley.size() / 2.0,
                    day_galley,
                    egui::Color32::WHITE,
                );

                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new("오늘의 미리알림")
                            .size(16.0)
                            .strong()
                            .color(FOREGROUND),
                    );
                    let date_korean = now.format("%Y-%m-%d").to_string();
                    ui.label(egui::RichText::new(date_korean).small().color(MUTED));
                });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if apple_button_outline(ui, &format!("관리 창 {}", icons::ARROW_SQUARE_OUT)).clicked() {
                        on_open_manager();
                    }

                    if total_count > 0 {
                        let pct = (done_count * 100) / total_count;
                        let progress_text = format!("{done_count}/{total_count} 완료 ({pct}%)");
                        let color = if pct == 100 { SUCCESS } else { PRIMARY };
                        ui.label(
                            egui::RichText::new(progress_text)
                                .small()
                                .strong()
                                .color(color),
                        );
                    }
                });
            });

            ui.add_space(8.0);

            // ── 오버레이 빠른 추가 인라인 입력창 ──
            let mut submit_add = false;
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::hover());
                ui.painter().circle_stroke(r.center(), 8.0, egui::Stroke::new(1.5_f32, PRIMARY));
                let plus_g = ui.painter().layout_no_wrap(
                    icons::PLUS.into(),
                    egui::FontId::proportional(11.0),
                    PRIMARY,
                );
                ui.painter().galley(r.center() - plus_g.size() / 2.0, plus_g, PRIMARY);

                let resp = ui.add(
                    egui::TextEdit::singleline(&mut self.quick_add_input)
                        .hint_text("오늘 일정 빠르게 추가... (Enter)")
                        .desired_width(ui.available_width() - 145.0),
                );
                if (resp.lost_focus() || resp.has_focus()) && ui.input(|i| i.key_pressed(Key::Enter)) {
                    submit_add = true;
                }

                let time_resp = ui.add(
                    egui::TextEdit::singleline(&mut self.quick_add_time)
                        .hint_text("시간(14:00)")
                        .desired_width(68.0),
                );
                if (time_resp.lost_focus() || time_resp.has_focus()) && ui.input(|i| i.key_pressed(Key::Enter)) {
                    submit_add = true;
                }

                if apple_button_primary(ui, &format!("{} 추가", icons::PLUS)).clicked() {
                    submit_add = true;
                }
            });

            if submit_add && !self.quick_add_input.trim().is_empty() {
                let mut draft = TodoDraft::new(self.quick_add_input.trim(), today);
                if !self.quick_add_time.trim().is_empty() {
                    if let Ok(Some(t)) = parse_flexible_time(&self.quick_add_time) {
                        draft.due_time = Some(t);
                    }
                }
                let _ = store.add(&draft);
                self.quick_add_input.clear();
                self.quick_add_time.clear();
                self.force_refresh(store);
            }

            ui.add_space(6.0);
            ui.separator();
            ui.add_space(6.0);

            if self.todos.is_empty() {
                ui.vertical_centered(|ui| {
                    ui.add_space(25.0);
                    ui.label(
                        egui::RichText::new(format!("{} 오늘 예정된 일정이 없습니다", icons::CHECK_CIRCLE))
                            .size(14.5)
                            .color(MUTED),
                    );
                    ui.add_space(25.0);
                });
                return;
            }

            // ── 할 일 체크리스트 ──
            let mut status_toggle = None;

            egui::ScrollArea::vertical()
                .max_height(310.0)
                .show(ui, |ui| {
                    for todo in &self.todos {
                        let cat_name = todo
                            .category_id
                            .and_then(|cid| self.categories.iter().find(|c| c.id == cid))
                            .map(|c| c.name.as_str())
                            .unwrap_or("미분류");
                        let cat_color = todo
                            .category_id
                            .and_then(|cid| self.categories.iter().find(|c| c.id == cid))
                            .map(|c| theme::rgb(c.color))
                            .unwrap_or(PRIMARY);

                        egui::Frame::new()
                            .fill(SURFACE)
                            .corner_radius(egui::CornerRadius::same(10))
                            .stroke(egui::Stroke::new(1.0_f32, BORDER))
                            .inner_margin(egui::Margin::symmetric(12, 8))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    // 1. 원형 체크박스 토글
                                    if circular_checkbox(ui, todo.status, cat_color).clicked() {
                                        let next = if todo.status == Status::Done {
                                            Status::InProgress
                                        } else {
                                            Status::Done
                                        };
                                        status_toggle = Some((todo.id, next));
                                    }

                                    // 2. 4대 상태 순환 배지
                                    if status_pill(ui, todo.status).clicked() {
                                        status_toggle = Some((todo.id, todo.status.next()));
                                    }

                                    // 3. 내용
                                    ui.vertical(|ui| {
                                        ui.horizontal(|ui| {
                                            let title_color = if todo.status.is_finished() {
                                                MUTED
                                            } else if todo.is_overdue(today) {
                                                theme::DANGER
                                            } else {
                                                FOREGROUND
                                            };

                                            let mut rt = egui::RichText::new(&todo.title)
                                                .color(title_color)
                                                .size(14.0);
                                            if todo.status.is_finished() {
                                                rt = rt.strikethrough();
                                            }
                                            ui.label(rt);

                                            category_chip(ui, cat_name, cat_color);
                                        });

                                        let mut sub_line = String::new();
                                        if let Some(t) = todo.due_time {
                                            sub_line.push_str(&format!("{} {} ", icons::CLOCK, t.format("%H:%M")));
                                        }
                                        if todo.is_overdue(today) && !todo.status.is_finished() {
                                            sub_line.push_str(&format!(
                                                "{} 기한초과({}) ",
                                                icons::WARNING,
                                                todo.due_date.format("%m-%d")
                                            ));
                                        }
                                        if !todo.condition_note.is_empty() {
                                            sub_line.push_str(&format!("조건: {} ", todo.condition_note));
                                        }

                                        if !sub_line.is_empty() {
                                            ui.label(egui::RichText::new(sub_line).small().color(MUTED));
                                        }
                                    });
                                });
                            });
                        ui.add_space(4.0);
                    }
                });

            if let Some((id, next_st)) = status_toggle {
                let _ = store.set_status(id, next_st);
                self.force_refresh(store);
            }
        });
    }
}
