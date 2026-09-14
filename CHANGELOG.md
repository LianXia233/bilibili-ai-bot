# 更新日志

<div align="center">

本仓库 fork 自 [chenluQwQ/bilibili-ai-bot](https://github.com/chenluQwQ/bilibili-ai-bot)，本文件记录**相对上游的变更**

格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，提交信息遵循 [Conventional Commits](https://www.conventionalcommits.org/)

![Status](https://img.shields.io/badge/status-maintained-0F9D58?style=flat-square)
![Convention](https://img.shields.io/badge/commits-Conventional%20Commits-FE5196?style=flat-square)
![Docs](https://img.shields.io/badge/changelog-single%20source%20of%20truth-087CBA?style=flat-square)

</div>

> 这里是本项目**唯一的更新日志文件**。问题根因与排查手法另见 [FIXES.md](FIXES.md)，生产部署见 [DEPLOY.md](DEPLOY.md)。

## 变更索引

| 日期 | 类型 | 标题 | 影响面 |
|------|------|------|--------|
| 2026-09-14 | `fix(bot)` | 评论中 @ Bot 不触发回复：补拉「@我的」消息流 | 故障修复 |
| 2026-09-14 | `fix(ui)` | 移动端汉堡按钮随面板消失，切面板后无法回到侧栏 | 故障修复 |
| 2026-09-14 | `fix(ui)` | 移动端点菜单后遮罩压住侧栏导致无法操作 | 故障修复 |
| 2026-09-14 | `feat(security)` | 关闭自动拉黑，改为「拉黑建议 + 人工确认」 | 行为变更 |
| 2026-09-14 | `fix(panel)` | 首页聊天 500：Embedding 不可用改为降级 | 故障修复 |
| 2026-09-14 | `feat(security)` | 面板口令密封、代理感知 TLS 与默认拒绝鉴权 | 安全加固 |
| 2026-09-14 | `fix(panel)` | Bot 头像 404 与静态目录收口 | 故障修复 |
| 2026-09-14 | `feat(ui)` | 角色立绘壁纸与玻璃拟态 | 界面 |
| 2026-09-14 | `chore(deploy)` | systemd + 内网穿透隧道部署 | 部署 |
| 2026-09-14 | `fix(bot)` | 修复私信与评论完全不回复等问题 | 故障修复 |
| 2026-09-14 | `feat(ui)` | WebUI 改用 PaperGrid / schale 设计语言 | 界面 |
| 2026-09-14 | `chore` | 行尾规范与忽略规则 | 工程 |
| 2026-09-14 | `docs` | 修复记录与部署注意事项 | 文档 |

---

## [2026-09-14]

### fix(bot) — 评论中 @ Bot 不触发回复：补拉「@我的」消息流

现象：别人在评论区 @ 机器人后毫无反应；只有「回复机器人自己的评论」才会被处理。

根因是 B站把两类消息拆成两个独立接口，而代码只轮询了其中一个：

| 消息类型 | 接口 | 修复前 |
|----------|------|--------|
| 回复我的评论 | `/x/msgfeed/reply` | 已轮询 |
| 在评论里 @ 我 | `/x/msgfeed/at` | 从未拉取（问题所在） |

改动：

| 项 | 说明 |
|------|------|
| 新增 `get_new_at_replies()` | 拉取「@我的」消息并归一成与 reply 流相同的字段结构 |
| 两条流合并去重 | `_merge_pending()` 按 `rpid` 去重，同一条评论不会被回复两次 |
| 只回真的 @ 到我 | 要求 `at_details` 包含本账号 mid（实测 100 条样本全部满足） |
| 剥掉 @ 昵称 | 昵称取自 `at_details` 精确替换，不用正则在含空格的昵称上切错 |
| 新增 `AT_REPLY_MAX_AGE` | 默认 3600 秒；消息流固定返回最新 N 条且不读即不消，不设时效会在首次启用时对历史 @ 补发一批回复 |
| 日志区分来源 | `📩 [回复]` / `📩 [被@]`，且 @ 消息附带原文，便于排查 |

字段差异（实测 100 条样本）：

| 字段 | `/msgfeed/reply` | `/msgfeed/at` |
|------|------------------|---------------|
| `root_id` | 真实根评论 id | **恒为 0**（被 @ 的评论本身就是根评论） |
| `at_details` | 多为空数组 | **恒非空**，含该评论 @ 到的全部用户 |
| 时间字段 | `reply_time` | `at_time` |

生产验证：重启后当轮识别 3 条时效内 @（另有 17 条超时效跳过），3 条回复全部发出，异常日志 0 行。

> 根因分析、字段取证过程与排查手法见 [FIXES.md](FIXES.md#七评论中--不触发回复)。

### fix(ui) — 移动端汉堡按钮随面板消失，切面板后无法回到侧栏

修完遮罩层级后实测发现的第二个问题：手机上从「聊天」切到任何其他面板（快捷总结、安全中心、记忆管理……）后，**左上角的汉堡按钮不见了**，用户再也打不开侧栏，只能刷新页面回到聊天面板。

根因是按钮的**归属位置**错了，不是样式问题：

| 项 | 修复前 | 修复后 |
|------|--------|--------|
| 汉堡按钮所在容器 | `#panel-chat` 内的 `.chat-header` | `.main` 下新增的 `.mobile-topbar`（所有面板之外） |
| 切到非聊天面板时 | `#panel-chat` 变 `display: none`，按钮随之消失 | 顶栏与面板切换完全解耦，始终存在 |
| `.panel` 高度策略 | `height: 100%` | `flex: 1; min-height: 0`（配合顶栏，正确填充剩余高度） |
| 顶栏标题 | 无 | 跟随当前面板（取自侧栏导航项文案） |

改动清单：

- 新增 `.mobile-topbar`：桌面端 `display: none`，移动端 `display: flex`，内含汉堡按钮 + 当前面板标题
- 移除 `.chat-header` 内原有的汉堡按钮，避免移动端聊天面板出现两个按钮
- 新增 `syncMobileTopbar(name)`，由 `switchPanel()` 调用并同步顶栏标题；标题优先取侧栏导航项文案（用户看到的菜单名，与内部英文命名解耦），取不到时回退到面板 `.panel-header h2`
- 页面加载时初始化一次顶栏标题

> 之所以把顶栏放在所有面板之外、而不是给 15 个面板各补一个按钮：后者需要改 15 处，且将来新增面板时极易遗漏，问题会以完全相同的形式复现。

<details>
<summary><b>验证方式</b></summary>

<br>

移动视口（390×844）走真实公网入口，逐面板断言（聊天 / 快捷总结 / 安全中心 / 记忆管理 / 系统设置）：

- 顶栏可见（390×47）且汉堡按钮可点（36×28）
- 顶栏标题与当前面板一致（含聊天面板 —— 它没有 `.panel-header`，是标题取值逻辑的边界用例）
- 点汉堡按钮后侧栏打开、遮罩显示，且侧栏中央 `elementFromPoint` 返回 `.nav-item` 而非遮罩
- 点侧栏里的另一导航项能成功切面板，遮罩同步关闭、侧栏同步收起、标题同步更新
- 布局断言：`顶栏 y + 面板高度 == 视口高`、`documentScrollHeight == innerHeight`（无纵向溢出）
- 桌面端回归：顶栏与汉堡按钮均 `display: none`，侧栏仍为 `position: static`、宽 280px，遮罩 `display: none`，无横向溢出

</details>

### fix(ui) — 移动端点菜单后遮罩压住侧栏导致无法操作

手机浏览器上点汉堡按钮打开侧栏后，页面出现一层「看不见的墙」：侧栏看得见却点不动，任何导航项都无响应，只能刷新页面。

根因是 **CSS 层叠上下文**嵌套错位，而非样式缺失。

| 项 | 修复前 | 修复后 |
|------|--------|--------|
| `.mobile-overlay` 的 DOM 位置 | `<body>` 直接子元素（与 `.app` 平级） | `.app` 的最后一个子元素 |
| `.mobile-overlay` 的 `position` | `fixed` | `absolute`（相对 `.app`，尺寸仍为满屏） |
| 移动端 `.sidebar` 的 `z-index` | `100` | `110` |
| 层叠比较对象 | 遮罩(90) vs `.app`(1) → 遮罩胜 | 遮罩(90) vs 侧栏(110) / `.main`(0) |

`.app` 声明了 `position: relative` + `z-index: 1`，这会**创建层叠上下文**。于是 `.sidebar` 的 `z-index: 100` 只在 `.app` 内部有效，对外只体现 `.app` 的 `z-index: 1`。遮罩作为兄弟节点，`90 > 1`，**永久压在包含侧栏在内的整个 `.app` 之上**。把遮罩移入 `.app` 后，它才与侧栏、`.main` 处于同一个层叠上下文，层级比较才有意义。

最终层级：`壁纸(0) < .main(0) < 遮罩(90) < 侧栏(110)` —— 遮罩盖住主内容、不盖侧栏。

<details>
<summary><b>验证方式</b></summary>

<br>

移动视口（390×844，`is_mobile` + `has_touch`）走真实公网入口，登录后逐点断言：

- 层级断言：打开侧栏后 `elementFromPoint` 在侧栏中央返回 `DIV.nav-item`（修复前返回 `DIV.mobile-overlay`），遮罩的 `parentElement` 为 `DIV.app`（修复前为 `BODY`）
- 交互断言：`page.click('.sidebar .nav-item')` 成功（修复前 Playwright 报 `intercepts pointer events` 超时），且遮罩与侧栏同步收起
- 功能保留断言：遮罩显示时主内容区中央仍命中遮罩，说明它依然拦截主内容点击，没有退化成无效元素
- 重复打开、二次点击、无 JS 报错

</details>

<details>
<summary><b>附带修正：改模板必须重启进程</b></summary>

<br>

`local-chat.py` 以 `debug=False` 运行（`app.run(..., debug=False, ...)`），且通过 `Flask(__name__, template_folder=".")` 直接加载仓库根目录的 `chat.html`。此时 Jinja 会**缓存已编译模板**，仅替换磁盘上的 `chat.html` 不会生效，必须 `systemctl restart bilibili-panel`。

本次排查中曾据此误判为「浏览器缓存」，实际是服务端模板缓存：回源 `GET /` 返回的 HTML 长度与旧版一致，重启后立即变为新版。

</details>

### feat(security) — 关闭自动拉黑，改为「拉黑建议 + 人工确认」

自动拉黑是真实账号操作，命中阈值即封人，事后很难补救。本次把决策权收回人工。

| 改动 | 说明 |
|------|------|
| 新增开关 | `AUTO_BLOCK_ON_AFFECTION`（默认 `false`），管好感度过低与连续负反馈两条链路 |
| 默认值变更 | `PRIVATE_MESSAGE_AUTO_BLOCK` 默认改为 `false` |
| 原行为保留 | 关闭后命中阈值的用户不再调 B站 API，改为写 `auto_block_suppressed` 安全事件 |
| 新增接口 | `GET /api/block_suggestions` — 把上述事件按 UID 聚合为待确认建议 |
| 新增接口 | `POST /api/block_suggestion/dismiss` — 忽略某条建议，带 `undo` 可撤销 |
| 面板入口 | 新增「安全中心」（带待处理数量角标），建议卡片提供「拉黑 / 忽略」按钮；「拉黑」才真正调用 `POST /api/block_user` |
| 语义不变 | 命中安全规则的私信依旧不回复、不送进 LLM，与是否拉黑无关 |

聚合建议包含的字段：命中次数、首次 / 最近时间、来源（评论或私信）、原因、最多 3 条原文样本。

聚合时会过滤：本人、已拉黑、已忽略。数据来自既有安全日志，因此**历史事件自动纳入，无需数据迁移**。

<details>
<summary><b>验证方式</b></summary>

<br>

- 服务器侧合成事件聚合断言：`hits=2`、来源取最新（私信）、已拉黑被过滤、忽略后消失、撤销后恢复、空 UID 返回 `400`、未登录返回 `401`
- 浏览器内真实点击「忽略」，并拦截校验「拉黑」请求参数（`uid` 匹配、`reason` 以「人工确认拉黑」开头）
- 测试使用每次唯一的合成 UID，跑完自动还原，断言建议条数回到注入前

</details>

---

### fix(panel) — 首页聊天 500：Embedding 不可用改为降级

**现象**：面板能登录、能看历史，但一发消息就失败，提示「连接出错了，检查一下后端是否在运行？」

**根因**：`config.json` 的 `EMBED_MODEL` 为空串（键存在但值为空）时，`get()` 的默认值不生效，实际以空模型名请求 Embedding 接口，网关回 `400 Model name not specified`。该异常在 `local-chat.py` 侧未被捕获，直接把整个 `POST /api/chat` 打成 `500`（HTML），前端 `resp.json()` 解析失败，于是报出与真实原因无关的网络错误。

`ai.py` 侧早有 `_EMBED_AVAILABLE` 降级，所以 Bot 仍在正常回评论，只有面板聊天整体挂掉，极易误判成「后端没起来」。

**修复**（面板侧对齐同一套语义）：

| 位置 | 改动 |
|------|------|
| `get_embedding()` | 模型未配置直接返回 `None`；调用异常也返回 `None`，只警告一次 |
| `get_relevant_memories()` | 无向量时退化为「最近 N 条记忆」，对话仍有上下文 |
| 记忆写入 | 无向量时不写 `embedding` 字段，不在数据里留 `null` |
| `chat()` / `chat_regenerate()` | 捕获未预期异常，返回 JSON 错误（`502`）而非 HTML `500` |
| `_generate_reply()` | 空正文视为失败并触发回退模型 —— 推理型模型吃光 `max_tokens` 的典型症状 |

<details>
<summary><b>验证方式</b></summary>

<br>

- 真实 `POST /api/chat` 返回 `200` + 非空回复，连续两次均可用
- 记忆新增条目无 `null` 向量
- 浏览器内真实打字发送并等到 Bot 气泡上屏
- 测试写入的聊天历史与记忆已原样还原

</details>

---

### feat(security) — 面板口令密封、代理感知 TLS 与默认拒绝鉴权

| 机制 | 实现 |
|------|------|
| 默认拒绝鉴权 | 白名单仅含 `/`、`/api/login`、`/api/auth_check`、`/api/handshake`、`/api/health`、`/api/branding`、`/media/bot-avatar`、`/static/*`；未登录的 `/api/*` 一律 `401`，其余路径 `404` |
| 口令密封 | `GET /api/handshake` 下发 SPKI(DER) 公钥，前端用 WebCrypto 以 RSA-OAEP(SHA-256) 加密后走 `sealed` 字段；明文 `password` 作为降级回退保留 |
| 会话加固 | `HttpOnly` + `SameSite=Lax`，`Secure` 由「是否经 TLS 访问」动态判定；会话密钥持久化在 `data/.secret_key`（`0600`） |
| 反代感知 | 仅当请求来自回环地址时才采信 `X-Forwarded-Proto`，避免伪造头骗过 HSTS 判定 |
| 静态目录收口 | `/media/bot-avatar` 不接受文件名参数，只回配置指定的那一张，拒绝路径分隔符与 `..`，统一输出 160px 缩略图并加 `Cache-Control` |

---

### fix(panel) — Bot 头像 404 与静态目录收口

鉴权收口后 `/data/**` 不再作为静态目录暴露，面板 `BOT_AVATAR` 指向的图片随之 404。改为专用路由 `/media/bot-avatar` 从配置读取文件名并加入白名单，头像恢复显示。

---

### feat(ui) — 角色立绘壁纸与玻璃拟态

- 新增角色立绘：亮色主题 Arona、暗色主题 Plana，两帧差分实现眨眼动效
- 壁纸层透明度统一由 CSS 变量 `--char-opacity` 控制（亮 `0.30` / 暗 `0.34`，移动端自动缩小压暗）
- 卡片改为半透明毛玻璃，让壁纸透出而不牺牲文字可读性
- 侧栏图标由 Emoji 换为内联 Lucide SVG，新增「安全中心」入口与角标样式
- 新增 4 张角色立绘到 `static/img/character/`，新增壁纸与 Logo 资源

---

### chore(deploy) — systemd + 内网穿透隧道部署

- 面板与 Bot 分别跑在两个 systemd 服务下，日志走 `logrotate`（daily / 7 份 / `copytruncate`）
- 面板保持本机 `5000` 明文监听，由内网穿透隧道在边缘终止 TLS，浏览器侧即获得安全上下文，前端口令密封因此可用
- 面板口令经 systemd `Environment=CHAT_PASSWORD=` 注入，优先级 `config.json` > 环境变量 > 默认值
- 详细步骤见 [DEPLOY.md](DEPLOY.md)，其中地址、端口、口令一律用占位符

---

### fix(bot) — 修复私信与评论完全不回复等问题

`c6e2692`

私信与评论不回复由**两个独立根因叠加**造成，各自都能单独导致完全不回复。

#### 根因一：休眠窗口命中

- `is_active_time()` 在 `SLEEP_START < SLEEP_END` 时判定为 `hour < SLEEP_START or hour >= SLEEP_END`；配置为 `2/8` 时从 2 点起即进入休眠，主循环跳过全部评论与私信
- 该提示文案为**硬编码**（写死 2:00-8:00），与实际配置无关，排查时不可采信
- 改为 `24/0` 使全天活跃
- 注意：`0/0` 会走 else 分支得到 `hour >= 0 and hour < 0`，等于**永久休眠**，不能用于关闭休眠

#### 根因二：`SEARCH_KEYWORDS` 未定义

- `ai.py` 仅通过 `from config import *` 导入，但该常量只定义在 `local-chat.py` 中
- `needs_search()` 引用后抛 `NameError`，导致私信链路 `generate_reply_and_score()` 每次都在生成阶段失败
- 在 `needs_search()` 前补入同名常量

#### 同批次其他修复

| 模块 | 内容 |
|------|------|
| 评论楼层归属 | `get_new_replies()` 增加 `root_rpid`；`thread_id` 改为 `根评论ID:用户UID`，避免同一评论串下不同用户共享上下文造成回复串台 |
| 评论楼层归属 | `send_reply()` 增加 `root_rpid` 参数，改为 `root=root_rpid or rpid, parent=rpid`，修正回复子评论时挂错楼层 |
| 模型兜底 | `claude_chat()` 改为三层串行短路，第一个非空正文即返回；推理型模型 token 预算放宽为 `max(1500, n)` |
| 记忆检索 | `get_embedding()` 失败时优雅降级为禁用语义记忆检索，避免异常中断主流程导致同一条评论被反复重试 |
| 可观测性 | 主循环 `except` 增加 `traceback` 输出 |
| 风控规避 | `ai.py / config.py / private_messages.py / Proactive.py / dynamic.py` 的 User-Agent 补齐为完整 Chrome UA，规避 B站风控 `code 30014` |
| 默认配置 | `config.py` 与 `config.example.json` 的休眠默认值改为 `24/0`（全天活跃） |

---

### feat(ui) — WebUI 改用 PaperGrid / schale 设计语言

`4ccad44`

| 要素 | 取值 |
|------|------|
| 圆角 | 不对称 `8px 3px 22px` |
| 阴影 | 硬边偏移 `4px 4px 0 var(--primary)`（无模糊） |
| 导航高亮 | 斜切块 `skew(-12deg)` |
| 装饰 | 卡片与气泡左上角的青色竖标记 |
| 配色 | `--primary #087cba`、`--ba-cyan #19b6ed`、背景 `#f2f7fb`、前景 `#17324b` |
| 动效 | 缓动 `cubic-bezier(.22,.8,.25,1)`；标题字重 800、导航 700 |
| 深色 | 新增 `prefers-color-scheme: dark` 适配，仅覆盖变量值、不改动规则 |

**实现方式**：仅替换 `chat.html` 的 `<style>` 块，HTML 结构与 `<script>` 未改动。

**校验结果**：script 段与 body 段 MD5、id 数、class 数、onclick 数在替换前后完全一致。

---

### chore — 行尾规范与忽略规则

`9b2e6ab`

- 新增 `.gitattributes`，强制 `text=auto eol=lf`，避免 CRLF 导致 BusyBox `ash` / `procd` 等环境启动失败
- 既有文本文件统一归一为 LF
- `.gitignore` 补充 `.env` / `*.key` / `*.pem` / `venv` / `*.bak` 等本地环境与密钥文件，防止敏感信息误入库

---

### docs — 修复记录与部署注意事项

`cfd71be`

- 新增 `FIXES.md`，记录两个根因、排查手法与部署安全注意事项
- `README` 追加索引

---

## 安全说明

本次推送前已完成审计。

| 审计项 | 结论 |
|--------|------|
| 工作区文本文件扫描 | 13 个文件，**未发现** API Key、Token、Cookie、密码等敏感内容 |
| git 历史扫描 | 全部 41 个文件版本，仅命中单元测试中的假数据（`SESSDATA=sess`、`bili_jct=csrf`、`DedeUserID=10001`），无需清理 |
| 未追踪文件 | `config.json`、`data/` 从未被 git 追踪，且已在 `.gitignore` 中忽略 |
| 硬编码检查 | 所有密钥均从 `config.json` / 环境变量读取，代码中无硬编码 |
| 示例文件 | `config.example.json` 仅含占位符，可直接复制为 `config.json` 使用 |
