//! 공통 테마: Apple 디자인 시스템 기반 라이트 모드 (SF Pro / Fog Canvas / White Surface / Apple Blue / Phosphor Icons).

use std::sync::Arc;

use egui::{Color32, CornerRadius, FontData, FontDefinitions, FontFamily, Stroke, Visuals};

use crate::model::Status;

/// Phosphor Icons (Bold) 유니코드 글리프 상수
pub mod icons {
    pub const ARROW_SQUARE_OUT: &str = "\u{e5de}";
    pub const CALENDAR: &str = "\u{e108}";
    pub const CALENDAR_BLANK: &str = "\u{e10a}";
    pub const CALENDAR_PLUS: &str = "\u{e714}";
    pub const CHECK: &str = "\u{e182}";
    pub const CHECK_CIRCLE: &str = "\u{e184}";
    pub const CIRCLE: &str = "\u{e18a}";
    pub const CIRCLE_HALF: &str = "\u{e18c}";
    pub const CLOCK: &str = "\u{e19a}";
    pub const FLOPPY_DISK: &str = "\u{e248}";
    pub const FOLDER_PLUS: &str = "\u{e258}";
    pub const GEAR: &str = "\u{e270}";
    pub const INFO: &str = "\u{e2ce}";
    pub const LIST_CHECKS: &str = "\u{eadc}";
    pub const MAGNIFYING_GLASS: &str = "\u{e30c}";
    pub const NOTE: &str = "\u{e348}";
    pub const PLUS: &str = "\u{e3d4}";
    pub const TAG: &str = "\u{e478}";
    pub const TERMINAL_WINDOW: &str = "\u{eae8}";
    pub const TRASH: &str = "\u{e4a6}";
    pub const TRAY: &str = "\u{e4aa}";
    pub const WARNING: &str = "\u{e4e0}";
    pub const WEBCAM: &str = "\u{e9b2}";
    pub const X: &str = "\u{e4f6}";
}

// ── Apple Color Palette (Light Mode) ──
/// Canvas (Fog Canvas) `#f5f5f7`
pub const CANVAS: Color32 = Color32::from_rgb(0xF5, 0xF5, 0xF7);
/// Pure white surface `#ffffff` (HIG card surface)
pub const SURFACE: Color32 = Color32::from_rgb(0xFF, 0xFF, 0xFF);
/// Secondary surface / hovered card `#F0F0F2`
pub const SURFACE_HOVER: Color32 = Color32::from_rgb(0xEB, 0xEB, 0xEE);
/// Subtle border `#E5E5E8`
pub const BORDER: Color32 = Color32::from_rgb(0xE0, 0xE0, 0xE4);
/// Strong border / focus `#D1D1D6`
pub const BORDER_STRONG: Color32 = Color32::from_rgb(0xD1, 0xD1, 0xD6);
/// Principal text `#1d1d1f`
pub const FOREGROUND: Color32 = Color32::from_rgb(0x1D, 0x1D, 0x1F);
/// Muted text `#6e6e73`
pub const MUTED: Color32 = Color32::from_rgb(0x6E, 0x6E, 0x73);
/// Secondary neutral `#515154`
pub const SECONDARY: Color32 = Color32::from_rgb(0x51, 0x51, 0x54);
/// Apple Primary Action Blue `#0071e3`
pub const PRIMARY: Color32 = Color32::from_rgb(0x00, 0x71, 0xE3);
/// Apple Primary Action Hover `#0076df`
pub const PRIMARY_HOVER: Color32 = Color32::from_rgb(0x00, 0x76, 0xDF);
/// Apple Text Link Blue `#0066cc`
pub const LINK: Color32 = Color32::from_rgb(0x00, 0x66, 0xCC);
/// Danger / Destructive Red `#ff3b30`
pub const DANGER: Color32 = Color32::from_rgb(0xFF, 0x3B, 0x30);
/// Success Green `#34c759`
pub const SUCCESS: Color32 = Color32::from_rgb(0x34, 0xC7, 0x59);
/// Warning Orange `#ff9500`
pub const WARNING: Color32 = Color32::from_rgb(0xFF, 0x95, 0x00);
/// Conditional / Purple `#af52de`
pub const PURPLE: Color32 = Color32::from_rgb(0xAF, 0x52, 0xDE);

// 하위 호환 상수 매핑
pub const BG: Color32 = CANVAS;
pub const PANEL: Color32 = SURFACE;
pub const CARD: Color32 = SURFACE;
pub const CARD_HOVER: Color32 = SURFACE_HOVER;
pub const TEXT: Color32 = FOREGROUND;
pub const TEXT_DIM: Color32 = MUTED;
pub const ACCENT: Color32 = PRIMARY;
pub const ACCENT_2: Color32 = LINK;

pub fn status_color(s: Status) -> Color32 {
    match s {
        Status::Planned => MUTED,
        Status::InProgress => WARNING,
        Status::Done => SUCCESS,
        Status::ConditionalDone => PURPLE,
    }
}

pub fn rgb(c: u32) -> Color32 {
    Color32::from_rgb((c >> 16) as u8, (c >> 8) as u8, c as u8)
}

pub fn to_u32(c: Color32) -> u32 {
    ((c.r() as u32) << 16) | ((c.g() as u32) << 8) | c.b() as u32
}

/// 반투명 글래스(Liquid Glass 효과) 배경색 (라이트 모드).
pub fn glass(alpha: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(0xFF, 0xFF, 0xFF, alpha)
}

fn load_korean_font() -> Option<Vec<u8>> {
    let candidates = [
        r"C:\Windows\Fonts\malgun.ttf",
        r"C:\Windows\Fonts\NanumGothic.ttf",
        "/System/Library/Fonts/AppleSDGothicNeo.ttc",
        "/System/Library/Fonts/SFProText-Regular.otf",
        "/usr/share/fonts/truetype/nanum/NanumGothic.ttf",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
    ];
    candidates.iter().find_map(|p| std::fs::read(p).ok())
}

fn load_emoji_font() -> Option<Vec<u8>> {
    std::fs::read(r"C:\Windows\Fonts\seguisym.ttf").ok()
}

pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();

    // 1. 한국어 폰트 탑재
    if let Some(bytes) = load_korean_font() {
        fonts.font_data.insert("korean".into(), Arc::new(FontData::from_owned(bytes)));
        for fam in [FontFamily::Proportional, FontFamily::Monospace] {
            fonts.families.entry(fam).or_default().insert(0, "korean".into());
        }
    }

    // 2. Phosphor Icons (Bold) 폰트 내장 탑재
    let phosphor_bytes = include_bytes!("../resources/Phosphor-Bold.ttf");
    fonts.font_data.insert(
        "phosphor".into(),
        Arc::new(FontData::from_static(phosphor_bytes)),
    );
    for fam in [FontFamily::Proportional, FontFamily::Monospace] {
        fonts.families.entry(fam).or_default().push("phosphor".into());
    }

    // 3. 심볼 폴백 폰트
    if let Some(bytes) = load_emoji_font() {
        fonts.font_data.insert("symbols".into(), Arc::new(FontData::from_owned(bytes)));
        fonts.families.entry(FontFamily::Proportional).or_default().push("symbols".into());
    }
    ctx.set_fonts(fonts);

    // Apple Clean Light Visuals
    let mut v = Visuals::light();
    v.panel_fill = CANVAS;
    v.window_fill = SURFACE;
    v.extreme_bg_color = Color32::from_rgb(0xFA, 0xFA, 0xFB);
    v.faint_bg_color = SURFACE;
    v.override_text_color = Some(FOREGROUND);
    v.hyperlink_color = LINK;
    v.selection.bg_fill = PRIMARY.linear_multiply(0.25);
    v.selection.stroke = Stroke::new(1.5_f32, PRIMARY);
    v.window_corner_radius = CornerRadius::same(18); // HIG 18px reference card
    v.window_stroke = Stroke::new(1.0_f32, BORDER);
    v.popup_shadow.color = Color32::from_black_alpha(20);

    // Controls: 8px radius
    let r_ctrl = CornerRadius::same(8);
    v.widgets.noninteractive.corner_radius = r_ctrl;
    v.widgets.noninteractive.bg_fill = SURFACE;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, BORDER);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, MUTED);

    v.widgets.inactive.corner_radius = r_ctrl;
    v.widgets.inactive.bg_fill = SURFACE;
    v.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, BORDER);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, FOREGROUND);

    v.widgets.hovered.corner_radius = r_ctrl;
    v.widgets.hovered.bg_fill = SURFACE_HOVER;
    v.widgets.hovered.weak_bg_fill = SURFACE_HOVER;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, PRIMARY);
    v.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, PRIMARY);

    v.widgets.active.corner_radius = r_ctrl;
    v.widgets.active.bg_fill = PRIMARY.linear_multiply(0.12);
    v.widgets.active.weak_bg_fill = PRIMARY.linear_multiply(0.12);
    v.widgets.active.bg_stroke = Stroke::new(1.5_f32, PRIMARY);
    v.widgets.active.fg_stroke = Stroke::new(1.0_f32, PRIMARY);

    v.widgets.open.corner_radius = r_ctrl;
    ctx.set_visuals(v);

    ctx.style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(10.0, 8.0);
        s.spacing.button_padding = egui::vec2(12.0, 7.0);
        s.spacing.interact_size.y = 32.0;

        use egui::{FontId, TextStyle};
        s.text_styles.insert(TextStyle::Heading, FontId::proportional(22.0));
        s.text_styles.insert(TextStyle::Body, FontId::proportional(14.5));
        s.text_styles.insert(TextStyle::Button, FontId::proportional(13.5));
        s.text_styles.insert(TextStyle::Small, FontId::proportional(12.0));
        s.text_styles.insert(TextStyle::Monospace, FontId::monospace(12.5));
    });
}

/// Apple Marketing Primary 버튼 (`#0071e3`, 둥근 알약, 흰색 텍스트)
pub fn apple_button_primary(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let text_color = Color32::WHITE;
    let galley = ui.painter().layout_no_wrap(text.to_string(), egui::FontId::proportional(13.5), text_color);
    let size = egui::vec2(galley.size().x + 28.0, 34.0);
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());

    let bg = if resp.is_pointer_button_down_on() {
        Color32::from_rgb(0x00, 0x6E, 0xDB)
    } else if resp.hovered() {
        PRIMARY_HOVER
    } else {
        PRIMARY
    };

    ui.painter().rect(rect, CornerRadius::same(255), bg, Stroke::NONE, egui::StrokeKind::Inside);
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, text_color);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Apple Danger 버튼 (`#ff3b30` 연한 배경, 빨간 글자, 알약)
pub fn apple_button_danger(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let galley = ui.painter().layout_no_wrap(text.to_string(), egui::FontId::proportional(13.5), DANGER);
    let size = egui::vec2(galley.size().x + 26.0, 34.0);
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());

    let bg = if resp.hovered() {
        DANGER.linear_multiply(0.18)
    } else {
        DANGER.linear_multiply(0.10)
    };

    ui.painter().rect(rect, CornerRadius::same(255), bg, Stroke::new(1.0_f32, DANGER.linear_multiply(0.35)), egui::StrokeKind::Inside);
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, DANGER);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Apple Marketing Outline 버튼 (투명 배경, `#0066cc` 테두리/글자, 980px 알약)
pub fn apple_button_outline(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let galley = ui.painter().layout_no_wrap(text.to_string(), egui::FontId::proportional(13.5), LINK);
    let size = egui::vec2(galley.size().x + 26.0, 34.0);
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());

    let bg = if resp.hovered() {
        PRIMARY.linear_multiply(0.08)
    } else {
        Color32::TRANSPARENT
    };

    ui.painter().rect(rect, CornerRadius::same(255), bg, Stroke::new(1.0_f32, LINK), egui::StrokeKind::Inside);
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, LINK);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// 작은 퀵 칩 버튼 (`오늘`, `내일`, `09:00` 등)
pub fn quick_chip_button(ui: &mut egui::Ui, text: &str, active: bool) -> egui::Response {
    let fg = if active { PRIMARY } else { FOREGROUND };
    let galley = ui.painter().layout_no_wrap(text.to_string(), egui::FontId::proportional(12.0), fg);
    let size = galley.size() + egui::vec2(18.0, 8.0);
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());

    let bg = if active || resp.hovered() {
        Color32::from_rgb(0xEA, 0xF2, 0xFD)
    } else {
        CANVAS
    };
    let stroke = if active || resp.hovered() {
        Stroke::new(1.0_f32, PRIMARY)
    } else {
        Stroke::new(1.0_f32, BORDER)
    };

    ui.painter().rect(rect, CornerRadius::same(255), bg, stroke, egui::StrokeKind::Inside);
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, fg);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// 상태 배지 (Apple HIG Pill 스타일 + Phosphor Icons)
pub fn status_pill(ui: &mut egui::Ui, s: Status) -> egui::Response {
    status_pill_selectable(ui, s, false)
}

pub fn status_pill_selectable(ui: &mut egui::Ui, s: Status, selected: bool) -> egui::Response {
    let c = status_color(s);
    let ph_icon = match s {
        Status::Planned => icons::CIRCLE,
        Status::InProgress => icons::CIRCLE_HALF,
        Status::ConditionalDone => icons::CHECK_CIRCLE,
        Status::Done => icons::CHECK_CIRCLE,
    };
    let text = format!("{} {}", ph_icon, s.label());
    let fg = if selected { Color32::WHITE } else { c };
    let galley = ui.painter().layout_no_wrap(text, egui::FontId::proportional(12.0), fg);
    let size = galley.size() + egui::vec2(18.0, 7.0);
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());

    let fill = if selected {
        c
    } else if resp.hovered() {
        c.linear_multiply(0.18)
    } else {
        c.linear_multiply(0.10)
    };

    let stroke = if selected {
        Stroke::new(1.5_f32, c)
    } else {
        Stroke::new(1.0_f32, c.linear_multiply(0.35))
    };

    ui.painter().rect(rect, CornerRadius::same(255), fill, stroke, egui::StrokeKind::Inside);
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, fg);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// 카테고리 칩
pub fn category_chip(ui: &mut egui::Ui, name: &str, color: Color32) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 5.0;
        let (r, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
        ui.painter().circle_filled(r.center(), 3.5, color);
        ui.label(egui::RichText::new(name).small().color(MUTED));
    });
}

/// Apple 미리알림 2x2 스마트 카드 (아이콘 원, 카운트, 라벨, 선택 테두리).
pub fn smart_card(
    ui: &mut egui::Ui,
    icon: &str,
    icon_bg: Color32,
    label: &str,
    count: usize,
    selected: bool,
    width: f32,
) -> egui::Response {
    let height = 68.0;
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::click());

    let bg = if selected {
        Color32::from_rgb(0xEA, 0xF2, 0xFD)
    } else if resp.hovered() {
        SURFACE_HOVER
    } else {
        CANVAS
    };

    let stroke = if selected {
        Stroke::new(1.5_f32, PRIMARY)
    } else {
        Stroke::new(1.0_f32, Color32::TRANSPARENT)
    };

    ui.painter().rect(rect, CornerRadius::same(12), bg, stroke, egui::StrokeKind::Inside);

    // 아이콘 원 (좌상단)
    let icon_center = egui::pos2(rect.min.x + 22.0, rect.min.y + 22.0);
    ui.painter().circle_filled(icon_center, 13.0, icon_bg);
    let icon_galley = ui.painter().layout_no_wrap(icon.to_string(), egui::FontId::proportional(13.0), Color32::WHITE);
    ui.painter().galley(icon_center - icon_galley.size() / 2.0, icon_galley, Color32::WHITE);

    // 카운트 (우상단, 크고 굵게)
    let count_text = count.to_string();
    let count_galley = ui.painter().layout_no_wrap(count_text, egui::FontId::proportional(21.0), FOREGROUND);
    let count_pos = egui::pos2(rect.max.x - 12.0 - count_galley.size().x, rect.min.y + 11.0);
    ui.painter().galley(count_pos, count_galley, FOREGROUND);

    // 라벨 (좌하단)
    let label_color = if selected { PRIMARY } else { MUTED };
    let label_galley = ui.painter().layout_no_wrap(label.to_string(), egui::FontId::proportional(13.0), label_color);
    let label_pos = egui::pos2(rect.min.x + 12.0, rect.max.y - 10.0 - label_galley.size().y);
    ui.painter().galley(label_pos, label_galley, label_color);

    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Apple 미리알림 원형 체크 토글 (체크 시 채워진 원 + 체크마크, 미완료 시 링).
pub fn circular_checkbox(ui: &mut egui::Ui, status: Status, highlight_color: Color32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(22.0, 22.0), egui::Sense::click());
    let center = rect.center();
    let radius = 9.5;

    match status {
        Status::Done => {
            let fill = SUCCESS;
            ui.painter().circle_filled(center, radius, fill);
            let check_galley = ui.painter().layout_no_wrap(icons::CHECK.into(), egui::FontId::proportional(11.5), Color32::WHITE);
            ui.painter().galley(center - check_galley.size() / 2.0, check_galley, Color32::WHITE);
        }
        Status::ConditionalDone => {
            let fill = PURPLE;
            ui.painter().circle_filled(center, radius, fill.linear_multiply(0.2));
            ui.painter().circle_stroke(center, radius, Stroke::new(1.8_f32, fill));
            let check_galley = ui.painter().layout_no_wrap(icons::CHECK.into(), egui::FontId::proportional(11.0), fill);
            ui.painter().galley(center - check_galley.size() / 2.0, check_galley, fill);
        }
        Status::InProgress => {
            let col = WARNING;
            ui.painter().circle_stroke(center, radius, Stroke::new(1.8_f32, col));
            ui.painter().circle_filled(center, 4.5, col);
        }
        Status::Planned => {
            let stroke_color = if resp.hovered() { highlight_color } else { BORDER_STRONG };
            let fill = if resp.hovered() { highlight_color.linear_multiply(0.1) } else { Color32::TRANSPARENT };
            ui.painter().circle_filled(center, radius, fill);
            ui.painter().circle_stroke(center, radius, Stroke::new(1.6_f32, stroke_color));
        }
    }

    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}
