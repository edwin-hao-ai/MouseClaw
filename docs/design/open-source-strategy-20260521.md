# MouseClaw 开源 / 商业化策略

> 决策日期：2026-05-21 · 状态：策略已拍板，落地分阶段
> 这份文档是开源切割线 + 商业化路径的**单一信源**。后续任何"这块要不要开源 / 怎么收费"的争议以本文为准。

---

## 0. 一句话策略

**壳开源（建立信任 + 获客 + 社区皮肤），中英混合语音引擎闭源（护城河 + 变现）。
商业化第一刀走"领域术语词包"（零托管成本、最快见效）。**

用户已拍板（2026-05-21）：
- 切割线 = **壳开源 + 语音引擎闭源**
- 变现起点 = **领域术语词包**

---

## 1. 先看清：MouseClaw 的钱在哪

把 ~15k 行 Rust + ~3.7k 行前端按"护城河强度"分三层：

| 层 | 模块 | 复制难度 | 定位 |
|---|---|---|---|
| **获客 / 品牌层** | PixelMouse、skins、animations、bubble、nudge、presence、cursor_trail、companion、intimacy | 中（靠品味+迭代，源码能抄 vibe 但追不上节奏） | 是脸，不是命门 |
| **OS 管线层** | overlay / overlay_size / anchor / pet_passthrough、mode_b 写回光标、selection、clipboard、tray、permissions | 高（macOS NSPanel + CGEvent + AX 全是脏活） | 是壁垒但**通用**，谁都能照着重做 |
| **真护城河层** | **本地中英混合语音链**：voice_ime + voice_correct + vocab（术语偏置 + 大小写还原） + transcribe_stream + punctuation + model_downloader | **极高**（sherpa 模型开源，但"调音"是我们的：偏置 score、CN/EN 混合、规则纠错、ALLCAPS 还原） | **这才是命门** |
| **编排胶水** | backend.rs（4 后端抽象）、claude_cli、provider_env、ai_queue、pipeline | 低-中 | 关键洞察，不是壁垒 |

### 最重要的一个判断

**MouseClaw 没有 AI 推理成本**——用户跑的是**他自己的** Claude / Codex / OpenClaw CLI，
推理费付给 Anthropic 不是付给我们。

- ✅ 好处：零边际成本，不会变成烧钱 SaaS。
- ⚠️ 代价：**没有天然可计量的收费点**（不像按 token 收钱的 SaaS）。
  → 所以商业化必须从"功能 / 资产"切，不能从"用量"切。

---

## 2. 为什么"壳"必须开源（不是想不想，是不得不）

MouseClaw 要 **Accessibility + 屏幕录制**权限，能读屏幕、能往任意 app 打字。
对这种权限的工具，**闭源 = 没人敢装**。开源在这里不是营销手段，是**信任的入场券**。

→ 所有碰 OS 的部分必须开源：overlay 管线、mode_b 写回、selection / clipboard、tray、权限申请。
  这恰好就是"OS 管线层 + 获客层"——它们护城河本来就不深，开源损失小、信任收益巨大。
→ 顺带 **skins 开源** → 社区贡献皮肤 = 免费内容 + 参与感（9 款皮肤之外最便宜的增长杠杆）。

---

## 3. 开源 / 闭源切割：`clawspeak` 方案

### 仓库一拆二

```
mouseclaw/              ← 开源（License 见 §6），建立信任、收 star、社区皮肤
  ├ 所有 OS 管线（overlay / mode_b / selection / clipboard / tray / permissions）
  ├ pet / skins / animations / onboarding 全部前端
  ├ backend 编排（4 后端抽象）、sessions、config
  └ 语音链调用方式：spawn 外部二进制 `clawspeak`（找不到 → 走开源 fallback）

clawspeak/              ← 闭源，专有 license，护城河
  └ voice_correct + vocab 偏置 + 中英混合调音 + punctuation 集成
     输入：audio buffer   输出：text   （纯函数，无网络无 I/O）
```

### 关键设计原则

1. **开源壳必须能独立跑**：内置一个"裸 sherpa fallback"（不带中英混合调音、不带规则纠错），
   让开源项目是个**完整可用**的东西，不是残废 demo。装了 `clawspeak` 的人自动升级到"精调版"。
   - 开源党满意（能跑、能审计 OS 行为）
   - 真本事（调音）在闭源二进制里，抄不走
   - `clawspeak` 还能单独卖给别的开发者（B2D）

2. **`clawspeak` 是纯函数，无 I/O 无网络** → 即使闭源，行为完全可审计
   （"它只把音频变文字，不上传"）。OS 交互（开麦、打字）全留在开源壳里。
   这一点对"碰麦克风的闭源组件"的信任质疑是决定性的化解。

### 拆分时机（重要）

`voice_correct.rs` / `vocab.rs` 现在在主仓。真拆是一次**有风险的仓库手术**
（build 钩子、`include_str!`、CI 全要改）。
→ **先不拆**，等开源正式发布前 1-2 周再做，降低对当前迭代的干扰。

---

## 4. 领域词包：架构 + 变现（商业化第一刀）

### 4.1 现有架构（已摸清）

`vocab.rs` 设计很干净，词包是它的自然扩展：

```
内置 programmer.txt (include_str!) + ~/.mouseclaw/vocab/user.txt
  → regenerate_active() 合并去重 → active.txt
  → sherpa hotwords_file 读 active.txt
```

**双用途**（关键）：词表既做
1. ASR hotword 偏置（boost 术语识别率）
2. **大小写还原**（`build_casing_map`：`OPENAI→OpenAI`、`PYTORCH→PyTorch`）
   → 所以词包里**大小写必须写对**，直接修中英混合的 ALLCAPS 输出。

### 4.2 词包架构扩展（不破坏现有）

```
~/.mouseclaw/vocab/
  ├ user.txt          ← 现有，用户手写
  ├ packs/
  │   ├ programmer.txt    （免费内置，已有）
  │   ├ ai-ml.txt         （免费，本次新增 · 见 resources/vocab/）
  │   ├ devops-cloud.txt  （免费，本次新增）
  │   ├ legal-cn-en.txt   （付费，未来）
  │   ├ medical-cn-en.txt （付费，未来）
  │   └ <pack>.json       ← manifest: {id, locale, version, entries, tier: free|paid, token?}
  └ active.txt        ← regenerate_active 合并：内置 + 启用的 packs + user.txt
```

- `regenerate_active(builtin_enabled)` → 改成 `regenerate_active(enabled_pack_ids)`，核心逻辑不变。
- 新增 `vocab_packs.rs` 管：注册 / 启用 / 安装 / license 校验。

### 4.3 变现机制（诚实版）

⚠️ **纯文本词表无法有效 DRM**——谁都能重建一份词。"卖静态 txt"这条不成立。
能立住的护城河是这两条：

1. **订阅制 + 持续更新**：法律 / 医疗 / 前端框架的术语一直在变。
   盗版拿到的是**会过期的快照**；卖的是"一直最新 + 调好的 score"。这是真正防盗版的锁。
2. **便利 + 集成**：一键装、随 app 更新、和偏置引擎调好。
   $1–3 的便利价，大多数人懒得自己攒。

→ **建议模型**：免费社区包（开源，引流） + 付费精选包（订阅，持续维护）。
  付费包用嵌入公钥**离线验签**（跟 notarize 一个套路），挡住"随手白嫖"，
  但别指望挡专业盗版——靠"持续更新会过期"才是真锁。

---

## 5. 未决策点（需用户拍板才能进 §4 的代码阶段）

| # | 决策点 | 影响 | 状态 |
|---|---|---|---|
| D1 | 词包变现走**订阅制** vs **一次性买断** | 决定 manifest 字段 + license 校验逻辑：订阅要存到期时间 + 联网续期（破坏"纯本地"卖点）；买断纯离线验签、最简单但防盗版弱 | ⏳ 待拍板 |
| D2 | 词包浏览 / 启用 / 购买 **UI** | 按项目硬规则**必须先做 prototype HTML 拍板**，才能写 Rust/TS | ⏳ 待 prototype |
| D3 | 开源 License 选 **BSL** vs **Apache + 商标 + 闭源引擎** | 见 §6 | ⏳ 待拍板 |

---

## 6. License 建议

- **开源壳**：用 **BSL（Business Source License）** 或 **Apache-2.0 + 商标保护**。
  - BSL：源码公开、可自用、N 年后转 Apache，但禁止拿去做竞品商业服务 → 既给信任又防白嫖竞品。
  - 纯 MIT/Apache：会让别人连壳带皮拿去套牌；靠商标 + 闭源引擎做护城河。
  - ⚠️ BSL 不是 OSI 认可的"开源"，HN / Reddit 会有人挑刺。取舍：要信任最大化 → Apache；要防竞品 → BSL。
- **`clawspeak` 闭源**：专有二进制，单独 license。
  开源壳里写清"语音引擎为闭源组件，纯本地、不联网"，并提供**降级开源 fallback**（裸 sherpa）。

---

## 7. 路线图（按风险从低到高）

1. **现在就能安全做（零风险、有价值）** ← 本次 step 1
   - [x] 把策略写进本文档存档（决策不丢）
   - [x] 再写 1-2 个**免费词包**（ai-ml / devops-cloud）验证 pack 格式 + 给开源社区种子内容
   - 纯内容，不碰 UI，不动仓库
2. **拍板 D1 + D2 后做**
   - 词包浏览 / 购买 UI 的 prototype HTML → 用户拍板 → 才写 `vocab_packs.rs` + 前端
3. **开源正式发布前 1-2 周再做**
   - 仓库一拆二 + License 文件 + `clawspeak` 接口契约

---

## 8. 风险 / 取舍（诚实版）

- **pet 开源 = vibe 可被抄**：但 charm 靠持续迭代品味，源码帮不了抄袭者追上节奏。
  收益（信任 + 社区皮肤）远大于风险。
- **闭源语音引擎 = 部分用户皱眉**：用"纯本地无网络 + 提供裸 sherpa fallback"化解。
- **BSL 不是 OSI 开源**：见 §6，需取舍。
- **词包无法 DRM**：见 §4.3，靠"订阅 + 持续更新会过期"立护城河，不靠加密。
- **零推理成本 = 无可计量收费点**：变现只能靠资产（词包 / clawspeak / Pro 功能），不能靠用量。
