/**
 * localStorage 门控的 Tauri mock —— 给 vite dev + Chrome MCP E2E 用。
 *
 * 触发：`localStorage.setItem("mouseclaw.e2e.tauriMock", "1") + location.reload()`
 *
 * 装上之后 `window.__TAURI_INTERNALS__` 和 `window.__TAURI__` 提供 invoke/listen 的桩，
 * 让 React 组件能在普通 Chromium / 真实浏览器里跑起来 —— 不需要 Tauri webview。
 *
 * 设计：
 *   - invoke(cmd, args)：按 cmd 名分发 deterministic 响应（reactive、process_reactive_action 等）
 *   - listen(event, cb)：注册 cb 到 EVENT_HANDLERS。test 通过 `window.__mcEmit(event, payload)` 触发
 *   - 不实现的 cmd → console.warn + resolve null（不让 await invoke 卡死）
 *
 * 参考 CLAUDE.md "E2E for Tauri / Electron / native-shell apps" 模式。
 */

type AnyFn = (...args: any[]) => any;

const FIXTURES: Record<string, (args?: any) => unknown> = {
  // reactive action — 永远返回一个 fake 处理结果
  process_reactive_action: (args: { action: string }) => {
    if (!args || !args.action) {
      throw new Error("missing action");
    }
    const samples: Record<string, string> = {
      clean: "Cleaned text via mock",
      translate: "翻译结果（mock）",
      explain: "这是一段被 mock 解释过的内容。",
      reply: "Thanks for reaching out. We'll get back to you shortly. (mock)",
    };
    return samples[args.action] ?? `mock result for ${args.action}`;
  },
  set_overlay_has_ui: () => null,
  set_overlay_content_size: () => null,
  get_skin: () => "classic",
  get_language: () => "zh",
  read_history: () => [],
  get_model_status: () => [],
  check_permissions: () => ({ accessibility: true, screen_recording: true, microphone: true }),
  // v0.4.x · session 状态（App 启动读，少了会 .continuing 崩）
  get_session_state: () => ({ continuing: false, round: 0, pinned: false, softHint: false }),
  // v0.5 · 桌宠音效配置
  get_sfx_config: () => ({ enabled: true, volume: 0.45 }),
  report_reduced_motion: () => null,
  // v0.5 · 定时任务 —— 给两条样例任务，方便 TasksView 渲染列表
  list_schedules: () => [
    {
      id: "demo-1", title: "每天早报", action: "总结今天的科技新闻",
      schedule: { kind: "daily", time: "09:00" }, enabled: true,
      createdAt: "2026-05-20T09:00:00Z",
      lastRun: { at: "2026-05-22T09:00:00Z", status: "ok", summary: "已发送早报" },
      nextRun: "2026-05-23T09:00:00+08:00",
    },
    {
      id: "demo-2", title: "每 30 分钟检查邮件", action: "看看有没有新的重要邮件",
      schedule: { kind: "interval", everyMinutes: 30, activeStart: "09:00", activeEnd: "18:00" },
      enabled: false, createdAt: "2026-05-21T10:00:00Z",
      nextRun: undefined,
    },
  ],
  get_schedule_runs: () => [
    { taskId: "demo-1", title: "每天早报", at: "2026-05-23T01:34:00Z", status: "ok", summary: "已整理 6 条 · 搜索智能体、企业AI加速",
      output: "## 📰 过去24小时 AI 要闻（5/22–5/23）\n\n### 模型 & 产品发布\n1. **Google I/O 2026 推出 Search「信息智能体」** — 可 24/7 后台运行。\n   来源：[Google I/O 2026](https://example.com)\n2. **Anthropic 发布 Claude Opus 4.7** — 1M 上下文。\n\n### 融资\n- 某 AI 编码公司完成 B 轮。" },
    { taskId: "demo-1", title: "每天早报", at: "2026-05-23T00:47:00Z", status: "ok", summary: "AI 行业 24h 速览",
      output: "## 📰 AI 行业 24h 速览\n\n今日 **4 条**重点：\n- 开源模型 GLM-5 登顶 coding 榜\n- 多家厂商跟进「长时记忆」特性" },
    { taskId: "demo-1", title: "每天早报", at: "2026-05-23T00:46:00Z", status: "failed", summary: "失败：没取到外部数据",
      output: "## ✗ 本次失败\n\n没能获取到实时新闻数据（WebSearch 超时）。**未编造内容** —— 按真实性铁律如实报告。" },
  ],
  parse_schedule_phrase: (args: { phrase?: string }) => ({
    title: args?.phrase ? args.phrase.slice(0, 20) : "新任务",
    action: args?.phrase ?? "做点什么",
    schedule: { kind: "daily", time: "09:00" }, enabled: true,
  }),
  create_schedule: () => null,
  update_schedule: () => null,
  delete_schedule: () => true,
  toggle_schedule: () => null,
  run_schedule_now: () => null,
  // v0.4.4 · 长期记忆查看器
  // v0.5.2 · 统一结果 feed（跨任务倒序）
  get_all_schedule_runs: () => [
    { taskId: "demo-1", title: "整理AI新闻", at: "2026-05-23T01:34:00Z", status: "ok", summary: "已整理 6 条 · 搜索智能体、企业AI加速",
      output: "## 📰 过去24小时 AI 要闻\n\n### 模型 & 产品发布\n1. **Google I/O 2026 推出 Search「信息智能体」**\n   来源：[Google I/O 2026](https://example.com)\n2. **Anthropic 发布 Claude Opus 4.7** — 1M 上下文" },
    { taskId: "demo-3", title: "检查重要邮件", at: "2026-05-23T01:30:00Z", status: "ok", summary: "2 封待回：合同确认、面试邀约",
      output: "## 📧 待回邮件 2 封\n- **合同确认**（法务）— 今天截止\n- **面试邀约**（HR）— 需选时间" },
    { taskId: "demo-4", title: "每日待办梳理", at: "2026-05-23T01:28:00Z", status: "failed", summary: "失败：日历未授权",
      output: "## ✗ 本次失败\n读不到日历（未授权）。**未编造** —— 如实报告，下次重试。" },
    { taskId: "demo-1", title: "整理AI新闻", at: "2026-05-22T09:00:00Z", status: "ok", summary: "AI 行业 24h 速览（5/21–22）",
      output: "## 📰 AI 行业速览\n昨日 **4 条**重点：GLM-5 登顶 coding 榜…" },
  ],
  memory_get_settings: () => ({ enabled: true, paused: false }),
  memory_list_turns: () => [
    { id: 1, ts: Math.floor(Date.now() / 1000) - 3600, app: "VSCode", role: "user", summary: "问 entrance.rs 的 700ms 竞态怎么修", importance: 4 },
    { id: 2, ts: Math.floor(Date.now() / 1000) - 7200, app: "Chrome", role: "assistant", summary: "解释了 MouseClaw 的多后端抽象", importance: 3 },
  ],
  memory_get_profile: () => ({
    insights: [
      { id: 1, kind: "profile", text: "用户是 MouseClaw 的作者，重视 UX 与代码整洁", confidence: 0.9 },
      { id: 2, kind: "pattern", text: "偏好先做 HTML prototype 再写实现", confidence: 0.8 },
    ],
    preferences: [
      { id: 1, key: "language", value: "中文", confidence: 0.95 },
      { id: 2, key: "answer_length", value: "精简可执行", confidence: 0.7 },
    ],
  }),
  memory_get_graph: () => ({
    nodes: [
      { id: 1, kind: "project", name: "MouseClaw", freq: 42 },
      { id: 2, kind: "tool", name: "Tauri", freq: 18 },
      { id: 3, kind: "topic", name: "桌宠动画", freq: 9 },
    ],
    edges: [{ src: 1, dst: 2, kind: "about" }, { src: 1, dst: 3, kind: "co_occurs" }],
  }),
  // v0.4.7 (PR #5) · 「我们的故事」关系页
  memory_get_story: () => ({
    has_data: true,
    days_known: 12,
    night_sessions: 5,
    total_turns: 47,
    profile_line: "用户是 MouseClaw 的作者，重视 UX 与代码整洁",
  }),
  prefetch_models: () => null,
  memory_delete_turn: () => null,
  memory_delete_profile_item: () => null,
  memory_clear_all: () => null,
  memory_set_paused: () => null,
  memory_set_enabled: () => null,
  // 其他命令静默 resolve null（test 不关心的）
};

// Tauri v2 真实 listen 协议：transformCallback(handler) → id → invoke("plugin:event|listen", {event, handler: id})
// Mock 要同时实现这两条路径才能截到 listen 注册。
const CALLBACKS = new Map<number, AnyFn>();
let nextCallbackId = 1;
const EVENT_LISTENERS = new Map<string, Set<number>>(); // event → set of callback ids
const EVENT_HANDLERS = new Map<string, Set<AnyFn>>(); // 旧路径（直接 window.__TAURI__.event.listen）

export function shouldInstallDevTauriMock(): boolean {
  try {
    return localStorage.getItem("mouseclaw.e2e.tauriMock") === "1";
  } catch {
    return false;
  }
}

export function installDevTauriMock(): void {
  const w = window as unknown as Record<string, unknown>;

  // 提供一个 helper 给 Chrome MCP 用，绕开内部 listen 机制直接 dispatch。
  // 用法: window.__mcEmit("clipboard-reactive", { source: "clipboard", tier: "hint", ... })
  (w as any).__mcEmit = (event: string, payload: unknown) => {
    const envelope = { event, id: Date.now(), payload, windowLabel: "mouse" };
    // 路径 1：Tauri v2 协议注册的 listener
    const ids = EVENT_LISTENERS.get(event);
    if (ids) {
      ids.forEach((id) => {
        const fn = CALLBACKS.get(id);
        if (fn) {
          try { fn(envelope); } catch (e) { console.error(`[mock] handler ${id} throw`, e); }
        }
      });
    }
    // 路径 2：旧路径 window.__TAURI__.event.listen 直接注册的
    const handlers = EVENT_HANDLERS.get(event);
    handlers?.forEach((h) => {
      try { h(envelope); } catch (e) { console.error("[mock listen handler]", e); }
    });
  };

  // 镜像一份 LAST invoke 调用 + listener 总数，给 Chrome MCP 自检
  (w as any).__mcLastInvoke = null;
  (w as any).__mcEventListeners = (ev: string) =>
    Array.from(EVENT_LISTENERS.get(ev) ?? []).concat(
      Array.from(EVENT_HANDLERS.get(ev) ?? []).map(() => -1)
    );

  // Tauri v2 内部接口 —— @tauri-apps/api 的 invoke / listen 走的就是这里
  (w as any).__TAURI_INTERNALS__ = {
    // getCurrentWindow() 读 metadata.currentWindow.label —— 缺了 PickerView 等
    // 用 window API 的视图会在挂载时崩（读 undefined.currentWindow）。
    metadata: { currentWindow: { label: "e2e" }, currentWebview: { windowLabel: "e2e", label: "e2e" } },
    // 关键：listen() 内部调它分配 handler id，然后把 id 作为 invoke 参数发给 Rust
    transformCallback: (handler: AnyFn, _once?: boolean) => {
      const id = nextCallbackId++;
      CALLBACKS.set(id, handler);
      return id;
    },
    runCallback: (id: number, payload: unknown) => {
      const fn = CALLBACKS.get(id);
      if (fn) fn(payload);
    },
    invoke: (cmd: string, args: Record<string, unknown> | undefined) => {
      (w as any).__mcLastInvoke = { cmd, args };

      // 拦截事件协议
      if (cmd === "plugin:event|listen") {
        const event = args?.event as string;
        const handlerId = args?.handler as number;
        if (typeof event === "string" && typeof handlerId === "number") {
          if (!EVENT_LISTENERS.has(event)) EVENT_LISTENERS.set(event, new Set());
          EVENT_LISTENERS.get(event)!.add(handlerId);
        }
        return Promise.resolve(handlerId);
      }
      if (cmd === "plugin:event|unlisten") {
        const event = args?.event as string;
        const handlerId = args?.eventId as number;
        EVENT_LISTENERS.get(event)?.delete(handlerId);
        CALLBACKS.delete(handlerId);
        return Promise.resolve();
      }
      if (cmd === "plugin:event|emit" || cmd === "plugin:event|emit_to") {
        return Promise.resolve(); // 应该不会被前端调，安全 noop
      }

      // 业务 fixture
      const handler = FIXTURES[cmd];
      try {
        const result = handler ? handler(args) : null;
        if (!handler) {
          console.debug(`[tauri-mock] unmapped cmd: ${cmd} → null`);
        }
        return Promise.resolve(result);
      } catch (e) {
        return Promise.reject(String(e));
      }
    },
  };

  // 用户态的 invoke 是一个简化封装
  (w as any).__TAURI__ = {
    core: {
      invoke: (cmd: string, args?: Record<string, unknown>) => {
        return (w as any).__TAURI_INTERNALS__.invoke(cmd, args);
      },
    },
    event: {
      listen: (event: string, cb: AnyFn) => {
        if (!EVENT_HANDLERS.has(event)) {
          EVENT_HANDLERS.set(event, new Set());
        }
        EVENT_HANDLERS.get(event)!.add(cb);
        return Promise.resolve(() => {
          EVENT_HANDLERS.get(event)?.delete(cb);
        });
      },
      emit: (event: string, payload?: unknown) => {
        (w as any).__mcEmit(event, payload);
        return Promise.resolve();
      },
    },
  };

  // @tauri-apps/api/core 的 invoke 实际是导入 @tauri-apps/api 转发到
  // __TAURI_INTERNALS__.invoke —— 装好上面就够了。
  // @tauri-apps/api/event 的 listen 同理走 __TAURI__.event.listen。

  console.info("[mouseclaw] 🧪 dev tauri mock installed (E2E mode)");
}
