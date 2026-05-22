/**
 * PickerView · v0.1.26
 *
 * 桌宠选择器窗口 —— 替代托盘里的 6-item 子菜单。
 *
 * 交互：
 *   - 单击卡片 = 临时切换（菜单栏老鼠立即换 + 大预览换）
 *   - 双击卡片 / 点「应用并关闭」 = 写 config + 关窗
 *   - 点「取消」 = 还原回打开窗口时的 skin + 关窗
 *
 * v0.1.26 只显示「内置」9 款（鼠 6 + 猫 1 + 狐 1 + 蛙 1）。
 * v0.1.27+ 会从 ~/.mouseclaw/skins/ 加载用户安装的 manifest，列在「社区」section。
 */
import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { PixelMouse } from "./components/PixelMouse";
import { SKINS, type Skin, type SkinId, type Species } from "./skins";
import { useCompanion } from "./hooks/useCompanion";
import { useT } from "./i18n";
import type { TranslationKey } from "./i18n/types";
import "./PickerView.css";

type Group = { species: Species; label: string; skins: Skin[] };

/** v0.4.4 · 9 性格 + 自定义,顺序与 config.rs Personality::all() 一致。 */
const PERSONA_IDS = [
  "warm", "snarky", "minimal", "companion", "pro",
  "cheerful", "calm", "curious", "tsundere", "custom",
] as const;
type PersonaId = (typeof PERSONA_IDS)[number];

function groupBySpecies(skins: readonly Skin[]): Group[] {
  const order: { species: Species; label: string }[] = [
    { species: "mouse", label: "🐭 鼠家族" },
    { species: "cat",   label: "🐱 猫家族" },
    { species: "fox",   label: "🦊 狐家族" },
    { species: "frog",  label: "🐸 蛙家族" },
  ];
  return order
    .map(({ species, label }) => ({
      species,
      label,
      skins: skins.filter(s => (s.species ?? "mouse") === species),
    }))
    .filter(g => g.skins.length > 0);
}

export default function PickerView() {
  const t = useT();
  const [originalSkin, setOriginalSkin] = useState<SkinId>("classic");
  const [selected, setSelected] = useState<SkinId>("classic");
  const [hovered, setHovered] = useState<SkinId | null>(null);
  // v0.4.4 · 起名 + 性格 —— 改动即时持久化(与 skin 的"单击即存"一致)
  const [petName, setPetName] = useState("");
  const [personality, setPersonality] = useState<PersonaId>("warm");
  const [customText, setCustomText] = useState("");
  // v0.4.5 · 起名交互优雅化（prototype: pet-naming-and-eye-tracking-20260523）
  const [suggestions, setSuggestions] = useState<string[]>([]); // 当前展示的 4 个名字建议
  const [diceSpin, setDiceSpin] = useState(false);              // 🎲 旋转一下
  const [greetBump, setGreetBump] = useState(false);            // 收到名字 → 桌宠轻跳
  // v0.4+ · 预览大老鼠接 companion — 在 picker 里移鼠标，预览的眼睛会追
  const previewStageRef = useRef<HTMLDivElement>(null);
  const companion = useCompanion(previewStageRef);

  // 拉当前 skin 作为 baseline —— 关窗时若用户没应用就回滚到这个
  useEffect(() => {
    invoke<string>("get_skin").then(id => {
      setOriginalSkin(id as SkinId);
      setSelected(id as SkinId);
    }).catch(() => {});
    // v0.4.4 · 拉当前身份(名字 + 性格)
    invoke<{ name: string; personality: string; custom: string }>("get_pet_identity")
      .then(id => {
        setPetName(id.name ?? "");
        setPersonality((PERSONA_IDS as readonly string[]).includes(id.personality)
          ? (id.personality as PersonaId) : "warm");
        setCustomText(id.custom ?? "");
      }).catch(() => {});
  }, []);

  // 即时存身份(显式传值,避免 setState 异步导致的 stale 闭包)
  const saveIdentity = (name: string, persona: PersonaId, custom: string) => {
    invoke("save_pet_identity", { name, personality: persona, custom }).catch(() => {});
  };

  // v0.4.5 · 名字建议池（i18n 逗号分隔 → 数组）+ 随机抽 4 个
  const namedPool = useMemo(
    () => t("picker.identity.name_suggestions").split(",").map(s => s.trim()).filter(Boolean),
    [t],
  );
  const pickFour = (pool: string[]) => [...pool].sort(() => Math.random() - 0.5).slice(0, 4);
  useEffect(() => { setSuggestions(pickFour(namedPool)); }, [namedPool]);
  // 选名（chip / 🎲）→ 即时持久化（与 skin「单击即存」一致）
  const chooseName = (n: string) => { setPetName(n); saveIdentity(n, personality, customText); };
  const rollRandomName = () => {
    setDiceSpin(true);
    setTimeout(() => setDiceSpin(false), 300);
    if (namedPool.length === 0) return;
    chooseName(namedPool[Math.floor(Math.random() * namedPool.length)]);
    setSuggestions(pickFour(namedPool)); // 顺便换一批建议
  };
  // 收到名字 → 桌宠轻跳一下（"它收到了这个名字"反馈）
  useEffect(() => {
    if (!petName.trim()) return;
    setGreetBump(true);
    const id = setTimeout(() => setGreetBump(false), 200);
    return () => clearTimeout(id);
  }, [petName]);

  // v0.1.31 · 拦截原生 ✕ 按钮 —— 不让它真 close（会让 accessory app 退出）
  // preventDefault 后走我们的 cancel 逻辑：还原 + hide。
  useEffect(() => {
    const w = getCurrentWindow();
    const unlistenPromise = w.onCloseRequested(async (event) => {
      event.preventDefault();
      if (selected !== originalSkin) {
        await invoke("save_skin", { skin: originalSkin }).catch(() => {});
      }
      await w.hide();
    });
    return () => { unlistenPromise.then(fn => fn()).catch(() => {}); };
  }, [selected, originalSkin]);

  const groups = useMemo(() => groupBySpecies(SKINS), []);
  const previewSkin = (hovered ?? selected) as SkinId;
  const previewMeta = SKINS.find(s => s.id === previewSkin) ?? SKINS[0];

  // 单击卡片 = 临时换（菜单栏立即变；config 没写）
  const peek = (id: SkinId) => {
    setSelected(id);
    invoke("save_skin", { skin: id }).catch(() => {});
  };

  // 双击 / Apply = 永久应用 + 关窗
  const apply = async (id: SkinId) => {
    try {
      await invoke("save_skin", { skin: id });
      setOriginalSkin(id);
      // v0.1.30 · hide 不要 close —— close 会让 Tauri 复位 activation policy
      // 进而把 accessory app 退出（macOS 把最后一个可见 window 关闭等价于退出）
      await getCurrentWindow().hide();
    } catch (e) {
      console.error("apply skin failed", e);
    }
  };

  // Cancel = 还原 + 关窗
  const cancel = async () => {
    if (selected !== originalSkin) {
      await invoke("save_skin", { skin: originalSkin }).catch(() => {});
    }
    // v0.1.31 · 同 apply 路径：用 hide 不要 close。
    // v0.1.30 只修了 apply 路径漏了 cancel —— 用户反馈点取消还是退出整个 app。
    await getCurrentWindow().hide();
  };

  return (
    <div className="picker-root">
      <header className="picker-header">
        <h1>🎨 选择你的桌宠</h1>
        <p>单击预览 · 双击应用 · 数据全在本地</p>
      </header>

      <div className="picker-body">
        <section className="picker-grid-wrap">
          {groups.map(g => (
            <div key={g.species} className="picker-group">
              <div className="picker-group-label">{g.label}</div>
              <div className="picker-grid">
                {g.skins.map(s => {
                  const isSel = selected === s.id;
                  const isHov = hovered === s.id;
                  return (
                    <button
                      key={s.id}
                      type="button"
                      className={`picker-card ${isSel ? "selected" : ""}`}
                      onClick={() => peek(s.id)}
                      onDoubleClick={() => apply(s.id)}
                      onMouseEnter={() => setHovered(s.id)}
                      onMouseLeave={() => setHovered(null)}
                      title={s.desc}
                    >
                      <div className="picker-card-stage">
                        <PixelMouse
                          state={isHov ? "jump" : "listen"}
                          size={64}
                          skin={s.id}
                        />
                      </div>
                      <div className="picker-card-name">{s.name}</div>
                      {s.tag && <div className="picker-card-tag">{s.tag}</div>}
                    </button>
                  );
                })}
              </div>
            </div>
          ))}
        </section>

        <aside className="picker-preview">
          <div className="picker-preview-stage" ref={previewStageRef}>
            <PixelMouse
              state="listen" size={128} skin={previewSkin}
              companionState={companion.state}
              eyeOffset={{ x: companion.eyeOffsetX, y: companion.eyeOffsetY }}
            />
          </div>
          <div className="picker-preview-name">{previewMeta.name}</div>
          <div className="picker-preview-desc">{previewMeta.desc}</div>
          <div className="picker-preview-meta">
            <span>物种：{previewMeta.species ?? "mouse"}</span>
            <span>id：{previewMeta.id}</span>
          </div>

          {/* v0.4.4 · 起名 + 性格（v0.4.5 起名交互优雅化）*/}
          <div className="picker-identity">
            <div className="picker-id-label">{t("picker.identity.name_label")}</div>
            <div className="picker-name-row">
              <input
                className="picker-name-input"
                value={petName}
                placeholder={t("picker.identity.name_placeholder")}
                maxLength={12}
                onChange={e => setPetName(e.target.value)}
                onBlur={() => saveIdentity(petName, personality, customText)}
              />
              <button
                type="button"
                className={`picker-dice ${diceSpin ? "spin" : ""}`}
                title={t("picker.identity.name_placeholder")}
                onClick={rollRandomName}
              >🎲</button>
            </div>
            <div className="picker-name-suggest">
              {suggestions.map(n => (
                <button
                  key={n}
                  type="button"
                  className={`picker-name-chip ${petName.trim() === n ? "on" : ""}`}
                  onClick={() => chooseName(n)}
                >{n}</button>
              ))}
            </div>
            <div className="picker-greet">
              <div className="picker-greet-pet" data-bump={greetBump ? "1" : undefined}>
                <PixelMouse state="listen" size={40} skin={previewSkin} />
              </div>
              <div className="picker-greet-txt">
                {petName.trim()
                  ? t("picker.identity.greet", { name: petName.trim() })
                  : <span className="picker-greet-muted">{t("picker.identity.greet_empty")}</span>}
              </div>
            </div>
            <div className="picker-id-label">{t("picker.identity.personality_label")}</div>
            <div className="persona-list">
              {PERSONA_IDS.map(pid => (
                <button
                  key={pid}
                  type="button"
                  className={`persona-item ${personality === pid ? "sel" : ""}`}
                  onClick={() => { setPersonality(pid); saveIdentity(petName, pid, customText); }}
                >
                  {t(`persona.${pid}.name` as TranslationKey)}
                </button>
              ))}
            </div>
            <div className="persona-preview">
              {t(`persona.${personality}.preview` as TranslationKey)}
            </div>
            {personality === "custom" && (
              <input
                className="persona-custom-input"
                value={customText}
                placeholder={t("picker.identity.custom_placeholder")}
                onChange={e => setCustomText(e.target.value)}
                onBlur={() => saveIdentity(petName, personality, customText)}
              />
            )}
          </div>

          <div className="picker-actions">
            <button type="button" className="picker-btn secondary" onClick={cancel}>
              取消
            </button>
            <button
              type="button"
              className="picker-btn primary"
              onClick={() => apply(selected)}
              disabled={selected === originalSkin}
            >
              {selected === originalSkin ? "✓ 当前桌宠" : "应用并关闭"}
            </button>
          </div>
        </aside>
      </div>

      <footer className="picker-footer">
        💡 v0.1.27 起将支持 <code>~/.mouseclaw/skins/</code> 安装社区桌宠
      </footer>
    </div>
  );
}
