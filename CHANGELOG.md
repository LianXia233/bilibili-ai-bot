# 更新日志 (Changelog)

本仓库 fork 自 [chenluQwQ/bilibili-ai-bot](https://github.com/chenluQwQ/bilibili-ai-bot)，
本文件记录相对上游的变更。格式参考 Keep a Changelog，提交信息遵循 Conventional Commits。

这里是本项目**唯一的更新日志文件**；问题根因与排查手法另见 [FIXES.md](FIXES.md)。

## [2026-09-14]

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
