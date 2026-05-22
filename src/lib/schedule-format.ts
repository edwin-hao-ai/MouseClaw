/**
 * 定时任务 —— 频率 / 下次时间 的可读化（i18n）。
 * 单一信源：TasksView / 确认卡 / 结果气泡 都用这里，保证文案一致。
 */
import type { Schedule } from "../types";
import type { TranslationKey } from "../i18n/types";

type T = (key: TranslationKey, args?: Record<string, string | number>) => string;

const pad = (n: number) => String(n).padStart(2, "0");

/** 周几数字(1=一..7=日) → 短名。 */
export function weekdayName(t: T, n: number): string {
  const key = `schedule.wd${Math.min(7, Math.max(1, n))}` as TranslationKey;
  return t(key);
}

/** 把 interval 的 everyMinutes 变成 "{n} 小时" / "{n} 分钟"。 */
function spanLabel(t: T, minutes: number): string {
  if (minutes % 60 === 0 && minutes >= 60) {
    return t("schedule.span_hours", { n: minutes / 60 });
  }
  return t("schedule.span_min", { n: minutes });
}

/** 频率可读化："每天 08:00" / "工作日 18:00" / "周一/周三 09:00" / "每隔 2 小时 · 09:00–22:00"。 */
export function scheduleLabel(t: T, s: Schedule): string {
  switch (s.kind) {
    case "daily":
      return t("schedule.daily", { time: s.time });
    case "weekday":
      return t("schedule.weekday", { time: s.time });
    case "weekly": {
      const days = [...s.days].sort((a, b) => a - b).map((d) => weekdayName(t, d)).join("/");
      return t("schedule.weekly", { days, time: s.time });
    }
    case "interval": {
      let label = t("schedule.interval", { span: spanLabel(t, s.everyMinutes) });
      if (s.activeStart && s.activeEnd) {
        label += t("schedule.interval.active", { start: s.activeStart, end: s.activeEnd });
      }
      return label;
    }
    case "monthly":
      return t("schedule.monthly", { day: s.day, time: s.time });
  }
}

/** 频率前缀 emoji —— interval(心跳) 用 🔁，其余用 ⏰。 */
export function scheduleIcon(s: Schedule): string {
  return s.kind === "interval" ? "🔁" : "⏰";
}

/** 下次执行的相对时间："今天 08:00" / "明天 08:00" / "5/25 08:00"。 */
export function formatWhen(t: T, iso: string): string {
  const d = new Date(iso);
  if (isNaN(d.getTime())) return iso;
  const time = `${pad(d.getHours())}:${pad(d.getMinutes())}`;
  const now = new Date();
  const dayKey = (x: Date) => `${x.getFullYear()}-${x.getMonth()}-${x.getDate()}`;
  const tomorrow = new Date(now);
  tomorrow.setDate(now.getDate() + 1);
  if (dayKey(d) === dayKey(now)) return t("when.today", { time });
  if (dayKey(d) === dayKey(tomorrow)) return t("when.tomorrow", { time });
  return t("when.other", { md: `${d.getMonth() + 1}/${d.getDate()}`, time });
}

/** 把一条执行记录的时间戳格式化成 "5/22 08:00"。 */
export function formatRunTime(iso: string): string {
  const d = new Date(iso);
  if (isNaN(d.getTime())) return iso;
  return `${d.getMonth() + 1}/${d.getDate()} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
}
