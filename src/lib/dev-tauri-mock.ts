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
  get_skin: () => "classic",
  get_language: () => "zh",
  read_history: () => [],
  get_model_status: () => [],
  check_permissions: () => ({ accessibility: true, screen_recording: true, microphone: true }),
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
