//! 鼠标轨迹采样 (v0.1.20) —— AI 召唤时让 AI 知道用户「圈了哪些地方」
//!
//! ## 思路
//! 用户按住 ⌘⇧Space 期间，每 80ms 采一次光标位置 + 左键状态。
//! 松开时把所有点烘到 screenshot 上 → AI 看到：
//!   - 灰细线：用户光标的移动轨迹（背景上下文）
//!   - 红粗线 + 端点圆：用户按住左键画的「标注」（"我说的是这块"）
//! 然后正常发给 Claude。AI prompt 里告诉它「这些标记是用户画的，重点关注」。
//!
//! ## 实现
//! - tokio task in on_shortcut_press 开个 sampler，写入 AppState.cursor_trail
//! - on_shortcut_release 之前停 sampler，拿 trail，烘到 last_screenshot 上
//! - 用 `image` crate 直接写 RGBA 像素到 PNG
//! - 性能：100 个点 ≈ 8 秒内，烘图 < 30ms

//! 跨平台说明：`TrailPoint` 数据类型在所有平台编译（被 AppState / pipeline 引用）；
//! 采样 + 烘图实现目前仅 macOS，调用点已在 pipeline.rs 用 `#[cfg(target_os = "macos")]`
//! 包裹。Win/Linux 的轨迹采样依赖「全局光标位置」，待平台抽象接入（Wayland 拿不到）。

#[cfg(target_os = "macos")]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(target_os = "macos")]
use std::sync::{Arc, Mutex};
#[cfg(target_os = "macos")]
use std::time::{Duration, Instant};
#[cfg(target_os = "macos")]
use tauri::{AppHandle, Emitter, Manager};

#[cfg(target_os = "macos")]
use serde::Serialize;

/// 一个采样点
#[derive(Debug, Clone, Copy)]
pub struct TrailPoint {
    pub x: f64,           // 屏幕 logical points, top-left
    pub y: f64,
    pub t_ms: u64,        // 自采样开始的相对毫秒
    pub left_button: bool, // 当时左键是按下的吗？
}

/// 给前端 DrawOverlay 的 payload —— x/y/t + drawing flag
#[cfg(target_os = "macos")]
#[derive(Serialize, Clone, Copy)]
struct TrailPointEvent {
    x: f64,
    y: f64,
    t: u64,
    drawing: bool,
}

/// 全局 trail 缓冲 —— Arc<Mutex<Vec<TrailPoint>>>
#[cfg(target_os = "macos")]
pub static TRAIL: once_cell::sync::Lazy<Arc<Mutex<Vec<TrailPoint>>>> =
    once_cell::sync::Lazy::new(|| Arc::new(Mutex::new(Vec::with_capacity(128))));

/// 控制采样 loop 的 atomic
#[cfg(target_os = "macos")]
static SAMPLING: AtomicBool = AtomicBool::new(false);

/// 开始采样 —— 在 on_shortcut_press 里调
/// v0.1.21：还会向 "draw" 窗口 emit trail-point 事件，让 React canvas 实时画线
#[cfg(target_os = "macos")]
pub fn start(app: AppHandle) {
    if SAMPLING.swap(true, Ordering::SeqCst) {
        return; // 已经在跑
    }
    TRAIL.lock().unwrap().clear();
    // v0.5.x · 用户只想去掉**灰色移动轨迹线**，**圈选（按住左键拖动的粉色标注）要保留**。
    //   所以画板 overlay + 实时 trail-point emit 恢复；灰/彩区分放前端 DrawOverlay
    //   （只画 drawing=true 的粉色圈选，跳过移动灰线）。TRAIL 仍记录全部点烘进截图给 AI。
    let _ = app.emit_to("draw", "trail-clear", ());
    show_draw_overlay(&app);

    let start = Instant::now();
    std::thread::Builder::new().name("mouseclaw-cursor-trail".into()).spawn(move || {
        while SAMPLING.load(Ordering::Relaxed) {
            let app_ref = &app;
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                if let Some((x, y, btn)) = sample_cursor() {
                    let t_ms = start.elapsed().as_millis() as u64;
                    TRAIL.lock().unwrap().push(TrailPoint {
                        x, y, t_ms, left_button: btn,
                    });
                    // 推给 draw overlay 实时画（DrawOverlay 只画 drawing=true 的粉色圈选）
                    let _ = app_ref.emit_to("draw", "trail-point", TrailPointEvent {
                        x, y, t: t_ms, drawing: btn,
                    });
                }
            }));
            if r.is_err() { eprintln!("[mouseclaw] 🐭 trail sample panic, 继续"); }
            std::thread::sleep(Duration::from_millis(60));
        }
        // 采样结束 → 1.5s 后隐藏画板（让用户看一眼刚画的圈选再隐）
        std::thread::sleep(Duration::from_millis(1500));
        if let Some(w) = app.get_webview_window("draw") {
            let _ = w.hide();
        }
    }).ok();
}

/// 开 / 显示全屏透明画板 overlay
#[cfg(target_os = "macos")]
fn show_draw_overlay(app: &AppHandle) {
    use tauri::{WebviewWindowBuilder, WebviewUrl};
    if let Some(w) = app.get_webview_window("draw") {
        let _ = w.show();
        // 确保点击穿透 + 不抢焦点
        let _ = w.set_ignore_cursor_events(true);
        // 提到最上层
        let _ = w.set_always_on_top(true);
        return;
    }

    // 拿屏幕 logical 尺寸来设置窗口大小
    let (sw, sh) = primary_screen_logical_size().unwrap_or((1440.0, 900.0));
    let builder = WebviewWindowBuilder::new(
        app, "draw",
        WebviewUrl::App("index.html?view=draw".into()),
    )
    .title("MouseClaw — Draw")
    .inner_size(sw, sh)
    .position(0.0, 0.0)
    .resizable(false)
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .focused(false)
    .accept_first_mouse(false)
    .visible_on_all_workspaces(true)
    .shadow(false);

    match builder.build() {
        Ok(w) => {
            // 关键：点击穿透 —— 用户的点击照样落到底层 app
            let _ = w.set_ignore_cursor_events(true);
        }
        Err(e) => eprintln!("[mouseclaw] 🎨 draw overlay build: {e}"),
    }
}

/// 停止采样 + 拿出 trail（move 出来，buffer 清空）
#[cfg(target_os = "macos")]
pub fn stop_and_take() -> Vec<TrailPoint> {
    SAMPLING.store(false, Ordering::SeqCst);
    std::mem::take(&mut *TRAIL.lock().unwrap())
}

#[cfg(target_os = "macos")]
fn sample_cursor() -> Option<(f64, f64, bool)> {
    use cocoa::base::{id, nil};
    use cocoa::foundation::NSPoint;
    use objc::{class, msg_send, sel, sel_impl};
    unsafe {
        let pool: id = msg_send![class!(NSAutoreleasePool), new];
        let result = (|| {
            // NSEvent.mouseLocation —— flipped bottom-left coords
            let cls: id = msg_send![class!(NSEvent), class];
            let p: NSPoint = msg_send![cls, mouseLocation];
            // 屏幕主 frame 高度（转 top-left）
            let screen: id = msg_send![class!(NSScreen), mainScreen];
            if screen == nil { return None; }
            let frame: cocoa::foundation::NSRect = msg_send![screen, frame];
            let h = frame.size.height;
            let top_left_y = h - p.y;
            // NSEvent.pressedMouseButtons —— bit 0 = left
            let buttons: u64 = msg_send![cls, pressedMouseButtons];
            let left = (buttons & 1) != 0;
            Some((p.x, top_left_y, left))
        })();
        if pool != nil { let _: () = msg_send![pool, drain]; }
        result
    }
}

// ────────────────── 烘图：把轨迹画到 PNG 上（仅 macOS 调用）──────────────────

#[cfg(target_os = "macos")]
use std::path::Path;
#[cfg(target_os = "macos")]
use anyhow::{Context, Result};

/// 把 trail 画在 screenshot PNG 上。原 path 被覆盖；如果 trail 空则不动。
///
/// 视觉：
///   - 左键松开的段（trajectory）：灰色 2px 线
///   - 左键按住的段（annotation）：粉红色 6px 线 + 端点圆
#[cfg(target_os = "macos")]
pub fn render_onto_screenshot(png_path: &Path, trail: &[TrailPoint]) -> Result<()> {
    if trail.is_empty() { return Ok(()); }
    use image::{ImageReader, Rgba, RgbaImage};

    let img = ImageReader::open(png_path)
        .context("open screenshot")?
        .decode()
        .context("decode screenshot")?;
    let mut rgba: RgbaImage = img.into_rgba8();
    let (iw, ih) = rgba.dimensions();

    // 屏幕 logical points → image pixel 比例（screencapture 出的是 retina @2x）
    // 拿一下屏幕 logical 尺寸：
    let (sw, sh) = primary_screen_logical_size().unwrap_or((iw as f64, ih as f64));
    let sx = iw as f64 / sw.max(1.0);
    let sy = ih as f64 / sh.max(1.0);

    let traj_color = Rgba([170u8, 170, 170, 220]);     // 灰
    let annot_color = Rgba([232u8, 99, 140, 255]);     // 粉红 (--accent)
    let annot_glow = Rgba([232u8, 99, 140, 80]);

    let mut prev: Option<(f64, f64, bool)> = None;
    for p in trail {
        let cx = p.x * sx;
        let cy = p.y * sy;
        if let Some((px, py, pbtn)) = prev {
            // 连线（取两端按键状态较强的）
            let drawing = p.left_button || pbtn;
            let (color, thick) = if drawing {
                (annot_color, 4.0)
            } else {
                (traj_color, 1.5)
            };
            draw_line(&mut rgba, px * sx, py * sy, cx, cy, thick, color);
            if drawing {
                // 标注段加发光
                draw_line(&mut rgba, px * sx, py * sy, cx, cy, 8.0, annot_glow);
            }
        }
        // 关键点：左键按下时打个端点圆
        if p.left_button {
            draw_disc(&mut rgba, cx, cy, 5.0, annot_color);
        }
        prev = Some((p.x, p.y, p.left_button));
    }

    rgba.save(png_path).context("save annotated screenshot")?;
    Ok(())
}

/// Bresenham-ish thick line —— 简单暴力，性能够用
#[cfg(target_os = "macos")]
fn draw_line(img: &mut image::RgbaImage, x0: f64, y0: f64, x1: f64, y1: f64,
             thick: f64, color: image::Rgba<u8>) {
    let dx = x1 - x0;
    let dy = y1 - y0;
    let len = (dx*dx + dy*dy).sqrt();
    if len < 0.5 { return; }
    let steps = (len.ceil() as usize).max(1);
    for i in 0..=steps {
        let t = i as f64 / steps as f64;
        let cx = x0 + dx * t;
        let cy = y0 + dy * t;
        draw_disc(img, cx, cy, thick * 0.5, color);
    }
}

#[cfg(target_os = "macos")]
fn draw_disc(img: &mut image::RgbaImage, cx: f64, cy: f64, r: f64, color: image::Rgba<u8>) {
    let (w, h) = img.dimensions();
    let x0 = ((cx - r).floor() as i32).max(0);
    let y0 = ((cy - r).floor() as i32).max(0);
    let x1 = ((cx + r).ceil() as i32).min(w as i32 - 1);
    let y1 = ((cy + r).ceil() as i32).min(h as i32 - 1);
    let r2 = r * r;
    for y in y0..=y1 {
        for x in x0..=x1 {
            let dx = x as f64 - cx;
            let dy = y as f64 - cy;
            if dx*dx + dy*dy <= r2 {
                blend(img, x as u32, y as u32, color);
            }
        }
    }
}

#[cfg(target_os = "macos")]
fn blend(img: &mut image::RgbaImage, x: u32, y: u32, src: image::Rgba<u8>) {
    if x >= img.width() || y >= img.height() { return; }
    let alpha = src[3] as f32 / 255.0;
    let inv = 1.0 - alpha;
    let dst = img.get_pixel_mut(x, y);
    dst[0] = (src[0] as f32 * alpha + dst[0] as f32 * inv).round() as u8;
    dst[1] = (src[1] as f32 * alpha + dst[1] as f32 * inv).round() as u8;
    dst[2] = (src[2] as f32 * alpha + dst[2] as f32 * inv).round() as u8;
    dst[3] = 255;
}

#[cfg(target_os = "macos")]
fn primary_screen_logical_size() -> Option<(f64, f64)> {
    use cocoa::base::{id, nil};
    use cocoa::foundation::NSRect;
    use objc::{class, msg_send, sel, sel_impl};
    unsafe {
        let pool: id = msg_send![class!(NSAutoreleasePool), new];
        let r = (|| {
            let s: id = msg_send![class!(NSScreen), mainScreen];
            if s == nil { return None; }
            let f: NSRect = msg_send![s, frame];
            Some((f.size.width, f.size.height))
        })();
        if pool != nil { let _: () = msg_send![pool, drain]; }
        r
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    #[test]
    fn empty_trail_no_op() {
        // 空 trail 不应该崩
        let dir = tempfile::tempdir().ok();
        let path = match dir {
            Some(d) => d.path().join("x.png"),
            None => return,
        };
        let r = render_onto_screenshot(&path, &[]);
        assert!(r.is_ok());
    }
}
