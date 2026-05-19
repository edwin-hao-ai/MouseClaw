# Reactive Ribbon · Chrome MCP E2E 剧本

真实浏览器（Chrome + chrome-devtools MCP）跑 vite dev 验证 reactive 全流程。
跟 vitest（组件层）和 acceptance grep 互补，捕真实 DOM 渲染 / 事件分发 / Tauri mock 协议 bug。

## 前置

1. Chrome 用 remote debugging 启动：
   ```bash
   open -na "Google Chrome" --args --remote-debugging-port=9222 \
     --user-data-dir=/tmp/mouseclaw-chrome-e2e
   ```
2. vite dev：
   ```bash
   bun run dev   # → http://localhost:1420
   ```
3. Chrome MCP 已连上（验证：`curl -s http://127.0.0.1:9222/json/version`）

## 步骤

### A. 装 mock + 重载

```js
// chrome_devtools.evaluate_script
() => {
  localStorage.setItem("mouseclaw.e2e.tauriMock", "1");
  return localStorage.getItem("mouseclaw.e2e.tauriMock");
}
```
然后 navigate_page type=reload。

### B. 验证 mock 注册成功

```js
() => ({
  mockInstalled: !!window.__TAURI_INTERNALS__,
  hasTransformCallback: typeof window.__TAURI_INTERNALS__.transformCallback === "function",
  hasStageMouse: !!document.querySelector(".stage-mouse"),
  reactiveListeners: window.__mcEventListeners("clipboard-reactive").length,
});
// 预期：mockInstalled=true, hasTransformCallback=true, hasStageMouse=true, reactiveListeners>=1
```

### C. 触发剪贴板 hint → ribbon 出现 + 桌宠抖耳

```js
() => {
  window.__mcEmit("clipboard-reactive", {
    source: "clipboard", tier: "hint", icon: "format",
    preview: "Hello  world  messy text", charLen: 22,
  });
  return new Promise(r => setTimeout(() => r({
    ribbon: !!document.querySelector('[data-testid="rx-ribbon"]'),
    source: document.querySelector('[data-testid="rx-ribbon"]')?.getAttribute("data-source"),
    twitching: document.querySelector(".mouse-wrap")?.classList.contains("mouse-twitch"),
  }), 200));
}
// 预期：ribbon=true, source="clipboard", twitching=true
```

### D. 点 clean → busy → done

```js
() => {
  document.querySelector('[data-testid="rx-action-clean"]').click();
  return new Promise(r => setTimeout(() => r({
    done: !!document.querySelector('[data-testid="rx-status-done"]'),
    doneText: document.querySelector('[data-testid="rx-status-done"]')?.textContent,
    invoked: window.__mcLastInvoke,
  }), 200));
}
// 预期：done=true, doneText="✓ 已写回 · ⌘V 粘贴",
//      invoked.cmd="process_reactive_action", invoked.args={action:"clean"}
```

### E. 选词 source + Esc 关闭

```js
() => new Promise(resolve => {
  setTimeout(() => {
    window.__mcEmit("clipboard-reactive", {
      source: "selection", tier: "hint", icon: "translate",
      preview: "外文一段", charLen: 20,
    });
    setTimeout(() => {
      const before = !!document.querySelector('[data-testid="rx-ribbon"]');
      window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
      setTimeout(() => {
        const after = !!document.querySelector('[data-testid="rx-ribbon"]');
        resolve({ before, after });
      }, 100);
    }, 80);
  }, 3000);
});
// 预期：before=true, after=false
```

## 已验证（2026-05-19）

- ✅ A. Mock 装上：transformCallback / __TAURI_INTERNALS__ 全套生效
- ✅ B. listen("clipboard-reactive") 注册到 mock 的 EVENT_LISTENERS（id 经 transformCallback 分配）
- ✅ C. __mcEmit 触发 → ribbon 出现 + 桌宠 .mouse-twitch class 加上 → 4 个按钮齐全
- ✅ D. 点 clean → invoke("process_reactive_action", {action:"clean"}) → done 状态 + 中文 done 文案
- ✅ E. selection source 切换正确 → Esc 立即关闭

## 截图

- `screenshots/01-ribbon-clipboard.png` —— ribbon 在 clipboard 源弹出
- `screenshots/02-action-done.png` —— 点 clean 后 done 状态
