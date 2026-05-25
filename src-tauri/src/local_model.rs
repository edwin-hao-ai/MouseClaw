//! 本地小模型 (v0.6) —— Qwen3-0.6B ONNX，没装任何 CLI 后端时兜底基础任务
//! （清理 / 翻译 / 整理）。纯 Rust + ONNX Runtime（`ort`，与 sherpa 同一家），
//! 无 Python、无 Ollama、无新运行时。Spike 已验证 ort 与 sherpa 同二进制共存。
//!
//! ## 定位（CLAUDE.md：把技术藏后面 + 别饿死即时听写）
//! - 只做**基础文本任务**；复杂 / agentic / 截图 → 真 CLI 后端（视觉 0.8B 实测不可用）。
//! - 模型按需下载（复用 `model_downloader`）、首次调用才加载进内存（懒加载）。
//! - 推理走 `spawn_blocking` 且全程持锁串行 —— 一次只跑一个，绝不和别的抢，
//!   保护语音输入法的即时性。
//!
//! ## 实现要点（spike 踩平的坑）
//! - 标准 transformer：`input_ids/attention_mask/position_ids + past_key_values.{0..27}.{key,value}`
//!   → `logits + present.{0..27}.{key,value}`。
//! - 空 KV cache（首轮 past_len=0）用 ndarray 构造（ort 的 (shape,vec) 路径拒绝 0 维）。
//! - KV cache 是 **f16**，logits 是 **f32**（取 logits 必须按 f32）。

use std::path::PathBuf;
use std::sync::Mutex;

use anyhow::{anyhow, bail, Result};
use half::f16;
use once_cell::sync::Lazy;
use ort::session::Session;
use ort::value::Tensor;
use tokenizers::Tokenizer;

const NUM_LAYERS: usize = 28;
const KV_HEADS: usize = 8;
const HEAD_DIM: usize = 128;
const DEFAULT_MAX_TOKENS: usize = 256;

/// 懒加载并缓存的 (session, tokenizer)。首次 generate 时载入；持锁串行推理。
#[allow(clippy::type_complexity)]
static MODEL: Lazy<Mutex<Option<(Session, Tokenizer)>>> = Lazy::new(|| Mutex::new(None));

/// 模型目录（`<models_dir>/qwen3-0.6b-onnx/`）—— 复用 model_downloader 的落盘约定。
fn model_dir() -> Result<PathBuf> {
    crate::model_downloader::qwen_06b_spec().dest_dir()
}

/// 模型文件是否已下载齐全。
pub fn is_ready() -> bool {
    crate::model_downloader::qwen_06b_spec().is_ready()
}

/// 异步入口：把口述 prompt 喂本地模型，返回生成文本。推理在 spawn_blocking 里跑。
pub async fn generate(prompt: &str) -> Result<String> {
    let p = prompt.to_string();
    tokio::task::spawn_blocking(move || generate_blocking(&p, DEFAULT_MAX_TOKENS))
        .await
        .map_err(|e| anyhow!("local_model join: {e}"))?
}

fn argmax_f32(slice: &[f32]) -> usize {
    let mut best = 0usize;
    let mut bestv = f32::MIN;
    for (i, &v) in slice.iter().enumerate() {
        if v > bestv {
            bestv = v;
            best = i;
        }
    }
    best
}

/// 阻塞式贪心解码（在 spawn_blocking 里调）。全程持 MODEL 锁 → 串行，不抢资源。
fn generate_blocking(user_prompt: &str, max_tokens: usize) -> Result<String> {
    if !is_ready() {
        bail!("本地模型未就绪（Qwen3-0.6B 未下载完成）");
    }
    let mut guard = MODEL.lock().map_err(|_| anyhow!("local_model lock poisoned"))?;
    if guard.is_none() {
        let dir = model_dir()?;
        let tok = Tokenizer::from_file(dir.join("tokenizer.json"))
            .map_err(|e| anyhow!("load tokenizer: {e}"))?;
        // 执行器：用默认 CPU EP。实测 CoreML EP 对这个动态形状 LLM 反而更慢
        // （首调 ~27s 图编译 + 稳态比 CPU 还慢，算子大量回退）—— 故不用 CoreML。
        let session = Session::builder()?.commit_from_file(dir.join("model_q4f16.onnx"))?;
        *guard = Some((session, tok));
        println!("[mouseclaw] 🧠 local model loaded (Qwen3-0.6B ONNX)");
    }
    let (session, tok) = guard.as_mut().unwrap();

    let im_end = tok.token_to_id("<|im_end|>").unwrap_or(151645) as i64;
    let eos = tok.token_to_id("<|endoftext|>").unwrap_or(151643) as i64;

    // 强 system 消息：压住小模型"把指令/规则当文本复述"的 echo bug（给 Claude 调的
    // 啰嗦 reactive 提示喂 0.6B 会触发，spike 实测过）。/no_think 抑制思考链。
    const SYS: &str = "你是文本处理工具。严格执行用户给的指令处理文本，\
只输出处理后的结果文本本身，绝不复述指令、规则、原文标记（如 ===== 原文 =====）或加任何解释。";
    let prompt = format!(
        "<|im_start|>system\n{SYS}<|im_end|>\n<|im_start|>user\n{user_prompt} /no_think<|im_end|>\n<|im_start|>assistant\n"
    );
    let enc = tok.encode(prompt, false).map_err(|e| anyhow!("encode: {e}"))?;
    let prompt_ids: Vec<i64> = enc.get_ids().iter().map(|&x| x as i64).collect();

    let mut past: Vec<Vec<f16>> = vec![Vec::new(); NUM_LAYERS * 2];
    let mut past_len = 0usize;
    let mut cur: Vec<i64> = prompt_ids;
    let mut pos_start = 0usize;
    let mut generated: Vec<u32> = Vec::new();

    for _ in 0..max_tokens {
        let seq = cur.len();
        let total = past_len + seq;

        let input_ids = Tensor::from_array(([1usize, seq], cur.clone()))?;
        let attn = Tensor::from_array(([1usize, total], vec![1i64; total]))?;
        let pos: Vec<i64> = (pos_start..pos_start + seq).map(|x| x as i64).collect();
        let position_ids = Tensor::from_array(([1usize, seq], pos))?;

        let mut inputs: Vec<(std::borrow::Cow<str>, ort::value::DynValue)> = vec![
            ("input_ids".into(), input_ids.into_dyn()),
            ("attention_mask".into(), attn.into_dyn()),
            ("position_ids".into(), position_ids.into_dyn()),
        ];
        for l in 0..NUM_LAYERS {
            // 空 past（past_len=0）用 ndarray 构造，ort 的 (shape,vec) 路径拒绝 0 维。
            let shape = (1usize, KV_HEADS, past_len, HEAD_DIM);
            let karr = ndarray::Array4::<f16>::from_shape_vec(shape, past[l * 2].clone())?;
            let varr = ndarray::Array4::<f16>::from_shape_vec(shape, past[l * 2 + 1].clone())?;
            inputs.push((format!("past_key_values.{l}.key").into(), Tensor::from_array(karr)?.into_dyn()));
            inputs.push((format!("past_key_values.{l}.value").into(), Tensor::from_array(varr)?.into_dyn()));
        }

        let outputs = session.run(inputs)?;

        let (lshape, logits) = outputs["logits"].try_extract_tensor::<f32>()?;
        let vocab = lshape[lshape.len() - 1] as usize;
        let last = &logits[(seq - 1) * vocab..seq * vocab];
        let next = argmax_f32(last) as i64;
        if next == im_end || next == eos {
            break;
        }
        generated.push(next as u32);

        for l in 0..NUM_LAYERS {
            let (_s, k) = outputs[format!("present.{l}.key").as_str()].try_extract_tensor::<f16>()?;
            past[l * 2] = k.to_vec();
            let (_s, v) = outputs[format!("present.{l}.value").as_str()].try_extract_tensor::<f16>()?;
            past[l * 2 + 1] = v.to_vec();
        }
        past_len = total;
        cur = vec![next];
        pos_start += seq;
    }

    let mut text = tok.decode(&generated, true).map_err(|e| anyhow!("decode: {e}"))?;
    // 剥掉空的 <think>…</think> 块（/no_think 仍会吐一个空块）。
    if let Some(idx) = text.rfind("</think>") {
        text = text[idx + "</think>".len()..].to_string();
    }
    Ok(text.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    // 端到端：需模型已下载才跑（CI 无模型自动跳过，不把 580MB 塞进 CI）。
    // 本地把模型放到 ~/.mouseclaw/models/qwen3-0.6b-onnx/ 即可验证生产代码路径。
    #[test]
    fn generate_basic_task_when_model_present() {
        if !is_ready() {
            eprintln!("skip: 本地模型未下载，跳过端到端测试");
            return;
        }
        // 跑两类有代表性的任务并计时（CoreML EP 真实生产延迟）。
        for (label, prompt) in [
            ("翻译", "把下面的中文翻译成英文，只输出译文：你好，世界。"),
            ("整理成清单", "把下面口述整理成有序清单，只输出清单：今天要先写周报然后给客户回邮件对了还要订会议室"),
        ] {
            // 预热一次（首调含模型加载），再计时
            let t = std::time::Instant::now();
            let out = generate_blocking(prompt, 96).expect("generate ok");
            eprintln!("[{label}] {}ms · 输出: {out:?}", t.elapsed().as_millis());
            assert!(!out.trim().is_empty(), "输出不应为空");
            assert!(!out.contains("<think>"), "不应残留 think 标签");
        }
    }

    // 喂 production 的啰嗦 reactive 提示（曾让小模型把规则当文本输出）——
    // 验证强 system 消息压住了 echo bug。需模型在场才跑。
    #[test]
    fn verbose_clipboard_prompt_no_echo() {
        if !is_ready() {
            eprintln!("skip: 本地模型未下载");
            return;
        }
        let verbose = "请翻译下面这段文字：\n\
             - 自动判断语向：中文 → 英文；非中文 → 中文\n\
             - 不要解释、不要加注释\n\
             - 只输出翻译后的纯文本\n\n\
             ===== 原文 START =====\n这个功能下周三上线。\n===== 原文 END =====";
        let out = generate_blocking(verbose, 80).expect("generate ok");
        eprintln!("啰嗦提示输出: {out:?}");
        assert!(!out.trim().is_empty(), "输出不应为空");
        // 不应把提示规则/标记复述出来
        assert!(!out.contains("====="), "不应复述原文标记");
        assert!(!out.contains("自动判断语向"), "不应复述规则");
    }
}
