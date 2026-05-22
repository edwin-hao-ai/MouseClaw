/**
 * PetMenu (v0.1.27 P2) — pop-up menu when user clicks the pet while idle.
 *
 * Design source: docs/prototypes/pet-anchor-menu-nudges-20260518.html §2
 * Bubble-style glass panel with vertical menu list, appears above-right of
 * the pet (overlay window is 320×320, menu ~220×280 fits inside).
 *
 * Items wired to existing commands in P2:
 *   🎤 summon       → emit shortcut-style summon (re-open overlay listening)
 *   📜 history      → open_hub_window (Hub has both clipboard + AI history)
 *   🍖 feed         → frontend-only visual ack (counter in localStorage)
 *   💤 nap          → frontend-only DnD timer (15min suppress)
 *   🎨 change skin  → open_picker_window
 *   ⚙️ settings    → open_status_window
 *
 * 🍖 / 💤 / 🎁 will get real backend wiring in P3 alongside the nudge system
 * (DnD state lives in presence buffer, feed count rewards animations, etc.).
 */
import { invoke } from "@tauri-apps/api/core";
import { useEffect, useRef, useState } from "react";
import { useT } from "../i18n";
import "./PetMenu.css";

interface PetMenuProps {
  open: boolean;
  onClose: () => void;
  /** Called when user clicks "feed" — host animates the pet eating. */
  onFeed: () => void;
  /** Called when user picks a nap duration (minutes). */
  onNap: (minutes: number) => void;
}

/** localStorage key for the lifetime feed counter — a tiny tamagotchi hook. */
const FEED_COUNT_KEY = "mouseclaw.feed.count";

export function PetMenu({ open, onClose, onFeed, onNap }: PetMenuProps) {
  const t = useT();
  const menuRef = useRef<HTMLDivElement | null>(null);
  // v0.4.3 · 菜单水平展开方向 —— 贴屏幕边时翻向内侧（后端按桌宠位置算），避免被切。
  const [menuH, setMenuH] = useState<"left" | "right" | "center">("center");

  useEffect(() => {
    if (!open) return;
    invoke<{ h?: string }>("get_pet_menu_orientation")
      .then((o) => setMenuH(o?.h === "left" || o?.h === "right" ? o.h : "center"))
      .catch(() => setMenuH("center"));
  }, [open]);

  // Click outside to close — listen on the document so clicks anywhere else
  // (including the pet itself) collapse the menu.
  useEffect(() => {
    if (!open) return;
    const onDocClick = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        onClose();
      }
    };
    // delay one tick so the click that opened the menu doesn't immediately close it
    const t = window.setTimeout(() => document.addEventListener("click", onDocClick), 0);
    return () => {
      window.clearTimeout(t);
      document.removeEventListener("click", onDocClick);
    };
  }, [open, onClose]);

  // Esc closes the menu
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation(); // don't let it bubble to App.tsx's dismiss handler
        onClose();
      }
    };
    document.addEventListener("keydown", onKey, true);
    return () => document.removeEventListener("keydown", onKey, true);
  }, [open, onClose]);

  if (!open) return null;

  const summon = () => {
    // v0.1.30 · 走新的 start_recording —— 等价"按下快捷键"，进 listening。
    // 之前调 toggle_recording 是反的（那个=松开，直接发送空录音）。
    onClose();
    invoke("start_recording").catch(() => {});
  };
  const openHub = () => { onClose(); invoke("open_hub_window").catch(() => {}); };
  const openPicker = () => { onClose(); invoke("open_picker_window").catch(() => {}); };
  const openStatus = () => {
    onClose();
    // Status window is the closest thing to "settings" today — has all toggles.
    invoke("check_permissions").catch(() => {});
    // open_status_window is invoked via tray; expose as a command path here
    // would need a new wrapper. For P2 we fall back to opening Hub which
    // surfaces the same content via tray menu. TODO P3: dedicated settings.
    invoke("open_hub_window").catch(() => {});
  };
  const feed = () => {
    const cur = Number(localStorage.getItem(FEED_COUNT_KEY) ?? 0) + 1;
    localStorage.setItem(FEED_COUNT_KEY, String(cur));
    onClose();
    onFeed();
  };
  const nap = (minutes: number) => { onClose(); onNap(minutes); };

  const feedCount = Number(localStorage.getItem(FEED_COUNT_KEY) ?? 0);

  return (
    <div
      className="pet-menu"
      data-h={menuH}
      ref={menuRef}
      role="menu"
      aria-label="Pet menu"
      // v0.1.30 · 截住 click 冒泡 —— 否则会传到 .stage-mouse 的 handleMouseClick
      // 再 toggle 一次菜单（刚 close 又 open）。点项目后正确收起的关键 fix。
      onClick={(e) => e.stopPropagation()}
    >
      <button className="pet-menu-item" role="menuitem" type="button" onClick={summon}>
        <span className="pet-menu-ico">🎤</span>
        <span className="pet-menu-label">{t("petmenu.summon")}</span>
        <span className="pet-menu-kbd" title={t("petmenu.summon_hint")}>⌘⇧Space</span>
      </button>
      <button className="pet-menu-item" role="menuitem" type="button" onClick={openHub}>
        <span className="pet-menu-ico">📜</span>
        <span className="pet-menu-label">{t("petmenu.history")}</span>
      </button>
      <button
        className="pet-menu-item" role="menuitem" type="button"
        onClick={() => { onClose(); invoke("open_tasks_window").catch(() => {}); }}
      >
        <span className="pet-menu-ico">⏰</span>
        <span className="pet-menu-label">{t("petmenu.tasks")}</span>
      </button>
      {/* v0.4.0 · 「📖 教我用」 —— 用户忘了功能可以随时重看引导 */}
      <button
        className="pet-menu-item" role="menuitem" type="button"
        onClick={() => { invoke("tour_start").catch(() => {}); onClose(); }}
      >
        <span className="pet-menu-ico">📖</span>
        <span className="pet-menu-label">教我用 MouseClaw</span>
      </button>

      <div className="pet-menu-divider" />

      <button className="pet-menu-item" role="menuitem" type="button" onClick={feed}>
        <span className="pet-menu-ico">🍖</span>
        <span className="pet-menu-label">{t("petmenu.feed")}</span>
        {feedCount > 0 && <span className="pet-menu-badge">+{feedCount}</span>}
      </button>
      <button className="pet-menu-item" role="menuitem" type="button" onClick={() => nap(15)}>
        <span className="pet-menu-ico">💤</span>
        <span className="pet-menu-label">{t("petmenu.nap")}</span>
      </button>

      <div className="pet-menu-divider" />

      <button className="pet-menu-item" role="menuitem" type="button" onClick={openPicker}>
        <span className="pet-menu-ico">🎨</span>
        <span className="pet-menu-label">{t("petmenu.skin")}</span>
      </button>
      <button className="pet-menu-item" role="menuitem" type="button" onClick={openStatus}>
        <span className="pet-menu-ico">⚙️</span>
        <span className="pet-menu-label">{t("petmenu.settings")}</span>
      </button>
    </div>
  );
}
