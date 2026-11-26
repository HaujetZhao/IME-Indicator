//! IME 切换模块 - 远程切换目标窗口的中/英输入状态
//!
//! 原理（与 demo_ime_switch.py 验证一致）：对 `ImmGetDefaultIMEWnd(焦点窗口)`
//! 发 `WM_IME_CONTROL`。有效中文态 = open=1 且 conv 含 NATIVE，缺一不可，
//! 所以两条消息必须一起压。

use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::Input::Ime::ImmGetDefaultIMEWnd;
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetGUIThreadInfo, GetWindowThreadProcessId, SendMessageTimeoutW,
    GUITHREADINFO, SMTO_ABORTIFHUNG,
};

const WM_IME_CONTROL: u32 = 0x283;
const IMC_GETCONVERSIONMODE: usize = 0x1;
const IMC_SETCONVERSIONMODE: usize = 0x2;
const IMC_SETOPENSTATUS: usize = 0x6;

/// 自动切换开关（托盘菜单切换，检测循环读取）
static AUTO: AtomicBool = AtomicBool::new(false);

pub fn auto_enabled() -> bool {
    AUTO.load(Ordering::SeqCst)
}

pub fn toggle_auto() -> bool {
    let v = !auto_enabled();
    AUTO.store(v, Ordering::SeqCst);
    v
}

/// 获取当前焦点窗口（与 ime_detector::get_focused_window 同逻辑）
fn focused_window() -> HWND {
    unsafe {
        let fore = GetForegroundWindow();
        if fore.0.is_null() {
            return HWND::default();
        }
        let tid = GetWindowThreadProcessId(fore, None);
        let mut info = GUITHREADINFO {
            cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };
        if GetGUIThreadInfo(tid, &mut info).is_ok() && !info.hwndFocus.0.is_null() {
            return info.hwndFocus;
        }
        fore
    }
}

/// 对目标窗口的默认 IME 窗口发 WM_IME_CONTROL，返回回读结果
fn ime_ctl(hwnd: HWND, imc: usize, value: usize) -> Option<usize> {
    unsafe {
        let ime_wnd = ImmGetDefaultIMEWnd(hwnd);
        if ime_wnd.0.is_null() {
            return None;
        }
        let mut result: usize = 0;
        let ok = SendMessageTimeoutW(
            ime_wnd,
            WM_IME_CONTROL,
            WPARAM(imc),
            LPARAM(value as isize),
            SMTO_ABORTIFHUNG,
            600,
            Some(&mut result),
        );
        if ok.0 != 0 {
            Some(result)
        } else {
            None
        }
    }
}

fn press(hwnd: HWND, chinese: bool) {
    let want = if chinese { 1 } else { 0 };
    ime_ctl(hwnd, IMC_SETOPENSTATUS, want);
    let conv = ime_ctl(hwnd, IMC_GETCONVERSIONMODE, 0).unwrap_or(0);
    let mode = if chinese { (conv & !3) | 1 } else { 0 };
    ime_ctl(hwnd, IMC_SETCONVERSIONMODE, mode);
}

/// 立即对当前焦点窗口生效（自动切换用，此时目标窗口就在前台）
pub fn apply(chinese: bool) {
    press(focused_window(), chinese);
}

/// 延迟后对「当时的前台焦点窗口」生效（给用户留时间切窗口）
pub fn switch_delayed(chinese: bool, delay: Duration) {
    thread::spawn(move || {
        thread::sleep(delay);
        press(focused_window(), chinese);
    });
}
