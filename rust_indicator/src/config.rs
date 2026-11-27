//! 输入指示器 - 零依赖 TOML 解析
//! 追求极致代码简洁度与二进制体积

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;

// ============================================================================
// 数据结构 (扁平化，删除冗余嵌套)
// ============================================================================

pub struct Config {
    pub poll_state_interval_ms: u64,
    pub poll_mouse_track_interval_ms: u64,
    /// caret 位置追踪间隔；VS Code 等走 MSAA COM，是检测线程的开销大头，可放宽
    pub poll_caret_track_interval_ms: u64,

    pub tray_enable: bool,

    pub caret_enable: bool,
    pub caret_color_cn: u32,
    pub caret_color_en: u32,
    pub caret_size: i32,
    pub caret_offset_x: i32,
    pub caret_offset_y: i32,
    pub caret_show_en: bool,
    pub caret_methods: Vec<String>,

    pub mouse_enable: bool,
    pub mouse_color_cn: u32,
    pub mouse_color_en: u32,
    pub mouse_size: i32,
    pub mouse_offset_x: i32,
    pub mouse_offset_y: i32,
    pub mouse_show_en: bool,
    pub mouse_target_cursors: Vec<u32>,

    /// 自动切换：启动时开关（运行时由托盘菜单切换）
    pub autoswitch_enable: bool,
    /// 进入输入框时切换的目标："cn" 中文 / "en" 英文（离开时恒切英文）
    pub autoswitch_enter: String,
    /// 匹配列表（通配符），空 = 不匹配任何程序
    pub autoswitch_match: Vec<String>,
    /// 排除列表（通配符），命中则该程序不自动切换
    pub autoswitch_exclude: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            poll_state_interval_ms: 100,
            poll_mouse_track_interval_ms: 10,
            poll_caret_track_interval_ms: 50,
            tray_enable: true,
            caret_enable: true,
            caret_color_cn: parse_color("#FF7800dd"),
            caret_color_en: parse_color("#0077ffdd"),
            caret_size: 9,
            caret_offset_x: 0,
            caret_offset_y: 0,
            caret_show_en: true,
            // 实测（2026-09）：gui_info 覆盖记事本，msaa_caret 覆盖 VS Code/Edge
            caret_methods: ["gui_info", "msaa_caret", "uia_selection"]
                .iter().map(|s| s.to_string()).collect(),
            mouse_enable: true,
            mouse_color_cn: parse_color("#FF7800dd"),
            mouse_color_en: parse_color("#0077ffdd"),
            mouse_size: 9,
            mouse_offset_x: 1,
            mouse_offset_y: 22,
            mouse_show_en: true,
            mouse_target_cursors: vec![32513, 32512],

            autoswitch_enable: true,
            autoswitch_enter: "cn".to_string(),
            autoswitch_match: vec!["*".to_string()],
            autoswitch_exclude: Vec::new(),
        }
    }
}

// ============================================================================
// 颜色与解析辅助
// ============================================================================

pub trait ConfigParseExt {
    fn parse_color(&self) -> u32;
}

impl ConfigParseExt for str {
    fn parse_color(&self) -> u32 {
        let clean = self.trim().trim_matches('"').trim_start_matches('#');
        if clean.len() >= 6 {
            let r = u32::from_str_radix(&clean[0..2], 16).unwrap_or(0);
            let g = u32::from_str_radix(&clean[2..4], 16).unwrap_or(0);
            let b = u32::from_str_radix(&clean[4..6], 16).unwrap_or(0);
            let a = if clean.len() == 8 { u32::from_str_radix(&clean[6..8], 16).unwrap_or(0xA0) } else { 0xA0 };
            (a << 24) | (r << 16) | (g << 8) | b
        } else {
            0xA0FF7800
        }
    }
}

pub fn parse_color(s: &str) -> u32 { s.parse_color() }

// ============================================================================
// 微型 TOML 解析器
// ============================================================================

fn load_config() -> Config {
    let mut config = Config::default();
    let path = get_config_path();

    if !path.exists() {
        let _ = fs::write(&path, generate_toml_template());
        return config;
    }

    if let Ok(content) = fs::read_to_string(&path) {
        let mut sections = HashMap::new();
        let mut cur_sec = String::new();

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') { continue; }
            
            if line.starts_with('[') && line.ends_with(']') {
                cur_sec = line[1..line.len()-1].to_lowercase();
            } else if let Some((k, v)) = line.split_once('=') {
                let key = k.trim().to_lowercase();
                // 智能移除行内注释：寻找 " #" (带空格的井号) 
                let val = v.split(" #").next().unwrap().trim().to_string();
                sections.entry(cur_sec.clone()).or_insert_with(HashMap::new).insert(key, val);
            }
        }

        // 缺项自动补全：exe 更新后老配置缺的新键，按默认值补写回文件（用户已有值原样保留）
        let (merged, missing) = merge_template(&sections);
        if missing > 0 {
            let _ = fs::write(&path, merged);
        }

        // 映射数据 (精简写法)
        let get = |sec: &str, key: &str| sections.get(sec)?.get(key);
        
        if let Some(v) = get("poll",  "state_interval_ms") { if let Ok(n) = v.parse() { config.poll_state_interval_ms = n; } }
        if let Some(v) = get("mouse",  "track_interval_ms") { if let Ok(n) = v.parse() { config.poll_mouse_track_interval_ms = n; } }
        if let Some(v) = get("caret",  "track_interval_ms") { if let Ok(n) = v.parse() { config.poll_caret_track_interval_ms = n; } }
        
        if let Some(v) = get("tray", "enable") { 
            match v.as_str() {
                "true" => config.tray_enable = true,
                "false" => config.tray_enable = false,
                _ => {} // 保持默认值
            }
        }
        
        if let Some(v) = get("caret", "enable") { 
            match v.as_str() {
                "true" => config.caret_enable = true,
                "false" => config.caret_enable = false,
                _ => {}
            }
        }
        if let Some(v) = get("caret", "color_cn") { config.caret_color_cn = v.parse_color(); }
        if let Some(v) = get("caret", "color_en") { config.caret_color_en = v.parse_color(); }
        if let Some(v) = get("caret", "size")     { if let Ok(n) = v.parse() { config.caret_size = n; } }
        if let Some(v) = get("caret", "offset_x") { if let Ok(n) = v.parse() { config.caret_offset_x = n; } }
        if let Some(v) = get("caret", "offset_y") { if let Ok(n) = v.parse() { config.caret_offset_y = n; } }
        if let Some(v) = get("caret", "show_en") {
            match v.as_str() {
                "true" => config.caret_show_en = true,
                "false" => config.caret_show_en = false,
                _ => {}
            }
        }
        if let Some(v) = get("caret", "methods") {
            let list: Vec<String> = v.trim_matches(|c| c == '[' || c == ']')
                .split(',').map(|s| s.trim().trim_matches('"').to_lowercase())
                .filter(|s| !s.is_empty()).collect();
            if !list.is_empty() { config.caret_methods = list; }
        }

        if let Some(v) = get("mouse", "enable") { 
            match v.as_str() {
                "true" => config.mouse_enable = true,
                "false" => config.mouse_enable = false,
                _ => {}
            }
        }
        if let Some(v) = get("mouse", "color_cn") { config.mouse_color_cn = v.parse_color(); }
        if let Some(v) = get("mouse", "color_en") { config.mouse_color_en = v.parse_color(); }
        if let Some(v) = get("mouse", "size")     { if let Ok(n) = v.parse() { config.mouse_size = n; } }
        if let Some(v) = get("mouse", "offset_x") { if let Ok(n) = v.parse() { config.mouse_offset_x = n; } }
        if let Some(v) = get("mouse", "offset_y") { if let Ok(n) = v.parse() { config.mouse_offset_y = n; } }
        if let Some(v) = get("mouse", "show_en") { 
            match v.as_str() {
                "true" => config.mouse_show_en = true,
                "false" => config.mouse_show_en = false,
                _ => {}
            }
        }
        if let Some(v) = get("mouse", "target_cursors") {
            config.mouse_target_cursors = v.trim_matches(|c| c == '[' || c == ']')
                .split(',').filter_map(|s| s.trim().parse().ok()).collect();
        }

        if let Some(v) = get("autoswitch", "enable") {
            match v.as_str() {
                "true" => config.autoswitch_enable = true,
                "false" => config.autoswitch_enable = false,
                _ => {}
            }
        }
        if let Some(v) = get("autoswitch", "enter") {
            config.autoswitch_enter = v.trim_matches('"').to_lowercase();
        }
        for (key, dest) in [("match", &mut config.autoswitch_match), ("exclude", &mut config.autoswitch_exclude)] {
            if let Some(v) = get("autoswitch", key) {
                *dest = v.trim_matches(|c| c == '[' || c == ']')
                    .split(',').map(|s| s.trim().trim_matches('"').to_lowercase())
                    .filter(|s| !s.is_empty()).collect();
            }
        }
    }
    config
}

pub(crate) fn get_config_path() -> PathBuf {
    std::env::current_exe().unwrap().parent().unwrap().join("config.toml")
}

fn generate_toml_template() -> String {
    r##"# 输入指示器 (IME Indicator) 配置文件
[poll]
state_interval_ms = 100

[tray]
enable = true

[caret]
enable = true
color_cn = "#FF7800dd"
color_en = "#0077ffdd"
size = 9
offset_x = 0
offset_y = 0
show_en = true
track_interval_ms = 50
# 光标检测方法及落级顺序（可删减、可调序）
# 可选: gui_info(记事本等原生) msaa_caret(caret 对象, VS Code/Edge)
#       uia_selection(选区矩形右缘, Listary 等自绘控件)
methods = ["gui_info", "msaa_caret", "uia_selection"]

[mouse]
enable = true
color_cn = "#FF7800dd"
color_en = "#0077ffdd"
size = 9
offset_x = 1
offset_y = 22
show_en = true
track_interval_ms = 10
target_cursors = [32513, 32512]

[autoswitch]
enable = true          # 自动切换初始开关（运行时可用托盘菜单切换）
enter = "cn"           # 进入输入框时切 "cn" 中文 / "en" 英文（离开时恒切英文）
match = ["*"]          # 匹配列表，支持通配符（* 任意段，? 单字符）
exclude = []           # 排除列表，同样支持通配符；命中则该程序不做自动切换
"##.to_string()
}

/// 以模板为骨架合并用户配置：模板里每个键，用户文件有值就用用户的，没有就保留模板默认行。
/// 返回（合并后的文件内容，缺失键数）。用户文件里模板之外的键会被丢弃。
fn merge_template(sections: &HashMap<String, HashMap<String, String>>) -> (String, usize) {
    let mut out = String::new();
    let mut missing = 0usize;
    let mut cur_sec = String::new();
    for line in generate_toml_template().lines() {
        let t = line.trim();
        if t.starts_with('[') && t.ends_with(']') {
            cur_sec = t[1..t.len() - 1].to_lowercase();
        }
        if let Some((k, _)) = t.split_once('=') {
            let key = k.trim().to_lowercase();
            match sections.get(&cur_sec).and_then(|m| m.get(&key)) {
                Some(user_val) => out.push_str(&format!("{} = {}\n", k.trim(), user_val)),
                None => {
                    missing += 1;
                    out.push_str(line);
                    out.push('\n');
                }
            }
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    (out, missing)
}

// ============================================================================
// 全局接口
// ============================================================================

static CONFIG: OnceLock<Config> = OnceLock::new();
pub fn get() -> &'static Config { CONFIG.get_or_init(load_config) }

pub fn state_poll_interval_ms() -> u64 { get().poll_state_interval_ms }
pub fn mouse_track_poll_interval_ms() -> u64 { get().poll_mouse_track_interval_ms }
pub fn caret_track_interval_ms() -> u64 { get().poll_caret_track_interval_ms }
pub fn tray_enable() -> bool { get().tray_enable }
pub fn caret_enable() -> bool { get().caret_enable }
pub fn caret_color_cn() -> u32 { get().caret_color_cn }
pub fn caret_color_en() -> u32 { get().caret_color_en }
pub fn caret_size() -> i32 { get().caret_size }
pub fn caret_offset_x() -> i32 { get().caret_offset_x }
pub fn caret_offset_y() -> i32 { get().caret_offset_y }
pub fn caret_show_en() -> bool { get().caret_show_en }
pub fn caret_methods() -> &'static [String] { &get().caret_methods }
pub fn mouse_enable() -> bool { get().mouse_enable }
pub fn mouse_color_cn() -> u32 { get().mouse_color_cn }
pub fn mouse_color_en() -> u32 { get().mouse_color_en }
pub fn mouse_size() -> i32 { get().mouse_size }
pub fn mouse_offset_x() -> i32 { get().mouse_offset_x }
pub fn mouse_offset_y() -> i32 { get().mouse_offset_y }
pub fn mouse_show_en() -> bool { get().mouse_show_en }
pub fn mouse_target_cursors() -> &'static [u32] { &get().mouse_target_cursors }
pub fn autoswitch_enable() -> bool { get().autoswitch_enable }
pub fn autoswitch_match() -> &'static [String] { &get().autoswitch_match }
pub fn autoswitch_enter_cn() -> bool { get().autoswitch_enter == "cn" }
pub fn autoswitch_exclude() -> &'static [String] { &get().autoswitch_exclude }

/// 通配符匹配：* 任意段，? 单字符，大小写不敏感
pub fn glob_match(pattern: &str, text: &str) -> bool {
    fn rec(p: &[u8], t: &[u8]) -> bool {
        match p.first() {
            None => t.is_empty(),
            Some(b'*') => (0..=t.len()).any(|i| rec(&p[1..], &t[i..])),
            Some(b'?') => !t.is_empty() && rec(&p[1..], &t[1..]),
            Some(&c) => t.first() == Some(&c) && rec(&p[1..], &t[1..]),
        }
    }
    rec(pattern.to_lowercase().as_bytes(), text.to_lowercase().as_bytes())
}

/// 自动切换的按程序过滤（纯逻辑）：进程名需命中 match（空列表 = 不匹配任何程序）且不命中 exclude
pub fn list_allows(exe: &str, match_list: &[String], exclude_list: &[String]) -> bool {
    !exclude_list.iter().any(|p| glob_match(p, exe))
        && match_list.iter().any(|p| glob_match(p, exe))
}

/// 按当前配置过滤前台程序
pub fn autoswitch_allows(exe: &str) -> bool {
    list_allows(exe, autoswitch_match(), autoswitch_exclude())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merge_template() {
        // 用户只配了两个键，其余全部缺失
        let mut user = HashMap::new();
        let mut poll = HashMap::new();
        poll.insert("state_interval_ms".to_string(), "200".to_string());
        user.insert("poll".to_string(), poll);
        let (merged, missing) = merge_template(&user);
        assert_eq!(missing, 23); // 模板共 24 个键，补 23 个默认
        // 用户已有值保留
        assert!(merged.contains("state_interval_ms = 200"));
        // 缺失键补默认
        assert!(merged.contains("track_interval_ms = 10"));
        assert!(merged.contains("target_cursors = [32513, 32512]"));
    }

    #[test]
    fn test_glob_and_list_allows() {
        assert!(glob_match("*.exe", "photoshop.exe"));
        assert!(glob_match("windows*.exe", "WindowsTerminal.exe"));
        assert!(!glob_match("*.exe", "photoshop"));
        assert!(glob_match("notepad.ex?", "notepad.exe"));

        let m = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        // 空 match = 不匹配任何程序（要通配就显式写 "*"）
        assert!(!list_allows("anything.exe", &[], &[]));
        assert!(list_allows("anything.exe", &m(&["*"]), &[]));
        // 白名单
        assert!(list_allows("notepad.exe", &m(&["*.exe"]), &[]));
        assert!(!list_allows("chrome.exe", &m(&["notepad.exe"]), &[]));
        // 拒绝优先于匹配
        assert!(!list_allows("photoshop.exe", &m(&["*.exe"]), &m(&["photo*"])));
    }
}
