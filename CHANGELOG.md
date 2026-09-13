# 更新日志 (Changelog)

本仓库 fork 自 [chenluQwQ/bilibili-ai-bot](https://github.com/chenluQwQ/bilibili-ai-bot)，
本文件记录相对上游的变更。格式参考 Keep a Changelog，提交信息遵循 Conventional Commits。

这里是本项目**唯一的更新日志文件**；问题根因与排查手法另见 [FIXES.md](FIXES.md)。

## [2026-09-14]

### feat(security) — 关闭自动拉黑，改为「拉黑建议 + 人工确认」

自动拉黑是真实账号操作，命中阈值即封人，事后很难补救。本次把决策权收回人工：

- 新增开关 `AUTO_BLOCK_ON_AFFECTION`（默认 `false`），管好感度过低与连续负反馈两条链路
- `PRIVATE_MESSAGE_AUTO_BLOCK` 默认改为 `false`
- 关闭后命中阈值的用户不再调 B站 API，改为写 `auto_block_suppressed` 安全事件
- 新增 `GET /api/block_suggestions`：把上述事件按 UID 聚合为待确认建议
  （命中次数、首次/最近时间、来源评论或私信、原因、最多 3 条原文样本），
  过滤本人、已拉黑、已忽略；历史事件自动纳入，无需数据迁移
- 新增 `POST /api/block_suggestion/dismiss`：忽略某条建议，带 `undo` 可撤销
- 面板新增「安全中心」入口（带待处理数量角标）：建议卡片提供「拉黑 / 忽略」按钮，
  「拉黑」才真正调用 `POST /api/block_user`
- 隔离语义保持不变：命中安全规则的私信依旧不回复、不送进 LLM，与是否拉黑无关

验证：服务器侧合成事件聚合（hits=2、来源取最新、已拉黑过滤、忽略与撤销）、
浏览器内真实点击「忽略」与拦截校验「拉黑」请求参数，跑完自动还原数据。

### fix(panel) — 首页聊天 500：Embedding 不可用改为降级

**现象**：面板能登录、能看历史，但一发消息就失败，提示「连接出错了，检查一下后端是否在运行？」。

**根因**：`config.json` 的 `EMBED_MODEL` 为空串（键存在但值为空）时，
`get()` 的默认值不生效，实际以空模型名请求 Embedding 接口，网关回
`400 Model name not specified`。该异常在 `local-chat.py` 侧未被捕获，直接把整个
`POST /api/chat` 打成 500（HTML），前端 `resp.json()` 解析失败，于是报出与真实原因无关的网络错误。

`ai.py` 侧早有 `_EMBED_AVAILABLE` 降级，所以 Bot 仍在正常回评论，只有面板聊天整体挂掉，
极易误判成「后端没起来」。

**修复**（面板侧对齐同一套语义）：

| 位置 | 改动 |
| --- | --- |
| `get_embedding()` | 模型未配置直接返回 `None`；调用异常也返回 `None`，只警告一次 |
| `get_relevant_memories()` | 无向量时退化为「最近 N 条记忆」，对话仍有上下文 |
| 记忆写入 | 无向量时不写 `embedding` 字段，不在数据里留 `null` |
| `chat()` / `chat_regenerate()` | 捕获未预期异常，返回 JSON 错误（502）而非 HTML 500 |
| `_generate_reply()` | 空正文视为失败并触发回退模型 —— 推理型模型吃光 `max_tokens` 的典型症状 |

验证：真实 `POST /api/chat` 返回 200 + 非空回复，连续两次均可用；记忆新增条目无 `null` 向量；
浏览器内真实打字发送并等到 Bot 气泡；测试写入的聊天历史与记忆已原样还原。

### feat(security) — 面板口令密封、代理感知 TLS 与默认拒绝鉴权

- 鉴权改为默认拒绝：仅登录页与少量静态资源在白名单内（`/`、`/api/login`、
  `/api/auth_check`、`/api/handshake`、`/api/health`、`/api/branding`、`/media/bot-avatar`、
  `/static/*`）；未登录的 `/api/*` 一律 401，其余路径 404，不泄露资源是否存在
- 口令密封：`GET /api/handshake` 下发 SPKI(DER) 公钥，前端用 WebCrypto 以
  RSA-OAEP(SHA-256) 加密后走 `sealed` 字段；明文 `password` 作为降级回退保留
- 会话加固：`HttpOnly` + `SameSite=Lax`，`Secure` 由「是否经 TLS 访问」动态判定，
  会话密钥持久化在 `data/.secret_key`（0600）
- 反代感知：仅当请求来自回环地址时才采信 `X-Forwarded-Proto`，避免伪造头骗过 HSTS 判定
- `/media/bot-avatar` 不接受文件名参数，只回配置指定的那一张，拒绝路径分隔符与 `..`，
  统一输出 160px 缩略图并加 `Cache-Control`

### fix(panel) — Bot 头像 404 与静态目录收口

鉴权收口后 `/data/**` 不再作为静态目录暴露，面板 `BOT_AVATAR` 指向的图片随之 404。
改为专用路由 `/media/bot-avatar` 从配置读取文件名并加入白名单，头像恢复显示。

### feat(ui) — 角色立绘壁纸与玻璃拟态

- 新增角色立绘：亮色主题 Arona、暗色主题 Plana，两帧差分实现眨眼动效
- 壁纸层透明度统一由 CSS 变量 `--char-opacity` 控制（亮 0.30 / 暗 0.34，移动端自动缩小压暗）
- 卡片改为半透明毛玻璃，让壁纸透出而不牺牲文字可读性
- 侧栏图标由 Emoji 换为内联 Lucide SVG，新增「安全中心」入口与角标样式
- 新增 4 张角色立绘到 `static/img/character/`，新增壁纸与 Logo 资源

### chore(deploy) — systemd + 内网穿透隧道部署

- 面板与 Bot 分别跑在两个 systemd 服务下，日志走 `logrotate`（daily / 7 份 / copytruncate）
- 面板保持本机 5000 明文监听，由内网穿透隧道在边缘终止 TLS，
  浏览器侧即获得安全上下文，前端口令密封因此可用
- 面板口令经 systemd `Environment=CHAT_PASSWORD=` 注入，优先级 config.json > 环境变量 > 默认值
- 详细步骤见 [DEPLOY.md](DEPLOY.md)，其中地址、端口、口令一律用占位符

### fix(bot) — 修复私信与评论完全不回复等问题

`c6e2692`

私信与评论不回复由**两个独立根因叠加**造成，各自都能单独导致完全不回复：

1. **休眠窗口命中**
   - `is_active_time()` 在 `SLEEP_START < SLEEP_END` 时判定为
     `hour < SLEEP_START or hour >= SLEEP_END`；配置为 `2/8` 时从 2 点起即进入休眠，
     主循环跳过全部评论与私信
   - 该提示文案为**硬编码**（写死 2:00-8:00），与实际配置无关，排查时不可采信
   - 改为 `24/0` 使全天活跃
   - 注意：`0/0` 会走 else 分支得到 `hour >= 0 and hour < 0`，等于**永久休眠**，
     不能用于关闭休眠

2. **`SEARCH_KEYWORDS` 未定义**
   - `ai.py` 仅通过 `from config import *` 导入，但该常量只定义在 `local-chat.py` 中
   - `needs_search()` 引用后抛 `NameError`，导致私信链路 `generate_reply_and_score()`
     每次都在生成阶段失败
   - 在 `needs_search()` 前补入同名常量

同批次其他修复：

| 模块 | 内容 |
| --- | --- |
| 评论楼层归属 | `get_new_replies()` 增加 `root_rpid`；`thread_id` 改为 `根评论ID:用户UID`，避免同一评论串下不同用户共享上下文造成回复串台 |
| 评论楼层归属 | `send_reply()` 增加 `root_rpid` 参数，改为 `root=root_rpid or rpid, parent=rpid`，修正回复子评论时挂错楼层 |
| 模型兜底 | `claude_chat()` 改为三层串行短路，第一个非空正文即返回；推理型模型 token 预算放宽为 `max(1500, n)` |
| 记忆检索 | `get_embedding()` 失败时优雅降级为禁用语义记忆检索，避免异常中断主流程导致同一条评论被反复重试 |
| 可观测性 | 主循环 `except` 增加 `traceback` 输出 |
| 风控规避 | `ai.py / config.py / private_messages.py / Proactive.py / dynamic.py` 的 User-Agent 补齐为完整 Chrome UA，规避 B 站风控 `code 30014` |
| 默认配置 | `config.py` 与 `config.example.json` 的休眠默认值改为 `24/0`（全天活跃） |

### feat(ui) — WebUI 改用 PaperGrid / schale 设计语言

`4ccad44`

- 不对称圆角 `8px 3px 22px`
- 硬边偏移阴影 `4px 4px 0 var(--primary)`（无模糊）
- 导航项斜切高亮块 `skew(-12deg)`
- 卡片与气泡左上角的青色竖标记
- 配色：`--primary #087cba`、`--ba-cyan #19b6ed`、背景 `#f2f7fb`、前景 `#17324b`
- 缓动 `cubic-bezier(.22,.8,.25,1)`；标题字重 800、导航 700
- 新增 `prefers-color-scheme: dark` 深色适配，仅覆盖变量值、不改动规则

实现方式：仅替换 `chat.html` 的 `<style>` 块，HTML 结构与 `<script>` 未改动。
校验结果：script 段与 body 段 MD5、id 数、class 数、onclick 数在替换前后完全一致。

### chore — 行尾规范与忽略规则

`9b2e6ab`

- 新增 `.gitattributes`，强制 `text=auto eol=lf`，避免 CRLF 导致
  BusyBox `ash` / `procd` 等环境启动失败
- 既有文本文件统一归一为 LF
- `.gitignore` 补充 `.env` / `*.key` / `*.pem` / `venv` / `*.bak`
  等本地环境与密钥文件，防止敏感信息误入库

### docs — 修复记录与部署注意事项

`cfd71be`

- 新增 `FIXES.md`，记录两个根因、排查手法与部署安全注意事项
- `README` 追加索引

---

## 安全说明

本次推送前已完成审计：

- 工作区 13 个文本文件扫描，**未发现** API Key、Token、Cookie、密码等敏感内容
- git 历史全部 41 个文件版本扫描，仅命中单元测试中的假数据
  （`SESSDATA=sess`、`bili_jct=csrf`、`DedeUserID=10001`），无需清理
- `config.json`、`data/` 从未被 git 追踪，且已在 `.gitignore` 中忽略
- 所有密钥均从 `config.json` / 环境变量读取，代码中无硬编码
- `config.example.json` 仅含占位符，可直接复制为 `config.json` 使用
