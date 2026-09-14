# 修复详解

<div align="center">

问题根因 · 代码判定逻辑 · 排查手法

![Scope](https://img.shields.io/badge/scope-root%20cause%20only-087CBA?style=flat-square)
![Companion](https://img.shields.io/badge/changes-CHANGELOG.md-6B7280?style=flat-square)

</div>

> 版本变更摘要与提交记录统一见 [CHANGELOG.md](CHANGELOG.md)。本文只讲**问题根因、代码层面的判定逻辑与排查手法**，不重复变更清单。

## 问题索引

| # | 问题 | 关键根因 | 排查手法 |
|:-:|------|----------|----------|
| [一](#一私信与评论完全不回复) | 私信与评论完全不回复 | 休眠窗口命中 + `SEARCH_KEYWORDS` 未定义 | `ast` 扫描漏导入常量 |
| [二](#二评论楼层归属与上下文串台) | 评论楼层归属与上下文串台 | `thread_id` 粒度太粗 | 观察同串多用户回复 |
| [三](#三模型兜底与记忆检索降级) | 模型兜底与记忆检索降级 | 推理型模型吃光 token | 看 `finish_reason` |
| [四](#四移动端遮罩压住侧栏) | 移动端点菜单后无法操作 | CSS 层叠上下文嵌套错位 | `elementFromPoint` 命中链 + 读 `parentElement` |
| [五](#五user-agent-不完整触发风控) | User-Agent 不完整触发风控 | UA 缺版本号 | 看错误码 + 异常类型 |
| [六](#六部署注意事项) | 部署注意事项 | — | — |

---

## 一、私信与评论完全不回复

两个独立根因叠加，各自都能单独造成「完全不回复」。

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

1. **提示文案是硬编码的**。日志打印的「当前不在工作时间（2:00-8:00 休眠中）」写死了 2:00-8:00，与实际配置无关。配置改成别的区间后，日志依然显示这句话，排查时不能直接采信。
2. **不能用 `0/0` 关闭休眠**。`SLEEP_START == SLEEP_END` 时走 else 分支，得到 `hour >= 0 and hour < 0`，恒为假，等于**永久休眠**。

要全天活跃，应配置为 `SLEEP_START=24 / SLEEP_END=0`，走 else 分支得到 `hour >= 0 and hour < 24`，恒为真。

判定式验证：

| 配置 | `hour=2` 时 | 结论 |
|------|-------------|------|
| `2 / 8` | `2<2` 假，`2>=8` 假 | 休眠 |
| `0 / 0` | `2>=0` 真，`2<0` 假 | 休眠（且全天休眠） |
| `24 / 0` | `2>=0` 真，`2<24` 真 | 活跃 |

### 根因二：`SEARCH_KEYWORDS` 未定义

**现象**：私信收到并打印，但紧接着 `私信生成失败 ... name 'SEARCH_KEYWORDS' is not defined`。

**链路**：

```
run() → generate_reply_and_score(channel="private")
      → needs_search(text)
      → for kw in SEARCH_KEYWORDS   # NameError
```

**原因**：

- `ai.py` 顶部是 `from config import *`，但 `config.py` 中并没有 `SEARCH_KEYWORDS`
- 该常量只定义在 `local-chat.py`（面板进程）中，bot 进程里不可见
- 因此只要走到联网搜索判断，就抛 `NameError`，私信生成阶段直接失败

<details>
<summary><b>排查手法（可复用）：用 ast 一次性找全漏导入常量</b></summary>

<br>

`from config import *` 的项目里，这类漏导入的常量不会在启动期暴露，只有运行到具体分支才炸。用 `ast` 扫描一遍即可一次性找全：

```python
# 收集各模块中"被引用"的全大写常量，减去：
#   本模块已定义 + 局部变量 + config.py 全部定义 + builtins
# 剩余项即为潜在的 NameError 隐患
```

本次扫描 `ai.py / private_messages.py / Proactive.py / dynamic.py`，仅此一处。

</details>

---

## 二、评论楼层归属与上下文串台

| 改动 | 说明 |
|------|------|
| `get_new_replies()` 增加 `root_rpid` 字段 | 记录根评论 ID，供回复时定位楼层 |
| 会话 `thread_id` 改为 `根评论ID:用户UID` | 原实现下同一评论串里所有用户共享一份上下文，多人评论时会串台 |
| `send_reply()` 增加 `root_rpid` 参数 | 见下方代码 |

```python
"root": root_rpid or rpid,
"parent": rpid,
```

原实现是 `root=parent=rpid`，回复的目标若是子评论，楼层会挂错。

---

## 三、模型兜底与记忆检索降级

### 三层串行短路

`claude_chat()` 改为三层串行短路：主模型 → 备用模型 → OpenRouter 兜底。**第一个返回非空正文即返回**，是串行而非并发，不会多个模型各回一次。

### 空正文会伪装成鉴权失败

推理型模型会先用 reasoning 消耗 token，`max_tokens` 给小了会返回空正文（`finish_reason=length, content=''`），后续 `json.loads('')` 抛 `Expecting value: line 1 column 1`。

**这条报错会伪装成接口鉴权失败**。非首选通道的 token 预算放宽为 `max(1500, n)`。

### 记忆检索降级

`get_embedding()` 增加可用性标志，失败即降级为禁用语义记忆检索。

原实现抛异常会中断主流程，而「标记已回复」在异常之后，导致同一条评论被无限重试（好感度被反复累加）。

---

## 四、移动端遮罩压住侧栏

现象：手机浏览器点汉堡按钮打开侧栏后，侧栏正常滑出、看得清清楚楚，但**点任何导航项都没反应**；反复点、反复刷新都一样，只有重新加载页面才能脱困。

### 根因：层叠上下文嵌套错位

三个元素的层级声明如下（`chat.html`）：

```
body 层叠上下文
├── body::before      z-index: 0     壁纸
├── .char-wallpaper   z-index: 0     角色立绘，pointer-events: none
├── #loginOverlay     z-index: 9999  登录页
├── .mobile-overlay   z-index: 90    ← 遮罩，body 的直接子元素
└── .app              z-index: 1     ← 创建层叠上下文！
    ├── .sidebar      z-index: 100   （仅对 .app 内部有效）
    └── .main         z-index: auto
```

`.app` 同时有 `position: relative` 与 `z-index: 1`，**这创建了一个层叠上下文**。规则是：

> 子元素的高 `z-index` 只在其父级层叠上下文**内部**参与比较；对外只体现出父级的 `z-index`。

所以 `.sidebar` 的 `100` 对页面其他部分是隐形的，`.app` 对外只报 `1`。遮罩作为**兄弟节点**直接与 `.app` 的 `1` 比较：

```
90 > 1  →  遮罩永久压在 .app（含侧栏）之上
```

`z-index` 再怎么调都无效：只要遮罩还在 `.app` 外面，把侧栏 `z-index` 提到多高都改变不了结局。

### 修复

1. 把 `.mobile-overlay` 移入 `.app` **内部**（作为其最后一个子元素），使其与 `.sidebar` / `.main` 处于同一层叠上下文
2. 遮罩 `position` 由 `fixed` 改为 `absolute`（相对已 `position: relative` 的 `.app`，尺寸仍为满屏 `390 × 844`，视觉无差异）
3. 移动端 `.sidebar` 的 `z-index` 由 `100` 提到 `110`

层叠关系变为 `壁纸(0) < .main(0) < 遮罩(90) < 侧栏(110)`：遮罩盖住主内容、不盖侧栏。

### 排查手法

在浏览器内用 `elementFromPoint` 判断「谁在最上面」，并**读取命中元素的 `parentElement`** —— 这一条是定位本 bug 的关键：

```js
const el = document.elementFromPoint(x, y);
el.parentElement;   // 修复前是 BODY，修复后是 DIV.app
```

只看 `getComputedStyle(...).zIndex` 是查不出来的：遮罩的 `z-index: 90` 自始至终都是 90，`.sidebar` 的 `100` 也一直读得到，数值层面完全「正常」。**必须结合 DOM 位置才能发现它们分属两个层叠上下文。**

<details>
<summary><b>踩坑记录：修 DOM 位置时别用「<code>&lt;/div&gt;</code> + <code>&lt;script&gt;</code>」这类模式匹配</b></summary>

<br>

第一版补丁用 `re.search(r"\n</div>\n\n<script>\n")` 去定位 `.app` 的闭合处，结果匹配到了页面里**第一个**满足该模式的位置 —— 那是「手动拉黑对话框」`#blockDialogOverlay` 的结尾，不是 `.app` 的结尾。遮罩被塞进了模态框内部：

- `parentElement` 变成 `DIV.modal-overlay#blockDialogOverlay`
- 模态框 `display: none`，遮罩实际尺寸塌成 `0 × 0`
- 结果：侧栏确实能点了，**但遮罩也彻底失效**，主内容区不再被拦截

正确做法是对 `<div class="app">` 起始标签做 **div 配对深度扫描**，拿到真正的配对闭合位置，并在改动后断言 `app < .sidebar < .main < overlay < app_close`，同时排除「遮罩落在任何 `modal-overlay` 内部」。

</details>

<details>
<summary><b>踩坑记录：改了模板但页面没变，先怀疑 Jinja 模板缓存</b></summary>

<br>

`local-chat.py` 以 `debug=False` 运行，`Flask(__name__, template_folder=".")` 直接加载仓库根目录的 `chat.html`。此配置下 Jinja **缓存已编译模板**：

- 只替换磁盘上的 `chat.html` → 页面**不变**
- 必须 `systemctl restart bilibili-panel` → 立即生效

本次曾误判为浏览器 / CDN 缓存，实际判据来自服务端：回源 `curl http://127.0.0.1:5000/` 返回的 HTML 长度与旧版一致，重启后才变成新版。**先查回源，再查浏览器，能少走一大圈。**

</details>

---

## 五、User-Agent 不完整触发风控

B站写操作会拒收不完整的 User-Agent，返回 `code 30014`（`Token is invalid`）。

> **注意 `30014` 并非只表示凭证失效**，AI 中转站也会返回同名错误码，只看数字会误判。

`ai.py / config.py / private_messages.py / Proactive.py / dynamic.py` 的 UA 已补齐为完整 Chrome UA。

**区分错误来源的经验**：

| 来源 | 表现 |
|------|------|
| AI 中转站 | 抛 `openai.AuthenticationError` |
| B站 | 走 `requests`，不会抛这个类型 |

---

## 六、部署注意事项

| # | 事项 | 要点 |
|:-:|------|------|
| 1 | `config.json` 不入库 | 已在 `.gitignore` 忽略。首次部署复制 `config.example.json` 为 `config.json` 后填入自己的密钥 |
| 2 | 休眠参数语义 | `SLEEP_START=24 / SLEEP_END=0` 表示全天活跃，**不要用 `0/0`**（等于永久休眠） |
| 3 | 行尾必须为 LF | 仓库通过 `.gitattributes` 的 `* text=auto eol=lf` 强制归一，CRLF 会导致 BusyBox `ash` / `procd` 等环境启动失败 |
| 4 | 配置热更新 | 主循环每 5 分钟 `reload_config()`（含休眠参数），只改 `config.json` 无需重启；改了 `.py` **或 `chat.html`** 必须重启服务 —— `debug=False` 下 Jinja 会缓存已编译模板，仅替换文件不生效 |
| 5 | 密钥管理 | 密钥、Cookie、Token 一律从 `config.json` 或环境变量读取，不要硬编码进代码或注释 |

> 完整的部署步骤、systemd 单元、TLS 暴露方式与上线自检清单见 [DEPLOY.md](DEPLOY.md)。
