/**
 * Onboarding · Step 8 · 三卡 learn-by-doing 教程（v0.4.x）
 *
 * 设计文档：docs/prototypes/auto-install-cli-20260520.html · State 8
 *
 * 「试这条」**真跑一次 pipeline**（invoke("submit_query", { text })），
 * 不是录像 / 不是动画占位 —— 第一次成功 = 用户留下来。
 *
 * 用户按下按钮后，pipeline 跑起来 → 桌宠头顶气泡显示流式回复 →
 * 用户能立即看到老鼠真的会做这件事。
 */
import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useT } from "../i18n";

interface OnboardingTutorialProps {
  onDone: () => void;
  onSkip: () => void;
}

type CardKey = "A" | "B" | "C";

const TEXTS: Record<CardKey, string> = {
  // 这三条 text 会原样塞进 pipeline 当作 voice transcript
  // —— Claude 收到 + 截图，会按 system_prompt 的能力路由真正调 agent-browser / officecli。
  A: "搜一下今天北京天气",
  B: "总结这个表",
  C: "写一句产品介绍",
};

export function OnboardingTutorial({ onDone, onSkip }: OnboardingTutorialProps) {
  const t = useT();
  const [busy, setBusy] = useState<CardKey | null>(null);
  const [fired, setFired] = useState<Set<CardKey>>(new Set());

  const fire = async (k: CardKey) => {
    setBusy(k);
    try {
      await invoke("submit_query", { text: TEXTS[k] });
      setFired(prev => new Set(prev).add(k));
    } catch (e) {
      console.warn("[tutorial] submit_query failed:", e);
    } finally {
      setBusy(null);
    }
  };

  const Card = ({ k, icon, title, line, outcome }: {
    k: CardKey; icon: string; title: string; line: string; outcome: string;
  }) => {
    const isBusy = busy === k;
    const ran = fired.has(k);
    return (
      <div className="ob-tut-card">
        <div className="ob-tut-card-icon">{icon}</div>
        <div className="ob-tut-card-title">{title}</div>
        <div className="ob-tut-card-line">{line}</div>
        <div className="ob-tut-card-outcome">{outcome}</div>
        <button className="ob-tut-card-try"
                disabled={isBusy}
                onClick={() => fire(k)}>
          {isBusy ? "…" : ran ? "↻ " + t("tutorial.try_this") : t("tutorial.try_this")}
        </button>
      </div>
    );
  };

  return (
    <div className="ob-root">
      <h1 className="ob-title">{t("tutorial.title")}</h1>
      <p className="ob-subtitle">{t("tutorial.lead")}</p>

      <div className="ob-tut-grid">
        <Card k="A" icon="🌐"
              title={t("tutorial.cardA.title")}
              line={t("tutorial.cardA.line")}
              outcome={t("tutorial.cardA.outcome")} />
        <Card k="B" icon="📄"
              title={t("tutorial.cardB.title")}
              line={t("tutorial.cardB.line")}
              outcome={t("tutorial.cardB.outcome")} />
        <Card k="C" icon="⌨️"
              title={t("tutorial.cardC.title")}
              line={t("tutorial.cardC.line")}
              outcome={t("tutorial.cardC.outcome")} />
      </div>

      <button className="ob-cta" onClick={onDone}>
        {t("tutorial.done")}
      </button>
      <button className="ob-cta ob-cta-secondary" onClick={onSkip}>
        {t("tutorial.skip")}
      </button>
    </div>
  );
}
