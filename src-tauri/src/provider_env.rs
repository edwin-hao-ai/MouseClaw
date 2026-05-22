//! Provider env loader (v0.1.25).
//!
//! 让用户**不用改 shell rc** 也能给非 Claude 后端（Codex / OpenClaw / Hermes）
//! 配置 provider 密钥。约定文件：`~/.mouseclaw/provider.env`
//!
//! 简单 KEY=VALUE 一行一对的 dotenv 子集：
//!   - 忽略空行 / `#` 注释行
//!   - 不展开 `$VAR`，不支持多行 / heredoc
//!   - VALUE 两端的 `"` 或 `'` 被去掉（方便从别处复制）
//!
//! 任何 key 都会传给 spawn 的子进程（CLI 后端）。常用：
//!   AI_GATEWAY_API_KEY · OPENAI_API_KEY · OPENAI_BASE_URL ·
//!   ANTHROPIC_API_KEY · OPENROUTER_API_KEY · GEMINI_API_KEY
//!
//! 想用 MDHub 的 Vercel AI Gateway 测试 4 个后端？模板：
//!
//! ```text
//! # ~/.mouseclaw/provider.env
//! AI_GATEWAY_BASE_URL=https://ai-gateway.vercel.sh/v1
//! AI_GATEWAY_API_KEY=vck_xxx
//! # Codex / OpenClaw 走 OpenAI 兼容接口，所以也把 gateway 当 OPENAI_* 灌进去
//! OPENAI_BASE_URL=https://ai-gateway.vercel.sh/v1
//! OPENAI_API_KEY=vck_xxx
//! ```

use std::collections::BTreeMap;
use std::path::PathBuf;

/// 从 `~/.mouseclaw/provider.env` 读出 key=value 对。
/// 文件不存在或读不出来 → 返回空 map（**不**报错，沉默兜底）。
pub fn load() -> BTreeMap<String, String> {
    let Some(path) = env_path() else {
        return BTreeMap::new();
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return BTreeMap::new();
    };
    parse(&text)
}

fn env_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(".mouseclaw/provider.env"))
}

fn parse(text: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let key = k.trim();
        if key.is_empty() {
            continue;
        }
        let val = strip_quotes(v.trim());
        out.insert(key.to_string(), val.to_string());
    }
    out
}

fn strip_quotes(v: &str) -> &str {
    let bytes = v.as_bytes();
    if bytes.len() >= 2
        && ((bytes.first() == Some(&b'"') && bytes.last() == Some(&b'"'))
            || (bytes.first() == Some(&b'\'') && bytes.last() == Some(&b'\'')))
    {
        return &v[1..v.len() - 1];
    }
    v
}

/// 把 provider.env 里的 key/value 套到 `cmd` 上 —— spawn 时子进程就能看到。
/// 已在进程环境里的同名变量**不**覆盖（shell rc 优先级高）。
pub fn apply_to(cmd: &mut tokio::process::Command) {
    for (k, v) in load() {
        if std::env::var_os(&k).is_none() {
            cmd.env(&k, &v);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic_kv() {
        let m = parse("FOO=bar\nBAZ=qux\n");
        assert_eq!(m.get("FOO").map(String::as_str), Some("bar"));
        assert_eq!(m.get("BAZ").map(String::as_str), Some("qux"));
    }

    #[test]
    fn skips_comments_and_blanks() {
        let m = parse("# comment\n\nKEY=val\n  # indented comment\n");
        assert_eq!(m.len(), 1);
        assert_eq!(m.get("KEY").map(String::as_str), Some("val"));
    }

    #[test]
    fn strips_surrounding_quotes() {
        let m = parse("A=\"with spaces\"\nB='single quoted'\n");
        assert_eq!(m.get("A").map(String::as_str), Some("with spaces"));
        assert_eq!(m.get("B").map(String::as_str), Some("single quoted"));
    }

    #[test]
    fn tolerates_malformed_lines() {
        let m = parse("no-equals\n=missing-key\nGOOD=ok\n");
        assert_eq!(m.len(), 1);
        assert_eq!(m.get("GOOD").map(String::as_str), Some("ok"));
    }

    #[test]
    fn vercel_gateway_template_roundtrip() {
        // 文档里给用户复制的模板必须能解析
        let m = parse(r#"
AI_GATEWAY_BASE_URL=https://ai-gateway.vercel.sh/v1
AI_GATEWAY_API_KEY=vck_test
OPENAI_BASE_URL=https://ai-gateway.vercel.sh/v1
OPENAI_API_KEY=vck_test
"#);
        assert_eq!(m.len(), 4);
        assert_eq!(
            m.get("OPENAI_BASE_URL").map(String::as_str),
            Some("https://ai-gateway.vercel.sh/v1")
        );
    }
}
