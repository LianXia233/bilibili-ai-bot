# 修复记录

> 按版本的变更摘要见 [CHANGELOG.md](CHANGELOG.md)，本文记录问题根因与排查过程。

## 2026-09-14

### fix(bot): 私信与评论完全不回复

两个独立根因叠加，各自都能单独造成"完全不回复"。

#### 根因一：休眠窗口命中

`ai.py` 的 `is_active_time()`：

```python
if SLEEP_START < SLEEP_END:
    return hour < SLEEP_START or hour >= SLEEP_END
else:
    return hour >= SLEEP_END and hour < SLEEP_START
```

配置为 `SLEEP_START=2 / SLEEP_END=8` 时，`hour=2` 起 `2<2` 与 `2>=8` 均为假 → bot 进入休眠，
主循环直接跳过全部评论与私信。日志表现为连续刷屏"当前不在工作时间（2:00-8:00休眠中）"。

需要注意的是，**该提示文案是硬编码的**（写死"2:00-8:00"），与实际配置无关，排查时不可采信。

修复：改为 `SLEEP_START=24 / SLEEP_END=0`，走 else 分支得到 `hour >= 0 and hour < 24`，全天活跃。

陷阱：`SLEEP_START == SLEEP_END`（例如 0/0）会走 else 分支算出 `hour >= 0 and hour < 0`，
结果是**永久休眠**。不能用 0/0 来"关闭休眠"，这也是本次踩过的坑。

#### 根因二：`SEARCH_KEYWORDS` 未定义

- `ai.py` 顶部是 `from config import *`，但 `config.py` 中并没有 `SEARCH_KEYWORDS`
- 该常量只定义在 `local-chat.py`（面板进程）中
- `ai.py` 的 `needs_search()` 引用了它 → `NameError: name 'SEARCH_KEYWORDS' is not defined`
- 私信链路 `generate_reply_and_score()` 内部会调用 `needs_search()`，
  导致每条私信都在生成阶段抛异常

日志表现为：`私信生成失败 UID xxx：name 'SEARCH_KEYWORDS' is not defined`。

修复：在 `ai.py` 的 `needs_search()` 之前补入同名常量（与 `local-chat.py` 保持一致）。

排查手法（可复用）：用 `ast` 扫描各模块引用的全大写常量，减去"本模块定义 + 局部变量 +
`config.py` 全部定义 + builtins"，即可一次性找出所有因 `from config import *` 覆盖不全
而导致的 `NameError` 隐患。本次扫描 `ai.py / private_messages.py / Proactive.py / dynamic.py`，
仅此一处。

### fix(bot): 评论楼层归属与上下文串台

- `get_new_replies()` 增加 `root_rpid` 字段
- 会话 `thread_id` 由 `根评论ID` 改为 `根评论ID:用户UID`，
  避免同一评论串下不同用户共享上下文导致回复串台
- `send_reply()` 增加 `root_rpid` 参数，改为 `root=root_rpid or rpid, parent=rpid`，
  修正回复子评论时挂错楼层的问题

### fix(bot): 对话模型无兜底、Embedding 异常中断

- `claude_chat()` 重写为三层串行短路：主模型 → 备用模型 → OpenRouter 兜底，
  第一个返回非空正文即返回，不会多个模型各发一次
- 推理型模型的 token 预算放宽为 `max(1500, n)`，避免 reasoning 吃光预算导致返回空正文
- `get_embedding()` 增加可用性标志，失败时优雅降级为禁用语义记忆检索，
  不再因异常中断主流程造成同一条评论被反复重试

### fix(bot): User-Agent 不完整触发 B 站风控

`ai.py / config.py / private_messages.py / Proactive.py / dynamic.py` 的 User-Agent
补齐为完整 Chrome UA。B 站写操作会拒收不完整的 UA，返回 `code 30014`。

### feat(ui): WebUI 改用 PaperGrid / schale 设计语言

- 不对称圆角 `8px 3px 22px`
- 硬边偏移阴影 `4px 4px 0 var(--primary)`（无模糊）
- 导航项斜切高亮块 `skew(-12deg)`
- 卡片与气泡左上角的青色竖标记
- 配色：`--primary #087cba`、`--ba-cyan #19b6ed`、背景 `#f2f7fb`、前景 `#17324b`
- 附带 `prefers-color-scheme: dark` 深色适配（只覆盖变量值，不动规则）

实现方式：只替换 `chat.html` 的 `<style>` 块，HTML 结构与 `<script>` 逐字节未改。

---

## 部署与安全注意事项

1. **`config.json` 不入库**（已在 `.gitignore` 中忽略）。请复制 `config.example.json`
   为 `config.json` 后填入自己的密钥，切勿提交真实配置。
2. **休眠参数语义**：`SLEEP_START=24 / SLEEP_END=0` 表示全天活跃。
   不要使用 `0/0`，那会导致永久休眠。
3. **行尾必须为 LF**。仓库已通过 `.gitattributes` 的 `* text=auto eol=lf` 强制归一，
   CRLF 会导致 BusyBox `ash` / `procd` 等环境启动失败。
4. **主循环每 5 分钟热更新配置**（含休眠参数）。只改 `config.json` 无需重启；
   但修改了 `.py` 代码必须重启服务。
5. 所有密钥、Cookie、Token 均应从 `config.json` 或环境变量读取，不要硬编码进代码。
