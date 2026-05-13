//! Whisper smoke test — verifies model loads + transcribe pipeline runs.
//!
//! Doesn't require a microphone; synthesizes 2 seconds of silence + a 440Hz tone
//! and runs Whisper on it. We don't assert on the transcript content (silence
//! → empty or "[BLANK]"); we just confirm: no panic, no error, model loads.
//!
//! Run: `cargo run --example whisper_smoke`

use mouseclaw_lib::transcribe;

fn main() -> anyhow::Result<()> {
    println!("🦞 Whisper smoke test\n");

    println!("① 模型可用？");
    if !transcribe::is_available() {
        println!("   ✘ 模型缺失，应该在 ~/.mouseclaw/models/ggml-base-q5_1.bin");
        std::process::exit(1);
    }
    println!("   ✓ 模型路径存在\n");

    // Synthesize 2s of mostly-silence at 16 kHz with a brief 440 Hz beep at the start
    // — purely to exercise the audio path, not to assert a specific transcript.
    let sample_rate = 16_000usize;
    let secs = 2;
    let mut samples = vec![0.0_f32; sample_rate * secs];
    for (i, s) in samples.iter_mut().enumerate().take(sample_rate / 4) {
        let t = i as f32 / sample_rate as f32;
        *s = 0.1 * (2.0 * std::f32::consts::PI * 440.0 * t).sin();
    }
    println!("② 合成 {}s @ 16kHz ({} 个样本)", secs, samples.len());

    println!("③ 加载 Whisper 模型并转写...");
    let started = std::time::Instant::now();
    let result = transcribe::transcribe(&samples)?;
    let dt = started.elapsed();
    println!("   ✓ 转写完成 ({:.2}s)", dt.as_secs_f32());
    println!("   结果：{:?}", result);
    if result.is_empty() {
        println!("   （静音 → 空转写，正常）");
    }

    println!("\n✅ smoke test 通过：模型加载、API 调用、转写 pipeline 全 ok");
    Ok(())
}
