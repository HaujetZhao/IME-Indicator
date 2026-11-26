#![windows_subsystem = "windows"]

mod caret_detector;
mod config;
mod cursor_detector;
mod ime_detector;
mod ime_switcher;
mod overlay;
mod tray;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::POINT;
use windows::Win32::UI::WindowsAndMessaging::{GetCursorPos, LoadIconW, IDI_APPLICATION};

use caret_detector::{CaretDetector, CaretPos};
use cursor_detector::CursorDetector;
use ime_detector::is_chinese_mode;
use overlay::IndicatorOverlay;
use tray::TrayManager;

fn main() {
    // 初始化 GDI+ (IndicatorOverlay 内部会处理，但这里为了可能需要的图标加载，我们显示初始化)
    // 实际上 IndicatorOverlay::new 内部会调用 GdiplusStartup
    
    // 配置高 DPI 感知
    set_dpi_awareness();

    // 设置运行状态
    let running = Arc::new(AtomicBool::new(true));
    let shared = Arc::new(SharedState {
        running: running.clone(),
        chinese: AtomicBool::new(false),
        mouse_active: AtomicBool::new(false),
    });

    // 鼠标跟随线程：轻量循环（GetCursorPos + 重绘），与阻塞型检测隔离
    if config::mouse_enable() {
        let s = shared.clone();
        thread::spawn(move || run_mouse_follower(s));
    }

    // 检测线程：caret 检测、状态判断、IME 读写（内含可能阻塞的跨进程 COM）
    let s_clone = shared.clone();
    thread::spawn(move || {
        run_detector_loop(s_clone);
    });

    // 主线程：根据配置决定是否创建系统托盘
    unsafe {
        // 先手动初始化一下 GDI+，因为托盘图标加载可能需要它
        let mut token = 0;
        let input = windows::Win32::Graphics::GdiPlus::GdiplusStartupInput {
            GdiplusVersion: 1,
            ..Default::default()
        };
        let _ = windows::Win32::Graphics::GdiPlus::GdiplusStartup(&mut token, &input, std::ptr::null_mut());

        if config::tray_enable() {
            // 尝试加载图标
            let h_instance = windows::Win32::System::LibraryLoader::GetModuleHandleW(None).unwrap();
            // 这里的 1 对应 resource.rc 中的 ID
            let mut icon = LoadIconW(h_instance, windows::core::PCWSTR(1 as _)).unwrap_or_else(|_| {
                LoadIconW(None, IDI_APPLICATION).unwrap()
            });
            
            // 1. 尝试从可执行文件同目录加载外部 icon.png (允许用户自定义)
            let mut loaded = false;
            if let Ok(exe_path) = std::env::current_exe() {
                if let Some(dir) = exe_path.parent() {
                    let external_icon = dir.join("icon.png");
                    if let Some(h) = TrayManager::load_icon_from_file(&external_icon) {
                        icon = h;
                        loaded = true;
                    }
                }
            }

            // 2. 如果没找到外部图标，使用内嵌在 EXE 资源中的 ICO (ID 为 1)
            if !loaded {
                // 我们在 main 顶部已经尝试加载了基于资源的 icon，此处保持逻辑一致。
                // 实际上由于 main 已经做好了，这里直接传进去即可。
            }

            let tray = TrayManager::new(icon);
            
            // 这将阻塞直到用户退出（主窗口收到 WM_QUIT）
            tray.run_message_loop();
            
            // 退出后清理
            running.store(false, Ordering::SeqCst);
            tray.destroy();
        } else {
            // 不创建托盘，主线程进入等待循环
            // 用户只能通过任务管理器结束进程
            while running.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(100));
            }
        }
        
        windows::Win32::Graphics::GdiPlus::GdiplusShutdown(token);
    }
}

/// 跟随线程与检测线程之间共享的状态
struct SharedState {
    running: Arc<AtomicBool>,
    /// 当前中文态（颜色用）
    chinese: AtomicBool,
    /// 鼠标指示器是否应显示
    mouse_active: AtomicBool,
}

fn run_mouse_follower(shared: Arc<SharedState>) {
    // 鼠标 overlay 归跟随线程所有：跟随是轻活（GetCursorPos + 重绘，微秒级），
    // 不与检测线程（跨进程 COM，可能被目标程序拖住几百毫秒）绑在一个线程里，
    // 否则点击等焦点切换瞬间 COM 阻塞会把跟随一起冻住
    let overlay = IndicatorOverlay::new(
        "Mouse",
        config::mouse_size(),
        config::mouse_color_cn(),
        config::mouse_color_en(),
        config::mouse_offset_x(),
        config::mouse_offset_y(),
    );
    let mut shown = false;
    while shared.running.load(Ordering::SeqCst) {
        let active = shared.mouse_active.load(Ordering::SeqCst);
        if active != shown {
            shown = active;
            if active {
                overlay.show();
            } else {
                overlay.hide();
            }
        }
        if active {
            let mut pt = POINT::default();
            unsafe {
                if GetCursorPos(&mut pt).is_ok() {
                    overlay.update(pt.x, pt.y, shared.chinese.load(Ordering::SeqCst), 0);
                }
            }
        }
        std::thread::sleep(Duration::from_millis(config::track_poll_interval_ms()));
    }
}

fn run_detector_loop(shared: Arc<SharedState>) {
    // 初始化检测器
    let mut caret_detector = CaretDetector::new();
    let cursor_detector = CursorDetector::new(config::mouse_target_cursors());

    // caret 悬浮窗归检测线程所有
    let caret_overlay = if config::caret_enable() {
        Some(IndicatorOverlay::new(
            "Caret",
            config::caret_size(),
            config::caret_color_cn(),
            config::caret_color_en(),
            config::caret_offset_x(),
            config::caret_offset_y(),
        ))
    } else {
        None
    };

    let state_interval = Duration::from_millis(config::state_poll_interval_ms());
    let track_interval = Duration::from_millis(config::track_poll_interval_ms());

    let mut last_state_check_time = Instant::now();
    let mut caret_active = false;
    let mut last_in_text = false;
    // A 段刚检测完的光标位置，B 段直接复用，避免同一拍重跑检测管线
    let mut fresh_caret_pos: Option<CaretPos> = None;

    // 主线程负责消息循环，子线程负责检测
    while shared.running.load(Ordering::SeqCst) {
        let now = Instant::now();

        // A. 状态检测 (100ms)
        if now.duration_since(last_state_check_time) >= state_interval {
            let chinese_mode = is_chinese_mode();
            shared.chinese.store(chinese_mode, Ordering::SeqCst);

            let auto = ime_switcher::auto_enabled();
            let caret_pos = if config::caret_enable() || auto {
                caret_detector.get_caret_pos()
            } else {
                None
            };
            // 只读文档判定（UIA COM）：有插入符才算一次，两处判断共用
            let readonly_doc = caret_pos.is_some() && caret_detector.focus_is_readonly_document();
            let in_text = auto && caret_pos.is_some() && !readonly_doc;

            // 自动切换：进入输入框切中文，离开切英文。
            // 只在翻转时动作一次，不持续强制——避免顶掉用户在输入框里的手动 Shift 切换
            if auto && in_text != last_in_text {
                last_in_text = in_text;
                ime_switcher::apply(in_text);
                shared.chinese.store(in_text, Ordering::SeqCst);
            }

            // Caret 状态判断
            if config::caret_enable() {
                if let Some(ref overlay) = caret_overlay {
                    // 可见性线（黑名单制）：位置有、且焦点不在只读正文中才显示
                    let should_caret = caret_pos.is_some()
                        && !readonly_doc
                        && (chinese_mode || config::caret_show_en());
                    if should_caret != caret_active {
                        caret_active = should_caret;
                        if caret_active {
                            overlay.show();
                        } else {
                            overlay.hide();
                        }
                    }
                }
            }

            // Mouse 状态判断（显示与否由跟随线程按共享状态自行执行）
            if config::mouse_enable() {
                let target_cursor = cursor_detector.should_show_indicator();
                let should_mouse = target_cursor
                    && (shared.chinese.load(Ordering::SeqCst) || config::mouse_show_en());
                shared.mouse_active.store(should_mouse, Ordering::SeqCst);
            }

            last_state_check_time = now;
            fresh_caret_pos = caret_pos;
        }

        // B. 追踪文本光标（优先复用 A 段同拍结果）
        if config::caret_enable() && caret_active {
            if let Some(ref overlay) = caret_overlay {
                let pos = fresh_caret_pos.take().or_else(|| caret_detector.get_caret_pos());
                if let Some((x, y, h)) = pos {
                    overlay.update(x, y, shared.chinese.load(Ordering::SeqCst), h);
                }
            }
        }

        std::thread::sleep(track_interval);
    }
}

/// 设置高 DPI 感知
fn set_dpi_awareness() {
    unsafe {
        // 尝试使用 SetProcessDpiAwareness (Windows 8.1+)
        let shcore = windows::Win32::System::LibraryLoader::LoadLibraryW(
            windows::core::w!("shcore.dll"),
        );
        if let Ok(h) = shcore {
            if let Some(func) = windows::Win32::System::LibraryLoader::GetProcAddress(
                h,
                windows::core::s!("SetProcessDpiAwareness"),
            ) {
                let set_dpi: extern "system" fn(i32) -> i32 = std::mem::transmute(func);
                let _ = set_dpi(2); // PROCESS_PER_MONITOR_DPI_AWARE
                return;
            }
        }
        // 回退到 SetProcessDPIAware (使用动态加载)
        let user32 = windows::Win32::System::LibraryLoader::LoadLibraryW(
            windows::core::w!("user32.dll"),
        );
        if let Ok(h) = user32 {
            if let Some(func) = windows::Win32::System::LibraryLoader::GetProcAddress(
                h,
                windows::core::s!("SetProcessDPIAware"),
            ) {
                let set_dpi: extern "system" fn() -> i32 = std::mem::transmute(func);
                let _ = set_dpi();
            }
        }
    }
}


