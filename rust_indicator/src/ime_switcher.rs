//! IME 切换模块 - 远程切换目标窗口的中/英输入状态
//!
//! 原理（与 demo_ime_switch.py 验证一致）：对 `ImmGetDefaultIMEWnd(焦点窗口)`
//! 发 `WM_IME_CONTROL`。有效中文态 = open=1 且 conv 含 NATIVE，缺一不可，
//! 所以两条消息必须一起压。

use std::sync::atomic::{AtomicBool, Ordering};

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

/// 程序启动时按配置设定初始状态（运行时的开关仍归托盘菜单管）
pub fn init(enable: bool) {
    AUTO.store(enable, Ordering::SeqCst);
}

pub fn auto_enabled() -> bool {
    AUTO.load(Ordering::SeqCst)
}

pub fn toggle_auto() -> bool {
    let v = !auto_enabled();
    AUTO.store(v, Ordering::SeqCst);
    v
}

/// 前台进程名（按前台窗口句柄缓存，每次前台切换才真正查一次）
fn foreground_exe() -> Option<String> {
    static CACHE: std::sync::Mutex<(usize, Option<String>)> = std::sync::Mutex::new((0, None));
    unsafe {
        use windows::Win32::System::Threading::{
            OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
            PROCESS_QUERY_LIMITED_INFORMATION,
        };
        use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

        let hwnd = GetForegroundWindow();
        let key = hwnd.0 as usize;
        let mut cache = CACHE.lock().unwrap();
        if key != 0 && cache.0 == key {
            return cache.1.clone();
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let name = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok().and_then(|h| {
            let mut buf = [0u16; 512];
            let mut len = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, windows::core::PWSTR(buf.as_mut_ptr()), &mut len)
                .is_ok();
            let _ = windows::Win32::Foundation::CloseHandle(h);
            ok.then(|| {
                let full = String::from_utf16_lossy(&buf[..len as usize]);
                full.rsplit('\\').next().unwrap_or(&full).to_lowercase()
            })
        });
        if key != 0 {
            *cache = (key, name.clone());
        }
        name
    }
}

/// 前台程序是否允许自动切换（按配置的 match/deny 过滤；查不到进程名时默认允许）
pub fn app_allows() -> bool {
    let Some(exe) = foreground_exe() else { return true };
    crate::config::autoswitch_allows(&exe)
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
