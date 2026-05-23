//! 跨平台（非 macOS）系统能力薄层 · v0.5 跨平台移植
//!
//! macOS 走各模块自己的 cocoa 实现，**不经过这里**（mac 路径已验证可用，不动）。
//! 本模块只在 Windows + Linux 编译，集中三件事的平台分发：
//!   1. 环境探测：`is_wayland()` / `can_inject()`
//!   2. 只读系统状态：`global_cursor()` / `frontmost_app()`
//!   3. 合成输出：`type_text()` / `delete_chars()` / `paste_via_ctrl_v()` /
//!      `set_clipboard_text()` / `speak()`
//!
//! ⚠️ 运行时未在真机验证（开发容器无 GUI）。Linux X11 路径本地能编译验证，
//! Windows 路径靠 CI 编译验证 + 用户真机测。Wayland 受合成器安全模型限制，
//! 注入 / 全局光标 / 前台窗口三样都降级（见 docs/design/cross-platform-port-20260522.md）。
#![cfg(not(target_os = "macos"))]

use anyhow::{anyhow, Result};

/// 当前 Linux 会话是否 Wayland —— 拿不到全局光标 / 任意键盘注入 / 前台窗口。
#[cfg(target_os = "linux")]
pub fn is_wayland() -> bool {
    std::env::var_os("WAYLAND_DISPLAY").is_some()
        || std::env::var("XDG_SESSION_TYPE")
            .map(|v| v.eq_ignore_ascii_case("wayland"))
            .unwrap_or(false)
}
#[cfg(not(target_os = "linux"))]
pub fn is_wayland() -> bool {
    false
}

/// 能否向前台 app 合成键盘输入。Wayland 多数合成器禁止任意注入 → false（走剪贴板兜底）。
pub fn can_inject() -> bool {
    !is_wayland()
}

/// 全局光标位置（top-left logical px）。Wayland 返回 None → 陪伴动效据此降级关闭。
pub fn global_cursor() -> Option<(f64, f64)> {
    #[cfg(windows)]
    {
        win::global_cursor()
    }
    #[cfg(target_os = "linux")]
    {
        if is_wayland() {
            return None;
        }
        linux::global_cursor()
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        None
    }
}

/// 前台 app 的 (标识, 显示名)。标识在 Win 是进程 exe 名、X11 是 WM_CLASS。
/// Wayland 拿不到 → ("","")，调用方据此放宽（session 边界只靠时间）。
pub fn frontmost_app() -> (String, String) {
    #[cfg(windows)]
    {
        win::frontmost_app()
    }
    #[cfg(target_os = "linux")]
    {
        if is_wayland() {
            return (String::new(), String::new());
        }
        linux::frontmost_app()
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        (String::new(), String::new())
    }
}

/// 距上次输入（键/鼠）秒数 —— 给桌宠"渐睡"动画判定。Wayland / 不支持 → None（不强制渐睡）。
pub fn idle_seconds() -> Option<f64> {
    #[cfg(windows)]
    {
        win::idle_seconds()
    }
    #[cfg(target_os = "linux")]
    {
        if is_wayland() {
            return None;
        }
        linux::idle_seconds()
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        None
    }
}

/// 用 enigo 合成 unicode 文本输入（Mode B 写回 / 听写打字）。
pub fn type_text(text: &str) -> Result<()> {
    use enigo::{Enigo, Keyboard, Settings};
    let mut e = Enigo::new(&Settings::default()).map_err(|e| anyhow!("enigo init: {e}"))?;
    e.text(text).map_err(|e| anyhow!("enigo text: {e}"))?;
    Ok(())
}

/// 退格 n 次（听写纠错 / 删除已写入）。
pub fn delete_chars(n: usize) -> Result<()> {
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};
    let mut e = Enigo::new(&Settings::default()).map_err(|e| anyhow!("enigo init: {e}"))?;
    for _ in 0..n {
        e.key(Key::Backspace, Direction::Click)
            .map_err(|e| anyhow!("enigo backspace: {e}"))?;
    }
    Ok(())
}

/// 模拟粘贴：Ctrl+V（配合 `set_clipboard_text` 用，作为 Wayland / 富文本编辑器的兜底）。
pub fn paste_via_ctrl_v() -> Result<()> {
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};
    let mut e = Enigo::new(&Settings::default()).map_err(|e| anyhow!("enigo init: {e}"))?;
    e.key(Key::Control, Direction::Press)
        .map_err(|e| anyhow!("enigo ctrl down: {e}"))?;
    e.key(Key::Unicode('v'), Direction::Click)
        .map_err(|e| anyhow!("enigo v: {e}"))?;
    e.key(Key::Control, Direction::Release)
        .map_err(|e| anyhow!("enigo ctrl up: {e}"))?;
    Ok(())
}

/// 写文本到系统剪贴板（arboard）。
pub fn set_clipboard_text(text: &str) -> Result<()> {
    let mut cb = arboard::Clipboard::new().map_err(|e| anyhow!("clipboard init: {e}"))?;
    cb.set_text(text.to_string())
        .map_err(|e| anyhow!("clipboard set: {e}"))
}

// ───────────────────── TTS（tts crate：Win SAPI / Linux speech-dispatcher）─────────────────────

static TTS: once_cell::sync::Lazy<std::sync::Mutex<Option<tts::Tts>>> =
    once_cell::sync::Lazy::new(|| std::sync::Mutex::new(tts::Tts::default().ok()));

/// 朗读 text（interrupt=true：新的一条打断上一条，避免叠播）。lang 暂未用于选声（follow-up）。
pub fn speak(text: &str, _lang: &str) {
    if let Ok(mut guard) = TTS.lock() {
        if guard.is_none() {
            *guard = tts::Tts::default().ok();
        }
        if let Some(t) = guard.as_mut() {
            if let Err(e) = t.speak(text, true) {
                eprintln!("[mouseclaw] 🔊 tts speak failed: {e}");
            }
        } else {
            eprintln!("[mouseclaw] 🔊 tts 不可用（Linux 需 speech-dispatcher 在跑）");
        }
    }
}

/// 停止当前朗读。
pub fn stop_speak() {
    if let Ok(mut guard) = TTS.lock() {
        if let Some(t) = guard.as_mut() {
            let _ = t.stop();
        }
    }
}

// ───────────────────── Windows 实现 ─────────────────────
#[cfg(windows)]
mod win {
    pub fn global_cursor() -> Option<(f64, f64)> {
        use windows::Win32::Foundation::POINT;
        use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
        let mut p = POINT::default();
        if unsafe { GetCursorPos(&mut p) }.is_ok() {
            Some((p.x as f64, p.y as f64))
        } else {
            None
        }
    }

    pub fn idle_seconds() -> Option<f64> {
        use windows::Win32::System::SystemInformation::GetTickCount;
        use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
        unsafe {
            let mut lii = LASTINPUTINFO {
                cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
                dwTime: 0,
            };
            if GetLastInputInfo(&mut lii).as_bool() {
                let now = GetTickCount();
                let idle_ms = now.wrapping_sub(lii.dwTime);
                Some(idle_ms as f64 / 1000.0)
            } else {
                None
            }
        }
    }

    /// (进程 exe 名小写, 窗口标题)。exe 名给终端 / 密码管理器排除用，标题给 session 边界。
    pub fn frontmost_app() -> (String, String) {
        use windows::Win32::Foundation::{CloseHandle, HWND, MAX_PATH};
        use windows::Win32::System::Threading::{
            OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
            PROCESS_QUERY_LIMITED_INFORMATION,
        };
        use windows::Win32::UI::WindowsAndMessaging::{
            GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId,
        };
        unsafe {
            let hwnd: HWND = GetForegroundWindow();
            if hwnd.0.is_null() {
                return (String::new(), String::new());
            }
            // 标题
            let mut buf = [0u16; 512];
            let n = GetWindowTextW(hwnd, &mut buf);
            let title = if n > 0 {
                String::from_utf16_lossy(&buf[..n as usize])
            } else {
                String::new()
            };
            // 进程 exe 名
            let mut pid: u32 = 0;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            let mut exe = String::new();
            if pid != 0 {
                if let Ok(handle) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
                    let mut path = [0u16; MAX_PATH as usize];
                    let mut size = path.len() as u32;
                    let pw = windows::core::PWSTR(path.as_mut_ptr());
                    if QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, pw, &mut size).is_ok()
                    {
                        let full = String::from_utf16_lossy(&path[..size as usize]);
                        exe = full
                            .rsplit(['\\', '/'])
                            .next()
                            .unwrap_or(&full)
                            .to_lowercase();
                    }
                    let _ = CloseHandle(handle);
                }
            }
            (exe, title)
        }
    }
}

// ───────────────────── Linux X11 实现 ─────────────────────
#[cfg(target_os = "linux")]
mod linux {
    use std::sync::{Mutex, OnceLock};
    use x11rb::connection::Connection;
    use x11rb::protocol::xproto::{AtomEnum, ConnectionExt, Window};
    use x11rb::rust_connection::RustConnection;

    struct X11 {
        conn: RustConnection,
        root: Window,
    }

    static X11_CONN: OnceLock<Mutex<Option<X11>>> = OnceLock::new();

    /// 拿（缓存的）X11 连接跑 f。f 返回 None 视为连接可能坏 → 清掉下次重连。
    fn with_x11<R>(f: impl FnOnce(&X11) -> Option<R>) -> Option<R> {
        let cell = X11_CONN.get_or_init(|| Mutex::new(None));
        let mut guard = cell.lock().ok()?;
        if guard.is_none() {
            let (conn, screen_num) = RustConnection::connect(None).ok()?;
            let root = conn.setup().roots.get(screen_num)?.root;
            *guard = Some(X11 { conn, root });
        }
        let result = {
            let x = guard.as_ref()?;
            f(x)
        };
        if result.is_none() {
            *guard = None; // 触发下次重连
        }
        result
    }

    pub fn global_cursor() -> Option<(f64, f64)> {
        with_x11(|x| {
            let reply = x.conn.query_pointer(x.root).ok()?.reply().ok()?;
            Some((reply.root_x as f64, reply.root_y as f64))
        })
    }

    pub fn frontmost_app() -> (String, String) {
        with_x11(|x| {
            let active = active_window(x)?;
            let class = wm_class(x, active).unwrap_or_default();
            let title = net_wm_name(x, active).unwrap_or_default();
            Some((class, title))
        })
        .unwrap_or((String::new(), String::new()))
    }

    pub fn idle_seconds() -> Option<f64> {
        use x11rb::protocol::screensaver::ConnectionExt as _;
        with_x11(|x| {
            let info = x.conn.screensaver_query_info(x.root).ok()?.reply().ok()?;
            Some(info.ms_since_user_input as f64 / 1000.0)
        })
    }

    fn intern(x: &X11, name: &str) -> Option<u32> {
        x.conn
            .intern_atom(false, name.as_bytes())
            .ok()?
            .reply()
            .ok()
            .map(|r| r.atom)
    }

    fn active_window(x: &X11) -> Option<Window> {
        let atom = intern(x, "_NET_ACTIVE_WINDOW")?;
        let reply = x
            .conn
            .get_property(false, x.root, atom, AtomEnum::WINDOW, 0, 1)
            .ok()?
            .reply()
            .ok()?;
        let win = reply.value32()?.next()?;
        if win == 0 {
            None
        } else {
            Some(win)
        }
    }

    /// WM_CLASS = "instance\0class\0" —— 取 class 段，小写化（给终端/排除匹配）。
    fn wm_class(x: &X11, win: Window) -> Option<String> {
        let reply = x
            .conn
            .get_property(false, win, AtomEnum::WM_CLASS, AtomEnum::STRING, 0, 256)
            .ok()?
            .reply()
            .ok()?;
        let raw = reply.value;
        let parts: Vec<&[u8]> = raw.split(|&b| b == 0).filter(|s| !s.is_empty()).collect();
        let class = parts.get(1).or_else(|| parts.first())?;
        Some(String::from_utf8_lossy(class).to_lowercase())
    }

    fn net_wm_name(x: &X11, win: Window) -> Option<String> {
        let atom = intern(x, "_NET_WM_NAME")?;
        let utf8 = intern(x, "UTF8_STRING")?;
        let reply = x
            .conn
            .get_property(false, win, atom, utf8, 0, 1024)
            .ok()?
            .reply()
            .ok()?;
        if reply.value.is_empty() {
            return None;
        }
        Some(String::from_utf8_lossy(&reply.value).into_owned())
    }
}
