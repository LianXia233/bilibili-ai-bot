# bilibili-ai-bot Rust 后端

`bilibili-ai-bot`（原 Python/Flask 实现）后端的完整 Rust 重写版：
Python/Flask → **Rust + Axum + tokio + reqwest + serde**，功能等价，API 契约与数据格式兼容。

## 功能对照

| 原 Python 模块 | Rust 模块 | 说明 |
| --- | --- | --- |
| ai.py（主循环/回复/记忆/视频分析） | `bot.rs` + `memory.rs` + `personality.rs` | 评论/@ 轮询、回复生成（逐字移植 prompt 与 JSON schema）、好感度/里程碑/心情/节日（含农历表驱动）、记忆压缩、用户档案、自动拉黑 |
| local-chat.py | `web.rs` | Axum 面板，40+ 路由全部对齐：登录（RSA-OAEP 密封口令 + HMAC 签名会话）、聊天/重新生成/生图、记忆、人格、配置、成本、安全中心、调度、导出 |
| bili_login.py | `bili_login.rs` | B站扫码登录完整状态机（86101/86090/86038/0） |
| private_messages.py | `private_msgs.rs` | 私信轮询、危险内容判定（URL 规范化、信任域、强/弱色情词）、回复边界 |
| Proactive.py | `proactive.rs` | 主动刷视频：yt-dlp 下载 → ffmpeg 抽帧 → 视觉识别 → 点赞/投币/收藏/关注/评论 |
| dynamic.py | `dynamic.rs` | 动态发布：文案生成 + 生图（OpenAI 兼容 modalities）+ B站图床上传 + 图文动态 |
| config.py | `config.rs` | 默认值全表对齐、热重载、四场景模型路由（chat/vision/search/image）+ 回退模型 |
| util.py | `util.rs` | 原子 JSON 读写、WBI 签名、BV↔aid、HMAC、base64、时间助手 |

## 构建

```bash
cargo build --release
# 产物：target/release/bilibili-ai-bot-rs（约 7.8MB）
```

依赖：Rust 1.98+；运行期可选外部命令：`yt-dlp`、`ffmpeg`（仅主动行为/视频分析需要）。

## 运行

```bash
# 在仓库根目录（含 config.json / chat.html / data/）运行
./rust-backend/target/release/bilibili-ai-bot-rs --base-dir . --port 5000

# 只跑 Web 面板（不跑评论轮询）
./rust-backend/target/release/bilibili-ai-bot-rs --base-dir . --port 5000 --no-bot

# 只跑 Bot 主循环（不开面板）
./rust-backend/target/release/bilibili-ai-bot-rs --base-dir . --no-web
```

- 环境变量：`CHAT_PASSWORD`（面板口令，默认 `admin()`）、`SECRET_KEY`（会话签名，缺省自动生成到 `data/.secret_key`）、`BOT_DIR`（数据目录）。
- 端口默认 5000，与原 Flask 一致。
- 首次启动自动生成 `data/.seal_key.pem`（RSA-2048，0600，用于登录口令密封）。

## 数据兼容

直接读写与 Python 版相同的 `data/*.json`：
`memory.json` / `affection.json` / `replied.json` / `user_profiles.json` / `permanent_memory.json` /
`personality_evolution.json` / `personas.json` / `mood.json` / `video_memory.json` / `cost_log.json` /
`security_log.json` / `block_log.json` / `block_suggestion_dismissed.json` / `schedule_today.json` /
`local_chat.json` / `dynamic_log.json` / `watch_log.json` / `external_memory.json` / `private_messages_state.json` 等。

前端 `chat.html` 无需任何改动。

## 行为对齐要点（与 Python 逐字对齐）

- 回复 prompt：`generate_reply_and_score` 的完整提示词原文移植（人格段、风格段、底线、好感度段、私信边界、今日状态+节日、视频/记忆/搜索/无内容分段、JSON schema 与 score_delta 规则、15-40 字）。
- 视频上下文：封面 OCR（vision 通道）→ 文本归纳（chat 通道，预算 2000 附近），`video_memory.json` 缓存，失败降级为元信息串。
- 记忆：余弦相似度 >0.45 取 3 条、最近对话 tail 6、压缩阈值 30 保留 10、档案 facts 20 / tags 10。
- 好感度等级：owner→special；≤-10 cold；≥51 close；≥31 friend；≥11 normal；其余 stranger；里程碑 10/30/50/80/99。
- 私信安全：NFKC 规范化（hxxps→https、[.]→.、全角点→半角、汉字「点」→.）、13 个强色情词、10 个色情域名特征、非信任域阻断、链接+弱色情词组合阻断。
- WBI 签名：64 项 mixin 重排表、密钥 1 小时缓存；BV 转换表保留。

## 已知差异（有意为之，均为增强或实现方式差异）

1. **二维码输出 SVG data URL**（原版为 PNG）：浏览器 `<img src>` 直接兼容；如需 PNG 可给 qrcode 依赖开启 image 特性。
2. **Web 会话**：自研 HMAC 签名 Cookie（原版为 Flask session），30 天有效，`HttpOnly`。
3. **并发模型**：单进程多任务（tokio），配置更新即时生效（共享 `Arc<RwLock<Config>>`），原版需等 5 分钟热重载或重启。
4. **主动行为/动态**：yt-dlp、ffmpeg 走子进程调用，与 Python 版行为等价；未安装时自动降级为「仅元信息分析 / 纯文字动态」。
5. 日志用 `tracing`，`RUST_LOG=debug` 可开更详细日志。

## 验证

- `cargo check` 0 error / 0 warning；`cargo build --release` 通过。
- 冒烟测试（隔离目录）：登录/认证、全部 26 个 GET 路由、配置写入持久化、二维码生成、私信/评论轮询降级路径均验证通过。
