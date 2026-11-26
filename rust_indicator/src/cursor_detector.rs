//! 鼠标光标形状检测模块
//!
//! 判定规则（阈值制）：系统规范光标的共享句柄都是 0x10003 一类的小规范值
//! （0x00010001~0x0001000F）；自绘光标程序（如 Photoshop）的句柄是运行时分配
//! 的大值且每次启动都不同。句柄大于阈值的一律视为自定义光标，显示指示器；
//! 系统光标则按配置的 target_cursors 决定。

use std::collections::HashSet;
use windows::Win32::Foundation::HINSTANCE;
use windows::Win32::UI::WindowsAndMessaging::{
    GetCursorInfo, LoadCursorW, CURSORINFO, CURSOR_SHOWING, HCURSOR,
};
use windows::core::PCWSTR;

/// 系统共享光标句柄上限（规范值最大约 0x1000F），超过即视为程序自绘光标。
/// 注意用 usize 比较：自绘句柄常带满高位（如 0xffffffffae3916a1），按 isize 会成负数
const SYSTEM_HANDLE_MAX: usize = 0x20000;

/// 鼠标形状检测器
pub struct CursorDetector {
    /// 配置指定的目标光标句柄（系统光标里哪几个要显示指示器）
    target_cursor_handles: HashSet<usize>,
}

impl CursorDetector {
    /// 创建新的检测器
    pub fn new(target_cursor_ids: &[u32]) -> Self {
        let mut handles = HashSet::new();
        for &cid in target_cursor_ids {
            unsafe {
                if let Ok(h) = LoadCursorW(HINSTANCE::default(), PCWSTR(cid as *const u16)) {
                    if !h.0.is_null() {
                        handles.insert(h.0 as usize);
                    }
                }
            }
        }
        Self {
            target_cursor_handles: handles,
        }
    }

    /// 当前光标是否应显示鼠标指示器：
    /// 自定义光标（句柄大于阈值）一律显示；系统光标按配置名单显示。
    pub fn should_show_indicator(&self) -> bool {
        unsafe {
            let mut ci = CURSORINFO {
                cbSize: std::mem::size_of::<CURSORINFO>() as u32,
                flags: CURSOR_SHOWING,
                hCursor: HCURSOR::default(),
                ptScreenPos: Default::default(),
            };
            if GetCursorInfo(&mut ci).is_err() {
                return false;
            }
            let handle = ci.hCursor.0 as usize;
            handle > SYSTEM_HANDLE_MAX || self.target_cursor_handles.contains(&handle)
        }
    }
}
