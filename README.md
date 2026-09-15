# Bilibili AI Bot

<div align="center">

**一个有记忆、有感情、会成长的 B站 AI 角色系统**

自动回复评论 · 收发私信 · 主动刷视频 · 发布动态 · 性格演化

在浏览器里通过 Web 面板管理一切，无需改代码

<br>

![Python](https://img.shields.io/badge/Python-3.8%2B-3776AB?style=flat-square&logo=python&logoColor=white)
![Flask](https://img.shields.io/badge/Flask-Web%20Panel-000000?style=flat-square&logo=flask&logoColor=white)
![Platform](https://img.shields.io/badge/Platform-Linux%20%7C%20Windows-4B5563?style=flat-square&logo=linux&logoColor=white)
![License](https://img.shields.io/badge/License-MIT-0F9D58?style=flat-square)

[![Repo](https://img.shields.io/badge/GitHub-LianXia233%2Fbilibili--ai--bot-181717?style=flat-square&logo=github)](https://github.com/LianXia233/bilibili-ai-bot)
[![Upstream](https://img.shields.io/badge/Fork%20of-chenluQwQ%2Fbilibili--ai--bot-6B7280?style=flat-square&logo=github)](https://github.com/chenluQwQ/bilibili-ai-bot)

[![Docs](https://img.shields.io/badge/Docs-DEPLOY.md-087CBA?style=flat-square&logo=readthedocs&logoColor=white)](DEPLOY.md)
[![Changelog](https://img.shields.io/badge/Docs-CHANGELOG.md-087CBA?style=flat-square&logo=readthedocs&logoColor=white)](CHANGELOG.md)
[![Fixes](https://img.shields.io/badge/Docs-FIXES.md-087CBA?style=flat-square&logo=readthedocs&logoColor=white)](FIXES.md)

</div>

---

## 目录

<table>
<tr>
<td valign="top" width="50%">

**使用**

- [功能一览](#功能一览)
- [Web 管理面板](#web-管理面板)
- [快速开始](#快速开始)
- [配置说明](#配置说明)

</td>
<td valign="top" width="50%">

**深入**

- [项目结构](#项目结构)
- [Cookie 自动刷新](#cookie-自动刷新)
- [安全机制](#安全机制)
- [常见问题](#常见问题)

</td>
</tr>
</table>

---

## 功能一览

<details open>
<summary><b>评论回复</b> — 自动检测新评论并生成 AI 回复</summary>

<br>

- 支持多模型（Claude / Gemini / GPT 等 OpenAI 兼容 API），主模型失败自动切换备用模型
- 识别评论中的**图片**，结合视觉模型理解内容后回复
- 识别评论所在的**视频内容**，结合上下文回复
- 触发关键词时联网搜索，回答时事类问题

</details>

<details open>
<summary><b>B站私信</b> — 在同一套人格、记忆、好感度与用户档案下回复新私信</summary>

<br>

| 行为 | 说明 |
|------|------|
| 首次开启 | 只建立当前位置，**不处理历史私信** |
| 消息类型 | 纯文本 + B站站内视频分享卡片 |
| 去重策略 | 每条消息只处理一次；发送失败也**不会自动重复发送** |
| 回复范围 | 仅主人 / 主人和白名单 / 所有安全私信 |
| 安全处理 | 不明外链、IP 链接、疑似色情引流 → 先隔离，再生成**拉黑建议**由人工确认 |
| 链接解析 | 安全判断只解析文字，**不会访问私信里的链接** |
| 命中规则 | 一律不回复、不送进 LLM，无论最终是否拉黑 |

</details>

<details open>
<summary><b>记忆系统</b> — 基于语义向量检索的长期记忆</summary>

<br>

- Embedding 模型：`BAAI/bge-m3`，按对话线程和用户分别管理
- 自动压缩过长记忆，保留关键信息
- **永久记忆**：AI 自动识别需要长期记住的重要事件
- 记忆在 B站评论与本地聊天之间**共享**

> 未配置 Embedding 时自动降级为「最近记忆」，聊天与回复不受影响，只是语义检索不生效。

</details>

<details open>
<summary><b>好感度与用户档案</b> — 每个用户独立的分数与印象</summary>

<br>

- 好感度区间 `0 ~ 100`，关系等级：陌生人 → 粉丝 → 熟人 → 好友 → 主人
- 不同关系等级对应不同回复态度与亲密程度
- AI 自动记录对每个用户的印象与关键信息
- 好感度过低或连续辱骂时**生成拉黑建议**，是否拉黑由你在面板「安全中心」确认

</details>

<details>
<summary><b>主动行为</b> — 每天在随机时间自然发生</summary>

<br>

| 行为 | 说明 |
|------|------|
| 主动刷视频 | 浏览推荐 / 关注 UP 主的视频 |
| 主动评论 | 看完视频后发表评价 |
| 互动行为 | 根据评价决定点赞 / 投币 / 收藏 / 关注 |
| 发动态 | 发布一条 B站动态（支持 AI 配图） |

所有时间随机生成，行为自然拟人。

</details>

<details>
<summary><b>性格成长 / 心情 / 多人格</b></summary>

<br>

- **性格成长**：每天自动反思当天互动，说话习惯与对事物的看法随时间动态演化
- **心情系统**：根据互动内容实时变化，影响回复的语气与风格
- **多人格**：支持创建与切换多个人格，各自独立持有系统提示词、风格提示词、主人提示词，一键切换无需重启

</details>

<details>
<summary><b>AI 画图 / 联网搜索</b></summary>

<br>

- **AI 画图**：本地聊天中支持画图（默认 Flux.2 Pro），发动态时可自动生成配图
- **联网搜索**：检测到时事类问题自动搜索，结果融入回复上下文

</details>

---

## Web 管理面板

全功能 Web 面板，桌面端与手机端完整适配。

| 面板 | 功能 |
|------|------|
| 聊天 | 与 Bot 直接对话，支持图片与 AI 画图 |
| 快捷总结 | 一键生成互动记忆总结 |
| 人格管理 | 创建 / 编辑 / 切换多个角色人格 |
| 用户管理 | 查看所有用户的好感度、档案、印象，手动拉黑指定 UID |
| **安全中心** | 待确认的拉黑建议（一键拉黑 / 忽略）、安全事件日志 |
| 记忆管理 | 浏览和搜索所有记忆，手动删除 |
| 成长日志 | 查看性格演化轨迹、说话习惯、对事物的看法 |
| 活动日志 | 今日计划、观影日记、主动评论、动态记录 |
| 费用统计 | API 调用费用按模型分类统计 |
| 系统设置 | Cookie 管理、API 配置、模型切换、功能开关、提示词编辑 |

<details>
<summary><b>面板特性</b></summary>

<br>

| 特性 | 说明 |
|------|------|
| 访问保护 | 默认拒绝 + 白名单收口；口令可用 RSA-OAEP 密封提交 |
| 配置热更新 | 改完即时生效，无需重启 |
| Cookie 管理 | B站 Cookie 状态检测 + 自动刷新（基于 `refresh_token`） |
| 健康检测 | 后端健康状态实时显示 |
| 模型测试 | 模型连接一键测试 |
| 移动适配 | 手机端完整适配 |

</details>

### 界面与主题

| 项目 | 实现 |
|------|------|
| 设计语言 | PaperGrid / schale 风格（不对称圆角、硬边偏移阴影、斜切高亮块） |
| 主题 | 明暗双主题，跟随系统 `prefers-color-scheme`，也可手动切换 |
| 角色立绘 | 亮色主题 Arona、暗色主题 Plana，两帧差分实现眨眼动效 |
| 透明度 | 由 CSS 变量 `--char-opacity` 统一控制，移动端自动缩小压暗 |
| 卡片 | 半透明毛玻璃（glassmorphism），让壁纸透出而不抢内容可读性 |
| 资源 | 壁纸与立绘放在 `static/img/`，换成自己的图片即可，无需改代码 |

---

## 项目结构

```
bilibili-ai-bot/
├── ai.py               # 主程序：评论/私信监听、主动行为调度、记忆管理
├── private_messages.py # 私信轮询、发送、去重与安全判断
├── bili_login.py       # B站二维码登录
├── Proactive.py        # 主动刷视频 + 评论模块
├── dynamic.py          # 动态发布模块
├── local-chat.py       # Flask Web 面板 + 本地聊天后端
├── chat.html           # Web 前端（聊天 + 管理面板）
├── static/             # 前端静态资源（背景图、角色立绘、Logo）
├── config.py           # 配置管理（热更新、Cookie 刷新）
├── config.json         # 运行时配置（自动生成，勿上传）
├── config.example.json # 配置示例
├── Requirements.txt    # Python 依赖
├── data/               # 运行时数据（记忆、好感度、日志等）
├── tests/              # 单元测试
├── DEPLOY.md           # 生产部署（systemd / 反向代理 / 安全加固）
├── CHANGELOG.md        # 更新日志（相对上游的变更，唯一变更记录）
├── FIXES.md            # 问题根因与排查记录
└── README.md
```

---

## 快速开始

### 1. 环境准备

| 项 | 要求 |
|------|------|
| Python | 3.8+ |
| B站账号 | 可在面板扫码登录，也可手动填写 Cookie |
| AI API Key | 任何兼容 OpenAI 格式的 API 均可 |
| Embedding API Key | 可选，用于记忆语义检索 |

### 2. 安装

```bash
git clone https://github.com/LianXia233/bilibili-ai-bot.git
cd bilibili-ai-bot
pip install -r Requirements.txt
```

### 3. 配置

首次运行会自动生成 `config.json`，也可以提前复制示例：

```bash
cp config.example.json config.json
```

最少只需填 4 项：

```json
{
  "SESSDATA": "你的B站SESSDATA",
  "BILI_JCT": "你的bili_jct",
  "OR_API_KEY": "你的API Key",
  "OR_CHAT_MODEL": "你选择的对话模型ID"
}
```

> 推荐启动面板后，在「系统设置 → B站 Cookie」点击扫码登录。也可以粘贴浏览器复制出的整段 Cookie，面板会自动拆分。

`OR_BASE_URL` 和模型 ID 取决于你使用的 API 提供商，填入对应的地址和模型名即可。其余配置都有默认值，可通过 Web 面板随时修改。

### 4. 启动

```bash
# 启动评论监听 + 主动行为（后台运行建议用 tmux 或 screen）
python ai.py

# 启动 Web 面板（另开一个终端）
python local-chat.py
```

访问 `http://你的IP:5000`，默认密码 `admin()`。

> 生产环境不要停留在默认口令上，部署细节见 [DEPLOY.md](DEPLOY.md)。

---

## 配置说明

所有配置都可以通过 Web 面板修改，无需编辑文件。以下是主要配置项。

### B站 Cookie

| 配置项 | 说明 |
|--------|------|
| `SESSDATA` | B站登录凭证 |
| `BILI_JCT` | CSRF Token |
| `DEDE_USER_ID` | 用户 UID |
| `OWNER_MID` | 主人的 UID（好感度永远 100） |
| `REFRESH_TOKEN` | 用于自动刷新 Cookie（可选但推荐） |

> 获取 `refresh_token`：B站网页 `F12` → Console → 输入 `localStorage.getItem('ac_time_value')`

### AI 模型

支持 4 种模型，每种可独立配置 API 地址和 Key。

| 模型类型 | 用途 | 配置项 |
|---------|------|--------|
| 对话模型 | 评论回复、本地聊天 | `OR_CHAT_MODEL` |
| 视觉模型 | 识别图片内容 | `OR_VISION_MODEL` |
| 搜索模型 | 联网搜索回答 | `OR_SEARCH_MODEL` |
| 图片生成 | AI 画图、动态配图 | `OR_IMAGE_MODEL` |

- 每种模型都支持设置**备用模型**（`_FALLBACK` 后缀），主模型失败自动切换
- 每种模型还可以单独配置 `_URL` 和 `_KEY`，留空则使用全局默认值，因此可以混合使用不同提供商的模型

#### 对话模型池（多套配置一键切换）

对话是最常换模型的场景（不同网关的额度、限速、价格差别很大）。面板「💬 对话模型」
卡片顶部提供**模型池**：把常用的几套 API / 模型存进去，点「启用」即刻切换，Bot 无需重启。

- 池中每条是一个完整配置：`名称 / Base URL / API Key / 模型 ID / 回退模型`
- **启用某条时，该条整体取代下方的单套配置**；点「停用池」则回落到单套配置
- 切换后立即生效（Bot 每轮读盘），面板顶部会显示「当前生效：xxx」
- API Key 只以掩码回显；保存时若该字段仍是掩码，则保留原 Key 不变
- 至少 5 个空位，可继续新增（上限 20 条）
- 对应配置项：`CHAT_MODEL_POOL`（数组）、`CHAT_MODEL_ACTIVE`（下标，`-1` = 不用池）

#### 永久记忆与记忆清空

永久记忆保存 Bot 的**人格与行为规则**，只支持在面板「💎 永久记忆」里手动添加，
Bot 不会自动写入。上限 40 条，注入提示词时不截断。

- 「📥 批量导入整合」可将整理好的规则整体替换现有内容（每行一条，自动去重）
- 「🗑 清空永久记忆」只清永久记忆，不影响人格配置、对话记忆与好感度
- 「设置 → 💾 数据管理」提供分项清空与**一键清空全部记忆**（覆盖永久记忆、对话记忆、
  用户档案、好感度、视频缓存、性格演化、当日心情），每个文件清空前自动备份


### 功能开关

| 开关 | 说明 | 默认 |
|------|------|:----:|
| `ENABLE_WEB_SEARCH` | 联网搜索 | 开 |
| `ENABLE_PROACTIVE` | 主动刷视频和评论 | 开 |
| `ENABLE_DYNAMIC` | 自动发动态 | 开 |
| `ENABLE_PERSONALITY_EVOLUTION` | 性格成长 | 开 |
| `ENABLE_MOOD` | 心情系统 | 开 |
| `ENABLE_AFFECTION` | 好感度系统 | 开 |
| `ENABLE_PRIVATE_MESSAGES` | 接收 B站新私信（首次开启跳过历史） | 关 |
| `PRIVATE_MESSAGE_AUTO_REPLY` | 用当前人格自动回复安全私信 | 开 |
| `PRIVATE_MESSAGE_AUTO_BLOCK` | 危险私信直接调用 B站拉黑；关闭时只隔离并生成建议 | 关 |
| `AUTO_BLOCK_ON_AFFECTION` | 好感度过低 / 连续辱骂直接拉黑；关闭时只生成建议 | 关 |

### 私信安全

| 配置 | 说明 | 默认 |
|------|------|------|
| `PRIVATE_MESSAGE_REPLY_SCOPE` | 回复范围：`all` / `owner` / `whitelist` | `all` |
| `PRIVATE_MESSAGE_REPLY_WHITELIST_UIDS` | `whitelist` 模式下可回复的 UID | `[]` |
| `PRIVATE_MESSAGE_BLOCK_WHITELIST_UIDS` | 永不自动拉黑的 UID；主人和 Bot 自己始终受保护 | `[]` |
| `PRIVATE_MESSAGE_TRUSTED_DOMAINS` | 私信允许出现的域名及其子域名 | `bilibili.com,b23.tv` |
| `PRIVATE_MESSAGE_MAX_MESSAGE_AGE` | 忽略超过该秒数的消息 | `3600` |
| `PRIVATE_MESSAGE_MAX_PER_POLL` | 单轮最多处理的私信数 | `3` |

> **关于「自动拉黑」**：这属于真实账号操作。私信总开关默认关闭；正式开启前请先正确填写 `OWNER_MID` 和免拉黑名单。关闭自动拉黑后，命中规则的消息仍会被隔离，不送进 LLM 也不回复；命中的用户会以「拉黑建议」形式出现在面板「安全中心」，由你点按钮决定是否拉黑。

### 行为控制

| 配置 | 说明 | 默认 |
|------|------|------|
| `PROACTIVE_VIDEO_COUNT` | 每天刷几个视频 | 3 |
| `PROACTIVE_COMMENT_COUNT` | 每天评论几条 | 2 |
| `PROACTIVE_TIMES_COUNT` | 每天触发几次 | 2 |
| `ENABLE_SLEEP` | 休眠总开关，`false` = 全天在线，不看时段 | `false` |
| `SLEEP_START` ~ `SLEEP_END` | 休眠时间段（总开关打开后才生效） | 2:00 ~ 8:00 |

> 休眠默认**关闭**（`ENABLE_SLEEP=false`，机器人 24 小时在线），面板「调度参数」里有对应勾选框。打开后才按 `SLEEP_START` ~ `SLEEP_END` 判定；`24 / 0` 也表示全天活跃，但**不要用 `0/0`** —— 该组合会走跨午夜分支得到恒假条件，等于永久休眠。详见 [FIXES.md 第十六节](FIXES.md#十六休眠总开关与模型-tpm-限速)。

### Token 预算

面板「Token 预算」卡片可按场景调整 `max_tokens`，无需改代码。**换用推理型模型（先思考、再回答）时最需要它**：这类模型的预算是「思考过程 + 正文」共用的，填太小会被思考过程吃光，正文为空、日志报「模型返回空正文」或 `Expecting value`。

| 配置 | 覆盖场景 | 默认 |
|------|----------|:----:|
| `MAX_TOKENS_CHAT` | Bot 对话、面板聊天、面板记忆总结 | 3000 |
| `MAX_TOKENS_REPLY` | 评论回复 / 私信回复 | 3000 |
| `MAX_TOKENS_MEMORY_COMPRESS` | 记忆压缩（摘要 + 标签 + 用户事实） | 3000 |
| `MAX_TOKENS_THREAD_COMPRESS` | 历史线程压缩 | 1000 |
| `MAX_TOKENS_EVOLVE` | 性格演化 | 3000 |
| `MAX_TOKENS_SEARCH` | 联网搜索 | 3000 |
| `MAX_TOKENS_VISION` | 视频 / 截图理解 | 4096 |
| `MAX_TOKENS_RECOGNIZE` | 评论配图识别 | 4096 |
| `MAX_TOKENS_DYNAMIC` | 动态文案 | 2000 |
| `MAX_TOKENS_PROACTIVE_COMMENT` | 主动评论 / 推荐语 | 2000 |
| `MAX_TOKENS_IMAGE_PROMPT` | 生图 prompt 精炼 | 1000 |
| `MAX_TOKENS_REASONING_FLOOR` | 预算被吃光时的重试上限，**0 = 不重试** | 6000 |

三点行为约定：

1. **留空 = 沿用默认值**。面板输入框留空则该项不写入 `config.json`，由 `config.py` 的默认值兜住。
2. **改完立即生效**，不需要重启 Bot（`get_max_tokens()` 每次读盘，不是启动时快照）。
3. **调大只抬高上限**，不会凭空增加费用 —— 实际计费仍按模型真实产出的 token 数。

> 面板「测试连接」所用的极小预算（图片模态探测 1 token、文本通道探测 5 token）刻意不纳入配置：它们只验证通道是否可用，调大只会拖慢测试。详见 [FIXES.md 第十二节](FIXES.md#十二模型返回空正文从缓解到根治)。

### 模型速率限制（TPM）

面板「🚦 模型速率限制」卡片按场景限制每分钟 token 数，用来兜住网关侧配额（超出会返回 429）。

| 配置 | 覆盖场景 | 默认 |
|------|----------|:----:|
| `RATE_LIMIT_CHAT_TPM` | 对话回复、视频信息文本归纳 | 1000000 |
| `RATE_LIMIT_SEARCH_TPM` | 联网搜索 | 1000000 |
| `RATE_LIMIT_VISION_TPM` | 视频封面 / 评论配图读图 | 0（不限） |
| `RATE_LIMIT_IMAGE_TPM` | 生图 prompt | 0（不限） |

**`0` 表示不限。** 视觉 / 生图类默认不限：OCR 单次输出只有一两百 token，
限流换不来配额保护，只会让视频分析平白多等一个窗口。

与 Token 预算同样是**每次读盘**，改完立即生效；面板留空 = 沿用默认值。
窗口按固定 60 秒滑动，累计将超限时先等到最早一笔滑出再发请求。
机制与坑见 [FIXES.md 第十六节](FIXES.md#十六休眠总开关与模型-tpm-限速)。

### 自定义提示词

面板中可编辑 7 个提示词模板：

| 提示词 | 用途 |
|------|------|
| 对话回复提示词 | 在人格中配置 |
| 主动评论提示词 | 刷完视频发表评价 |
| 视频评价提示词 | 决定互动行为 |
| 性格演化提示词 | 每日反思 |
| 搜索前缀提示词 | 联网搜索结果融合 |
| 动态发布提示词 | 发布 B站动态 |
| AI 画图提示词 | 生成图片 |

---

## Cookie 自动刷新

B站 Cookie 会定期过期，本项目支持全自动刷新。

1. 在面板扫码登录（会自动保存 `REFRESH_TOKEN`），或手动填入
2. 后台每 6 小时自动检查 Cookie 状态
3. B站提示需要刷新时，自动用 RSA 加密完成 5 步刷新流程
4. 新 Cookie 自动写入配置，无需人工干预

也可以在面板中手动点击「自动刷新」按钮。

---

## 安全机制

### 账号侧（对 B站）

| 机制 | 说明 |
|------|------|
| 关键词过滤 | 自动屏蔽包含不良关键词的评论 |
| 好感度惩罚 | 辱骂性评论扣减好感度 |
| **拉黑建议（默认）** | 好感度降至 `-30` 或连续辱骂 5 次，只写入安全日志并生成建议，**不调用 B站 API**；是否拉黑由你在面板「安全中心」点按钮确认 |
| 自动拉黑（可选） | 把 `AUTO_BLOCK_ON_AFFECTION` / `PRIVATE_MESSAGE_AUTO_BLOCK` 置为 `true` 可恢复旧行为，让 Bot 直接调用 B站 API 拉黑 |
| 安全日志 | 屏蔽、隔离、拉黑、建议全部记录在案，可在面板分页翻阅 |

### 面板侧（对访问者）

| 机制 | 说明 |
|------|------|
| 默认拒绝 | 除登录页与少量静态资源外，其余路径（含 `/api/*`、`/data/**`）一律先过鉴权；未登录返回 `401`，敏感路径直接 `404`，不泄露资源是否存在 |
| 口令密封 | `/api/handshake` 下发 RSA 公钥，前端用 WebCrypto 以 RSA-OAEP(SHA-256) 加密口令后提交，链路上只有密文；明文通道作为降级回退保留 |
| 会话加固 | `HttpOnly` + `SameSite=Lax`，`Secure` 由「是否经 TLS 访问」动态判定；会话密钥持久化在 `data/.secret_key`（`0600`），重启不掉线 |
| 反代感知 | 仅当请求来自回环地址时才采信 `X-Forwarded-Proto`，据此决定是否发 HSTS |
| 上传与路径 | `/media/bot-avatar` 不接受文件名参数，只回配置里指定的那一张，拒绝路径分隔符与 `..`，并统一缩略图输出 |

> 口令密封依赖浏览器安全上下文（`https://` 或 `localhost`）。若面板直接以明文 HTTP 暴露在公网，浏览器不提供 WebCrypto，前端会自动回退到明文提交。**生产环境请务必在面板前放一层 TLS**（反向代理或内网穿透隧道均可），具体做法见 [DEPLOY.md](DEPLOY.md)。

---

## 手机端

Web 面板完整适配手机浏览器：

- 侧边栏滑出式菜单
- 聊天界面全屏优化
- 设置表单触屏友好
- 兼容 iPhone SE 等小屏设备
- 底部输入框避让地址栏、软键盘与 Home Indicator（`visualViewport` 同步真实可视高度 + `safe-area-inset-bottom`，见 [FIXES.md 第十三节](FIXES.md#十三移动端底部输入框被遮挡)）

---

## 兼容性

### API 提供商

任何兼容 **OpenAI API 格式**的提供商均可使用，包括但不限于：

- 各类 API 聚合平台
- 各大模型官方 API（Claude、Gemini、GPT、Qwen 等）
- 自建 API 代理 / 中转站

只需在面板中填入对应的 `Base URL`、`API Key` 和 `模型 ID` 即可。

### 模型选择建议

| 模型类型 | 选择要点 |
|---------|---------|
| 对话模型 | 角色扮演和中文能力强的模型效果更好 |
| 视觉模型 | 需要支持图片输入的多模态模型 |
| 搜索模型 | 需要支持联网搜索（online）的模型 |
| 图片生成 | 需要支持图片输出的模型 |
| Embedding | 支持中文的 embedding 模型（用于记忆检索） |

> **选推理型模型时**：这类模型会先产出思考过程再产出正文，两者共用 `max_tokens`。默认预算是按普通模型估的，换成推理型后若日志出现「返回空正文」或 `Expecting value`，请到面板「Token 预算」把对应场景调大（对白类建议 3000；视觉 / OCR 场景别超过 4096 —— 部分网关在 8192 会直接返回 500）。机制与排查手法见 [FIXES.md 第十二节](FIXES.md#十二模型返回空正文从缓解到根治)。

---

## 常见问题

<details>
<summary><b>Cookie 多久过期一次？</b></summary>

<br>

有效期会随账号和 B站策略变化。扫码登录会同时保存 `refresh_token`；需要刷新时可在面板续期，失效后重新扫码即可。

</details>

<details>
<summary><b>不填 Embedding API Key 会怎样？</b></summary>

<br>

记忆系统的语义检索功能不可用，但其他功能正常。

</details>

<details>
<summary><b>可以用免费模型吗？</b></summary>

<br>

可以，只要兼容 OpenAI API 格式就行。但回复质量和角色扮演能力取决于模型本身的能力。

</details>

<details>
<summary><b>面板能登录，但一发消息就提示「检查后端是否在运行」？</b></summary>

<br>

先看 `data/` 同级目录下后端日志里的真实异常。历史上出现过一类「假网络故障」：

Embedding 模型没配置（`EMBED_MODEL` 为空串）导致记忆检索抛异常，把聊天接口打成 `500`，前端解析不到 JSON 就报成网络错误。

现已降级处理 —— 未配置或调用失败时自动退化为「最近记忆」，聊天不受影响，只是语义检索不生效。

</details>

<details>
<summary><b>语义记忆检索为什么没效果？</b></summary>

<br>

需要填 `EMBED_MODEL` 与 `EMBED_BASE_URL`（以及对应的 `SILICON_API_KEY`），且该接口要真的提供 embedding 模型。没配也不影响聊天和回复，只是记忆按时间取最近几条。

</details>

<details>
<summary><b>怎么让 Bot 只回复特定视频的评论？</b></summary>

<br>

目前 Bot 会监听你账号下所有视频的新评论。如需限制范围，可修改 `ai.py` 中的评论获取逻辑。

</details>

<details>
<summary><b>启动后没有回复评论？</b></summary>

<br>

按顺序检查：

1. Cookie 是否有效（面板中检查状态）
2. 休眠总开关是否被打开（`ENABLE_SLEEP`，默认 `false` = 全天在线；打开后才按 `SLEEP_START` ~ `SLEEP_END` 判定）
3. 是否有新评论（Bot 只回复启动后的新评论）
4. 终端日志是否有报错 —— 尤其注意 `NameError` 这类只在特定分支才暴露的漏导入

</details>

---

## License

[MIT License](LICENSE) — 随便用，随便改。

---

<div align="center">

**让每个 B站 UP 主都能拥有一个有记忆、有感情、会成长的 AI 伙伴。**

<br>

[更新日志](CHANGELOG.md) · [问题排查详解](FIXES.md) · [生产部署指南](DEPLOY.md)

</div>
