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
import { useT } from "./i18n";
import { EV_SCHEDULE_RESULT } from "./types";
import type { Schedule, ScheduleInput, ScheduleView, RunRecord } from "./types";
import { scheduleLabel, scheduleIcon, formatWhen, formatRunTime } from "./lib/schedule-format";
import "./TasksView.css";

export default function TasksView() {
  const t = useT();
  const [items, setItems] = useState<ScheduleView[]>([]);
  const [loading, setLoading] = useState(true);
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const [runs, setRuns] = useState<Record<string, RunRecord[]>>({});
  const [editingId, setEditingId] = useState<string | null>(null);
  const [toast, setToast] = useState<string | null>(null);

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
      listen(EV_SCHEDULE_RESULT, () => {
        setRuns({}); // 清结果历史缓存，下次展开重新拉
        refresh();
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
      await invoke("run_schedule_now", { id: task.id }).catch(() => {});
      flash(`▶ ${task.title}`);
    },
    [flash],
  );

  const toggleExpand = useCallback(
    async (id: string) => {
      if (expandedId === id) {
        setExpandedId(null);
        return;
      }
      setExpandedId(id);
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
                runs={runs[task.id]}
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
  runs?: RunRecord[];
  onToggle: (enabled: boolean) => void;
  onExpand: () => void;
  onEdit: () => void;
  onDelete: () => void;
  onRunNow: () => void;
}
function TaskCard({ task, expanded, runs, onToggle, onExpand, onEdit, onDelete, onRunNow }: CardProps) {
  const t = useT();
  const last = task.lastRun;
  const lastCls = last ? (last.status === "ok" ? "ok" : last.status === "failed" ? "fail" : "skip") : "";
  const lastText = last
    ? `${last.status === "ok" ? "✓" : last.status === "failed" ? "✗" : "—"} ${formatRunTime(last.at)} · ${last.summary}`
    : t("tasks.last.never");

  return (
    <div className={`task ${task.enabled ? "" : "task-off"}`}>
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
        {task.enabled && task.nextRun && (
          <span className="task-next">{t("tasks.next", { when: formatWhen(t, task.nextRun) })}</span>
        )}
        {!task.enabled && <span className="task-next">{t("tasks.paused")}</span>}
        <span className={`task-last ${lastCls}`}>{lastText}</span>
        <span className="task-acts">
          <button className="task-ico" title={t("tasks.run_now")} onClick={onRunNow}>▶</button>
          <button className="task-ico" title={t("tasks.history.title")} onClick={onExpand}>
            {expanded ? "▴" : "▾"}
          </button>
          <button className="task-ico" title={t("tasks.edit")} onClick={onEdit}>✎</button>
          <button className="task-ico" title={t("tasks.delete")} onClick={onDelete}>🗑</button>
        </span>
      </div>
      {expanded && (
        <div className="task-history">
          {!runs || runs.length === 0 ? (
            <div className="task-hist-empty">{t("tasks.history.empty")}</div>
          ) : (
            <>
              {runs[0]?.output && <div className="task-hist-preview">{runs[0].output}</div>}
              {runs.map((r, i) => (
                <div className="task-run" key={i}>
                  <span className={`task-run-dot ${r.status}`} />
                  <span className="task-run-time">{formatRunTime(r.at)}</span>
                  <span className="task-run-sum">{r.summary}</span>
                </div>
              ))}
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
        {busy ? "…" : "＋"}
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
