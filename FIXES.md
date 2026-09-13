# 修复详解

> 版本变更摘要与提交记录统一见 [CHANGELOG.md](CHANGELOG.md)。
> 本文只讲**问题根因、代码层面的判定逻辑与排查手法**，不重复变更清单。

---

## 一、私信与评论完全不回复

两个独立根因叠加，各自都能单独造成"完全不回复"。

### 根因一：休眠窗口命中

`ai.py` 中：

```python
def is_active_time():
    from config import SLEEP_START, SLEEP_END
    hour = datetime.now().hour
    if SLEEP_START < SLEEP_END:
        return hour < SLEEP_START or hour >= SLEEP_END
    else:  # 跨午夜，比如 23-6
        return hour >= SLEEP_END and hour < SLEEP_START
```

两个陷阱：

1. **提示文案是硬编码的**。日志打印的"当前不在工作时间（2:00-8:00休眠中）"写死了
   2:00-8:00，与实际配置无关。配置改成别的区间后，日志依然显示这句话，
   排查时不能直接采信。
2. **不能用 `0/0` 关闭休眠**。`SLEEP_START == SLEEP_END` 时走 else 分支，
   得到 `hour >= 0 and hour < 0`，恒为假，等于**永久休眠**。

要全天活跃，应配置为 `SLEEP_START=24 / SLEEP_END=0`，走 else 分支得到
`hour >= 0 and hour < 24`，恒为真。

判定式验证：

| 配置 | hour=2 时 | 结论 |
| --- | --- | --- |
| `2 / 8` | `2<2` 假，`2>=8` 假 | 休眠 |
| `0 / 0` | `2>=0` 真，`2<0` 假 | 休眠（且全天休眠） |
| `24 / 0` | `2>=0` 真，`2<24` 真 | 活跃 |

### 根因二：`SEARCH_KEYWORDS` 未定义

现象：私信收到并打印，但紧接着 `私信生成失败 ... name 'SEARCH_KEYWORDS' is not defined`。

链路：

```
run() → generate_reply_and_score(channel="private")
      → needs_search(text)
      → for kw in SEARCH_KEYWORDS   # NameError
```

原因：

- `ai.py` 顶部是 `from config import *`，但 `config.py` 中并没有 `SEARCH_KEYWORDS`
- 该常量只定义在 `local-chat.py`（面板进程）中，bot 进程里不可见
- 因此只要走到联网搜索判断，就抛 `NameError`，私信生成阶段直接失败

排查手法（可复用）：`from config import *` 的项目里，这类漏导入的常量不会在启动期暴露，
只有运行到具体分支才炸。用 `ast` 扫描一遍即可一次性找全：

```python
# 收集各模块中"被引用"的全大写常量，减去：
#   本模块已定义 + 局部变量 + config.py 全部定义 + builtins
# 剩余项即为潜在的 NameError 隐患
```

本次扫描 `ai.py / private_messages.py / Proactive.py / dynamic.py`，仅此一处。

---

## 二、评论楼层归属与上下文串台

- `get_new_replies()` 增加 `root_rpid` 字段
- 会话 `thread_id` 由 `根评论ID` 改为 `根评论ID:用户UID`。
  原实现下同一评论串里所有用户共享一份上下文，多人评论时会串台
- `send_reply()` 增加 `root_rpid` 参数，改为：

  ```python
  "root": root_rpid or rpid,
  "parent": rpid,
  ```

  原实现是 `root=parent=rpid`，回复的目标若是子评论，楼层会挂错

---

## 三、模型兜底与记忆检索降级

- `claude_chat()` 改为三层串行短路：主模型 → 备用模型 → OpenRouter 兜底。
  **第一个返回非空正文即返回**，是串行而非并发，不会多个模型各回一次
- 推理型模型会先用 reasoning 消耗 token，`max_tokens` 给小了会返回空正文
  （`finish_reason=length, content=''`），后续 `json.loads('')` 抛
  `Expecting value: line 1 column 1`，**这条报错会伪装成接口鉴权失败**。
  非首选通道的 token 预算放宽为 `max(1500, n)`
- `get_embedding()` 增加可用性标志，失败即降级为禁用语义记忆检索。
  原实现抛异常会中断主流程，而"标记已回复"在异常之后，导致同一条评论被
  无限重试（好感度被反复累加）

---

## 四、User-Agent 不完整触发风控

B 站写操作会拒收不完整的 User-Agent，返回 `code 30014`（`Token is invalid`）。
**注意 30014 并非只表示凭证失效**，AI 中转站也会返回同名错误码，只看数字会误判。

`ai.py / config.py / private_messages.py / Proactive.py / dynamic.py` 的 UA
已补齐为完整 Chrome UA。

区分错误来源的经验：`openai.AuthenticationError` 来自 AI 中转站；
B 站走 `requests`，不会抛这个类型。

---

## 五、部署注意事项

1. **`config.json` 不入库**，已在 `.gitignore` 忽略。首次部署复制
   `config.example.json` 为 `config.json` 后填入自己的密钥。
2. **休眠参数语义**：`SLEEP_START=24 / SLEEP_END=0` 表示全天活跃，
   不要用 `0/0`（等于永久休眠）。
3. **行尾必须为 LF**。仓库通过 `.gitattributes` 的 `* text=auto eol=lf` 强制归一，
   CRLF 会导致 BusyBox `ash` / `procd` 等环境启动失败。
4. **配置热更新**：主循环每 5 分钟 `reload_config()`（含休眠参数），
   只改 `config.json` 无需重启；改了 `.py` 必须重启服务。
5. 密钥、Cookie、Token 一律从 `config.json` 或环境变量读取，不要硬编码进代码或注释。
