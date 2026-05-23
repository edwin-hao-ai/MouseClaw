/**
 * TasksView · v0.5 —— 定时任务管理窗（?view=tasks）。
 *
 * 入口：托盘「⏰ 定时任务…」/ 桌宠菜单 / 结果气泡"展开"。
 * 设计源：docs/prototypes/scheduled-tasks-20260521.html §②
 * 所有视觉 token 来自 DESIGN.md / tokens.css，无内联字面色值。
 *
 * 能力：列表 + 开关 + 下次/上次 + 展开看结果历史 + 编辑(结构化) + 删除 + 立刻跑，
 *      底部「一句话新建」走 backend 解析。
 */
import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useT } from "./i18n";
import { EV_SCHEDULE_RESULT } from "./types";
import type { Schedule, ScheduleInput, ScheduleView, RunRecord } from "./types";
import { scheduleLabel, scheduleIcon, formatWhen, formatRunTime } from "./lib/schedule-format";
import { renderMarkdown } from "./lib/markdown";
import "./TasksView.css";

export default function TasksView() {
  const t = useT();
  const [items, setItems] = useState<ScheduleView[]>([]);
  const [loading, setLoading] = useState(true);
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const [runs, setRuns] = useState<Record<string, RunRecord[]>>({});
  // v0.5.1 · 结果面板里当前选中的历史条目下标（点左栏切换看不同次执行的完整输出）
  const [selectedRun, setSelectedRun] = useState(0);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [toast, setToast] = useState<string | null>(null);
  // v0.5.1 · 手动"立刻跑"进行中的任务 id —— 卡片显示转圈 + "执行中…"，
  // 收到 schedule-result（成功/失败都 emit）即清掉。防"点了没反应"。
  const [runningId, setRunningId] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const list = await invoke<ScheduleView[]>("list_schedules");
      setItems(list);
    } catch {
      /* browser-only dev */
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  // 任务跑完（定时到点 / 手动「立刻跑」）→ 实时刷新列表的"上次"；
  // 切回窗口（focus）也刷一次，覆盖"在别处用语音新建了任务"的情况。
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    try {
      listen<{ taskId?: string }>(EV_SCHEDULE_RESULT, async (e) => {
        // 清掉"执行中"态（手动立刻跑跑完了）。事件带 taskId，匹配则清；
        // 反正一次只跑一个（ai_queue 串行），保险起见也直接清空。
        const tid = e?.payload?.taskId;
        setRunningId((cur) => (cur && (!tid || cur === tid) ? null : cur));
        await refresh();
        // v0.5.1 · 完成即可见 —— 真实任务跑完(taskId 非空)自动展开它的结果 + 选中最新一条，
        // 不用用户再去找按钮。空 taskId 是"发现提示"，不展开。
        if (tid) {
          try {
            const r = await invoke<RunRecord[]>("get_schedule_runs", { taskId: tid });
            setRuns({ [tid]: r });
          } catch {
            setRuns({});
          }
          setExpandedId(tid);
          setSelectedRun(0);
        } else {
          setRuns({});
        }
      })
        .then((fn) => { unlisten = fn; })
        .catch(() => {});
    } catch {
      /* browser-only dev */
    }
    const onFocus = () => refresh();
    window.addEventListener("focus", onFocus);
    return () => {
      if (unlisten) unlisten();
      window.removeEventListener("focus", onFocus);
    };
  }, [refresh]);

  const flash = useCallback((msg: string) => {
    setToast(msg);
    window.setTimeout(() => setToast(null), 2600);
  }, []);

  // v0.5.1 · 聚焦某个任务（拉它的历史 + 展开 + 选最新）—— 从完成气泡点开时直达结果，
  // 不用在列表里找。两条触发：①新开窗口读 URL ?focus= ②已开窗口收 tasks-focus 事件。
  const focusTask = useCallback(async (id: string) => {
    try {
      const r = await invoke<RunRecord[]>("get_schedule_runs", { taskId: id });
      setRuns((m) => ({ ...m, [id]: r }));
    } catch {
      /* dev */
    }
    setExpandedId(id);
    setSelectedRun(0);
  }, []);

  useEffect(() => {
    const f = new URLSearchParams(window.location.search).get("focus");
    if (f) focusTask(f);
  }, [focusTask]);

  useEffect(() => {
    let unlisten: (() => void) | null = null;
    try {
      listen<string>("tasks-focus", (e) => { if (e.payload) focusTask(e.payload); })
        .then((fn) => { unlisten = fn; })
        .catch(() => {});
    } catch {
      /* dev */
    }
    return () => { if (unlisten) unlisten(); };
  }, [focusTask]);

  const toggle = useCallback(
    async (id: string, enabled: boolean) => {
      await invoke("toggle_schedule", { id, enabled }).catch(() => {});
      refresh();
    },
    [refresh],
  );

  const del = useCallback(
    async (task: ScheduleView) => {
      if (!window.confirm(t("tasks.delete_confirm", { title: task.title }))) return;
      await invoke("delete_schedule", { id: task.id }).catch(() => {});
      if (expandedId === task.id) setExpandedId(null);
      refresh();
    },
    [refresh, t, expandedId],
  );

  const runNow = useCallback(
    async (task: ScheduleView) => {
      setRunningId(task.id);
      await invoke("run_schedule_now", { id: task.id }).catch(() => {});
      // 兜底：万一没收到 schedule-result（极端情况），90s 后强制清掉转圈，不让它永远卡住。
      window.setTimeout(() => setRunningId((cur) => (cur === task.id ? null : cur)), 90_000);
    },
    [],
  );

  const toggleExpand = useCallback(
    async (id: string) => {
      if (expandedId === id) {
        setExpandedId(null);
        return;
      }
      setExpandedId(id);
      setSelectedRun(0); // 展开时默认看最新一条
      if (!runs[id]) {
        try {
          const r = await invoke<RunRecord[]>("get_schedule_runs", { taskId: id });
          setRuns((m) => ({ ...m, [id]: r }));
        } catch {
          /* dev */
        }
      }
    },
    [expandedId, runs],
  );

  const saveEdit = useCallback(
    async (id: string, input: ScheduleInput) => {
      await invoke("update_schedule", { id, input }).catch(() => {});
      setEditingId(null);
      setRuns({});
      refresh();
    },
    [refresh],
  );

  const createFromInput = useCallback(
    async (input: ScheduleInput) => {
      await invoke("create_schedule", { input }).catch(() => {});
      flash(t("schedule.confirm.created"));
      refresh();
    },
    [refresh, flash, t],
  );

  const onCount = items.filter((i) => i.enabled).length;

  return (
    <div className="tasks-root">
      <header className="tasks-hd">
        <span className="tasks-h">⏰ {t("tasks.title")}</span>
        {items.length > 0 && (
          <span className="tasks-c">{t("tasks.count", { n: items.length, on: onCount })}</span>
        )}
      </header>

      {toast && <div className="tasks-toast">{toast}</div>}

      {loading ? null : items.length === 0 ? (
        <EmptyState />
      ) : (
        <div className="tasks-list">
          {items.map((task) =>
            editingId === task.id ? (
              <EditForm
                key={task.id}
                task={task}
                onSave={(input) => saveEdit(task.id, input)}
                onCancel={() => setEditingId(null)}
              />
            ) : (
              <TaskCard
                key={task.id}
                task={task}
                expanded={expandedId === task.id}
                running={runningId === task.id}
                runs={runs[task.id]}
                selectedRun={selectedRun}
                onSelectRun={setSelectedRun}
                onToggle={(en) => toggle(task.id, en)}
                onExpand={() => toggleExpand(task.id)}
                onEdit={() => setEditingId(task.id)}
                onDelete={() => del(task)}
                onRunNow={() => runNow(task)}
              />
            ),
          )}
        </div>
      )}

      <NewRow onCreate={createFromInput} />
    </div>
  );
}

/** markdown 结果里点外链 → 走系统浏览器（webview 里直接跳会顶掉任务 UI）。 */
function handleMdLinkClick(e: React.MouseEvent) {
  const a = (e.target as HTMLElement).closest("a");
  const href = a?.getAttribute("href");
  if (href && /^https?:\/\//i.test(href)) {
    e.preventDefault();
    openUrl(href).catch(() => {});
  }
}

function EmptyState() {
  const t = useT();
  return (
    <div className="tasks-empty">
      <div className="tasks-empty-big">⏰</div>
      <div className="tasks-empty-title">{t("tasks.empty.title")}</div>
      <div className="tasks-empty-hint">{t("tasks.empty.hint")}</div>
      <div className="tasks-empty-ex">
        <code>{t("tasks.empty.ex1")}</code>
        <code>{t("tasks.empty.ex2")}</code>
      </div>
    </div>
  );
}

interface CardProps {
  task: ScheduleView;
  expanded: boolean;
  running: boolean;
  runs?: RunRecord[];
  selectedRun: number;
  onSelectRun: (i: number) => void;
  onToggle: (enabled: boolean) => void;
  onExpand: () => void;
  onEdit: () => void;
  onDelete: () => void;
  onRunNow: () => void;
}
function TaskCard({
  task, expanded, running, runs, selectedRun, onSelectRun,
  onToggle, onExpand, onEdit, onDelete, onRunNow,
}: CardProps) {
  const t = useT();
  const last = task.lastRun;
  const lastCls = last ? (last.status === "ok" ? "ok" : last.status === "failed" ? "fail" : "skip") : "";
  const lastText = last
    ? `${last.status === "ok" ? "✓" : last.status === "failed" ? "✗" : "—"} ${formatRunTime(last.at)} · ${last.summary}`
    : t("tasks.last.never");

  return (
    <div className={`task ${task.enabled ? "" : "task-off"} ${expanded ? "task-open" : ""}`}>
      <div className="task-row1">
        <span className="task-name">{task.title}</span>
        <button
          className={`task-tog ${task.enabled ? "on" : ""}`}
          role="switch"
          aria-checked={task.enabled}
          onClick={() => onToggle(!task.enabled)}
        />
      </div>
      <div className="task-row2">
        <span className="task-chip">
          {scheduleIcon(task.schedule)} {scheduleLabel(t, task.schedule)}
        </span>
      </div>
      <div className="task-action">{task.action}</div>
      <div className="task-meta">
        {running ? (
          <span className="task-running"><span className="tasks-spin" />{t("tasks.running")}</span>
        ) : (
          <>
            {task.enabled && task.nextRun && (
              <span className="task-next">{t("tasks.next", { when: formatWhen(t, task.nextRun) })}</span>
            )}
            {!task.enabled && <span className="task-next">{t("tasks.paused")}</span>}
            <span className={`task-last ${lastCls}`}>{lastText}</span>
          </>
        )}
      </div>

      {/* v0.5.1 · 带文字的操作按钮 —— 不再 ▶·✎🗑▾ 猜谜 */}
      <div className="task-actions">
        <button className="task-btn primary" onClick={onRunNow} disabled={running}>
          {running ? <><span className="tasks-spin" />{t("tasks.running")}</> : <>▶ {t("tasks.run_now")}</>}
        </button>
        <button className={`task-btn ${expanded ? "on" : ""}`} onClick={onExpand}>
          📄 {t("tasks.history.title")}
        </button>
        <button className="task-btn" onClick={onEdit}>✏️ {t("tasks.edit")}</button>
        <button className="task-btn icon" title={t("tasks.delete")} onClick={onDelete}>🗑</button>
      </div>

      {/* v0.5.1 · 结果/历史面板：左栏可切换的执行记录 + 右栏 markdown 渲染输出 */}
      {expanded && (
        <div className="task-result">
          {!runs || runs.length === 0 ? (
            <div className="task-hist-empty">{t("tasks.history.empty")}</div>
          ) : (
            <>
              <div className="task-runs">
                {runs.map((r, i) => (
                  <button
                    key={i}
                    className={`task-run ${i === selectedRun ? "sel" : ""}`}
                    onClick={() => onSelectRun(i)}
                  >
                    <span className="task-run-top">
                      <span className={`task-run-dot ${r.status}`} />
                      {formatRunTime(r.at)}
                    </span>
                    <span className="task-run-sum">{r.summary}</span>
                  </button>
                ))}
              </div>
              <div className="task-out">
                {(() => {
                  const r = runs[selectedRun] ?? runs[0];
                  const body = r?.output?.trim() || r?.summary || "";
                  return body ? (
                    <div
                      className="task-out-md"
                      onClick={handleMdLinkClick}
                      dangerouslySetInnerHTML={{ __html: renderMarkdown(body) }}
                    />
                  ) : (
                    <div className="task-hist-empty">{t("tasks.history.empty")}</div>
                  );
                })()}
              </div>
            </>
          )}
        </div>
      )}
    </div>
  );
}

function NewRow({ onCreate }: { onCreate: (input: ScheduleInput) => void }) {
  const t = useT();
  const [text, setText] = useState("");
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState(false);

  const submit = async () => {
    const phrase = text.trim();
    if (!phrase || busy) return;
    setBusy(true);
    setErr(false);
    try {
      const input = await invoke<ScheduleInput>("parse_schedule_phrase", { phrase });
      onCreate(input);
      setText("");
    } catch {
      setErr(true);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="tasks-new">
      <input
        className="tasks-new-ipt"
        value={text}
        disabled={busy}
        placeholder={err ? t("tasks.new.failed") : t("tasks.new.placeholder")}
        onChange={(e) => {
          setText(e.target.value);
          setErr(false);
        }}
        onKeyDown={(e) => {
          if (e.key === "Enter") submit();
        }}
      />
      <button className="tasks-new-add" disabled={busy} onClick={submit} title={t("common.confirm")}>
        {busy ? <span className="tasks-spin" /> : "＋"}
      </button>
    </div>
  );
}

// ── 结构化编辑表单 ──────────────────────────────────────────────
type Kind = Schedule["kind"];

function EditForm({
  task,
  onSave,
  onCancel,
}: {
  task: ScheduleView;
  onSave: (input: ScheduleInput) => void;
  onCancel: () => void;
}) {
  const t = useT();
  const s = task.schedule;
  const [title, setTitle] = useState(task.title);
  const [action, setAction] = useState(task.action);
  const [kind, setKind] = useState<Kind>(s.kind);
  const [time, setTime] = useState("time" in s ? s.time : "08:00");
  const [days, setDays] = useState<Set<number>>(
    new Set(s.kind === "weekly" ? s.days : [1, 2, 3, 4, 5]),
  );
  const [hours, setHours] = useState(
    s.kind === "interval" ? Math.max(1, Math.round(s.everyMinutes / 60)) : 2,
  );
  const [activeStart, setActiveStart] = useState(s.kind === "interval" ? s.activeStart ?? "09:00" : "09:00");
  const [activeEnd, setActiveEnd] = useState(s.kind === "interval" ? s.activeEnd ?? "22:00" : "22:00");
  const [dom, setDom] = useState(s.kind === "monthly" ? s.day : 1);

  const build = (): Schedule => {
    switch (kind) {
      case "daily":
        return { kind: "daily", time };
      case "weekday":
        return { kind: "weekday", time };
      case "weekly":
        return { kind: "weekly", days: [...days].sort((a, b) => a - b), time };
      case "interval":
        return {
          kind: "interval",
          everyMinutes: Math.min(24, Math.max(1, Math.round(hours) || 1)) * 60,
          activeStart,
          activeEnd,
        };
      case "monthly":
        return { kind: "monthly", day: Math.min(31, Math.max(1, Math.round(dom) || 1)), time };
    }
  };

  const save = () => {
    if (!title.trim() || !action.trim()) return;
    onSave({ title: title.trim(), action: action.trim(), schedule: build(), enabled: task.enabled });
  };

  const KINDS: Kind[] = ["daily", "weekday", "weekly", "interval", "monthly"];

  return (
    <div className="task task-edit">
      <label className="ed-lbl">{t("tasks.edit.title_label")}</label>
      <input className="ed-ipt" value={title} onChange={(e) => setTitle(e.target.value)} />

      <label className="ed-lbl">{t("tasks.edit.action_label")}</label>
      <textarea className="ed-ta" value={action} rows={2} onChange={(e) => setAction(e.target.value)} />

      <label className="ed-lbl">{t("tasks.edit.freq_label")}</label>
      <div className="ed-seg">
        {KINDS.map((k) => (
          <button
            key={k}
            className={kind === k ? "active" : ""}
            onClick={() => setKind(k)}
          >
            {t(`schedule.opt.${k}` as never)}
          </button>
        ))}
      </div>

      {(kind === "daily" || kind === "weekday" || kind === "weekly" || kind === "monthly") && (
        <div className="ed-grid">
          {kind === "monthly" && (
            <label className="ed-field">
              {t("tasks.edit.day_label")}
              <input
                type="number"
                min={1}
                max={31}
                value={dom}
                onChange={(e) => setDom(Number(e.target.value))}
              />
            </label>
          )}
          <label className="ed-field">
            {t("tasks.edit.time_label")}
            <input type="time" value={time} onChange={(e) => setTime(e.target.value)} />
          </label>
        </div>
      )}

      {kind === "weekly" && (
        <div className="ed-days">
          {[1, 2, 3, 4, 5, 6, 7].map((d) => (
            <button
              key={d}
              className={days.has(d) ? "active" : ""}
              onClick={() => {
                const next = new Set(days);
                if (next.has(d)) next.delete(d);
                else next.add(d);
                setDays(next);
              }}
            >
              {t(`schedule.wd${d}` as never)}
            </button>
          ))}
        </div>
      )}

      {kind === "interval" && (
        <div className="ed-grid">
          <label className="ed-field">
            {t("tasks.edit.every_label")}
            <input
              type="number"
              min={1}
              max={24}
              value={hours}
              onChange={(e) => setHours(Number(e.target.value))}
            />
          </label>
          <label className="ed-field">
            {t("tasks.edit.time_label")}
            <span className="ed-range">
              <input type="time" value={activeStart} onChange={(e) => setActiveStart(e.target.value)} />
              <span>–</span>
              <input type="time" value={activeEnd} onChange={(e) => setActiveEnd(e.target.value)} />
            </span>
          </label>
        </div>
      )}

      <div className="ed-acts">
        <button className="ed-cancel" onClick={onCancel}>{t("common.cancel")}</button>
        <button className="ed-save" onClick={save} disabled={!days.size && kind === "weekly"}>
          {t("tasks.save")}
        </button>
      </div>
    </div>
  );
}
