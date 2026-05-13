//! 端到端 smoke test：不依赖 Tauri runtime / 全局快捷键，直接走
//! 截屏 → Claude CLI → 打印回复 + Mode 检测。
//!
//! 跑法：`cargo run --example smoke_pipeline`
//! 大约 10-30 秒（取决于 Claude API 响应速度）。

use mouseclaw_lib::{claude_cli, screenshot};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("🦞 MouseClaw smoke test\n");

    println!("① 截屏中...");
    let img = screenshot::capture_main_screen().await?;
    let size = std::fs::metadata(&img)?.len();
    println!("   ✓ {} ({:.1} KB)\n", img.display(), size as f64 / 1024.0);

    let transcript = "看一眼当前屏幕，用一句中文告诉我你看到了什么主要内容。";
    println!("② 调 Claude CLI：");
    println!("   transcript = {transcript:?}");
    println!("   image = {}", img.display());
    println!("   等待回复...");

    let started = std::time::Instant::now();
    let reply = claude_cli::ask_claude(transcript, &img, None).await?;
    let dt = started.elapsed();
    println!("   ✓ 回复 ({:.1}s):", dt.as_secs_f32());
    println!("   ─────────────────────────────");
    for line in reply.lines() {
        println!("   {line}");
    }
    println!("   ─────────────────────────────\n");

    println!("③ Mode 检测：");
    match claude_cli::parse_insert_directive(&reply) {
        Some(text) => println!("   ⌨  Mode B — 要写入光标：{text:?}"),
        None => println!("   💬  Mode A — 仅气泡显示"),
    }

    Ok(())
}
