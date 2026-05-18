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
import { useEffect, useRef } from "react";
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
    // Re-trigger pipeline by invoking dismiss → user can press shortcut.
    // Real "summon from menu" would need a new command; for P2 we just close
    // and let the user press the shortcut. TODO P3: add summon_now command.
    onClose();
    invoke("toggle_recording").catch(() => {});
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
        <span className="pet-menu-kbd">⌘⇧Space</span>
      </button>
      <button className="pet-menu-item" role="menuitem" type="button" onClick={openHub}>
        <span className="pet-menu-ico">📜</span>
        <span className="pet-menu-label">{t("petmenu.history")}</span>
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
