/**
 * 共享的极简 markdown → HTML 渲染（marked）。
 *
 * 主回复气泡 (Bubble) 和定时任务结果面板 (TasksView) 共用，避免各写一份。
 * 安全：marked 默认逃逸 HTML，不 XSS。流式半截 markdown parse 失败时兜底原文。
 */
import { marked } from "marked";

marked.setOptions({
  gfm: true, // 表格、删除线、autolink
  breaks: true, // 单换行 = <br>（更贴近聊天直觉）
});

export function renderMarkdown(text: string): string {
  try {
    return marked.parse(text, { async: false }) as string;
  } catch {
    return text; // 流式中间态可能 parse 失败 —— 兜底原文
  }
}
