//! v0.4 · "拖文档喂桌宠" 的文件摄入层。
//!
//! 负责：分类（图片 / 文本 / PDF / DOCX / 拒绝）+ 抽文本。
//! 不负责：UI、录音、调 AI —— 那些在 feed_flow / pipeline 里。
//!
//! 抽文本策略（macOS 自带工具，零新依赖）：
//!   - 纯文本类：直接 fs::read_to_string
//!   - PDF：`mdls -raw -name kMDItemTextContent <path>` —— Spotlight 已索引的文件秒回
//!     fallback: 没索引就跳过（极少见，用户可以再问）
//!   - DOCX / RTF / odt：`textutil -convert txt -stdout <path>` —— Apple 自带
//!   - 图片：不抽文本，作为附件图直接给后端
//!
//! 设计目标：单文件 ≤ 400 行，所有 IO 异步，错误友好。

use anyhow::{anyhow, Context, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};
use tokio::process::Command;

/// 单个文件大小上限：超过会拒绝。
pub const MAX_FILE_BYTES: u64 = 20 * 1024 * 1024;
/// 单次 feed 最多吃几个文件。
pub const MAX_FILES: usize = 5;
/// 文本类内联到 prompt 的字符上限（再多就只给路径让 Claude Read）。
pub const INLINE_TEXT_CHAR_LIMIT: usize = 30_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FeedKind {
    Image,
    Text,
    Pdf,
    Docx,
    Xlsx,
    Reject,
}

impl FeedKind {
    pub fn label_zh(self) -> &'static str {
        match self {
            FeedKind::Image => "图片",
            FeedKind::Text => "文本",
            FeedKind::Pdf => "PDF",
            FeedKind::Docx => "Word",
            FeedKind::Xlsx => "Excel",
            FeedKind::Reject => "拒收",
        }
    }
}

/// 已分类 + 已抽取的单文件。
#[derive(Debug, Clone, Serialize)]
pub struct FedFile {
    pub path: PathBuf,
    pub name: String,
    pub kind: FeedKind,
    pub size_bytes: u64,
    /// 抽出的纯文本（仅 Text / Pdf / Docx 有）。
    /// 已经被 INLINE_TEXT_CHAR_LIMIT 截过的话，末尾会带 `\n…(截断 N 字)`。
    #[serde(skip)]
    pub text: Option<String>,
    /// 拒收原因（仅 Reject 有）。
    pub reject_reason: Option<String>,
}

/// 一次 drop 摄入的全部文件。
#[derive(Debug, Clone, Default, Serialize)]
pub struct FeedBundle {
    pub files: Vec<FedFile>,
}

impl FeedBundle {
    pub fn has_accepted(&self) -> bool {
        self.files.iter().any(|f| f.kind != FeedKind::Reject)
    }
    pub fn accepted_count(&self) -> usize {
        self.files.iter().filter(|f| f.kind != FeedKind::Reject).count()
    }
    pub fn rejected_count(&self) -> usize {
        self.files.iter().filter(|f| f.kind == FeedKind::Reject).count()
    }
    pub fn names_zh(&self) -> String {
        self.files
            .iter()
            .map(|f| f.name.as_str())
            .collect::<Vec<_>>()
            .join("、")
    }
    pub fn image_paths(&self) -> Vec<PathBuf> {
        self.files
            .iter()
            .filter(|f| f.kind == FeedKind::Image)
            .map(|f| f.path.clone())
            .collect()
    }
    /// 拼成给 Claude 的 prompt 前置上下文 —— 文本内联，图片只给路径。
    /// 空 bundle 返回 None。
    pub fn prompt_preamble(&self) -> Option<String> {
        if !self.has_accepted() {
            return None;
        }
        let mut out = String::from("📎 用户刚刚把以下文件拖给了你（喂给桌宠）：\n");
        for (i, f) in self.files.iter().enumerate() {
            if f.kind == FeedKind::Reject {
                continue;
            }
            let n = i + 1;
            match f.kind {
                FeedKind::Image => {
                    out.push_str(&format!(
                        "  {n}. 🖼️ 图片 `{}`\n     路径：{}\n     用 Read 工具看图（已开 Read 权限）。\n",
                        f.name,
                        f.path.display()
                    ));
                }
                FeedKind::Text | FeedKind::Pdf | FeedKind::Docx | FeedKind::Xlsx => {
                    if let Some(text) = &f.text {
                        if text.chars().count() <= INLINE_TEXT_CHAR_LIMIT {
                            out.push_str(&format!(
                                "  {n}. 📄 {} `{}`（{} 字节，{} 字符）\n     <<<DOC_{n}\n{}\n     DOC_{n}>>>\n",
                                f.kind.label_zh(),
                                f.name,
                                f.size_bytes,
                                text.chars().count(),
                                text
                            ));
                        } else {
                            out.push_str(&format!(
                                "  {n}. 📄 {} `{}`（{} 字节，太长已存到本地，请用 Read 工具读：{}）\n",
                                f.kind.label_zh(),
                                f.name,
                                f.size_bytes,
                                f.path.display()
                            ));
                        }
                    } else {
                        out.push_str(&format!(
                            "  {n}. 📄 {} `{}`（抽文本失败，请用 Read 工具直接读：{}）\n",
                            f.kind.label_zh(),
                            f.name,
                            f.path.display()
                        ));
                    }
                }
                FeedKind::Reject => {}
            }
        }
        let rejected = self
            .files
            .iter()
            .filter(|f| f.kind == FeedKind::Reject)
            .collect::<Vec<_>>();
        if !rejected.is_empty() {
            out.push_str("\n⚠️ 以下文件被拒收（不用回答这部分）：\n");
            for f in rejected {
                out.push_str(&format!(
                    "  - `{}`：{}\n",
                    f.name,
                    f.reject_reason.as_deref().unwrap_or("未知")
                ));
            }
        }
        out.push_str("\n用户接下来的语音是关于这些文件的问题。回答时引用具体文件名让用户知道你看了哪份。\n\n用户问题：");
        Some(out)
    }
}

/// 按扩展名 + magic bytes 判类。失败兜底 Reject。
pub fn classify(path: &Path) -> FeedKind {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        // 图片
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "heic" | "heif" | "bmp" | "tiff" => FeedKind::Image,
        // 纯文本类（含源码）
        "txt" | "md" | "markdown" | "rst" | "json" | "yaml" | "yml" | "toml" | "csv" | "tsv"
        | "log" | "ini" | "conf" | "xml" | "html" | "htm" | "css" | "scss"
        | "js" | "jsx" | "ts" | "tsx" | "mjs" | "cjs"
        | "py" | "rb" | "go" | "rs" | "java" | "kt" | "swift" | "c" | "cc" | "cpp" | "h" | "hpp"
        | "cs" | "php" | "lua" | "sh" | "bash" | "zsh" | "fish" | "sql" | "graphql" | "proto" => {
            FeedKind::Text
        }
        "pdf" => FeedKind::Pdf,
        "docx" | "doc" | "rtf" | "odt" => FeedKind::Docx,
        "xlsx" | "xls" => FeedKind::Xlsx,
        _ => FeedKind::Reject,
    }
}

/// 把一组文件路径变成 FeedBundle —— 异步，能跑外部命令。
/// 顺序保留；超出 MAX_FILES 的尾部被拒收。
pub async fn ingest(paths: Vec<PathBuf>) -> FeedBundle {
    let mut bundle = FeedBundle::default();
    for (i, path) in paths.into_iter().enumerate() {
        if i >= MAX_FILES {
            bundle.files.push(FedFile {
                name: path
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| path.display().to_string()),
                size_bytes: 0,
                path,
                kind: FeedKind::Reject,
                text: None,
                reject_reason: Some(format!("一次最多喂 {} 个文件", MAX_FILES)),
            });
            continue;
        }
        bundle.files.push(ingest_one(path).await);
    }
    bundle
}

async fn ingest_one(path: PathBuf) -> FedFile {
    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| path.display().to_string());
    let size = tokio::fs::metadata(&path)
        .await
        .map(|m| m.len())
        .unwrap_or(0);
    if size == 0 && !path.exists() {
        return FedFile {
            name,
            size_bytes: 0,
            path,
            kind: FeedKind::Reject,
            text: None,
            reject_reason: Some("文件不存在".into()),
        };
    }
    if size > MAX_FILE_BYTES {
        return FedFile {
            name,
            size_bytes: size,
            path,
            kind: FeedKind::Reject,
            text: None,
            reject_reason: Some(format!(
                "文件太大（{:.1} MB），上限 {} MB",
                size as f64 / 1024.0 / 1024.0,
                MAX_FILE_BYTES / 1024 / 1024
            )),
        };
    }
    let kind = classify(&path);
    match kind {
        FeedKind::Image => FedFile {
            name,
            size_bytes: size,
            path,
            kind,
            text: None,
            reject_reason: None,
        },
        FeedKind::Text => {
            let text = tokio::fs::read_to_string(&path).await.ok().map(truncate_for_inline);
            FedFile {
                name,
                size_bytes: size,
                path,
                kind: if text.is_some() { kind } else { FeedKind::Reject },
                text,
                reject_reason: None,
            }
        }
        FeedKind::Pdf => {
            let text = extract_pdf_text(&path).await.ok().map(truncate_for_inline);
            FedFile {
                name,
                size_bytes: size,
                path,
                kind,
                text,
                reject_reason: None,
            }
        }
        FeedKind::Docx => {
            let text = extract_docx_text(&path).await.ok().map(truncate_for_inline);
            FedFile {
                name,
                size_bytes: size,
                path,
                kind,
                text,
                reject_reason: None,
            }
        }
        FeedKind::Xlsx => {
            let text = extract_xlsx_text(&path).await.ok().map(truncate_for_inline);
            FedFile {
                name,
                size_bytes: size,
                path,
                kind,
                text,
                reject_reason: None,
            }
        }
        FeedKind::Reject => FedFile {
            name,
            size_bytes: size,
            path,
            kind,
            text: None,
            reject_reason: Some("不支持的格式（app/exe/zip/视频/音频等吃不下）".into()),
        },
    }
}

fn truncate_for_inline(s: String) -> String {
    let total = s.chars().count();
    if total <= INLINE_TEXT_CHAR_LIMIT {
        return s;
    }
    let kept: String = s.chars().take(INLINE_TEXT_CHAR_LIMIT).collect();
    format!(
        "{kept}\n…（截断 {} 字，剩余请用 Read 工具看原文件）",
        total - INLINE_TEXT_CHAR_LIMIT
    )
}

/// PDF 抽文本：用 Spotlight 的 `mdls -raw -name kMDItemTextContent`。
/// 已被 Spotlight 索引的 PDF 秒回；没索引的会回 `(null)`，我们 fallback 到 Reject 的 text 字段。
async fn extract_pdf_text(path: &Path) -> Result<String> {
    let out = Command::new("mdls")
        .args(["-raw", "-name", "kMDItemTextContent"])
        .arg(path)
        .output()
        .await
        .context("spawn mdls")?;
    if !out.status.success() {
        return Err(anyhow!(
            "mdls failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() || s == "(null)" {
        return Err(anyhow!("Spotlight 还没索引这份 PDF，请稍后再试或换文本格式"));
    }
    Ok(s)
}

/// XLSX 抽文本：用 macOS 自带的 `unzip` 拆出 sharedStrings.xml + 各 sheet。
/// 输出格式：先列出 sharedStrings（字符串池），再按行列出每个 sheet 的单元格值。
/// 不解析合并单元格、不渲染表格 —— AI 自己从字符串理解结构。
async fn extract_xlsx_text(path: &Path) -> Result<String> {
    // 1. sharedStrings.xml: 所有字符串单元格的值池
    let shared = Command::new("unzip")
        .args(["-p"])
        .arg(path)
        .arg("xl/sharedStrings.xml")
        .output()
        .await
        .context("spawn unzip sharedStrings")?;
    let shared_str = String::from_utf8_lossy(&shared.stdout).into_owned();
    let strings = extract_xml_t_values(&shared_str);

    // 2. 列出 sheet 文件名
    let listing = Command::new("unzip")
        .args(["-Z1"])
        .arg(path)
        .output()
        .await
        .context("spawn unzip -Z1")?;
    let sheets: Vec<String> = String::from_utf8_lossy(&listing.stdout)
        .lines()
        .filter(|l| l.starts_with("xl/worksheets/sheet") && l.ends_with(".xml"))
        .map(|s| s.to_string())
        .collect();

    if sheets.is_empty() {
        // 没拆出 sheet —— 兜底：sharedStrings 已有的话也算
        if strings.is_empty() {
            return Err(anyhow!("xlsx 里没找到 sheet 或 sharedStrings"));
        }
        return Ok(format!("[XLSX 字符串池]\n{}", strings.join("\n")));
    }

    let mut out = String::new();
    for (i, sheet) in sheets.iter().enumerate() {
        let sheet_out = Command::new("unzip")
            .args(["-p"])
            .arg(path)
            .arg(sheet)
            .output()
            .await
            .context("spawn unzip sheet")?;
        let sheet_xml = String::from_utf8_lossy(&sheet_out.stdout).into_owned();
        out.push_str(&format!("\n[Sheet {}]\n", i + 1));
        out.push_str(&xlsx_sheet_to_csv(&sheet_xml, &strings));
    }
    if out.trim().is_empty() {
        return Err(anyhow!("xlsx 文本为空"));
    }
    Ok(out)
}

/// 抽 XML 里所有 `<t>...</t>` 内的文本（XLSX sharedStrings + DOCX word/document.xml 都用这套）。
fn extract_xml_t_values(xml: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut i = 0;
    let bytes = xml.as_bytes();
    while i < bytes.len() {
        // 找 <t> 或 <t xml:space="preserve">
        if let Some(start) = find_subseq(bytes, b"<t", i) {
            // 找 > 结束这个开标签
            if let Some(gt) = find_byte(bytes, b'>', start) {
                let content_start = gt + 1;
                if let Some(end) = find_subseq(bytes, b"</t>", content_start) {
                    let raw = &xml[content_start..end];
                    out.push(xml_unescape(raw));
                    i = end + 4;
                    continue;
                }
            }
        }
        break;
    }
    out
}

/// 找下一个 `<c ` 或 `<c>` 起点（XLSX cell 标签可以有或没有属性）。
fn next_cell_start(haystack: &[u8], from: usize) -> Option<usize> {
    let mut cursor = from;
    while let Some(p) = find_subseq(haystack, b"<c", cursor) {
        if p + 2 < haystack.len() {
            let next = haystack[p + 2];
            if next == b' ' || next == b'>' {
                return Some(p);
            }
        }
        cursor = p + 2;
    }
    None
}

fn find_subseq(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if from >= haystack.len() {
        return None;
    }
    haystack[from..].windows(needle.len()).position(|w| w == needle).map(|p| p + from)
}
fn find_byte(haystack: &[u8], b: u8, from: usize) -> Option<usize> {
    if from >= haystack.len() {
        return None;
    }
    haystack[from..].iter().position(|&x| x == b).map(|p| p + from)
}
fn xml_unescape(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

/// 把 sheet XML 转成简易 CSV —— 每行一个 `<row>`，每个 `<c>` 用 \t 分隔。
/// 引用 sharedStrings 池（t="s" 时 `<v>index</v>`）；其他类型直接取 `<v>` 值。
fn xlsx_sheet_to_csv(xml: &str, strings: &[String]) -> String {
    let mut out = String::new();
    let bytes = xml.as_bytes();
    let mut i = 0;
    while let Some(row_start) = find_subseq(bytes, b"<row", i) {
        let Some(row_end) = find_subseq(bytes, b"</row>", row_start) else { break };
        let row_xml = &xml[row_start..row_end];
        let mut cells: Vec<String> = Vec::new();
        let row_bytes = row_xml.as_bytes();
        let mut j = 0;
        while let Some(c_start) = next_cell_start(row_bytes, j) {
            let Some(c_end) = find_subseq(row_bytes, b"</c>", c_start)
                .or_else(|| find_subseq(row_bytes, b"/>", c_start)) else { break };
            let cell_xml = &row_xml[c_start..c_end];
            let is_shared = cell_xml.contains("t=\"s\"");
            let val = if let Some(v_start) = cell_xml.find("<v>") {
                let v_off = v_start + 3;
                if let Some(v_end_rel) = cell_xml[v_off..].find("</v>") {
                    Some(cell_xml[v_off..v_off + v_end_rel].to_string())
                } else {
                    None
                }
            } else if let Some(t_start) = cell_xml.find("<t>") {
                let t_off = t_start + 3;
                if let Some(t_end_rel) = cell_xml[t_off..].find("</t>") {
                    Some(xml_unescape(&cell_xml[t_off..t_off + t_end_rel]))
                } else {
                    None
                }
            } else {
                None
            };
            let cell_text = match val {
                Some(v) if is_shared => v
                    .parse::<usize>()
                    .ok()
                    .and_then(|idx| strings.get(idx).cloned())
                    .unwrap_or(v),
                Some(v) => v,
                None => String::new(),
            };
            cells.push(cell_text);
            j = c_end + 4;
        }
        out.push_str(&cells.join("\t"));
        out.push('\n');
        i = row_end + 6;
    }
    out
}

/// DOCX / RTF / ODT 抽文本：用 Apple 自带的 `textutil -convert txt -stdout`。
async fn extract_docx_text(path: &Path) -> Result<String> {
    let out = Command::new("textutil")
        .args(["-convert", "txt", "-stdout"])
        .arg(path)
        .output()
        .await
        .context("spawn textutil")?;
    if !out.status.success() {
        return Err(anyhow!(
            "textutil failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    if s.trim().is_empty() {
        return Err(anyhow!("textutil 抽不到文本"));
    }
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn classify_known_extensions() {
        assert_eq!(classify(&PathBuf::from("a.png")), FeedKind::Image);
        assert_eq!(classify(&PathBuf::from("a.jpg")), FeedKind::Image);
        assert_eq!(classify(&PathBuf::from("a.HEIC")), FeedKind::Image);
        assert_eq!(classify(&PathBuf::from("a.md")), FeedKind::Text);
        assert_eq!(classify(&PathBuf::from("a.rs")), FeedKind::Text);
        assert_eq!(classify(&PathBuf::from("a.json")), FeedKind::Text);
        assert_eq!(classify(&PathBuf::from("a.pdf")), FeedKind::Pdf);
        assert_eq!(classify(&PathBuf::from("a.docx")), FeedKind::Docx);
        assert_eq!(classify(&PathBuf::from("a.xlsx")), FeedKind::Xlsx);
        assert_eq!(classify(&PathBuf::from("installer.app")), FeedKind::Reject);
        assert_eq!(classify(&PathBuf::from("a.zip")), FeedKind::Reject);
        assert_eq!(classify(&PathBuf::from("no_ext")), FeedKind::Reject);
    }

    #[test]
    fn xlsx_xml_t_extract() {
        let xml = "<sst><si><t>name</t></si><si><t>price</t></si><si><t xml:space=\"preserve\">apple &amp; pear</t></si></sst>";
        let v = extract_xml_t_values(xml);
        assert_eq!(v, vec!["name".to_string(), "price".into(), "apple & pear".into()]);
    }

    #[test]
    fn xlsx_sheet_resolves_shared_strings() {
        let strings = vec!["hello".to_string(), "world".into()];
        let sheet = "<sheetData>\
            <row><c t=\"s\"><v>0</v></c><c t=\"s\"><v>1</v></c></row>\
            <row><c><v>42</v></c><c><v>3.14</v></c></row>\
        </sheetData>";
        let csv = xlsx_sheet_to_csv(sheet, &strings);
        assert!(csv.contains("hello\tworld"));
        assert!(csv.contains("42\t3.14"));
    }

    #[test]
    fn preamble_inlines_short_text() {
        let mut bundle = FeedBundle::default();
        bundle.files.push(FedFile {
            path: PathBuf::from("/tmp/notes.md"),
            name: "notes.md".into(),
            kind: FeedKind::Text,
            size_bytes: 11,
            text: Some("hello world".into()),
            reject_reason: None,
        });
        let p = bundle.prompt_preamble().unwrap();
        assert!(p.contains("notes.md"));
        assert!(p.contains("hello world"));
        assert!(p.contains("DOC_1"));
    }

    #[test]
    fn preamble_emits_path_for_huge_text() {
        // 超过 INLINE_TEXT_CHAR_LIMIT 的文本：truncate_for_inline 把它砍到 limit 但末尾加
        // 提示字，最终 char_count 略大于 limit → preamble 走"太长，请 Read"分支。
        let huge: String = "a".repeat(INLINE_TEXT_CHAR_LIMIT + 100);
        let mut bundle = FeedBundle::default();
        bundle.files.push(FedFile {
            path: PathBuf::from("/tmp/big.txt"),
            name: "big.txt".into(),
            kind: FeedKind::Text,
            size_bytes: huge.len() as u64,
            text: Some(truncate_for_inline(huge)),
            reject_reason: None,
        });
        let p = bundle.prompt_preamble().unwrap();
        assert!(p.contains("/tmp/big.txt"));
        assert!(p.contains("太长"));
    }

    #[test]
    fn truncate_keeps_short_text() {
        let s = "short".to_string();
        assert_eq!(truncate_for_inline(s.clone()), s);
    }

    #[test]
    fn truncate_chops_long_text() {
        let s = "x".repeat(INLINE_TEXT_CHAR_LIMIT + 50);
        let out = truncate_for_inline(s);
        assert!(out.contains("截断 50"));
    }
}
