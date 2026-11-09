//! OpenLab / OpenSchedule 메인 진입점 (`openlab.exe`, `openschedule-agent.exe`).
//! - `openlab start -s`         : 대화형 CLI 일정 관리 프로그램 실행
//! - `openlab start -s program` : GUI 데스크톱 프로그램 ([투두리스트 / 캘린더 뷰] + 상단 웹캠 핫존 오버레이) 실행
//! - `openlab start -s <명령>`  : 단일 CLI 명령 실행 (AI 에이전트 및 스크립트 연동)

mod hotzone;
mod overlay_todo;
mod ui_main;

use std::time::{Duration, Instant};

use eframe::egui::{self, ViewportBuilder, ViewportCommand, ViewportId};
use openschedule_core::{
    cli::{parse_launch_action, LaunchAction},
    theme, Settings, Store,
};
use tray_icon::{
    menu::{Menu, MenuItem},
    TrayIcon, TrayIconBuilder,
};

struct OpenScheduleApp {
    settings: Settings,
    store: Store,
    main_view: ui_main::MainView,
    todo_overlay: overlay_todo::TodoOverlayView,

    overlay_visible: bool,
    hotzone_entered_at: Option<Instant>,
    leave_hotzone_at: Option<Instant>,

    _tray_icon: Option<TrayIcon>,
    tray_menu_open_id: tray_icon::menu::MenuId,
    tray_menu_quit_id: tray_icon::menu::MenuId,
}

impl OpenScheduleApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::install(&cc.egui_ctx);

        let settings = Settings::load();
        let store = Store::open_default().expect("SQLite 스토어 열기 실패");
        let main_view = ui_main::MainView::new(&store);
        let todo_overlay = overlay_todo::TodoOverlayView::new();
        let (tray, open_id, quit_id) = setup_tray();

        Self {
            settings,
            store,
            main_view,
            todo_overlay,
            overlay_visible: false,
            hotzone_entered_at: None,
            leave_hotzone_at: None,
            _tray_icon: tray,
            tray_menu_open_id: open_id,
            tray_menu_quit_id: quit_id,
        }
    }

    fn check_tray_events(&mut self, ctx: &egui::Context) {
        if let Ok(event) = tray_icon::menu::MenuEvent::receiver().try_recv() {
            if event.id == self.tray_menu_open_id {
                ctx.send_viewport_cmd(ViewportCommand::Visible(true));
                ctx.send_viewport_cmd(ViewportCommand::Focus);
            } else if event.id == self.tray_menu_quit_id {
                std::process::exit(0);
            }
        }
    }

    fn check_hotzone(&mut self) {
        if !self.settings.hotzone_enabled {
            return;
        }

        let Some(pt) = hotzone::get_cursor_pos() else {
            return;
        };
        let in_zone = hotzone::is_in_hotzone(&pt, &self.settings);
        let in_overlay = self.overlay_visible
            && hotzone::is_in_overlay_area(&pt, self.settings.panel_width.max(460.0), 420.0);
        let now = Instant::now();

        if in_zone {
            self.leave_hotzone_at = None;
            if self.hotzone_entered_at.is_none() {
                self.hotzone_entered_at = Some(now);
            } else if let Some(entered) = self.hotzone_entered_at {
                if now.duration_since(entered)
                    >= Duration::from_millis(self.settings.hotzone_dwell_ms)
                {
                    if !self.overlay_visible {
                        self.overlay_visible = true;
                        self.todo_overlay.force_refresh(&self.store);
                    }
                }
            }
        } else {
            self.hotzone_entered_at = None;
            if self.overlay_visible {
                if !in_overlay {
                    if self.leave_hotzone_at.is_none() {
                        self.leave_hotzone_at = Some(now);
                    } else if let Some(left) = self.leave_hotzone_at {
                        if now.duration_since(left)
                            >= Duration::from_millis(self.settings.hide_delay_ms)
                        {
                            self.overlay_visible = false;
                        }
                    }
                } else {
                    self.leave_hotzone_at = None;
                }
            }
        }
    }
}

impl eframe::App for OpenScheduleApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.check_tray_events(ctx);
        self.check_hotzone();

        let mut manual_toggle = false;
        self.main_view
            .show(ctx, &self.store, &mut self.settings, || {
                manual_toggle = true;
            });
        if manual_toggle {
            self.overlay_visible = !self.overlay_visible;
            if self.overlay_visible {
                self.todo_overlay.force_refresh(&self.store);
                self.leave_hotzone_at = None;
            }
        }

        // 상단 웹캠 핫존 드롭다운 오버레이 뷰포트
        if self.overlay_visible {
            let panel_w = self.settings.panel_width.max(460.0);
            let panel_h = 410.0;
            let screen_w = hotzone::primary_screen_width();
            let x = ((screen_w - panel_w) / 2.0).max(0.0);

            let vp_id = ViewportId::from_hash_of("webcam_hotzone_overlay");
            let builder = ViewportBuilder::default()
                .with_title("OpenSchedule Overlay")
                .with_decorations(false)
                .with_always_on_top()
                .with_taskbar(false)
                .with_resizable(false)
                .with_position([x, 6.0])
                .with_inner_size([panel_w, panel_h]);

            let mut focus_main = false;
            ctx.show_viewport_immediate(vp_id, builder, |vp_ctx, _class| {
                egui::CentralPanel::default()
                    .frame(
                        egui::Frame::new()
                            .fill(theme::SURFACE)
                            .corner_radius(egui::CornerRadius::same(18))
                            .stroke(egui::Stroke::new(1.5_f32, theme::PRIMARY))
                            .inner_margin(egui::Margin::same(16)),
                    )
                    .show(vp_ctx, |ui| {
                        self.todo_overlay
                            .show(ui, &self.store, &self.settings, || {
                                focus_main = true;
                            });
                    });
            });

            if focus_main {
                self.overlay_visible = false;
                self.main_view.force_refresh(&self.store);
                ctx.send_viewport_cmd(ViewportCommand::Visible(true));
                ctx.send_viewport_cmd(ViewportCommand::Focus);
            }
        }

        ctx.request_repaint_after(Duration::from_millis(50));
    }
}

fn setup_tray() -> (
    Option<TrayIcon>,
    tray_icon::menu::MenuId,
    tray_icon::menu::MenuId,
) {
    let menu = Menu::new();
    let open_item = MenuItem::new("OpenLab Schedule 열기", true, None);
    let quit_item = MenuItem::new("종료", true, None);

    let open_id = open_item.id().clone();
    let quit_id = quit_item.id().clone();

    let _ = menu.append(&open_item);
    let _ = menu.append(&quit_item);

    let icon = load_tray_icon();
    let tray = icon.and_then(|ic| {
        TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("OpenLab Schedule")
            .with_icon(ic)
            .build()
            .ok()
    });

    (tray, open_id, quit_id)
}

fn load_tray_icon() -> Option<tray_icon::Icon> {
    let mut rgba = Vec::with_capacity(16 * 16 * 4);
    for _ in 0..(16 * 16) {
        rgba.extend_from_slice(&[0, 113, 227, 255]);
    }
    tray_icon::Icon::from_rgba(rgba, 16, 16).ok()
}

fn main() -> anyhow::Result<()> {
    let raw_args: Vec<String> = std::env::args().skip(1).collect();

    match parse_launch_action(&raw_args) {
        LaunchAction::RunInteractiveCli => {
            return openschedule_core::cli::run_interactive_cli();
        }
        LaunchAction::RunCliCommand(cmd_args) => {
            return openschedule_core::cli::run_cli(&cmd_args);
        }
        LaunchAction::RunGui => {}
    }

    env_logger::init();

    let viewport = ViewportBuilder::default()
        .with_title("OpenLab Schedule - 투두리스트 & 캘린더")
        .with_inner_size([1160.0, 750.0])
        .with_min_inner_size([880.0, 560.0]);

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "OpenLab Schedule",
        options,
        Box::new(move |cc| Ok(Box::new(OpenScheduleApp::new(cc)))),
    )
    .map_err(|e| anyhow::anyhow!("{e}"))
}
