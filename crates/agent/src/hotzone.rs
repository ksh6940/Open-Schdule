//! 마우스 커서 위치 감지 및 모니터 상단 중앙 핫존(웹캠 영역) 판정.

use openschedule_core::Settings;

pub struct CursorPoint {
    pub x: i32,
    pub y: i32,
}

#[cfg(windows)]
pub fn get_cursor_pos() -> Option<CursorPoint> {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;

    unsafe {
        let mut pt: POINT = std::mem::zeroed();
        if GetCursorPos(&mut pt) != 0 {
            Some(CursorPoint { x: pt.x, y: pt.y })
        } else {
            None
        }
    }
}

#[cfg(not(windows))]
pub fn get_cursor_pos() -> Option<CursorPoint> {
    None
}

#[cfg(windows)]
pub fn primary_screen_width() -> f32 {
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN};
    unsafe { GetSystemMetrics(SM_CXSCREEN) as f32 }
}

#[cfg(not(windows))]
pub fn primary_screen_width() -> f32 {
    1920.0
}

/// 주 모니터 해상도 및 중앙 상단 핫존 계산
#[cfg(windows)]
pub fn get_primary_monitor_hotzone(settings: &Settings) -> Option<(f32, f32, f32, f32)> {
    let screen_w = primary_screen_width();
    let hw = settings.hotzone_width;
    let hh = settings.hotzone_height.max(6.0);
    let left = (screen_w - hw) / 2.0;
    let right = left + hw;
    let top = 0.0;
    let bottom = hh;

    Some((left, right, top, bottom))
}

#[cfg(not(windows))]
pub fn get_primary_monitor_hotzone(_settings: &Settings) -> Option<(f32, f32, f32, f32)> {
    None
}

pub fn is_in_hotzone(pt: &CursorPoint, settings: &Settings) -> bool {
    if !settings.hotzone_enabled {
        return false;
    }
    if let Some((left, right, top, bottom)) = get_primary_monitor_hotzone(settings) {
        let x = pt.x as f32;
        let y = pt.y as f32;
        x >= left && x <= right && y >= top && y <= bottom
    } else {
        false
    }
}

/// 오버레이 패널이 열려 있을 때 커서가 오버레이 영역 내부에 있는지 판정
pub fn is_in_overlay_area(pt: &CursorPoint, panel_width: f32, panel_height: f32) -> bool {
    let screen_w = primary_screen_width();
    let left = (screen_w - panel_width) / 2.0 - 16.0;
    let right = left + panel_width + 32.0;
    let x = pt.x as f32;
    let y = pt.y as f32;
    x >= left && x <= right && y >= 0.0 && y <= panel_height + 24.0
}
