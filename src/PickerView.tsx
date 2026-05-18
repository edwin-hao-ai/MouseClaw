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
import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { PixelMouse } from "./components/PixelMouse";
import { SKINS, type Skin, type SkinId, type Species } from "./skins";
import "./PickerView.css";

type Group = { species: Species; label: string; skins: Skin[] };

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
  const [originalSkin, setOriginalSkin] = useState<SkinId>("classic");
  const [selected, setSelected] = useState<SkinId>("classic");
  const [hovered, setHovered] = useState<SkinId | null>(null);

  // 拉当前 skin 作为 baseline —— 关窗时若用户没应用就回滚到这个
  useEffect(() => {
    invoke<string>("get_skin").then(id => {
      setOriginalSkin(id as SkinId);
      setSelected(id as SkinId);
    }).catch(() => {});
  }, []);

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
    await getCurrentWindow().close();
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
          <div className="picker-preview-stage">
            <PixelMouse state="listen" size={128} skin={previewSkin} />
          </div>
          <div className="picker-preview-name">{previewMeta.name}</div>
          <div className="picker-preview-desc">{previewMeta.desc}</div>
          <div className="picker-preview-meta">
            <span>物种：{previewMeta.species ?? "mouse"}</span>
            <span>id：{previewMeta.id}</span>
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
