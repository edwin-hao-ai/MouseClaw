# 桌宠身份(名字+性格) + 长期记忆(SQLite Graph RAG) · 设计文档

> **日期:** 2026-05-21 · **分支:** `claude/desktop-pet-memory-features-46oSV`
> **状态:** 设计稿(未写代码,未做 prototype)。本文档先把 UX 全流程和架构选型定下来,
> 用户拍板后再进 prototype → Rust 实现。
>
> **Scope:** ①给桌宠起名字 + 选性格 ②长期记忆(记得跨天的交互、偏好、用过的 app/项目)
> ③让 AI 调用时把「视觉(截图)+ 音频(语音)+ 记忆」三者联动。
> **Out of scope(明确不做):** 全天候键盘记录 / 屏幕录制 / 记忆上云 / v1 上本地向量模型 /
> 真·图谱可视化(留 v2)。

---

## 0. 三个功能不是一个量级,先把关系讲清

| 功能 | 工程量 | 隐私风险 | 依赖 | 建议时序 |
|---|---|---|---|---|
| ①名字 + 性格 | 小(~半天-1天) | 无 | 无 | **第一刀,独立可交付** |
| ②长期记忆 | 大(基建+UX 多个 PR) | **中-高(取决于采集范围)** | 无(素材已有) | 第二步,先出 prototype |
| ③视觉+音频+记忆联动 | 小(prompt 拼接) | 随②走 | **依赖②** | ②做完后自然延伸 |

③不是独立功能,它就是②的「读取期」表现。所以本文档主体是 ① 和 ②。

---

## 1. 名字 + 性格

### 1.1 这功能到底给用户什么(UX 价值)

桌宠是 **daily-use 常驻陪伴物**。名字 + 性格不是花架子,它改变两件事:

1. **归属感 → 留存**:一只叫「老李」、会毒舌吐槽你的老鼠,和一只匿名灰鼠,用户愿意一直留着的概率完全不同。常驻工具的第一杀手是「装了就忘」,身份是最便宜的反制。
2. **同一条记忆,不同的陪伴温度**(这点和②叠加才完整):
   - 毒舌性格 + 记忆:「你这周第 4 次问我同一个 git rebase 了,要不要我给你写个 alias。」
   - 暖心性格 + 记忆:「这个 bug 你卡两天了,别急,我们一步步来。」
   - 极简性格:同样的答案,只给结论,不寒暄。

性格本质是 **system prompt 的一段语气指令**,成本极低,收益是「每一次交互的体感」。

### 1.2 Config 新字段(v19 → v20)

```rust
// config.rs — 都用 #[serde(default)],老配置无痛升级
pub struct Config {
    // ...
    /// v0.4.4 · 用户给桌宠起的名字。None = 还没起名,桌宠用通用自称。
    #[serde(default)]
    pub pet_name: Option<String>,
    /// v0.4.4 · 性格预设。
    #[serde(default)]
    pub personality: Personality,
    /// custom 性格时的一句话描述(personality == Custom 才用)。
    #[serde(default)]
    pub personality_custom: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Personality {
    #[default] Warm,   // 暖心陪伴(默认 —— 最不容易出错的语气)
    Snarky,            // 毒舌助理
    Minimal,           // 极简话少
    Companion,         // 话痨陪伴(主动接话、给情绪反应)
    Custom,            // 用户自定义一句话
}
```

> 同步 checklist(CLAUDE.md §8 已知坑 1):加字段后要改 `Config::default()` +
> `commands.rs::save_*` 里所有字面量构造 `Config { ... }`,否则 cargo 报 E0063。

### 1.3 注入点 —— 必须走共享 `system_prompt()`(多后端硬规则)

`claude_cli.rs::system_prompt()` 被 4 个 backend 共用(`backend.rs` 三处调用)。
在它**最前面**注入身份段,4 个后端自动生效,**不给某个 backend 写独立 prompt**:

```rust
pub fn system_prompt() -> String {
    let cfg = Config::load();
    let name = cfg.pet_name.as_deref().unwrap_or("MouseClaw");
    let persona = personality_fragment(cfg.personality, cfg.personality_custom.as_deref());
    let mut p = format!(
        "你的名字是「{name}」,是用户的桌面助手桌宠。\n\
         {persona}\n\
         【硬约束】性格只影响语气和措辞,绝不影响答案的正确性、完整性和是否执行任务。\
         需要写入光标 / 调工具时,该干的活照干,语气归语气。\n\n"
    );
    // ... 既有的任务指令、INSERT 标记规则、BROWSER_CAPABILITY 拼在后面 ...
    p
}
```

`personality_fragment` 返回各预设的一句话语气描述。**注意那条硬约束**:这是防止「极简」性格把答案也省了、「毒舌」把任务也拒了 —— 性格不能侵蚀功能。

### 1.4 入口与可发现性(UX 10 问之「入口」「首次」)

- **主入口:复用已有的 picker 窗口**(桌宠选择器,`?view=picker`)。CLAUDE.md 规定「不做图形设置界面(除一步 Onboarding)」,但 picker 窗口已是既定例外。在 picker 里皮肤选择旁边加一栏:
  - 名字输入框(单行,占位符「给我起个名字…」)
  - 性格 4 选 1 + custom 输入
- **首次提示(克制版)**:用户第一次召唤桌宠完成一次任务后,**仅一次**在气泡里轻提示「我还没名字,点托盘可以给我起名 🐭」。不弹窗、不抢焦点、可忽略。绝不每次都问。
- **托盘菜单**:已有项前面把「召唤老鼠」改成「召唤 {name}」(有名字时),让身份在最常见的入口可见。
- **i18n**:性格预设的 label / 描述 / 首次提示文案,三处都要加(`types.ts` + `zh.ts` + `en.ts`)。性格 prompt 片段本身按 `current_lang` 出中/英(参考 tray.rs 双语写法)。

### 1.5 全皮肤(动画硬规则)

名字+性格是**文本**,不是动画,所以「全皮肤动画一视同仁」硬规则不直接适用。但 picker UI 必须 9 款皮肤都能正常起名(prototype 阶段 9 只同屏排开验证)。**默认名不按皮肤 hardcode** —— 未起名时统一通用自称,避免「猫皮肤却自称老鼠」的穿帮。

---

## 2. 长期记忆(本文档的重头戏)

### 2.1 先回答你的问题:这功能到底给用户什么体验?(具体案例)

记忆的素材 = 每次召唤那一刻已经合法捕获的「前台 app 标题 + 整屏截图 + 语音问题 + AI 回复 + 时间戳」(都在 `sessions.jsonl`)。**纯本地、纯交互时刻,零新增监控**。基于这个,下面是真实会发生的场景:

**案例 A · 跨天接续(最高频价值)**
> 周一你在 VSCode 里召唤:「这个 borrow checker 报错怎么解」。周三同一个项目再召唤,
> 桌宠:「上次在这个文件你卡在 `'a` lifetime,后来改成 `Arc` 通了。这次还是它吗?」
>
> 没有记忆:每次都冷启动,你得重新解释上下文。有记忆:它接得上昨天的话。

**案例 B · 模糊回忆(人脑最差、工具最强)**
> 「上次那个讲 sqlite 全文索引的网页叫啥来着?」
> → 桌宠:「周二下午你在 Chrome 看的,标题是『SQLite FTS5 Extension』,当时你还问了 trigram tokenizer。」
>
> 你只记得个模糊片段,它把时间/app/标题精确还给你。

**案例 C · 日/周回顾(隐私友好的杀手级功能)**
> 晚上问:「我今天都干了啥?」
> → 「上午在 VSCode 调 overlay sizing(问了 3 次),中午查 sqlite,下午写发布脚本。
>    今天一共召唤我 12 次,主要围着 MouseClaw 这个项目转。」
>
> **关键**:这全部来自「你主动召唤我的那些时刻」,不是后台监控你一整天。这是它和
> 「全天候记录」在体验和伦理上的根本区别 —— 它只记得「你跟我一起做过的事」。

**案例 D · 偏好学习(越用越懂你)**
> 你每次都把回答改短、要求用 Tauri+React 给例子。
> → 记忆沉淀:「这用户偏好简短、技术栈 Tauri/React」→ 之后默认就对,不用每次交代。

**案例 E · 跨 app 桥接**
> 你在终端复制一段 error,切到浏览器搜,再召唤桌宠。
> → 「5 分钟前你终端里那个 `linker command failed`,是要查这个吗?」
>
> (reactive ribbon / clipboard 已经在捕获这些片段。)

**案例 F · 主动接续未完成的事(要克制,见 UX §2.8 第 9 问)**
> 「上次让我帮你写的 notarize 脚本后来跑通了吗?需要接着弄吗?」

把 A-F 串起来,记忆带来的核心体验是一句话:**「它记得我们一起做过什么,所以不用每次从零开始,而且越用越懂我。」**

### 2.2 隐私模型(红线 + 默认)

这是整个功能的地基,先定死:

- ✅ **只从交互时刻建记忆**:素材 = `sessions.jsonl` 已有的东西(召唤时的 app 标题 + 截图 + 语音 + 回复)。零新增采集。符合 MouseClaw「纯后台、平时不存在」哲学。
- ❌ **不做全天候 keylogging / 屏幕录制 / 记录你做了什么操作**。CLAUDE.md 明确把「读键盘内容」列为隐私红线。「记得你操作了什么」如果指持续监控,**不做**。
- ✅ **纯本地**:DB 落 `~/.mouseclaw/memory.db`,和 `sessions.jsonl` / clipboard 一个级别,**不上云**。
- 🤔 **加密待决策**:`clipboard_crypto.rs` 已有加密能力。记忆 DB 里的文本敏感度 ≈ 现有 `sessions.jsonl`(目前明文)。是否对 memory.db 也加密 = 开放决策点(§2.9)。

> **「app 使用时间线」选项(你问卷里的中间档)未选,故本设计只做交互时刻记忆。**
> 若以后想加「记录前台 app 切换时间线」,那是 opt-in + 桌宠常驻「记录中」可见指示的独立功能,不混进这一版。

### 2.3 正面回答:「不用向量行不行?」—— 行,而且对这个产品更合适

你的直觉对:本地向量模型(如 bge-small)≈ 100MB+ 且每次检索要跑推理,顶着你
`<600MB 内存 / 待机<400MB` 的预算,sherpa ASR + 标点已经占了大头,再塞个 embedding 模型不划算。

**核心思路:把「语义理解」从读取期(向量相似度)挪到写入期(LLM 抽取)。**

我们本来就有 AI backend + 串行队列(`ai_queue`)。每次一轮交互结束,**花一次便宜的纯文本 LLM 调用**(`backend::ask_text_only`),把这轮提炼成结构化的东西:

- **实体**:项目、app、文件、话题、工具、人名
- **一行摘要**:这轮在干什么
- **意图/结果**:asked / solved / unfinished
- **偏好信号**:用户是否要求更短 / 指定了技术栈 / 纠正了语气

这些存成 SQLite 里的**图节点 + 边**。于是**读取期变成纯 SQL,零模型**:

1. **FTS5 全文检索**(SQLite 内置,CJK 用 `trigram` tokenizer)—— 关键词/片段召回
2. **图遍历**:从当前 app/项目节点 → 相关话题/历史(1-2 跳 SQL join)
3. **时间衰减 + 频次**加权

这就是「Graph RAG」,只不过**图谱本身就是 SQLite 的表**(nodes 表 + edges 表 + FTS5 虚表),
检索靠「关键词 + 图遍历」,**不需要向量模型**。准确度来自「LLM 在写入期已经把每条记忆嚼成了干净的结构化标签」。

**向量到底什么时候才有用?**(诚实交代它的边界)
当查询和历史**语义相近但用词完全不同**、且写入期 LLM 标签没覆盖到时。例:
「怎么让程序跑快点」要匹配过去一条「优化 render loop」—— 没有共享关键词,FTS5 会漏,向量能抓。
**不上向量的兜底**:检索前先让 LLM 把 query 扩展成同义关键词组(这步本来就在 AI 管道里),
再喂 FTS5。这能补掉大部分缺口。所以:

> **结论:v1 = SQLite(FTS5 + 关系图表) + 写入期 LLM 抽取 + 查询期 LLM 扩词。
> 三者合起来逼近向量召回,零额外模型、零额外常驻内存。向量是 v2 的可选增强,不是 v1 必需。**

直接回答你「是不是可以用 sqlite 呢」:**对,SQLite 一个文件同时当「图存储 + 全文索引」,这正是让它「轻但准」的关键。**

### 2.4 SQLite Schema

```sql
-- 一轮交互的记录(可从 sessions.jsonl 迁移/镜像进来)
CREATE TABLE memory_turn (
  id         INTEGER PRIMARY KEY,
  session_id INTEGER,
  ts         INTEGER,        -- unix epoch
  app        TEXT,           -- 前台 app / 窗口标题
  role       TEXT,           -- user / assistant
  text       TEXT,
  screenshot TEXT,           -- 截图路径(文件本身不进 DB)
  summary    TEXT            -- LLM 写入期抽取的一行摘要(可空,失败则降级)
);

-- 实体节点(项目/app/文件/话题/工具/人)
CREATE TABLE entity (
  id         INTEGER PRIMARY KEY,
  kind       TEXT,           -- project | app | file | topic | tool | person
  name       TEXT,
  first_seen INTEGER,
  last_seen  INTEGER,
  freq       INTEGER DEFAULT 1,
  UNIQUE(kind, name)
);

-- 边:turn↔entity(提及)、entity↔entity(共现)、turn↔turn(时序接续)
CREATE TABLE edge (
  src    INTEGER,
  dst    INTEGER,
  kind   TEXT,               -- mentions | co_occurs | followed_by | about
  weight REAL DEFAULT 1,
  ts     INTEGER
);

-- 全文检索虚表(CJK + 英文都靠 trigram,无需中文分词器/外部依赖)
CREATE VIRTUAL TABLE memory_fts USING fts5(
  text, summary, app,
  content='memory_turn', content_rowid='id',
  tokenize='trigram'
);

-- 用户偏好沉淀(案例 D)
CREATE TABLE preference (
  key        TEXT PRIMARY KEY,  -- answer_length | tech_stack | tone | ...
  value      TEXT,
  confidence REAL,
  updated    INTEGER
);
```

体积:SQLite 本体编译进二进制 ~1-2MB;DB 文件只存文本,增长缓慢(截图仍是磁盘文件路径)。
**无模型加载,稳稳在 600MB 预算内。**

### 2.5 写入期:抽取管道(必须守 `ai_queue` 串行硬规则)

```
一轮交互结束(pipeline 拿到 reply)
   → 把原始 turn 立刻写 memory_turn(即使抽取失败也保底可搜)
   → 排一个【低优先级】抽取任务:
        ai_queue::acquire()  ← 必须走队列,绝不抢用户任务的 CPU
        backend::ask_text_only(抽取 prompt)  ← lower_priority(nice+10)
        解析 JSON → upsert entity / edge / 回填 summary / 更新 preference
```

- **必须走 `ai_queue`**(CLAUDE.md AI 串行硬规则):抽取是后台 AI 任务,不能和用户召唤抢。
  设计成**最低优先级**:仅在队列没有用户发起的任务时才跑(或空闲批处理积压的未抽取 turn)。
- **抽取失败降级**:LLM 调用挂了 → 这条 turn 只有原始 text,FTS5 照样能搜,图谱缺这一条而已,不阻塞任何东西。
- **多后端**:抽取走 `backend::ask_text_only(Config::load().backend, prompt)`,4 个后端通用,不裸调 `claude`。

### 2.6 读取期:检索(纯 SQL,零模型)

```sql
-- 当前 app=X、query 关键词=K(经 LLM 扩词)→ 召回相关历史
SELECT t.id, t.summary, t.ts, t.app
FROM   memory_fts f
JOIN   memory_turn t ON t.id = f.rowid
WHERE  memory_fts MATCH :keywords
ORDER  BY bm25(memory_fts) - (:now - t.ts) * :recency_decay   -- 相关度 + 时间衰减
LIMIT  8;
```

外加一跳图遍历:`当前项目节点 → 最近相关话题/未完成任务`。两路结果合并去重 → top-K。
全程本地 SQL,**<10ms,绝不拖慢召唤**。

### 2.7 遗忘 / 衰减(长期使用最怕越攒越乱 —— UX 第 10 问)

- 实体 `last_seen` 太旧 + `freq` 低 → 检索时降权(不删,只沉底)
- 提供「归档 N 天前的记忆」(从热表移走,仍可全量搜但不参与默认召回)
- 用户可手动删单条 / 清空(§2.8 第 6 问)

### 2.8 记忆功能的 UX 全流程演练(CLAUDE.md mandatory 10 问)

1. **入口可发现性**:① picker 里「记忆」开关 + 「我记得什么」查看入口;② 记忆命中时气泡顶部小标记 `🧠 结合了 3 条记忆`(对照已有的续 session `🔗`)。
2. **首次使用**:第一天记忆是空的,价值看不出来 → 首次开启时一句话说明「我会记得我们一起做过的事,用得越久越懂你;全部存在你本地,随时可看可删」。**这句必须讲清「本地 + 可删」打消顾虑。**
3. **常见操作路径**:召唤 → 自动带上相关记忆,**零额外步骤,用户无感知地受益**。查看/删除是低频但必须有。
4. **快/慢路径**:检索本地 SQL <10ms,不拖召唤;写入期抽取异步排队,不阻塞回复。慢路径(抽取积压)用户根本看不到。
5. **状态可见性**:命中标记可点开 → 看「这次用了哪几条记忆」(信任 + 可当场纠错「这条不对,删掉」)。
6. **取消/后悔**:必须能 ①删单条 ②清空全部 ③暂停记忆(类比已有的 `clipboard_paused`)。「不能删」= 不敢用。
7. **失败模式**:抽取失败→只存原始可搜;DB 损坏→重建空库不崩;记忆为空→静默不注入,体验回退到现状。
8. **可访问性/边界**:查看记忆窗口走 DESIGN.md token;中英混排测「🦞 Hello 你好 ABC」;键盘可达。
9. **会不会打扰**:案例 F「主动接续」**默认关闭或极克制**,否则烦。命中标记要轻(一个小图标,不抢戏),绝不弹窗。
10. **第一印象 vs 长期**:首日空库→靠首次说明立人设;长期→遗忘/衰减(§2.7)防止越攒越乱。

### 2.9 开放决策点(需要你拍板,我不替你默砍)

- **D1 · 记忆默认开还是关?** 隐私默认(默认关,picker 主动介绍)vs 价值默认(默认开 + 首次透明说明 + 一键关)。我倾向**默认开 + 透明 + 易关**,因为空库默认关的话用户永远发现不了价值;但这是隐私取向问题,你定。
- **D2 · 写入期抽取频率/成本**:每轮立刻抽取(实时但费一次 AI 调用)vs 空闲批处理(省、但记忆有延迟)。我倾向**空闲批处理 + 召唤时若该 turn 没抽取则即时补**。
- **D3 · memory.db 是否加密**(对齐 `clipboard_crypto` vs 跟 `sessions.jsonl` 一样明文)。
- **D4 · 案例 C 日/周回顾**:做成「问了才答」(安全)还是「主动推送」(撞第 9 问打扰边界)?我倾向**只在用户问时答**,v1 不主动推。
- **D5 · 性格预设具体几款 + 文案**:目前拟 Warm/Snarky/Minimal/Companion/Custom 5 款,你增删。

---

## 3. 视觉 + 音频 + 记忆联动(就是②的读取期表现)

现在 `build_prompt`(`claude_cli.rs`)已经拼:`语音 transcript + 截图路径 + 前台 app + 光标 + 鼠标轨迹`。
联动 = 多拼一段「记忆」block:

```
[记忆 · 仅供参考,以当前任务为准]
- 3 天前在这个项目你问过 borrow checker,结论是改用 Arc
- 你偏好简短回答、技术栈 Tauri/React
- 5 分钟前你在终端遇到 linker command failed
[/记忆]
```

**三模态怎么联动的**:
- **音频(语音 transcript)** → 经 LLM 扩词得到检索关键词
- **视觉(截图/前台 app)** → 定位「当前上下文节点」(在哪个项目/app)
- **记忆** → 用上面两者做 SQL 检索 → 注入 prompt

即:**视觉定位「在哪」、音频定位「问什么」、记忆补上「以前发生过什么」**,三者在 prompt 拼接这一步合流。
实现上是给 `build_prompt` 加一个 `memory: Option<&str>` 参数,不是新系统。

---

## 4. 分期(Phasing)

| 阶段 | 内容 | 产出 |
|---|---|---|
| **P1** | 名字 + 性格 | config v20 + `system_prompt` 注入 + picker UI + i18n + **prototype HTML(9 皮肤)** |
| **P2** | 记忆基建 | SQLite schema + sessions 迁移 + 写入期抽取(走 ai_queue)+ 读取期 SQL 检索 |
| **P3** | 记忆 UX | 「我记得什么」查看/删除窗口 + 命中标记 + 暂停开关 + 日/周回顾(问答式) + **prototype HTML** |
| **P4(可选/v2)** | 增强 | 本地向量召回兜底 + 真图谱可视化 + 主动接续 |

> 每个含 UI 的阶段,**先 prototype HTML 给你看,拍板后才写 Rust**(CLAUDE.md 硬规则)。

---

## 5. 明确不做的事

- ❌ 全天候 keylogging / 屏幕录制 / 记录所有操作(隐私红线)
- ❌ 记忆上云(全本地,对齐 sessions.jsonl / clipboard)
- ❌ v1 上本地向量模型(顶内存预算;FTS5 + 写入期抽取 + 扩词已够)
- ❌ 在 prototype 之前写任何记忆/性格相关的 UI 代码
- ❌ 性格 prompt 侵蚀任务正确性(硬约束兜底)
- ❌ 给某个 backend 写独立 system prompt(4 后端共用)

---

## 6. 待你确认后我会做的下一步

1. 你回答 §2.9 的 D1-D5(尤其 D1 默认开关、D5 性格款数)
2. 我先做 **P1 的 prototype HTML**(起名 + 选性格的 picker 界面,9 皮肤同屏)给你看
3. 同时做 **P3 记忆查看/命中标记的 prototype HTML**
4. 你对 prototype 拍板 → 进 Rust 实现,按 P1 → P2 → P3 顺序
