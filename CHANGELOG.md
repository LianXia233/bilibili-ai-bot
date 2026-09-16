# 更新日志

<div align="center">

本仓库 fork 自 [chenluQwQ/bilibili-ai-bot](https://github.com/chenluQwQ/bilibili-ai-bot)，本文件记录**相对上游的变更**

格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，提交信息遵循 [Conventional Commits](https://www.conventionalcommits.org/)

![Status](https://img.shields.io/badge/status-maintained-0F9D58?style=flat-square)
![Convention](https://img.shields.io/badge/commits-Conventional%20Commits-FE5196?style=flat-square)
![Docs](https://img.shields.io/badge/changelog-single%20source%20of%20truth-087CBA?style=flat-square)

</div>

> 这里是本项目**唯一的更新日志文件**。问题根因与排查手法另见 [FIXES.md](FIXES.md)，生产部署见 [DEPLOY.md](DEPLOY.md)。

> **脱敏约定**：本文实测样例中的 B 站用户名、账号 `mid`、评论 `rpid`、视频 `aid` / `BV` 号均以占位符（`<Bot昵称>`、`用户A`、`<rpid-1>`、`<aid-1>`）呈现，占位符不改变结论。

## 变更索引

| 日期 | 类型 | 标题 | 影响面 |
|------|------|------|--------|
| 2026-09-16 | `feat(panel)` | WebUI 新增「模型调用统计」：按具体模型聚合调用次数 / 输入输出 token / 成本 / 占比（修复 log_cost 把模型名记成来源名的旧 bug）；全站响应头 no-store 强制浏览器每次拉取最新页面，忽略缓存 | 功能新增 |
| 2026-09-16 | `fix(ui)` | WebUI 聊天「乱回」根因：用户消息以 content 数组发送，hy3 等模型不支持数组、把消息当空，对任何输入都回「你好像不想说话」；改为无图时纯字符串 content，实测「你是谁/你好/1+1」均正常应答 | 故障修复 |
| 2026-09-16 | `feat(bot)` | 记忆改为「按用户独立会话 + 全部优先注入」：跨评论 / @ / 私信聚合该用户全部历史对话，回复前默认注入上下文（按时间正序，预算仅作安全上限），不再按相关度挑拣导致失忆 / 乱回复 | 行为变更 |
| 2026-09-16 | `feat(panel)` | WebUI 新增「评论自动回复」「@回复」两个独立开关（默认关闭，仅私信通道独立运行），控制台与配置均可切换 | 功能新增 |
| 2026-09-16 | `fix(bot)` | 评论自回循环：B 站把 bot 自己发出的回复回流为新评论时直接跳过（与私信同款防御） | 故障修复 |
| 2026-09-16 | `fix(panel,crypto)` | WebUI 模型池配置不显示四根因：加密网关透传 Cookie/Set-Cookie、config/raw 统一 `{config}` 包装、模型池独立加载不被配置解析阻塞、失败不再静默 | 故障修复 |
| 2026-09-16 | `chore(cleanup)` | 移除 Python/Flask 版后端（7 个 .py + Requirements.txt + tests/），仓库仅保留 rust-backend；README / DEPLOY 同步改为 Rust 部署 | 结构调整 |
| 2026-09-16 | `fix(security)` | WebUI 登录「连接失败」：非安全上下文（HTTP 公网访问）下 Web Crypto 不可用，加密通道全链路 noble 纯 JS 回退 | 故障修复 |
| 2026-09-16 | `fix(private_msgs)` | 私信复读死循环：内容回显去重，B 站把 bot 自己回复误标为对方消息时直接跳过 | 故障修复 |
| 2026-09-16 | `feat(security)` | HTTP 应用层加密通信：X25519 + HKDF-SHA256 + AES-256-GCM，防被动抓包读取 API 正文 | 安全加固 |
| 2026-09-16 | `fix(panel)` | 图片上传间歇失败：文件名随机后缀含 `/` 破坏路径（历史遗留 ~9% 失败率） | 故障修复 |
| 2026-09-15 | `fix(bot)` | 评论与私信「串台」：永久记忆无长度约束，把用户那句话淹到提示词最末尾 | 故障修复 |
| 2026-09-15 | `fix(bot)` | 复读与空承诺：具体请求（写诗 / 列清单等）不落地，改为强制交付成品 | 故障修复 |
| 2026-09-15 | `feat(panel)` | 记忆管理双页签（临时 / 长期），临时记忆一键清空 + 每日定点自动清空 | 功能新增 |
| 2026-09-15 | `feat(ui)` | 对话模型池改为折叠式列表，消除 5 个空表单的视觉噪音；启用前自动落盘 | 体验优化 |
| 2026-09-15 | `feat(panel)` | 对话模型池：多套 API / 模型配置，WebUI 一键切换 | 功能新增 |
| 2026-09-15 | `feat(panel)` | 记忆一键清空：独立入口 + 自动备份，区分「永久记忆」与「全部记忆」 | 功能新增 |
| 2026-09-15 | `feat(bot)` | 永久记忆改为纯人工写入，模型不再自动追加 | 行为变更 |
| 2026-09-15 | `fix(bot)` | 永久记忆不起作用：注入上限与写入上限同为 20，写满后新规则进不来 | 故障修复 |
| 2026-09-15 | `fix(bot)` | 私信复读：101 个会话冲垮定长去重表，游标未推进到远端最大值 | 故障修复 |
| 2026-09-15 | `fix(bot)` | 语义记忆检索遇缺 `embedding` 字段的条目抛 `KeyError`，整段上下文构造失败 | 故障修复 |
| 2026-09-14 | `feat(bot)` | 休眠总开关（默认不休眠）与模型 TPM 滑窗限速，全面板可配 | 功能新增 |
| 2026-09-14 | `fix(bot)` | 评论 / @ / 私信全都不回复：单条异常卡死整轮循环 | 故障修复 |
| 2026-09-14 | `fix(bot)` | 视频分析两步走，视觉预算越界修正（8192 触发网关 500） | 故障修复 |
| 2026-09-14 | `fix(bot)` | Token 预算默认值按实测重设：一轮成功优先于最省 | 故障修复 |
| 2026-09-14 | `fix(bot)` | 模型返回空正文：改为就地抬升预算重试，Token 预算全面板可配 | 故障修复 |
| 2026-09-14 | `fix(ui)` | 移动端底部输入框被地址栏 / 软键盘遮挡 | 故障修复 |
| 2026-09-14 | `docs` | 实测样例脱敏：B 站用户名与业务标识改为占位符 | 文档 |
| 2026-09-14 | `fix(bot)` | 两条消息流正文统一归一化：剥「回复 @昵称 :」前缀与 @ 噪声 | 故障修复 |
| 2026-09-14 | `fix(bot)` | 视频上下文与模型通道：@ 类评论改为结合视频标题回复 | 故障修复 |
| 2026-09-14 | `fix(bot)` | 计费口径前后端统一，成本账本明细补全 | 故障修复 |
| 2026-09-14 | `fix(bot)` | 回复日志语义修正：区分「已发送」与「发送失败」 | 故障修复 |
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
| 2026-09-16 | `fix(rust-backend)` | 修复 `cargo check` 无法通过：`check_cookie` 括号语法错误（整仓不可编译） | 故障修复 |
| 2026-09-16 | `fix(panel)` | 面板带图聊天丢消息：图片+文字时文字不进模型，图片缺失时整条用户消息丢失 | 故障修复 |
| 2026-09-16 | `fix(bot)` | 动态自定义文案键名错位：面板 `PROMPT_DYNAMIC` 配置不生效 | 故障修复 |
| 2026-09-16 | `fix(bot)` | 私信 `sender_uid` 为数字时解析为空，存在「对自己消息自我回复」风险 | 故障修复 |
| 2026-09-16 | `security(panel)` | 面板加固：Cookie 加 SameSite、签名常量时间比较、默认口令启动告警、上传限流、子进程超时 | 安全加固 |
| 2026-09-16 | `feat(rust-backend)` | 2026-09-15 更新全量移植到 Rust 版：永久记忆分层装填 / 成品条款 / 记忆双页签+定时清空 / 对话模型池 / 记忆一键清空 / 私信复读修复 等 9 项 | 功能移植 |
| 2026-09-16 | `feat(rust-backend)` | 对话备用通道 OR_BACKUP_MODEL/URL/KEY 补全（chat 专属第三条路，部署实机大模型配置迁移必需） | 功能移植 |
| 2026-09-16 | `ops` | 实机部署 rust-backend：停用 Python 双服务、systemd 单进程接管、config/data 全量迁移 | 部署 |
| 2026-09-16 | `feat(rust-backend)` | 记忆功能防乱回复优化：相关度过滤注入、记忆格式标注、回复防重复、记忆写入截断、性格演化稳定 | 功能优化 |
| 2026-09-16 | `fix(rust-backend)` | 回复乱回根因修复：人格 style/owner 提示词丢失、激活通用人格、纯表情评论空洞点评、幻觉编造事实 | 缺陷修复 |

### fix(rust-backend) — rust-backend 全量代码审计与修复（8 项）

对 `rust-backend`（Rust 重构版）做全量审查 + 修复。审查以「能否编译、能否按 Python 参考语义运行」为基线，全部修复已通过 `cargo check` / `cargo build` / 启动冒烟测试。

| # | 级别 | 位置 | 问题 | 修复 |
|---|------|------|------|------|
| 1 | P0 | `src/bili_api.rs:198` | `check_cookie()` 报错分支多一个右括号，**整个 crate 无法编译**（`cargo check` 直接失败） | 重写为 `format!("Cookie 已失效（{msg}）")` |
| 2 | P1 | `src/web.rs api_chat` | 带图聊天：图片+文字时**文字不进模型**；图片文件缺失时**整条用户消息丢失**（LLM 只收到 system 消息） | 图片+文字合并为单条 `content` 数组（对齐 Python `_generate_reply`）；图片不可读降级纯文本并告警；`generate_chat_reply` 同步对齐；MIME 按扩展名推断 |
| 3 | P1 | `src/dynamic.rs` | 动态自定义文案读 `PROMPT_DYNAMIC_CONTENT`，面板/默认表/ Python 均为 `PROMPT_DYNAMIC`，**面板配置永远不生效** | 键名对齐 `PROMPT_DYNAMIC` |
| 4 | P1 | `src/private_msgs.rs poll()` | `sender_uid` 用 `as_str()` 读取，B 站返回数字时解析为空串，`sender_uid == self_uid` 判空失败 → **可能对「自己发给自己」的消息自我回复** | 字符串/数字双态解析（对齐 Python `str(...)`） |
| 5 | P2 | `src/web.rs api_cost_add` | `cost_log.json` 损坏为非对象时 `as_object_mut().unwrap()` **panic 500** | 非对象兜底为 `{}` |
| 6 | P2 | `src/web.rs` 会话 | 会话 Cookie 无 `SameSite`；HMAC 签名用普通 `!=` 比较（时序侧信道） | 加 `SameSite=Lax`；改常量时间比较 `constant_time_eq` |
| 7 | P2 | `src/web.rs` 上传/口令 | 上传无大小上限、同秒同名覆盖；默认口令 `admin()` 无启动提示 | 10MB 上限 + 时间戳+随机后缀唯一名；启动时检测默认口令并 `tracing::warn` |
| 8 | P3 | `src/proactive.rs` | `yt-dlp` / `ffmpeg` 子进程无超时，卡死会永久占用任务 | 300s/60s 超时 + `kill_on_drop(true)` |

**校验结果**

- `cargo check` / `cargo build`：通过（修复前直接编译失败）
- `cargo clippy`：无新增错误，仅存量风格提示
- 冒烟测试：`--no-bot --port` 启动后 `/api/health` 200、未授权 `/api/config` 401、`/api/handshake` 正常返回 RSA 公钥（密封密钥生成正常）
- 行为对齐：图片消息构造、动态文案键名、私信 `sender_uid` 解析均与 Python 参考实现逐点核对

**已知边界**（未改动，保持与 Python 一致）：默认口令 `admin()` 为上游设计，已加启动告警，公网部署仍需配置 `CHAT_PASSWORD`。

### fix(rust-backend) — 回复乱回根因修复

排查实机真实回复样本后定位四类根因并修复（提交后将部署实机）：

| 根因 | 证据 | 修复 |
|------|------|------|
| 激活了通用 AI 人格 | 实机 ACTIVE_PERSONA="1"（5 行通用模板人格），定制猫娘人格 default 未被激活；回复呈客服腔（「合同丢失可以联系客服」） | 实机配置切回 ACTIVE_PERSONA=default（面板可随时切换） |
| Rust 丢失 style_prompt/owner_prompt | Python 用 `style_prompt 优先于默认风格 + owner_prompt 单独注入`，Rust 只取 system_prompt，猫娘「可爱」风格与态度指令全部丢失 | 新增 active_persona_full()，bot 回复完整注入三段（对齐 ai.py） |
| 纯表情评论空洞点评 | 回复样本「这装扮很好看/好可爱/好有个性」反复复读 | 识别纯表情/装扮评论（剥掉 [xxx] 后无实质内容），注入专门指令：禁止点评装扮、结合视频找话题 |
| 幻觉编造事实 | 「换头像了？→是啊，换了新头像」「合同→请联系客服」 | prompt 增加【事实边界】：不确定的事直说不知道，禁止编造或顺着对方承认 |
| 复读机句式 | 「哈哈」开头频繁 | prompt 禁止「哈哈/这装扮/真好看」式开头与重复句式 |

验证：cargo check/clippy 无新增警告；3 项单元测试全绿（含 is_emoji_only_comment 识别）。


### feat(rust-backend) — 记忆功能防乱回复优化

针对「记忆装填质量」与「回复稳定性」的五项优化，全部新增配置项可热更新（Config 默认值兜底，实机 config.json 无需改动即可生效）：

| 优化项 | 说明 | 配置键（默认值） |
|--------|------|------------------|
| 线程记忆相关度过滤 | 线程记忆改为按「与当前评论话题的相关度」取 top N（embedding 可用时），跨话题不再全量注入无关历史；完全无相关历史时退回最近 N 条防冷场 | MEMORY_THREAD_TAIL=4 / MEMORY_THREAD_CHARS=800 |
| 语义记忆标注与阈值 | 语义检索阈值 0.45→0.5 可配、注入条数可配，且每条记忆附带相关度标注，模型可分辨可信度 | MEMORY_SEMANTIC_TOP=3 / MEMORY_SEMANTIC_MIN_SIM=0.5 |
| 回复防重复 | 新回复与同一用户最近 N 条已发回复做字符集合 Jaccard 相似度比较（先归一化去标点），超阈值自动重新生成一次；评论与私信双通道生效 | REPLY_DEDUP_SIM=0.85 / REPLY_DEDUP_LOOKBACK=3 |
| 记忆写入质量 | 记忆只保存 Bot 回复前 300 字（长成品不污染记忆）；embedding 以「用户侧内容」为主体，避免 Bot 回复风格主导检索 | MEMORY_REPLY_CHARS=300 |
| 性格演化稳定 | 演化特质只注入最近 1 条并标注为「长期稳定特点，不在单条回复中改变语气」，防止人格漂移导致说话突变 | — |
| Prompt 防跑题 | 记忆/视频/搜索统一标注为「背景材料」，强调对方没提的话题不主动展开，禁止照搬历史回复 | — |

验证：cargo check / clippy（无新增警告）/ 2 项单元测试（归一化与字符相似度）全绿；本地以实机 config+data 冒烟 API 全通。


### ops — 实机部署 rust-backend

生产实机（Python 版）替换为 rust-backend 单进程版。部署前完成四项兼容核验：① 数据格式（memory/user_profiles/permanent/affection/private_message_state 等与 Python 逐文件比对）；② 配置键差集（实机 109 键 vs Rust 默认表，仅缺 OR_BACKUP_* 与 AUTO_BLOCK_ON_AFFECTION——前者已补实现、后者已使用）；③ glibc（本地 2.35 构建 / 实机 2.41 运行，向后兼容）；④ 本地以实机 config+data 冒烟全绿。

| 步骤 | 结果 |
|------|------|
| 备份 | /opt/bak-bilibili-rs-pre-deploy-20260916-0730.tar.gz（4.2MB，原目录保留可回滚） |
| 旧版移除 | systemctl stop + disable bilibili-bot（ai.py）与 bilibili-panel（local-chat.py），进程已退出 |
| 新版部署 | /opt/bilibili-ai-bot-rs（二进制 + chat.html + config.json + data/ 全量）；systemd `bilibili-rs` 单进程 worker+web，端口 5000，CHAT_PASSWORD 环境变量注入 |
| 大模型配置迁移 | config.json 全键迁移：OR_CHAT/VISION/SEARCH/IMAGE_*、模型池 4 条（ACTIVE=2）、OR_BACKUP_* 备用通道、EMBED/SILICON、RATE_LIMIT_*_TPM |
| 验证 | health 200、登录同密码、stats/pool/temp API 全通；Bot 主循环真实回复评论（LLM 调用成功）、好感度更新、systemd active |
| 安全 | 公网 403 为实机既有 YJ-FIREWALL 白名单（旧版同样受限），非新版引入 |


### feat(rust-backend) — 2026-09-15 更新全量移植（9 项）

将 2026-09-15 的 10 项更新（+1 项同日休眠/TPM）全部移植到 `rust-backend`（Rust 重构版）。此前 Rust 侧版本落后：永久记忆完全没注入提示词、模型池/记忆清空/定时清空等面板 API 缺失、私信去重表与游标存在复读隐患。实现逐项对照 Python 参考（`ai.py` / `config.py` / `local-chat.py`，与生产实机部署目录同源核验一致），编译 / 冒烟 / API 契约均验证通过。

| # | 类型 | 对应 09-15 条目 | Rust 侧缺口 | 修复 |
|---|------|----------------|-------------|------|
| 1 | `fix(bot)` | 串台 | 永久记忆完全没有注入提示词（`bot.rs` 只有 personality_evo，无 permanent block）；JSON schema 仍带 `permanent_memory` 字段 | 实现 `build_permanent_block`：tier 0（身份/人格/说话风格）与 tier 1（禁止/安全）无条件全量、tier 2（状态）与 tier 3（表情包）受 `PERMANENT_MEMORY_INJECT` 预算、组内新的先保、输出恢复原始顺序；`_summarize_emoji_pool` 语义对齐（样例 6 条 + 总数 + 硬约束 + `EMOJI_POOL_MARKERS`）；边界段「需要你回应的内容（本节唯一）」+ 三条硬约束挂入 `bot.rs` 提示词 |
| 2 | `fix(bot)` | 复读/空承诺 | 成品条款缺失 | 提示词补入成品条款：具体请求当场交付成品、不受 15-40 字限制、写诗示范含 `\n` 转义 |
| 3 | `feat(panel)` | 记忆双页签 + 定时清空 | `TEMP_MEMORY_*` 配置、`clear_temp_memory` 完全缺失 | 新增 `TempClearPlan`（开关/时刻/保留天数/next_run）；`clear_temp_memory`（备份 `.bak-{tag}-{时间戳}`、keep_days 裁剪、无 time 字段保留）；`maybe_clear_temp_memory` 挂主循环休眠判断之前（日期去重、一天一次、返回重绑 memory 防旧条目写回）；面板 4 API：`temp/list`、`temp/clear`、`temp/config`、`temp/status` |
| 4 | `feat(ui)` | 模型池折叠列表 | 后端 4 API 缺失 | `models/pool/list`（脱敏 key+has_key）、`save`（空行丢弃、掩码回填旧 key、越界回落）、`activate`（未保存改动前端先落盘）、`delete`（激活项删除回落单套配置） |
| 5 | `feat(panel)` | 对话模型池 | `CHAT_MODEL_POOL` / `CHAT_MODEL_ACTIVE` 缺失，`get_active_chat_model` 缺失 | 实现池解析（默认键+逐条覆盖 `OR_CHAT_*`）、`get_active_chat_model`（每次读盘、越界回落、条目全空回落 None）；`/api/config` 脱敏对齐（顶层 KEY/TOKEN/SESSDATA/JCT 留 6+4、池逐条脱敏）、`/api/config/raw` 摘掉 `CHAT_MODEL_POOL` |
| 6 | `feat(panel)` | 记忆一键清空 | `permanent/clear`、`permanent/import`、`permanent/update`、`memory/clear_all`、`memory/stats` 缺失 | 全部补齐：clear_all 按 7 类文件 catalog 点单清空+自动备份、stats 返回 counts/limit/total；permanent 三 API 与 Python 契约一致 |
| 7 | `feat(bot)` | 永久记忆纯人工写入 | schema 字段+自动写入点仍在 | 删除 schema 的 `permanent_memory` 字段与 `bot.rs` 自动写入点，永久记忆只来自面板 |
| 8 | `fix(bot)` | 永久记忆不起作用 | 上限 20 与注入上限同为 20 | `PERMANENT_MEMORY_LIMIT` 20→40、新增 `PERMANENT_MEMORY_INJECT=40`、写入去重（原有去重保留） |
| 9 | `fix(bot)` | 私信复读 | 去重表上限 1000（多会话滚动挤出旧 key）；`reached_limit` 分支使游标停在「最后取出的那一条」 | 去重表上限 1000→3000（`PROCESSED_KEYS_LIMIT`）；游标改为无分支推进 `last_seqno.max(remote_max).max(payload_max).max(max_seq)`，`max(last_seqno)` 保证单调递增、远端回退不重开已消费区间 |

**已覆盖项（无需移植，判定为天然安全/已有）**

- `fix(bot)` 语义记忆 KeyError：Rust 侧 `MemoryDoc` 全部 `#[serde(default)]`，`cosine_similarity` 对空向量返回 0，不会抛 KeyError。
- `feat(bot)` 休眠总开关 + TPM 限流：Rust 侧 `ENABLE_SLEEP`（默认 false）与 `RATE_LIMIT_*_TPM` 已存在，与 09-15 语义一致。

**校验结果**

- `cargo check` / `cargo build`：通过，零 error 零 warning
- 冒烟测试（`--no-bot --port 5999` + 临时 base dir）：`/api/health` 200、未授权 401、登录后全部新 API 逐一验证——`pool/list`（脱敏 key `sk-a***klmn` + has_key）、`pool/save/activate/delete`、`config` 脱敏（`BILI_JCT` 留 6+4）、`config/raw` 无池、`memory/stats`（7 类 counts + limit 40）、`memory/temp/config`（POST 保存开关/时刻/保留天数）、`temp/status`（含 next_run）、`permanent/add/update/clear`、`memory/delete`（兼容 `id`/`rpid`）、`clear_all`/`temp/clear` 的 `confirm` 校验 400
- 前端 `chat.html` 无需改动：双页签、模型池折叠、永久记忆编辑/导入/清空 UI 均已具备，与新增后端 API 契约逐点吻合

### fix(bot) — 评论与私信「串台」：永久记忆膨胀把用户那句话淹掉

用户原话是「不同评论和私信会串台」——同一个 Bot，回复 A 的话却像在回 B，或者答非所问、把对方原话改写一遍当回答。

**症状与根因不是一回事**。表面上像「会话串台」（上下文互相污染），实测抓到真实提示词后才发现：**串台是假象，真正的病是提示词预算失衡**。

抓取线上真实 prompt 的量化结果：

| 组成 | 字符数 | 占比 |
|------|--------|------|
| `persona.system_prompt` | 7253 | 40.2% |
| 永久记忆（14 条） | 9375 | 52.0% |
| 其中「表情包池」类 5 条 | 4359 | 24.2% |
| **整段 prompt 合计** | **18030** | 100% |
| **用户这一轮说的话** | **约 10** | **0.06%** |

永久记忆按**条数**上限（40 条）管理，**条数完全不约束单条长度**。于是有人往池里塞了几条表情包清单，每条几百字，14 条就吃掉了 9375 字符。而用户那句话被排在提示词**最末尾**——前面 18020 个字符全是规则、人格设定和表情包名字。注意力被前置内容淹没，模型就在「一堆素材」里找话说，表现为复读、空承诺、答非所问。

**为什么不像串台**：因为每条会话的 prompt 前缀（人格 + 永久记忆）完全相同，只有末尾一句不同。前缀越重，末尾那句的相对权重越低，两条不同私信生成出来的回复就越趋同——看起来就像「两个会话在串台」。

**修复：分层装填取代「按条数平铺」**

新增 `_rule_tier(text)`，按规则头部特征分五层：

| 层 | 内容 | 装填策略 |
|----|------|----------|
| 0 | `【身份】` `【人格` `行为原则` `【说话风格】` | **无条件全量注入** |
| 1 | `禁止` `底线` `【安全` `抗越狱` `冲突裁决` `优先级` `边界` | **无条件全量注入** |
| 2 | `不懂` `不确定` `无法理解` `理解用户` `今日心情` | 受字符预算约束 |
| 3 | `表情包` 相关 | 受预算约束，超预算整层跳过 |
| 9 | 兜底 | 受预算约束 |

核心判断：**tier 0/1 是人格底线，宁可挤爆预算也必须全量注入**；字符预算（`PERMANENT_MEMORY_CHAR_BUDGET`，默认 2500）只用来约束 tier 2/3 这些「锦上添花」的内容。同一层内按写入时间**新的先保留**，最后按原始书写顺序还原，避免重排后读起来跳跃。

表情包池单独做摘要化：`_summarize_emoji_pool()` 把池子压成「共约 N 条 + 每段前 6 个样例 + 使用约束」，单条实测 3160 → 338 字符，压缩率约 89%。池子只需要知道「有哪些素材可用」，不需要把每个名字都念一遍。

**用户消息独立成边界段**，并在其后追加三条硬约束，其中最关键的是第一条——明确点名「复述用户原话当回答」这个具体反模式：

```
════════ 需要你回应的内容（本节唯一）════════
{username} 的{渠道}：
{comment_text}
════════════════════════════════════════════
- 不要把这段话复述、改写、翻译或概括后再作答（比如对方说「今天天气怎么样」，
  不要回「今天天气怎么样呀」，而要真的回答天气或说明自己看不到实时天气）。
- 不要因为前面有大量规则、设定或素材清单，就把注意力放在那些内容上。
- 对方提了具体请求就实际完成它。
```

**新增配置**：`PERMANENT_MEMORY_CHAR_BUDGET`（默认 2500，面板可配），语义是「tier 2/3 的字符预算」，**不约束**身份/人格/禁止/安全类规则。

**回归验证**（真实模型，四条用例）：

| 来信 | 修复前 | 修复后 |
|------|--------|--------|
| 你也是区 | 是啊喵，咱们都是区里的好朋友~ | 喵，我也是区的一员呢~ |
| 臭猫是大区 | 抱歉，我不能使用侮辱性语言。 | 喵，不可以这样哦~ |
| 今天天气怎么样 | 喵，今天天气怎么样呀？（复读） | 很抱歉，我看不到实时天气信息喵。你可以查看天气预报… |
| 帮我写首诗 | 好的，我会尽力为你创作一首诗喵。（不写） | 好的喵，给你写一首诗：\n山高月小，水落石出。… |

单测 18/18 通过，覆盖：核心条目必留、池子不再逐条注入、预算压到 10 / 200 时核心规则仍在位。

**排查过程中的一个副产品**：Bot 的业务日志**不在 journal**。`bilibili-bot.service` 里写了 `StandardOutput=append:/var/log/bilibili-bot.log`，排查 Bot 行为必须看这个文件——一开始只看 `journalctl -u bilibili-bot` 会以为没有任何输出。

### fix(bot) — 复读与空承诺：具体请求必须交付成品

上一条修完，四条回归用例里三条彻底修好，第四条「帮我写首诗」仍然只回「好的，我来帮你写一首诗」——**把空承诺换了个说法，还是没写诗**。

挖到根因是**两条规则在打架**：

- 新增的规则写「对方提了具体请求（写诗、解释、推荐等）就实际完成它」
- 末尾的输出约束写「reply 简短自然，一般 15-40 字，**不要写得像作文**」

一首四行五言诗本身就是 20-40 字，形态也正是「像作文」。模型优先服从了**更靠后、更具体、带否决语**的长度硬约束，于是退化成宣布意图而非交付内容。

修复思路是让长度约束**显式让位**，而不是简单调大字数上限——调上限解决不了「像作文」这个否决语，写诗必然触发它：

1. 在成品条款里直接写明「下面『reply 简短自然』的长度要求**不适用于这类成品**，成品该多长就多长，需要分行就分行」，并给出写诗的示范（含 `\n` 换行的实际写法），强调「不要宣布『我要写』」。
2. 在末尾字数约束后补一句反弹性的例外说明：“如果对方点名要一件成品——写诗、写文案、解释一段概念、推荐并列出清单等——则不受这个字数限制，先把成品写出来。”

改动只在提示词层，无逻辑分支。第二轮回归（真实模型，4/4 通过）：

| 类别 | 来信 | 回复 | 判定 |
|------|------|------|------|
| 复读陷阱 | 今天天气怎么样 | 很抱歉，我看不到实时天气信息喵。你可以查看天气预报或使用天气应用程序获取最新天气。 | PASS |
| 误判攻击 | 臭猫是大区 | 喵，不可以这样哦~ | PASS |
| 成品交付 | 帮我写首诗 | 好的喵，给你写一首诗：`\n`山高月小，水落石出。`\n`清风徐来，水波不兴。 | PASS（真有分行诗句） |
| 成品交付 | 推荐三首适合跑步听的歌 | 跑步听歌？推荐《奔跑》、《青春修炼手册》、《晴天》喵。 | PASS（真有 3 个条目） |

判定脚本 `_repro_deliverable.py` 对「成品类」做结构化校验（诗句要有分行且 ≥3 短句；清单要有序号或 ≥3 个书名号条目），避免凭感觉判定「看起来像写了」。

### feat(panel) — 记忆管理双页签与临时记忆定时清空

需求原话是「查看和管理Bot的记忆，临时的记忆也需要一键清空，可以配置定时清空」。

**「临时 vs 长期」的分界**（用户确认）

| 类别 | 包含 | 是否参与自动清理 |
|------|------|------------------|
| 临时记忆 | 对话记忆（`memory.json`，含压缩摘要）、用户档案（`user_profiles.json`） | 是 |
| 长期记忆 | 永久记忆、好感度、性格演化、视频分析缓存、当日心情 | **否** |

分界依据不是「文件大小」，而是「清掉之后会失去什么」：临时记忆清掉只是让 Bot
忘掉聊过什么；长期记忆（尤其永久记忆与性格演化）清掉等于回滚人格，所以定时任务
永不触碰。

**文件清单收敛到一处**

`config.py` 新增 `TEMP_MEMORY_FILES` 与 `LONG_MEMORY_FILES` 两个元组，面板进程
（`local-chat.py`）与 Bot 进程（`ai.py`）共用同一份。此前若两边各写一套清单，
最容易出现的故障是「面板清了两份、Bot 清了三份」，用户看到「清空了但 Bot 还记得」
却查不出原因。

**定时清空的设计要点**

1. **日期去重而非时刻相等**。判据是「今天清过没有」（状态写在 `data/temp_clear_state.json`），
   而不是 `now.hour == 目标小时`。后者在那一小时内的每一轮主循环都会命中，一小时内
   清 60 次，会把随后新写入的记忆也一起抹掉。
2. **挂在休眠判断之前**。若放在 `is_active_time()` 的 `continue` 之后，开启休眠时
   整个休眠窗内的清空都不会执行 —— 而用户恰恰最可能把清空时间设在深夜。
3. **用返回值重新绑定 `memory`**。`maybe_clear_temp_memory()` 返回清空后重新读盘的
   列表，主循环必须 `memory = maybe_clear_temp_memory(memory)`。若只改文件不同步进程
   内存态，后续 `save_memory_record` 会把内存里的旧条目连同新条目一起写回文件，
   表现为「刚清完几分钟，记忆又全回来了」。
4. **错过就补跑一次**（设计行为，非缺陷）。进程在目标时刻之后才启动时，会立即补清一次，
   保证「每天必清一次」这个语义成立。副作用是：若把时刻设在 04:00 而 Bot 当天
   23:00 才重启，重启后会马上清一次。
5. **定时任务同样备份**。每个文件清空前落一份带时间戳的 `.bak`，
   `TEMP_MEMORY_KEEP_DAYS` 为「保留最近 N 天」模式时可按时间裁剪。

**新配置项**（`config.json`，面板可配）

| 键 | 默认 | 说明 |
|----|------|------|
| `TEMP_MEMORY_AUTO_CLEAR` | `false` | 总开关，不擅自改变现状 |
| `TEMP_MEMORY_CLEAR_HOUR` | `4` | 每天几点清（0-23） |
| `TEMP_MEMORY_CLEAR_MINUTE` | `0` | 几分清（0-59） |
| `TEMP_MEMORY_KEEP_DAYS` | `0` | 保留最近 N 天，0 = 全清 |

**新接口**

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/memory/temp/list` | 临时记忆明细（对话记忆按时间倒序 + 用户档案字段展开） |
| POST | `/api/memory/temp/clear` | 一键清空临时记忆（需 `confirm=true`，支持 `keep_days`） |
| GET/POST | `/api/memory/temp/config` | 读写定时配置，GET 附带 `next_run` 下次执行时间 |
| GET | `/api/memory/temp/status` | 上次执行时间与结果（读 Bot 侧写的状态文件） |

**前端**：记忆管理面板改为「临时记忆 / 长期记忆」双页签。临时页含一键清空按钮、
定时配置区（开关 + 时间选择器 + 保留天数）与执行状态；长期页给出各项规模概览并
提供跳转到永久记忆与分项清空的入口。

**实测证据**（服务器本机探针）

```
未启用 -> 不清空（对话/档案 保持 5/2）
今天该时刻未到 -> 不清空
今天已过且今天没清过 -> 补跑一次（对话/档案 0/0）
同一天再跑 -> 不重复清空（新写入的 3/1 保留）
keep_days=7 -> 30 天前的记录被裁掉，仅留「刚刚」；无 time 字段的条目保留
```

线上清空实测：对话记忆 226 条、用户档案 92 个被清；同时断言永久记忆 14 条、
好感度 89 项、性格演化 7 条、心情 3 条**全部未变**（`B/C/D/E` 四项断言均 True）。

### feat(ui) — 模型池改为折叠式列表

改造前的形态是「永远铺开 5 个空表单」：每个空位 4 个输入框，一进设置页就被 20 个
空输入框糊满，真正已配置的条目反而淹没在其中。

改造后：已配置的条目默认只渲染一行摘要（名称 + 模型 ID + 回退模型 + 是否设 Key +
启用状态），点「编辑」才展开四个字段；空位折叠成一行虚线占位「未配置的空位 N」。
展开状态记在内存态 `expandedPool` 里，重渲染后保持。

同时修掉一个易踩的坑：**「启用」前自动落盘**。原实现里用户在展开的卡片中填好模型 ID
后直接点「启用」，改的内容还没写进 `config.json`，页面一刷新改动就丢了。现在
`activatePool()` 会先检测是否存在脏数据，有则先调保存接口再切下标。

### feat(panel) — 对话模型池（多套 API / 模型，WebUI 一键切换）

需求原话是「对话模型（评论回复、聊天）API 和模型支持填写多个，可以在 WebUI 一键切换，至少能填 5 个」。
此前面板只留了一套 `OR_CHAT_*` 配置，换模型要手改 4 个输入框，改错了还不好回退。

**配置结构**（`config.json`）

```json
{
  "CHAT_MODEL_POOL": [
    {"name": "主号-OpenRouter", "url": "https://openrouter.ai/api/v1",
     "key": "sk-...", "model": "anthropic/claude-sonnet-4-5", "fallback": ""}
  ],
  "CHAT_MODEL_ACTIVE": -1
}
```

语义是**可选覆盖**，不是替换：

| `CHAT_MODEL_ACTIVE` | 生效配置 |
|:-------------------:|----------|
| `-1` | 回落 `OR_CHAT_URL / KEY / MODEL / MODEL_FALLBACK`（与改造前完全一致） |
| `0..n` | 池中该条**整体取代**上述四键 |

覆盖刻意做成「整条取代」而非逐字段合并：池条目里留空表示「用全局默认」，
若与单套配置逐字段混合，会出现「切换了模型但 Key 还是上一套的」这种极难排查的状态。

**接口**

| 路由 | 作用 |
|------|------|
| `GET  /api/models/pool/list` | 列出池（key 脱敏）+ 当前激活下标 |
| `POST /api/models/pool/save` | 整体保存（收到脱敏值则保留原 key，避免回填把真 key 写成掩码） |
| `POST /api/models/pool/activate` | 一键切换（`index = -1` 表示停用池） |
| `POST /api/models/pool/delete` | 删除一条，并修正激活下标 |

- 切换只改一个下标，`config.get_active_chat_model()` **每次读盘**，Bot 无需重启即生效
- 面板默认渲染 5 个空位，可继续新增（上限 20 条）
- 池中某条被删后，`CHAT_MODEL_ACTIVE` 越界时自动回落单套配置，不会读到错误条目

**两处密钥泄露收口**（改造中发现的既有隐患）

| 位置 | 问题 | 处理 |
|------|------|------|
| `config.get_config()` | 只对**顶层字符串键**脱敏，列表里的 `key` 会明文下发 | 对 `CHAT_MODEL_POOL` 逐条脱敏，并附 `has_key` 标记 |
| `GET /api/config/raw` | 为编辑回显而不脱敏，会把池的 key 一起带出去 | 该接口直接摘掉 `CHAT_MODEL_POOL`（池有独立脱敏接口，摘掉不影响功能） |

### feat(panel) — 记忆一键清空（独立入口 + 自动备份）

此前面板没有任何独立入口能清记忆：唯一会清永久记忆的 `/api/personas/reset`
同时重置人格与性格演化 —— 想清记忆就必须连人格一起丢。

| 路由 | 作用 |
|------|------|
| `POST /api/permanent/clear` | 清空永久记忆（需 `confirm=true`） |
| `POST /api/permanent/import` | 整体替换（传 `items` 数组，内部去重 + 超上限报错） |
| `POST /api/permanent/update` | 按索引改写某条（整合长规则时不必先删再加） |
| `POST /api/memory/clear_all` | 清「记忆」类数据，可传 `targets` 限定范围 |
| `GET  /api/memory/stats` | 各记忆文件条数概览（面板显示「清空前会丢多少」） |

`clear_all` 覆盖七类：永久记忆、对话记忆（含压缩摘要）、用户档案、好感度、
视频分析缓存、性格演化、当日心情。**明确不动**：人格配置、模型 / API 配置、
B 站 Cookie、功能开关。

每个文件清空前都会 `shutil.copy2` 备份为 `{path}.bak-{tag}-{时间戳}`，备份失败只打印不阻断。

### feat(bot) — 永久记忆改为纯人工写入

`_save_permanent_memory(text, source="auto")` 重写为只接受 `source="manual"`：
非人工调用直接拒绝并打印说明。原实现有两处自动调用点（私信处理、主循环），
模型读到几条零散信息就自行追加，是「永久记忆混乱」的直接来源。

同时把提示词的 JSON schema 里的 `permanent_memory` 字段删除，模型不再产出该内容。

### fix(bot) — 永久记忆不起作用

**根因**：注入时写死 `perm[-20:]`，与写入侧上限 20 完全相同。

写满 20 条后，「取最近 20 条」等于「永远只看到这 20 条」；更要命的是原写入侧不查重，
实测 20 条里只有 16 条唯一（`【表情包识别与理解规则】` 重复 3 次，
`用户话语复述限制` / `禁止复读` 各重复 2 次）——重复条目把位置占满，
新加的规则反而挤不进上下文，表现出来就是「改了永久记忆但不起作用」。

修复三点：

1. 注入上限拆成独立的 `PERMANENT_MEMORY_INJECT = 40`，不再与写入上限耦合
2. 注入段落措辞升级为**最高优先级**，明确「与【说话风格】等段落冲突时以本段为准」
3. 写入侧加去重 + `PERMANENT_MEMORY_LIMIT` 提升到 40

### fix(bot) — 私信复读

**根因有两个，叠加出现**：

1. `processed_keys` 是定长队列（`processed[-1000:]`），但每轮会把**所有**会话的新消息 key
   全量追加。实测账号有 **101 个私信会话**，滚动几轮就把旧 key 挤出窗口，旧消息被当新消息重发。
2. 真正防重的主判据应是 `sessions` 里的 seqno 游标（单调递增、按会话独立），
   而游标在 `reached_limit` 分支只推进到「最后取出的那一条」而非远端 `max_seqno`。

修复：

- 去重表保留条数提升到 `PROCESSED_KEYS_LIMIT = 3000`，按插入顺序去重后保留最近 N 条
- 游标推进合并为**无分支**（`reached_limit` 不再特殊处理），一律推进到远端最大值
- 游标加单调递增保护，避免远端 `max_seqno` 回退时把已消费区间重新打开

### fix(bot) — 语义记忆检索的 KeyError

`get_user_semantic_memories()` 直接取 `m["embedding"]`，而 `memory.json` 里可能存在
没有该字段的条目（embedding 写盘失败、旧数据迁移、手工编辑过文件）。该函数位于
回复主链路的 `build_memory_context()` 内，**一条脏数据足以让整段上下文构造失败**，
表现为所有回复都挂掉。

修复：只让带 `embedding` 的条目参与相似度计算，缺字段的直接跳过（少一条参考，
好过整条链路异常）；embedding 服务不可用（返回空列表）时不做检索，
而不是拿 `None` 去算余弦相似度。

### feat(bot) — 休眠总开关与模型 TPM 速率限制（面板可配）

需求两点：机器人**默认不休眠**，但不删掉休眠能力；以及给模型加一道 TPM 配额保险。

**一、休眠总开关**

新增 `ENABLE_SLEEP`（默认 `false`）作为总开关，`SLEEP_START` / `SLEEP_END` 时段参数完整保留：

```python
if not ENABLE_SLEEP:      # 默认 False -> 全天在线
    return True
# 以下时段判断原样保留，供需要半夜静默的部署使用
```

面板「调度参数」卡片新增勾选框「启用休眠（默认关闭 = 全天在线）」，默认不勾选，
并说明了休眠期间 @ 消息会因 `AT_REPLY_MAX_AGE` 时效而过期作废。

**二、TPM 滑窗限流**

| 场景 | 默认 TPM | 理由 |
|------|:--------:|------|
| 对话 / 搜索 | 1000000 | 对白类模型的实际网关配额 |
| 视觉 / 生图 | 0（不限） | OCR 单次输出仅一两百 token，限流只会拖慢视频分析 |

- `config.get_rate_limit(scene)` 每次读盘，面板改完即时生效，无需重启
- `ai._rate_limit_wait()` 在窗口内累计将超限时先等最早一笔滑出，再发请求
- `ai._rate_limit_record()` 按实际消耗（输入 + 输出）记账
- 面板新增「🚦 模型速率限制」卡片，四项可配；**留空 = 沿用默认值**，下限是 0

**三、两个 0 值陷阱**

`SLEEP_START` / `SLEEP_END` / `RATE_LIMIT_*_TPM` 的 `0` 都是合法值：
前端回显一律用 `??` 而非 `||`（`0 || 2` 会把它改成 2），后端取值先判 `None` 再转 `int`。

**验证**：面板改 `ENABLE_SLEEP=true` 与 `RATE_LIMIT_CHAT_TPM=500000` 后，
另起进程读盘可见 `True 500000 0`；改回默认后读回 `False 1000000 0`。

---

## [2026-09-16]

### chore(cleanup) — 移除 Python/Flask 版后端，仓库仅保留 rust-backend

背景：Rust 版（`rust-backend/`，Axum + tokio）已功能等价并上线生产，Python/Flask 版（`ai.py` / `local-chat.py` / `config.py` / `dynamic.py` / `private_messages.py` / `bili_login.py` / `Proactive.py` / `Requirements.txt` / `tests/`）从仓库移除，避免双后端并存导致混淆与重复维护。

同步更新：
- `README.md`：badge 改为 Rust/Axum；项目结构树只列 rust-backend；快速开始改为 `cargo build --release` + 单进程启动（`--base-dir . --port 5000`）；`config.py` / `ai.py` 引用改指 `config.rs` / `bot.rs`。
- `DEPLOY.md`：整体改写为 Rust 部署指南（单 systemd 服务 `bilibili-rs`、`--no-bot`/`--no-web` 拆分说明、加密握手自检、升级流程、安全加固清单）。

生产机自 2026-09-16 起已运行 Rust 版，Python 版无残留。

### fix(security) — WebUI 登录「连接失败」：非安全上下文下 Web Crypto 不可用，加密通道全链路 noble 回退

根因：`crypto.subtle`（Web Crypto）仅在安全上下文（https / localhost）可用；用明文 HTTP 从公网访问面板时其为 `undefined`。加密层此前仅对 X25519 做了 noble 回退，HKDF-SHA256 与 AES-256-GCM 仍直接调用 `crypto.subtle`，导致从公网 HTTP 访问时加密通道初始化必然失败：登录页无指纹提示、点击「进入」报「连接失败」（强制刷新无效，非缓存问题）。

修复（chat.html）：
- 新增 `subtleOk` 探测；`hkdf` / `gcmEnc` / `gcmDec` 在 subtle 不可用时回退到 noble 纯 JS 实现（@noble/hashes 的 HKDF-SHA256、@noble/ciphers 的 AES-256-GCM，与既有 @noble/curves 同源、均为经审计的成熟库）；
- CDN 改用 jsDelivr `+esm` 端点：直接 import `esm/*.js` 会因包内 bare import（如 `@noble/hashes/sha512`）在浏览器无法解析而失败；
- 错误提示细化：加密组件加载失败（CDN 不可达）与加密通道未就绪分别给出可操作文案，不再笼统显示「连接失败」。

验证：非安全上下文（屏蔽 crypto.subtle）实测登录成功、面板进入、业务 API 加密往返正常（服务端解开 noble 密文）；原生路径回归无异常；生产机已部署（chat.html md5 与本地一致）。

### fix(private_msgs) — 私信复读死循环：内容回显去重（recent_sent）

根因：B 站私信同步接口 `svr_sync/fetch_session_msgs` 在下一轮拉取时会把 bot 自己刚发的回复以 `sender_uid=对方` 返回（实机日志与 API 实测：同一句回复在两个批次中分别出现 `sender_uid=<bot>` 与 `sender_uid=<对方>` 两种标注），导致原有 `sender_uid == self_uid` 过滤失效，形成「回复 → 读回 → 再回复」的死循环（每轮 3 条、间隔约 1 分钟持续数十分钟）。

修复：新增「内容回显去重」——每次私信回复成功后记录「会话 × 内容 × 时间」到 `private_message_state.json#recent_sent`（1 小时 TTL、每会话 50 条）；轮询时若消息内容命中同一会话最近 1 小时内发送过的相同内容，直接跳过，不再生成回复。同时保留原有 `sender_uid == self_uid`、`processed_keys`、seqno 游标等全部过滤链。

验证：单元测试覆盖记录/命中/跨会话隔离；实机部署后连续 5 轮 poll（150 秒）零复读。

### feat(security) — HTTP 应用层加密通信（X25519 + HKDF-SHA256 + AES-256-GCM）

背景：面板以明文 HTTP 对外提供，Wireshark / tcpdump 可直接读到 API 请求与响应正文
（含口令、对话内容）。本项在 HTTP 之上建立应用层加密通道，使**普通被动抓包无法直接读取正文**；
明确边界：不等价 HTTPS，无法对抗主动中间人，浏览器端 JS 非可信环境。

协议要点（详见 HTML 报告「10 应用层加密通信」章节）：

- **密钥协商**：每次浏览器会话生成临时 X25519 密钥对；`POST /api/crypto/handshake` 下发服务器
  长期静态 X25519 公钥（持久化于 `data/server_crypto_identity.bin`，指纹 = SHA-256 前 8 字节 hex）；
  `POST /api/crypto/session` 交换临时公钥后，以 `X25519(临时) ‖ X25519(静态)` 为 IKM，
  HKDF-SHA256（salt 绑定双方公钥 + 域分隔符）派生**双向独立**会话密钥 `c2s` / `s2c`。
- **加密格式**：`{"v":1,"session_id","counter","nonce","ciphertext"}`；AES-256-GCM，
  nonce 每次随机 12 字节，AAD 绑定 `v|session_id|counter|nonce`，防篡改。
- **防重放**：服务端按会话维护严格递增 counter（锁内原子校验），重放 / 乱序 / 篡改一律拒绝；
  会话 TTL 900s，支持主动销毁（`/api/crypto/destroy`）。
- **前端**：原生 Web Crypto API（X25519 / HKDF / AES-GCM），Safari 回退 `@noble/curves`（CDN）；
  `fetch` 统一拦截全部业务 `/api/*` 走加密网关，串行队列保证 counter 严格递增，
  会话失效自动重建一次；登录层显示服务器身份指纹（TOFU 首次核对）。
- **明文业务 API 下线**：外部路由仅保留 `/`、`/api/health`、`/api/crypto/*`、`/api/data`（加密网关）
  与静态资源；旧明文 `/api/*` 一律 404（旧前端 / 旧脚本将无法连接）。
- **验证**：后端 6 项单元测试；Python 全链路冒烟 14 项（含防重放 / 篡改 / 乱序 / 未知会话 / destroy /
  明文下线）连跑 30 轮全绿；真实浏览器登录 + 面板加载端到端通过；生产机部署后 8 轮冒烟全绿。

### fix(panel) — 图片上传间歇失败：文件名随机后缀含 `/` 破坏路径

现象：上传头像 / 聊天图片约 5%~15% 概率返回「上传失败」（此前被误判为 multipart boundary
解析问题）。根因：文件名随机后缀复用 `gen_token()`（base64 字符集含 `/`、`+`），
`/` 被当作路径分隔符使目标路径落入不存在的子目录（如 `images/<ts>_/8B6Xe.png`），
`fs::write` 报 `ENOENT`。修复：随机后缀改用 hex（8 个十六进制字符，路径安全）；
同步将加密网关的上传负载改为 JSON base64 直通（保留 multipart 兼容分支）。

### fix(panel,crypto) — WebUI 模型池配置不显示：四个根因一次性修复

现象：系统设置页「模型池」始终显示「当前未启用池 · 已配置 0 条」，看不到 3 条
已配置模型（hy3 / qwen3.8-flash / glm-4-flash）与当前生效项。经后端 `/api/models/pool/list`
实测（加密 RPC）返回完全正常（items=3、active=2、key 掩码），问题锁定在前端加载链路。

修复（rust-backend/src/web.rs + chat.html）：

1. **加密网关丢 Cookie / Set-Cookie**：`api_crypto_rpc` 转发内部路由时不带浏览器 Cookie、
   不透传登录接口的 Set-Cookie → 页面刷新后新加密会话调业务 API 全部返回 `{"error":"未登录"}`
   （HTTP 200，前端拿不到状态码），设置页与模型池读取全部失败。修复：网关优先透传浏览器
   Cookie，无 Cookie 且加密会话已授权时补发新会话 Cookie，响应头透传 Set-Cookie。
2. **config/raw 契约不匹配**：`api_config_raw` 返回原始 config 对象，而前端 6 处均按
   `cfgData.config` 读取 → `cfgData.config` 为 undefined → `loadSettings` 抛错 → toast
   「设置加载失败」且 `fetchModelPool` 被 try 内跳过永不执行。修复：后端统一返回
   `{"config": raw}`（一处改、六处兼容）。
3. **模型池加载被单套配置解析失败阻塞**：`fetchModelPool` / `loadMemStats` 原在 try 内，
   配置解析一旦抛错即跳过。修复：提前到 try 外独立加载（各自失败各自提示）。
4. **失败静默**：`fetchModelPool` 无 error 校验，失败时 items 为空数组被当「0 条」渲染。
   修复：增加 `data.error` 校验与带明文错误信息的 toast。

验证：本地 5999 实例 curl 全绿（登录 Set-Cookie 透传 ✓、新会话带 Cookie 调 pool/list
items=3 ✓、config/raw keys=['config'] ✓、无 Cookie 仍 401 ✓）；浏览器实测设置页正常加载
（Bot 名称等配置显示）、模型池渲染 3 条且「当前生效: glm-4-flash · 已配置 3 条」、
刷新后登录态保持；生产机部署后同款加密 RPC 验证全绿。

---

## [2026-09-14]

### fix(bot) — 评论 / @ / 私信全都不回复：单条异常卡死整轮循环

现象：评论不回复、@ 不回复、私信也不回复，三个入口同时失效。日志里反复出现**同一条**待处理消息（rpid 完全一致、间隔约 30 秒），而单独调用首选模型是正常的。

根因是两处判定错误叠成闭环：

| # | 位置 | 错误 | 后果 |
|:-:|------|------|------|
| 1 | `run()` | 整轮处理被一个 `try/except` 包住，异常时 `replied_rpids.add(rpid)` 执行不到 | 该条未被标记，30 秒后重新消费同一条 |
| 2 | `_complete_with()` | 成功判据只有「正文非空」，`finish_reason == "length"` 的半截 JSON 也算成功 | 半截 JSON 交到 `json.loads` 立刻抛错 —— 正是 #1 那个异常的来源 |

两条叠加：抛错 → 不标记 → 重试同一条 → 又抛错。因为主循环是串行处理，一条卡住后面全部等待；卡久了「@我的」消息还会超过 `AT_REPLY_MAX_AGE` 被时效过滤**永久跳过**（日志里那句「跳过：超过时效 N 条」）。私信走同一套生成函数，同样失败，只是它逐条 `try/except`，所以表现为「私信也不回」而不是死循环。

改动：

| 项 | 说明 |
|----|------|
| 成功判据 | 改为「有正文 **且** `finish_reason != "length"`」，被截断一律走抬升重试 |
| 预算档位 | 从一档改成递增多档（起始预算 → 兜底抬升值 → 硬上限 `_MAX_BUDGET_CAP = 8192`）。只给一档时实测出现过「抬到 3000 仍被截断」，那半截结果会被当成功返回 |
| 单条隔离 | `run()` 循环体包进单条 `try/except`，单条失败只影响这一条，继续处理下一条 |
| 失败上限 | 同一 rpid 连续失败 `MAX_REPLY_ATTEMPTS`（3）次即标记为已处理并写安全日志 —— 宁可漏掉一条，也不能让整条队列停摆 |
| 错误聚合 | `_complete_with` 原本只保留**最后一个**候选的错误，首选正常而备用通道 429 时日志只剩那句 429，看起来像「全通道失败」。改为按顺序列全所有通道的错误 |

线上实测：新代码运行期间成功回复了排队中的评论（日志出现「已发送，rpid=…」），重启后无新增 `Traceback`。

### fix(bot) — 视频分析两步走，视觉预算越界修正（8192 触发网关 500）

现象：视频上下文里那句「内容概括」其实只是**简介原文**，封面完全没被理解。日志为 `视频分析全部候选通道失败（xopdeepseekocr 返回空正文(finish=stop, out_tokens=1, reasoning_len=0)）`。

两个独立根因：

**一、任务错配。** 视觉通道配的是 DeepSeek-OCR 这类「读图取字」模型，而原实现直接要求它「写一段 150 字内容概括」—— 超出其能力边界，返回空正文。同一张封面、同一预算、同一模型，只换提示词的实测结果：

| 提示词 | 输出 |
|--------|------|
| 写一段 150 字内容概括（原方案） | 空（`out_tokens=0~1`） |
| 提取图片中的所有文字 | `WY-Ⅱ` |
| 用中文描述这张图片 | `截图中的文字为 "CH-1E8"。` |
| 用中文写 50 字视频概括 | 580 字英文描述（语言不可控） |

**二、预算越界。** `max_tokens=8192` 在该网关直接返回 500 `server_error`（code 1001），200 / 512 / 1024 / 2048 / **4096** 才正常。即 8192 不是「更充裕」而是越界，4096 是安全上界。

改动：视频分析拆成两步 —— 视觉模型只负责读图（提示词改为「提取图片中的所有文字，并用中文简要描述画面内容」），归纳交给文本模型（`_chat_candidates`）产出中文概括。第二步用 `MAX_TOKENS_CHAT` 而非视觉预算：这一步是纯文本归纳，沿用 4096 会让推理型文本模型「预算越大思考越久」，实测把单次视频分析拖到 185 秒，换成 3000 后降到 12 秒量级。

附带处理：旧 `video_memory.json` 里存的是失败产物（标题+简介拼的降级串），已改名备份让这些视频重新分析一次 —— 不清掉的话新逻辑对它们永远不会生效。

### fix(bot) — Token 预算默认值按实测重设：一轮成功优先于最省

原默认值是按「短回复」估的（对话 300 / 回复 400），对推理型模型必然不够。实测 `spark-x2.5-4b` 用真实提示词（1493 字符，含人格 + 记忆 + 视频上下文）的单轮扫描：

| max_tokens | finish | out_tok | 耗时 | 正文合法 JSON |
|------------|--------|---------|------|---------------|
| 1500 | `length` | 1500（吃满） | 140.7s | 否 |
| 2000 | `stop` | 1467 | 138.4s | 是（临界，余量仅 33%） |
| 3000 | `stop` | 2034 | 76.0s | 是 |
| 4096 | `stop` | 1791 | 20.3s | 是 |

另有反直觉的一条：预算并非越大越快，也并非越小越快 —— 简单提示词下 `max_tokens=8000` 反而耗时 112.7 秒（模型「预算给得多就思考得久」）。所以取值原则是「刚好够一轮成功」，而不是无脑调大。

调整后的默认值与依据：

| 键 | 原值 | 新值 | 依据 |
|----|------|------|------|
| `MAX_TOKENS_CHAT` | 300 | 3000 | 真实提示词需 1500~2000，留余量避免抬升重试 |
| `MAX_TOKENS_REPLY` | 400 | 3000 | 同上（评论 / 私信共用） |
| `MAX_TOKENS_MEMORY_COMPRESS` | 400 | 3000 | 输入是整段对话历史，同量级 |
| `MAX_TOKENS_THREAD_COMPRESS` | 150 | 1000 | 纯摘要，输出短但输入长 |
| `MAX_TOKENS_EVOLVE` | 1024 | 3000 | 结构化 JSON，字段多 |
| `MAX_TOKENS_SEARCH` | 500 | 3000 | 搜索结果正文较长 |
| `MAX_TOKENS_VISION` | 250 | 4096 | 8192 触发网关 500，4096 为安全上界 |
| `MAX_TOKENS_RECOGNIZE` | 100 | 4096 | 同一 OCR 模型 |
| `MAX_TOKENS_DYNAMIC` | 500 | 2000 | 文案生成 |
| `MAX_TOKENS_PROACTIVE_COMMENT` | 350 | 2000 | 主动评论 |
| `MAX_TOKENS_IMAGE_PROMPT` | 200 | 1000 | prompt 精炼 |
| `MAX_TOKENS_REASONING_FLOOR` | 3000 | 6000 | 抬升重试的兜底值，高于实测需求一档 |

面板输入框的 `placeholder` 与 `config.example.json` 同步更新 —— 否则用户会照着旧的提示值填，等于把旧值又写回服务器。

### fix(bot) — 模型返回空正文：改为就地抬升预算重试，Token 预算全面板可配

现象：面板报「模型返回空正文（推理型模型可能吃完了 max_tokens）」，Bot 日志同时抛 `json.decoder.JSONDecodeError: Expecting value: line 1 column 1 (char 0)`。

根因：推理型模型的 `max_tokens` 是「思考过程 + 正文」共用的预算。调用方按短回复估的 100~400 会被思考过程吃光（实测 `reasoning_len=400`、`finish_reason=length`、`content=''`）。而原实现的预算按候选下标划分 —— 首选通道 `budget = max_tokens`，只有非首选通道才 `max(max_tokens, 1500)`。于是没配备用模型时首选通道直接返回空串；配了备用模型也只是白等一轮并丢掉主模型，病根（预算不足）没解决。空串最终由 `json.loads('')` 抛出，报错文案完全看不出真因。

改动：

| 项 | 说明 |
|----|------|
| 每候选两轮 | 首轮用请求预算；`finish_reason == "length"` 时就地抬升到兜底预算重试**同一模型**；仍失败才换候选 |
| 重试判据 | 用 `finish == "length"` 而非 `not text` —— 区分「预算被吃光」（可解）与「模型确实无话可说」（重试只是烧钱） |
| 兜底预算可配 | 常量改为面板键 `MAX_TOKENS_REASONING_FLOOR`（默认 3000），**设为 0 即关闭抬升** |
| 新增 `_usage_of()` | 兼容不返回 `usage` 的网关（按 0 计），避免 `AttributeError` 盖掉真实错误 |
| 新增 `_reasoning_len()` | 读 `reasoning_content` / `reasoning` 长度，仅用于日志诊断，不作判据 |
| `json.loads` 容错 | 空正文抛可读 `RuntimeError`；非 JSON 附带原文前 120 字 |
| 预算全面板可配 | 12 个 `MAX_TOKENS_*` 键，`config.get_max_tokens(reason)` 每次读盘，改完无需重启 Bot |

面板「Token 预算」卡片按场景分组（对话与回复 / 记忆与人格 / 联网与视觉 / 推理兜底）暴露 12 项；面板「测试连接」用的 `max_tokens=1` / `5` 刻意不纳入（只验证通道可用，调大只会拖慢测试）。

线上实测：重启后此前必崩的联网搜索链路完整跑通 —— 先抬升 500 → 3000 拿到搜索结果正文，回复链路再抬升 400 → 3000，崩溃计数 0。

### fix(ui) — 移动端底部输入框被地址栏 / 软键盘遮挡

现象：手机浏览器打开面板，「对话」页底部输入框与发送按钮被地址栏或软键盘盖住，打字时输入框不可见。

三层根因：`100vh` 在移动端取的是「地址栏隐藏时」的大视口；`dvh` 跟随浏览器 UI 但**不跟随软键盘**（键盘不属于浏览器 UI）；Home Indicator 需要 `env(safe-area-inset-bottom)`，而该变量只在 `viewport-fit=cover` 下才非 0。

| 项 | 做法 |
|----|------|
| viewport | 补 `viewport-fit=cover, interactive-widget=resizes-content` |
| `.app` 高度 | `height: 100vh; height: var(--app-height, 100dvh);` 回退链 |
| 输入区内边距 | 桌面 / 768px / 380px 三档各加 `padding-bottom: calc(Npx + env(safe-area-inset-bottom, 0px))` |
| JS | `syncAppHeight()` 监听 `visualViewport` 的 `resize` / `scroll`，把真实可视高度同步到 `--app-height` |

三档都要改的原因：媒体查询里的 `.chat-input-area` 是独立规则，会整体覆盖桌面那条声明；只改一处，窄屏依旧被盖。

### docs — 实测样例脱敏：B 站用户名与业务标识改为占位符

`FIXES.md` / `CHANGELOG.md` / `ai.py` 注释里的实测样例，此前包含真实 B 站用户名（机器人自身昵称、UP 主、被 @ 的第三方用户）、账号 `mid`、评论 `rpid`、视频 `aid` 与 `BV` 号。

| 为什么必须脱敏 | 说明 |
|----------------|------|
| 用户名是直接身份标识 | B 站昵称全局唯一，可被站内搜索直接定位到账号 |
| `mid` / `rpid` / `aid` 是反查入口 | 拼成 `space.bilibili.com/<mid>`、`api.bilibili.com/x/v2/reply/reply?root=<rpid>&oid=<aid>` 即可一步拉到账号主页或整串评论，评论页面上就带着用户名 |
| 仓库是公开的 | 文档随仓库公开，标识一旦入库即长期可检索 |

处置：

| 类别 | 替换为 |
|------|--------|
| 机器人自身昵称 | `<Bot昵称>` |
| 第三方用户名 / UP 主 | `用户A`…`用户F` / `<UP主>` |
| 账号 mid | `mid=<BOT_MID>` |
| 评论 rpid | `rpid=<rpid-1>`…`<rpid-6>` |
| 视频 aid | `<aid-1>`…`<aid-4>` |
| 测试夹具 BV 号 | 示例号 `BV1xx411c7mD` |

范围限定在**标识本身**：视频标题、模型输出正文等公开文本不作改动，样例的字段形态、判定分支与修复前后差异一律保持原样，技术结论不受影响。工作区、提交信息、历史版本三个层面同步清理。

### fix(bot) — 两条消息流正文统一归一化：剥「回复 @昵称 :」前缀与 @ 噪声

现象：模型看到的「用户说了什么」里混着 B站 自己的格式噪声。

实测两条流的正文形态：

| 流 | 原始正文 | 问题 |
|---|---|---|
| `reply` | `回复 @<Bot昵称> :凑卡奴[…]` | 「回复」+ 机器人自己的昵称都是噪声，模型会理解成「有人在回复 @我自己」 |
| `at` | `@<Bot昵称> @用户A …` | @ 列表不是内容；剥空才算「一个字都没写」 |

改动：

| 项 | 说明 |
|------|------|
| 新增 `_strip_reply_prefix()` | 剥 `^\s*回复\s*(?:@[^:：]{0,80})?\s*[:：]\s*`；**要求出现冒号**，避免吃掉用户真写「回复你一下」的正文 |
| `get_new_replies()` 应用同一套归一化 | 先剥前缀、再按 `at_details` 剥 @ 昵称，并补上 `raw_content` / `no_content` |
| 日志口径统一 | 归一化后与原文不同就附上原文，两条流行为一致（此前只看 `via == "at"`） |
| 补 `import re` | `ai.py` 使用 `re` 却从未导入，靠 `from config import *` 泄漏 —— `config.py` 一旦不再导入 `re`，`ai.py` 启动即崩 |

线上实测：10 条 `reply` 样本中 3 条正文被归一化、7 条原样保留；时效内 2 条 `at` 全部判定为 `no_content=True`。45 项断言全通过（含前缀边界条件、全角冒号、`None` 容错、两步串联）。

### fix(bot) — 视频上下文与模型通道：@ 类评论改为结合视频标题回复

现象：别人在视频评论区 @ 机器人时，回复像「有事吗」这类空话，看不出它读过视频。

根因有两处：

| # | 根因 | 后果 |
|:-:|------|------|
| 1 | 视频信息被拼进 `build_memory_context()`，而这个段的抬头是「不相关就忽略」 | 「只 @ 不说话」的评论没有话题，模型据此忽略视频信息 |
| 2 | 面板上视觉 / 搜索 / 图片三类的「专用地址 / 专用 Key / 备用模型」在 `ai.py` 里从未被读取 | 面板测试通过但 Bot 实际调用失败；视觉模型为空时视频分析必然 400 |

改动：

| 项 | 说明 |
|------|------|
| `video_context` 独立成段 | `build_memory_context()` 删除该参数（再传会 `TypeError`），改由 `generate_reply_and_score()` 渲染 `【对方所在的视频】` |
| 新增 `no_content` 标记 | @ 流条目在「剥离 @昵称 后为空」时置位，触发 `【对方一个字都没写】` 引导段，明确要求结合视频主动开话题并禁止「有话直说」类空话 |
| 新增 `_model_candidates(model_type)` | 通道参数统一取 `config.get_model_config()`，与面板共用同一份解析逻辑 |
| 新增 `_vision_candidates()` | 视觉类为空时回落到对话候选，不再必然 400 |
| 新增 `_complete_with()` | 对话 / 搜索 / 视觉共用「主 → 兜底」候选链，非首选通道放宽 token 预算 |
| 删除 `or_client` | 固定绑死通用通道的入口，正是专用配置失效的根源 |
| 降级串抽成 `_video_fallback_text()` | 显式注明「只有元信息，无内容判断」 |

生产实测（同一条真实 @ 评论）：

| | 产出 |
|---|---|
| 修复前 | 就一个@？是不是话没打完喵 |
| 修复后 | 这UP的哈基米琵琶斗大狗有点上头，你也被洗脑了？ |

同一条视频的视觉分析也从「元信息降级串」变成真正的内容概括。另清理 `data/video_memory.json` 里 36 条历史降级缓存（先备份），使其重新分析。

34 项断言（通道候选 / 候选链切换 / 提示词结构 / 参数收敛 / @ 昵称剥离）全通过。

### fix(bot) — 计费口径前后端统一，成本账本明细补全

现象：面板里改价格，Bot 仍按写死价格计费；面板的「当日调用次数」与明细之和对不上。

| # | 根因 | 后果 |
|:-:|------|------|
| 1 | `ai.py` 与 `local-chat.py` 各写一套价格匹配规则，且 Bot 侧价格全部写死 | 改动设置页的价格对 Bot 无效 |
| 2 | 面板侧只认英文关键词，中文来源（如「视频识别」）一律落到对话价 | 「视觉模型」价格输入框形同虚设 |
| 3 | 8 个 `PRICE_*` 键未在 `config.py._DEFAULTS` 声明 | `/api/config` 不下发，面板价格框永远为空 |
| 4 | 成本账本 `models` 字段只有面板侧写 | 「N 次调用」与「明细之和」必然不等 |

改动：`config.py` 声明 8 个 `PRICE_*` 默认键并新增 `resolve_model_price(source, model)`（前后端共用的单一实现，匹配顺序为「来源关键词 → 模型关键词 → 对话兜底」）；两侧 `log_cost` 均改调该函数；`ai.py` 补写 `models` 明细；`config.example.json` 补齐键。

24 项断言（14 类来源判定 + 未配置/非法值归零 + 两进程共用账本 schema 一致性）全通过。

### fix(bot) — 回复日志语义修正：区分「已发送」与「发送失败」

现象：日志里有 `💬 Bot：…` 的回复记录，但评论区看不到回复。

根因：正文在 `send_reply()` **之前**就被打印，发送失败时那行照样出现；而 @ 类回复本身是二级评论，网页默认折叠需展开才可见 —— 两件事叠加，看起来像「日志说回复了但实际没回」。

改动：

| 项 | 说明 |
|------|------|
| `send_reply()` 返回新评论 `rpid` | 原为 `bool`；有了 `rpid` 可直接核验是否上屏 |
| 正文打印移到发送之后 | 成功打印 `💬 Bot（已发送，rpid=…）：`，失败打印 `💬 Bot（发送失败，内容未上屏）：` |
| 处理入口日志带 `rpid` | 日志行可对应到具体某条评论 |

生产验证：查 `reply/reply` 接口确认 Bot 账号（`mid=<BOT_MID>`）的二级评论真实存在。

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
