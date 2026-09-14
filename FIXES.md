# 修复详解

<div align="center">

问题根因 · 代码判定逻辑 · 排查手法

![Scope](https://img.shields.io/badge/scope-root%20cause%20only-087CBA?style=flat-square)
![Companion](https://img.shields.io/badge/changes-CHANGELOG.md-6B7280?style=flat-square)

</div>

> 版本变更摘要与提交记录统一见 [CHANGELOG.md](CHANGELOG.md)。本文只讲**问题根因、代码层面的判定逻辑与排查手法**，不重复变更清单。

> **脱敏约定**：本文的实测样例里，所有 B 站用户名、账号 `mid`、评论 `rpid`、视频 `aid` / `BV` 号一律替换为占位符（如 `<Bot昵称>`、`用户A`、`<rpid-1>`、`<aid-1>`）。占位符只改变标识，不改变技术结论 —— 每条样例的字段形态、判定分支与修复前后差异均与线上实测一致。

## 问题索引

| # | 问题 | 关键根因 | 排查手法 |
|:-:|------|----------|----------|
| [一](#一私信与评论完全不回复) | 私信与评论完全不回复 | 休眠窗口命中 + `SEARCH_KEYWORDS` 未定义 | `ast` 扫描漏导入常量 |
| [二](#二评论楼层归属与上下文串台) | 评论楼层归属与上下文串台 | `thread_id` 粒度太粗 | 观察同串多用户回复 |
| [三](#三模型兜底与记忆检索降级) | 模型兜底与记忆检索降级 | 推理型模型吃光 token | 看 `finish_reason` |
| [四](#四移动端遮罩压住侧栏) | 移动端点菜单后无法操作 | CSS 层叠上下文嵌套错位 | `elementFromPoint` 命中链 + 读 `parentElement` |
| [五](#五移动端汉堡按钮随面板消失) | 切面板后无法回到侧栏 | 按钮被放在单个面板内部 | 逐面板遍历断言按钮可见性 |
| [六](#六面板安全加固) | 面板公网暴露的加固 | `static_folder="."` 暴露根目录、逐路由鉴权易漏 | 以未登录身份打接口断言响应码 |
| [七](#七评论中--不触发回复) | 评论里 @ Bot 毫无反应 | 只拉「回复我的」，漏了「@我的」消息流 | 打印两个接口的 items 数与字段键集合 |
| [八](#八日志说已回复但实际没上屏) | 日志有回复记录、评论区却看不到 | 正文在 `send_reply` 之前打印 + @ 回复是二级评论默认折叠 | 抓 `reply/reply` 按自己 mid 过滤子评论 |
| [九](#九视频标题没被用上面板死配置与提示词结构) | @ 类评论不按视频内容回复 | 视频信息塞进「不相关可忽略」段 + 三类模型的专用通道成死配置 | 同一条真实评论做提示词 A/B 对比 |
| [十](#十计费口径前后端不一致) | 面板改价格 Bot 不生效、次数与明细对不上 | 两个进程各写一套价格匹配规则 | 交叉核对两侧解析入口 |
| [十一](#十一user-agent-不完整触发风控) | User-Agent 不完整触发风控 | UA 缺版本号 | 看错误码 + 异常类型 |
| [十二](#十二部署注意事项) | 部署注意事项 | — | — |

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

## 五、移动端汉堡按钮随面板消失

现象：手机上点侧栏里的「安全中心」切过去之后，左上角的汉堡按钮就没了，**再也打不开侧栏**，只能刷新页面。

### 根因：按钮被放在了单个面板内部

汉堡按钮的原始位置是 `#panel-chat` 里的 `.chat-header`：

```html
<div id="panel-chat" class="panel active">
  <div class="chat-panel">
    <div class="chat-header">
      <button class="mobile-menu-btn" ...>   <!-- 只在这里 -->
```

而面板切换靠 `display` 控制：

```css
.panel        { display: none; }
.panel.active { display: flex; }
```

切到任何非聊天面板时，`#panel-chat` 立刻 `display: none`，**按钮作为它的后代一起消失**。其余 14 个面板只有 `.panel-header`（标题 + 描述），压根没有汉堡按钮。

这不是样式错误，而是**元素归属位置错误** —— 一个「全局导航入口」被放进了「其中一个页面的局部容器」里。

### 修复：把入口提到所有面板之外

在 `.main` 下、所有 `.panel` 之前插入独立顶栏：

```html
<div class="main">
  <div class="mobile-topbar">          <!-- 面板之外，与切换逻辑解耦 -->
    <button class="mobile-menu-btn" ...>
    <span class="mobile-topbar-title" id="mobileTopbarTitle">聊天</span>
  </div>
  <div id="panel-chat" class="panel active"> ...
```

配套三处：

1. `.chat-header` 内原有的汉堡按钮删除，否则移动端聊天面板会同时出现两个
2. `.panel` 由 `height: 100%` 改为 `flex: 1; min-height: 0` —— `.main` 是 flex 列容器，顶栏占了 47px，`height: 100%` 的面板会撑出纵向溢出
3. `switchPanel()` 里同步顶栏标题，让用户知道自己在哪个面板

> **为什么不给 15 个面板各补一个按钮？** 那要改 15 处，且将来新增面板时极易遗漏，同一个问题会原样复现。把入口提到面板之外，新增面板自动覆盖。

### 顶栏标题的取值顺序

标题优先取**侧栏导航项文案**，而不是面板内部的 `h2`：

1. 首选：匹配 `.sidebar .nav-item` 的 `onclick`，取其文案（去掉尾部角标数字）
2. 次选：面板自身的 `.panel-header h2`（去掉开头的图标字符）
3. 兜底：面板的英文名

原因：**聊天面板没有 `.panel-header`**，它的头部是 `.chat-header`，里面只有一个显示 Bot 名的 `h2#chatBotName`。只用次选方案时，聊天面板的顶栏标题会退化成英文的 `chat` —— 这个边界用例是靠遍历全部面板断言标题才发现的。

### 排查手法：遍历断言，而不是只测当前页

这类「只在某些页面上坏」的问题，单测一个页面必然漏掉。做法是**遍历所有需要覆盖的页面状态**，逐个断言：

```python
for name in ("chat", "summary", "security", "memory", "settings"):
    switch_panel(name)
    assert topbar.display == "flex"
    assert menu_btn.display == "flex" and menu_btn.width > 0
    assert title.text == expected_label[name]
```

顺带也能抓住布局副作用 —— 本次就靠它确认了 `顶栏 y(47) + 面板高(797) == 视口高(844)`，即 `.panel` 的高度改动是正确的。

<details>
<summary><b>踩坑记录：新增顶栏 CSS 时踩到特异性反转</b></summary>

<br>

基础规则是 `.mobile-menu-btn { display: none }`（特异性 `0,1,0`），移动端媒体查询里再用 `.mobile-menu-btn { display: flex }` 打开。

写顶栏时顺手加了 `.mobile-topbar .mobile-menu-btn { display: flex; }`（特异性 `0,2,0`），**它高于基础规则，于是在桌面端也生效**。表现很隐蔽：顶栏本身 `display: none`，所以按钮的 `getBoundingClientRect()` 是 `0×0`，用户既看不见也点不到，但 `getComputedStyle().display` 是 `flex` —— 属于「无害但错误」的状态。

是桌面端回归测试里那条 `assert menuBtn.display == "none"` 把它揪出来的。**修法**：删掉这条多余规则，移动端显示交给媒体查询里既有的那条即可 —— 顶栏 `display: flex` 后，子按钮自然可见，不需要额外声明。

</details>

---

## 六、面板安全加固

面板会长期挂在公网（经 natfrp 隧道暴露），一旦被人扫到端口，默认配置下的攻击面相当大。本章讲的是**加固后的判定逻辑与动机**，而不是「做了哪些项」的清单 —— 变更摘要见 [CHANGELOG.md](CHANGELOG.md) 的 `feat(security)` 条目，部署要点见 [DEPLOY.md](DEPLOY.md) 第 10 节。

### 6.1 静态目录收口：一个参数差点公开整个项目

最严重的一处来自 Flask 构造参数：

```python
# 危险写法（历史版本）
app = Flask(__name__, template_folder=".", static_folder=".")
```

`static_folder="."` 意味着**应用根目录成了静态根**。静态路由不需要登录，于是下面这些全都可以被直接下载：

| 路径 | 后果 |
|------|------|
| `/config.json` | API Key、B站 Cookie、面板口令全部泄露 |
| `/ai.py` `/config.py` | 源码泄露，等于把内部逻辑和密钥读取方式一并交出 |
| `/data/**` | 聊天记录、记忆库、头像原图 |

修复只有一行，但**判定逻辑值得记住**：Flask 的 `static_folder` 是「信任即公开」的语义，设成 `.` 相当于把整个仓库目录挂成 CDN。正确做法是指向专门的前端资源目录：

```python
app = Flask(__name__, template_folder=".", static_folder="static",
            static_url_path="/static")
```

> 排查方式：不看代码，而是**直接请求敏感路径**，断言返回 `404`。`/config.json`、`/ai.py`、`/data/config.json` 三个探针足以覆盖这一类问题。

### 6.2 默认拒绝，而不是逐路由放行

加固前的模式是「每个接口自己记得检查登录」。这种模式的问题不在当下，而在于**新增路由时容易漏**：60 多个路由，漏一个就是一次数据泄露。

改成在 `before_request` 里做全局兜底，语义是**默认拒绝**：

```python
PUBLIC_PATHS = frozenset({
    "/", "/favicon.ico",
    "/api/login", "/api/auth_check", "/api/handshake", "/api/health",
    "/api/branding", "/media/bot-avatar",
})
PUBLIC_PREFIXES = ("/static/",)

@app.before_request
def check_auth():
    path = request.path
    if path in PUBLIC_PATHS or path.startswith(PUBLIC_PREFIXES):
        return
    if session.get("authed"):
        return
    # 未登录：API 回 401 让前端弹登录框；其余一律 404，不泄露路径是否存在
    if path.startswith("/api/"):
        return jsonify({"error": "未登录", "need_login": True}), 401
    return ("Not Found", 404)
```

两点设计取舍：

| 决策 | 理由 |
|------|------|
| 白名单用 `frozenset` 逐条列举，而非前缀匹配 | 前缀匹配（如 `/api/auth`）会连带放行 `/api/authz_dump` 之类的意外路由；逐条列举必须显式新增 |
| 未登录的非 API 路径返回 `404` 而非 `403` | `403` 等于告诉扫描者「这个路径存在，只是你没权限」；`404` 不区分「不存在」与「存在但不可见」 |

`/api/*` 单独回 `401` 是给前端用的 —— 前端据此判断该弹登录框；其余路径用户不会直接看到响应体，用 `404` 更划算。

> 副产物：因为兜底逻辑与路由无关，**将来新增的接口自动受保护**，不需要维护者记得加装饰器。这也是本次没有给 60 多个路由逐个加 `@login_required` 的原因 —— 加装饰器是「记得就安全」，兜底是「忘记也安全」。

### 6.3 口令密封：避免明文口令过链路

登录接口最初接收明文口令：

```json
POST /api/login   {"password": "..."}
```

在自建的 TLS 面板里这本身可接受，但项目实际跑在 natfrp 隧道后面 —— **边缘节点终止 TLS**，也就是说口令在中间那一段是可读的。此外若因证书缺失回落 HTTP（见 6.4），明文口令就完全裸露在网络路径上。

加固方式是加一层传输前的 RSA-OAEP 密封，**明文通道保留作降级**：

```python
@app.route("/api/handshake", methods=["GET"])
def handshake():
    """下发 RSA 公钥。公钥不是秘密，但只在登录前用得上。"""
    return jsonify({"alg": "RSA-OAEP-256", "pubkey": _seal_pubkey_b64()})
```

前端用 WebCrypto 加密后提交 `sealed` 字段，后端解密：

```python
def _seal_decrypt(token):
    """RSA-OAEP(SHA-256) 解密。任何异常都归为 None，不把失败细节漏给请求方。"""
    try:
        raw = base64.b64decode((token or "").strip(), validate=True)
        if len(raw) != 256:            # RSA-2048 密文长度必须正好 256 字节
            return None
        return SEAL_PRIVATE_KEY.decrypt(raw, _asym_padding.OAEP(...))
    except Exception:
        return None
```

几个边界处理：

1. **密文长度前置校验 `len(raw) != 256`**：RSA-2048 的密文恒为 256 字节。先判长度再调 `decrypt()`，可以把大量畸形请求挡在昂贵的模幂运算之前，同时也是一层便宜的输入校验。
2. **异常一律归 `None`**：区分「密文格式错」「密钥不匹配」「padding 错」并回报给请求方，等于给出了 oracle。统一成一句「密文无法解密，请刷新页面重试」。
3. **公钥可匿名获取是设计意图**：公钥本身不构成秘密，放行 `/api/handshake` 不扩大攻击面。

`cryptography` 缺失时 `SEAL_ENABLED=False` 自动降级回明文通道，**不阻断启动** —— 加密是增强项，不该成为可用性的单点。

### 6.4 反代感知：为什么只信回环地址

面板要判断「浏览器那一侧是不是 HTTPS」，才能决定是否发 HSTS、是否给 Cookie 打 `Secure`。判断依据是 `X-Forwarded-Proto`，但**这个头是客户端可伪造的**：

```python
_TRUSTED_PROXY_ADDRS = ("127.0.0.1", "::1", "::ffff:127.0.0.1")

def _client_is_secure():
    if request.remote_addr in _TRUSTED_PROXY_ADDRS:
        proto = (request.headers.get("X-Forwarded-Proto") or "").split(",")[0].strip().lower()
        if proto:
            return proto == "https"
    return bool(request.is_secure)
```

只有请求确实来自本机（反代进程）才采信该头；直连请求即使带了 `X-Forwarded-Proto: https` 也走 `request.is_secure`。**否则任何人都能伪造这个头，把面板骗成「安全上下文」而发出 HSTS 与 `Secure` Cookie。**

取 `.split(",")[0]` 是因为代理链会追加成 `https, http`，第一个才是客户端真实协议。

### 6.5 一个差点把登录锁死的写法

Cookie 的 `Secure` 标志需要跟实际协议联动：

```python
# 注意不要直接写 or PANEL_TLS —— 万一证书缺失回落到 HTTP，Secure Cookie 会把登录悄悄锁死。
SECURE_COOKIES = (os.environ.get("SECURE_COOKIES", "0").strip() == "1")
```

看起来更简洁的 `SECURE_COOKIES = SECURE_COOKIES or PANEL_TLS` 是错的：`PANEL_TLS=1` 只表示「打算起 HTTPS」，而证书缺失时代码会回落 HTTP（见 `__main__` 里 `os.path.exists(PANEL_TLS_CERT)` 的判断）。此时浏览器会拒绝在 HTTP 上存储 `Secure` Cookie，表现为**登录请求返回成功、页面却一直停在登录页** —— 排查时很容易误以为是会话或密钥问题。

正确做法是在真正起了 HTTPS 之后才强制打开：

```python
if ssl_ctx:
    app.config["SESSION_COOKIE_SECURE"] = True
```

> 这是「配置意图」与「运行时事实」不一致的典型陷阱。凡是有降级路径的开关，联动对象必须是**降级后的实际状态**，而不是配置值本身。

### 6.6 会话密钥：重启掉线的根因

会话签名密钥若写成 `app.secret_key = uuid.uuid4().hex`，每次重启都换新密钥 ⇒ 所有会话 Cookie 立即失效 ⇒ 用户莫名其妙被登出。持久化到 `data/.secret_key` 解决，并要求权限 `0600`：

```python
fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
```

复用前校验 `len(k) >= 32`，防止被截断或写坏的短密钥上生产。任何异常回落单个 `uuid4().hex`（当次会话可用，但重启会掉线）—— 宁愿掉线也不要起不来。

同一目录下还有口令密封的私钥 `data/.seal_key.pem`（RSA-2048 / PKCS8 / `0600`），`data/` 整体已在 `.gitignore` 中。

### 6.7 头像路由：把可控输入去掉，而不是过滤它

登录页需要展示 Bot 头像，但此刻用户还没登录。最初的直觉是「把 `/data/images/<filename>` 加进白名单」，那会直接把整个图片目录公开，且 `<filename>` 是可控输入，需要额外防目录穿越。

改法是把**可控输入彻底消掉** —— 用一个不接受任何参数的专用路由：

```python
@app.route("/media/bot-avatar")
def bot_avatar():
    """只认 config 的 BOT_AVATAR 指向的文件，不接受任何来自请求的文件名参数 ——
    没有可控输入，也就没有穿越面。"""
```

文件名只从 `config.json` 读，且做单层校验：

```python
name = unquote(raw[len("/data/images/"):])
if not name or "/" in name or "\\" in name or ".." in name:
    return ("Not Found", 404)
```

> **安全设计的优先顺序**：能去掉可控输入 > 能加白名单 > 能做黑名单过滤。「没有可控输入也就没有穿越面」比「过滤掉 `..`」更可靠，因为前者不需要假设自己想到了所有编码变体（URL 编码、双重编码、反斜杠、Unicode 折叠…）。

顺带做了件事：原图 3 MB 而登录页只显示 48px，经隧道限速下发很浪费，因此现场缩到 160px（`im.thumbnail`），失败则回落原图。该路由响应带 `Cache-Control: public, max-age=3600` —— 这张图本来就公开，加缓存不扩大暴露面。

### 6.8 安全响应头

```python
resp.headers.setdefault("X-Content-Type-Options", "nosniff")
resp.headers.setdefault("X-Frame-Options", "DENY")
resp.headers.setdefault("Referrer-Policy", "no-referrer")
if request.path.startswith("/api/"):
    resp.headers.setdefault("Cache-Control", "no-store")
if _client_is_secure():
    resp.headers.setdefault("Strict-Transport-Security", "max-age=31536000")
```

| 头 | 挡的问题 |
|----|----------|
| `X-Content-Type-Options: nosniff` | 浏览器把上传内容猜成可执行类型 |
| `X-Frame-Options: DENY` | 面板被嵌进 iframe 做点击劫持 |
| `Referrer-Policy: no-referrer` | 面板 URL 作为 Referer 泄露给外部站点 |
| `/api/*` 的 `no-store` | 聊天记录、UID 等数据被浏览器或中间层缓存 |
| HSTS 仅在安全上下文发 | 避免在 HTTP 下发出后被浏览器长时间强制跳 HTTPS 而打不开 |

用 `setdefault` 而非 `=` 赋值，是为了不覆盖视图函数自己设置的更具体值。

### 6.9 排查手法：不做静态审查，直接打接口

这类问题的验证方式不是读代码，而是**以未登录身份请求真实接口**，断言响应码：

```python
cases = [
    ("GET",  "/api/summary",      401),   # 数据接口：需要登录
    ("GET",  "/api/config",       401),   # 配置接口
    ("GET",  "/config.json",      404),   # 敏感文件：不存在于静态根
    ("GET",  "/ai.py",            404),
    ("GET",  "/api/handshake",    200),   # 登录前必须可达
    ("GET",  "/",                 200),   # 登录页本身
]
```

只测「已登录时能不能用」会漏掉全部这四类问题 —— **权限问题的判据永远在「未登录」这一侧**，而且要看的是「拒绝了没有」，不是「有没有正常返回数据」。

---

## 七、评论中 @ 不触发回复

现象：别人在评论区里 **@ 机器人**，机器人毫无反应。但如果别人是**回复机器人自己的评论**，回复就正常 —— 两者在用户看来都是「有人在评论里叫了机器人」。

### 根因：两类消息在 B站是两个独立接口，只拉了一个

B站消息中心把「回复我的」和「@我的」拆成两个接口：

```python
# ai.py 修复前 —— 只有这一个
def get_new_replies():
    url = "https://api.bilibili.com/x/msgfeed/reply"
    ...
```

| 消息类型 | 接口 | 修复前状态 |
|----------|------|-----------|
| 回复我的评论 | `/x/msgfeed/reply` | 已轮询 |
| 在评论里 @ 我 | `/x/msgfeed/at` | **完全没有拉取** |

所以「@ 不响应」不是过滤逻辑写错，而是**这条消息从来就没进过程序**。判定这类问题的关键一步：先确认目标接口本身有没有数据，再去查代码拉的是哪个接口 —— 顺序反过来会一直在过滤条件里绕。

### 两个流的字段差异（100 条真实样本实测）

结构看起来相似，但有三处关键差异，直接决定能不能复用同一套解析逻辑：

| 字段 | `/msgfeed/reply` | `/msgfeed/at` |
|------|------------------|---------------|
| `type` / `business_id` | `reply` / `1` | `reply` / `1`（一致，可复用回复逻辑） |
| `root_id` | 真实根评论 id | **恒为 0** |
| `at_details` | 多为空数组 | **恒非空**，含该评论 @ 到的**全部**用户 |
| 时间字段 | `reply_time` | `at_time` |

因此 `at` 流的 `root_rpid` 必须回落到 `source_id`：

```python
# root_id 为 0 时 or 会回落到 source_id —— 被 @ 的评论本身就是根评论
root_rpid = r.get("root_id") or rpid
```

> 好在原 `reply` 流已经是 `r.get("root_id") or r["source_id"]` 的写法，这个 `or` 回落顺带把 `at` 流也覆盖了。**如果当初写的是 `r["root_id"]`，@ 流会因为 `root=0` 把回复挂到错误楼层。**

### 修复要点

**1. 两条流合并，按 `rpid` 去重**

同一条评论可能「既回复了我、又在正文 @ 了我」，会同时出现在两个流里。不去重就会被回复两次 —— 用户看到重复回复，token 也白烧一份。

```python
pending = _merge_pending(replies, at_replies)   # 靠前的流优先
```

抽成独立函数是为了能脱离主循环单测（见下）。

**2. 只回复确实 @ 到本账号的条目**

`at_details` 里是这条评论 @ 到的**所有人**。一条评论可以同时 @ 九个人，其中顺带带上了机器人 —— 若不加判断，机器人会去回复所有这类「群 @」评论。

```python
if at_details and me not in {str(u.get("mid")) for u in at_details}:
    continue          # 这条 @ 了别人但没 @ 到我
```

实测 100 条样本中 `at_details` 全部非空且全部含本账号，反例 0 条 —— 过滤条件不会误伤真 @。`at_details` 为空时选择信任接口语义（接受），宁可回一条也不漏。

**3. 剥掉 @ 昵称，且不用正则**

正文长这样，直接丢给模型没有意义：

```
@用户E @用户F @<Bot昵称> @用户D 来看[doge][doge][doge]
```

剥离方式是按 `at_details` 里的**确切昵称**做字面替换，而不是 `@\S+` 这类正则：

```python
nicks = sorted({(u.get("nickname") or "").strip() for u in at_details},
               key=len, reverse=True)      # 长度倒序，防短昵称切碎长昵称
for nick in nicks:
    out = out.replace("@" + nick, " ")
```

原因：**B站昵称允许含空格**，正则会在空格处切错边界，把昵称后半段当成正文留下。既然接口已经把被 @ 者的确切昵称给了我们，就没有必要去猜边界。

正文剥完为空（只 @ 了一下、没写别的）时给一句兜底文案，而不是把空串交给模型：

```
📩 [被@] 某某：…   →   💬 Bot：诶，只@我一下呀，想聊视频吗喵
```

**4. 时效上限：不加就会在首次启用时补发一批历史回复**

B站消息流固定返回最新 N 条，且**不读即不消**（同一个 `pn=1` 反复请求返回同一批）。这意味着如果直接上线，程序会对着列表里所有历史 @ 一次性补发回复。

这与私信链路早就踩过的坑完全相同，所以直接沿用同一约定：

```python
"AT_REPLY_MAX_AGE": 3600,      # 秒；配 0 或负数表示不限时效
```

生产实测这个保护是必要的：当轮 20 条 @ 里**只有 3 条在一个小时内**，另外 17 条历史 @ 被拦下。

**5. 日志区分来源**

```
📬 获取到 10 条通知
📣 @我的评论 3 条待处理（跳过：超过时效 17 条）
📩 [被@] 某某（陌生人🌙 | 1分）：来看[doge][doge][doge]（原文：@<Bot昵称> @用户F …）
```

`[被@]` / `[回复]` 前缀是必需的 —— 这个项目此前已经因为日志文案与实际配置不符而误导过一次排查（见第一节的休眠文案），同类问题不该再犯第二次。@ 消息额外打出原文，因为它的正文可能只有一串 @ 昵称，不还原原文就看不出用户到底说了什么。

<details>
<summary><b>排查手法：消息类问题的取证顺序</b></summary>

<br>

「某某情况不响应」这类问题的第一嫌疑是过滤逻辑，但真正的根因往往在更前面。按下面顺序查，可以少绕很多：

**第 1 步：先证明接口里有数据**

```python
for name, url in (("reply", ".../x/msgfeed/reply"), ("at", ".../x/msgfeed/at")):
    d = requests.get(url, headers=H, params={"ps": 20, "pn": 1}).json()
    print(name, d["code"], len(d["data"]["items"]))
```

如果 `at` 接口有 10~20 条而程序毫无反应，问题就锁定在「没拉」或「拉了但被过滤」，而不是「接口没数据」。

**第 2 步：打印字段键集合，不要猜字段名**

排查时最容易犯的错是照着 `reply` 流的字段名去 `at` 流里取，结果 `KeyError` 或取到 0。正确做法是把整条样本的键集合打出来：

```python
print(sorted(inner.keys()))
print(sorted(user.keys()))
# 逐条统计字段分布，确认哪些字段恒存在、哪些会缺
```

本次就是靠这个发现 `root_id` 恒为 0、`at_details` 恒非空、时间字段叫 `at_time`。**分布统计（而不是只看第一条）才能区分「字段偶尔缺失」与「字段语义不同」。**

**第 3 步：检查是否有回复流之外的旁路**

这个项目里同一条评论可能有多条进入路径（消息流、动态流、主动行为）。修完一处要确认另一处不需要同样处理，否则问题会以另一种形式复现。

**第 4 步：用 fixture 做回归，且禁止写操作**

解析逻辑的验证不该依赖真实评论，也不该真的发出回复。做法是：抓一份真实响应存盘当 fixture，之后把 `requests.get` 打桩、把 `requests.post` 打桩成**直接断言失败**：

```python
def _post_forbidden(*a, **kw):
    record["post"] += 1
    raise AssertionError("禁止写操作：被测量的代码路径不应调用 requests.post")
```

这样「修复没有副作用」这件事就有了可执行的证据，而不是靠人眼检查代码。

</details>

### 附带发现（本次未修，已记录）

- **`OWNER_MID` 配置为 0**：`config.example.json` 里它就是占位默认值 `0`，线上一直没填。影响两处判定 —— `max_score = 100 if str(mid) == str(OWNER_MID) else 99` 使主人永远拿不到满分，`str(mid) != str(OWNER_MID)` 的主人豁免也失效。实际账号 mid 与 `DEDE_USER_ID` 相同，配置值应填该 mid。
- **`/x/msgfeed/at` 的其它业务类型未处理**：本次只处理 `business_id == 1`（视频评论），专栏 / 动态等业务调回复接口时 `type` 参数口径与 `business_id` 并非一一对应，未经取证不贸然发写操作。样本中未出现其它业务类型，故未扩大范围。

---

## 八、日志说「已回复」但实际没上屏

用户报告：别人 @ 我，日志里明明有回复记录，翻评论区却看不到回复。

**结论：回复其实发出去了。是「日志」和「页面」两处一起误导了判断。**

### 根因一：回复正文在发送之前就被打印

主循环原本的顺序是「生成正文 → 立刻打印 `💬 Bot：<正文>` → 再调用 `send_reply()`」。发送发生在打印**之后**，于是只要模型产出了正文，日志里就会出现一行 `💬 Bot：…`，与是否真正发出毫无关系：

```
💛 好感度：1 → 2（+1）| 陌生人
💬 Bot：诶，只@我一下呀，想聊视频吗喵        <-- 生成即打印，与发送无关
⚠️ 发送失败: code=-412, msg=请求过于频繁，请稍后再试
```

失败提示在下一行，但人的注意力先落在 `💬 Bot：` 上 —— 扫日志时极易误判成「已经回了」。

**修复**：正文打印移到发送之后，并拆成两种语义明确的写法。

| 时机 | 日志行 |
|------|--------|
| 发送成功 | `💬 Bot（已发送，rpid=<rpid-1>）：诶，只@我一下呀…` |
| 发送失败 | `💬 Bot（发送失败，内容未上屏）：…` |

配套改动：`send_reply()` 的返回值从 `bool` 改为「新评论的 `rpid`」（取不到则 `True`，失败 `None`）。有了 `rpid`，「到底上屏没有」可以用接口直接核验，不必再靠推断。

处理入口的日志也补上 `rpid`，让日志行能对应到具体某条评论：

```
📩 [被@] rpid=<rpid-3> 用户C（陌生人🌙 | 1分）：…
```

### 根因二：@ 类回复是二级评论，网页默认折叠

回复「别人 @ 我」的那条评论时，接口参数是 `root` = 被 @ 的评论、`parent` = 被回复的评论，B站会把它挂成**二级评论**。网页端二级评论默认折叠，必须点开「N 条回复」才看得到，首屏只显示一级评论。

所以「看不到回复」有相当一部分是「回复在折叠层里」。核验要查接口，不要靠肉眼翻页：

```python
d = requests.get("https://api.bilibili.com/x/v2/reply/reply", headers=H,
                 params={"oid": oid, "type": 1, "root": root_rpid}, timeout=20).json()
mine = [x for x in (d.get("data") or {}).get("replies", [])
        if (x.get("member") or {}).get("mid") == MY_MID]
```

本次就是靠它确认 Bot 账号（`mid=<BOT_MID>`）的二级评论确实存在（如 `rpid=<rpid-1>`、`rpid=<rpid-2>`）。

<details>
<summary><b>排查手法：先分清「没发出去」和「发出去了看不到」</b></summary>

1. 日志里出现 `发送失败` 或 `code=` → 属发送环节问题，按错误码定位（`-412` 频率限制、`-101` 未登录、`-111` csrf 错误）
2. 日志里没有任何失败行、但页面看不到 → 用 `reply/reply` 查子评论，按自己的 `mid` 过滤
3. 子评论存在 → 是折叠，不是故障，无需修

**通用结论**：日志必须打印「动作完成后的结果」，不能打印「动作开始前的意图」。任何「生成 → 打印 → 发送」的顺序都会制造这种误导；反过来说，只要日志里带上了动作产物的标识（这里是 `rpid`），排查就能一步到位。

</details>

---

## 九、视频标题没被用上：面板死配置与提示词结构

用户提问：别人在视频下面 @ 我，能读到视频标题吗？那种只 @ 不说其他的，能根据视频标题回复吗？

**结论：标题一直读得到，但「用不用得上」被两个缺陷卡住。**

### 标题链路本身是通的

`get_video_context(oid, comment_type)` 对视频评论（`comment_type == 1`）调 `https://api.bilibili.com/x/web-interface/view?aid=<oid>`，取出 `title` / `owner.name` / `desc` / `tname`，拼成 `【当前视频信息】` 注入提示词，并落盘 `data/video_memory.json` 做缓存。

实测 4 条真实 @ 评论，标题全部拿到：

| 被@评论 `oid` | 取到的标题 |
|---|---|
| <aid-1> | 琵琶曲｜耄耋VS大狗 |
| <aid-2> | 🐧：如果分享史会判刑🐧 |
| <aid-3> | 【全损】义和团の小曲，五十五天在北京 |
| <aid-4> | 今天又栽在妹妹手里了。(悲) |

### 缺陷一：视频信息被塞进了「不相关就忽略」的段落

`build_memory_context(..., video_context=…)` 把视频信息拼进了**记忆参考**段落，而这个段的抬头写着：

```
【记忆参考（仅在与当前话题直接相关时参考，否则忽略）】
```

「只 @ 不说话」的评论正文被替换成占位符 `（对方在评论里 @ 了我，但没有写别的内容）` —— 它没有任何话题。模型据此有充分理由判定「视频信息和当前话题无关」，于是退回通用回复。同一条真实评论的产出的对比：

| | 提示词结构 | 产出 |
|---|------|------|
| 修复前 | 视频信息混在记忆段，无专门引导 | 「就一个@？是不是话没打完喵」 |
| 修复后 | 视频信息独立成段 + 「一个字都没写」引导 | 「这UP的哈基米琵琶斗大狗有点上头，你也被洗脑了？」 |

**修复**：

| 项 | 说明 |
|------|------|
| `build_memory_context()` 不再收 `video_context` | 参数直接删掉，再传会 `TypeError` 显式报错，不会被静默接受后又混回记忆段 |
| 新增 `video_context` 参数到 `generate_reply_and_score()` | 视频信息渲染成独立段落 `【对方所在的视频】`，并注明「可自然引用，不要照抄标题」 |
| @ 流条目新增 `no_content` 标记 | 「剥离全部 @昵称 后为空」即为真；命中时插入 `【对方一个字都没写】` 引导段 |
| 引导段明确禁空话 | 要求「结合视频主动抛出一个具体话题」，并点名禁止「有什么事」「有话直说」这类回复；没有视频信息时改为「自己起一个具体话题」 |

### 附带修复：两条消息流的正文里都混着 @ 噪声

模型最终看到的「用户说了什么」，取决于归一化做得多干净。实测两条流的正文形态差得很远：

| 流 | 原始正文（实测样本） | 问题 |
|---|---|---|
| `reply` | `回复 @<Bot昵称> :凑卡奴[…]` | 正文带 B站 自动加的「回复 @昵称 :」前缀，「回复」二字与机器人**自己的昵称**都是噪声 —— 模型看到的是「有人在回复 @我自己」，而不是「有人对我说了句话」 |
| `at` | `@<Bot昵称> @用户A @用户B …` | @ 列表本身不是内容；全部剥掉后为空，才是真正的「一个字都没写」 |

原实现只对 `at` 流做了 @ 剥离，`reply` 流是**直接把原文**喂给模型的。现已统一：两条流都先剥 B站 自动加的前缀，再按 `at_details` 精确剥 @ 昵称，剥完为空则打上 `no_content` 标记走同一套引导。

剥离前缀的边界条件：**必须出现冒号才剥**，模式为 `^\s*回复\s*(?:@[^:：]{0,80})?\s*[:：]\s*`。否则用户真写「回复你一下」这种正文会被吃掉开头。实测 10 条 `reply` 样本中 3 条被归一化、7 条原样保留。

实测效果（对线上真实数据）：

| rpid | 原文 | 归一后 |
|---|---|---|
| <rpid-4> | `回复 @<Bot昵称> :凑卡奴[…]` | `凑卡奴[…]` |
| <rpid-5> | `回复 @<Bot昵称> :[…]喵喵[…]` | `[…]喵喵[…]` |
| <rpid-6> | `可爱捏` | `可爱捏`（无前缀，不动） |

`at` 流同样归一：时效内 2 条全部判定为 `no_content=True`（正文只有 @ 列表）。

顺带补上 `import re`：`ai.py` 此前使用 `re` 却从未导入，靠 `from config import *` 把 `config` 里的 `re` 泄漏进来 —— 能用，但 `config.py` 一旦不再导入 `re`，`ai.py` 会立刻崩在启动阶段。这类「靠 `import *` 蹭到的名字」属于隐性依赖，发现即补。

### 缺陷二：面板上视觉 / 搜索 / 图片三类的专用通道，Bot 侧一个都不读

面板设置页里四类模型（对话 / 视觉 / 搜索 / 图片）各有一组输入框：主模型、备用模型、专用 API 地址、专用 API Key。实测各键在两侧的使用次数：

| 配置键 | `chat.html`（面板有输入框） | `ai.py`（Bot 是否读） |
|--------|:-:|:-:|
| `OR_CHAT_MODEL_FALLBACK` | 3 | 1 |
| `OR_SEARCH_MODEL_FALLBACK` | 3 | 0 |
| `OR_VISION_MODEL_FALLBACK` | 3 | 0 |
| `OR_IMAGE_MODEL_FALLBACK` | 3 | 0 |
| `OR_CHAT_URL` / `OR_CHAT_KEY` | 3 / 4 | 0 / 0 |
| `OR_SEARCH_URL` / `OR_SEARCH_KEY` | 3 / 4 | 0 / 0 |
| `OR_VISION_URL` / `OR_VISION_KEY` | 3 / 4 | 0 / 0 |
| `OR_IMAGE_URL` / `OR_IMAGE_KEY` | 3 / 4 | 0 / 0 |

原因：`ai.py` 里所有模型调用都走一个**固定绑死** `OR_BASE_URL` / `OR_API_KEY` 的 `or_client`，只按模型名区分；而面板侧 `local-chat.py` 用的是 `config.get_model_config()`，会正确取每类的专用地址 / 密钥 / 兜底模型。同一份配置，两边解析口径不一致，后果是：

- 面板点「测试连接」走专用通道（显示通过），Bot 实际回复走通用通道（可能失败）
- `OR_VISION_MODEL` 为空时，视频分析与评论图片识别必然返回 `400 Model name not specified`

线上实测：`data/video_memory.json` 里 36 条缓存的 `analysis` 字段**全部**是这条降级串，没有一条是真正的分析结果：

```
视频《琵琶曲｜耄耋VS大狗》，UP主：<UP主>，分区：。简介：哈基米VS大狗叫…
```

这串是 `analyze_video_with_gemini` 的 `except` 分支拼出来的元信息重排 —— 看着像分析，其实只是把标题 / UP主 / 简介抄了一遍。

**修复**：

| 项 | 说明 |
|------|------|
| 新增 `_model_candidates(model_type)` | 通道参数统一取 `config.get_model_config()`，与面板共用同一份解析逻辑；四类模型的专用 URL / KEY / 备用模型全部生效 |
| 新增 `_vision_candidates()` | 视觉类没配任何模型时**回落到对话候选**，不再必然 400 |
| 新增 `_complete_with(candidates, content, max_tokens, label)` | 对话 / 联网搜索 / 视觉共用一条「主 → 兜底」候选链；`content` 兼容纯文本与多模态数组；非首选通道放宽 token 预算（推理型模型会把预算吃光在推理上，正文可能是空串） |
| 删除 `or_client` | 它就是绕过专用通道的入口；`embed_client` 保留 |
| 降级串抽成 `_video_fallback_text()` | 显式注明「只有元信息，没有任何内容判断」，避免再被当成分析结果 |
| 清理 36 条降级缓存 | 清空 `data/video_memory.json`（先备份 `.bak.<时间戳>`），让这些视频下次被评论时重新分析 |

修复后实跑同一条视频：

```
修复前：视频《琵琶曲｜耄耋VS大狗》，UP主：<UP主>，分区：。简介：哈基米VS大狗叫…
修复后：该视频是一段约1分钟的动画二创玩梗短片，借用油管KotteAnimation的CG猫狗素材，
        配上魔性"哈基米"音乐与琵琶曲，演绎了一场荒诞搞笑的"猫狗大战"…
实际命中通道：qwen3.8-flash（候选 3 条，视觉类为空故回落到对话类）
```

<details>
<summary><b>附带发现（本次未修，已记录）</b></summary>

- **`tname`（分区名）上游已不再返回**：`/x/web-interface/view` 里 `tid=235` 仍在，但 `tname` 与 `tname_v2` 恒为空串（带 Cookie 与不带 Cookie 表现一致）。代码对空值按「未知」处理；要真正显示分区名，只能自建 `tid → 名称` 映射表。
- **`log_cost` 里写死的模型名**：联网搜索与视频识别原本都记 `model="gemini"`，账本里既不是 gemini 也追不到真实模型。现改为传实际命中的模型名。
- **图片识别此前不记账**：`recognize_images` 没有 `log_cost`，面板的调用次数与真实调用数对不上。现以来源 `评论图片识别`（含「识别」二字 → 按视觉价计费）入账。

</details>

<details>
<summary><b>排查手法：用「提示词 A/B」而不是读代码来判断模型看不看得到上下文</b></summary>

「模型有没有用上这段上下文」无法靠读代码确认 —— 代码只保证「塞进去了」，模型是否采纳是另一回事。可靠做法是固定同一条真实输入，只改提示词结构，各跑一次对比产出：

```python
# B：旧结构（视频信息混在记忆段、无引导）
b = generate_reply_and_score(placeholder_text, username, level, ctx + memory_ctx)
# A：新结构（视频独立成段 + 只@引导）
a = generate_reply_and_score("", username, level, memory_ctx,
                             video_context=ctx, no_content=True)
```

两次调用都用同一条真实 @ 评论、同一个视频，差异只来自提示词结构。这样得到的结论是「模型确实在用视频信息」，而不是「我认为它应该会用」。

配套注意：验证脚本要把 `log_cost` 打桩成空操作，避免验证产生的 token 混进生产账本；同时全程不调用任何 B站 写接口，即不产生任何评论。

</details>

---

## 十、计费口径前后端不一致

面板设置页里改价格，Bot 的调用仍按写死的价格计费，两边数字对不上。

### 根因一：两个进程各写一套价格匹配规则

`ai.py`（Bot 工作进程）与 `local-chat.py`（面板进程）都要按价格换算金额，但各自写了一套匹配逻辑：

| | `ai.py`（修复前） | `local-chat.py`（修复前） |
|---|---|---|
| 视觉 | 仅按 `model == "gemini"` 判断 | 仅认英文关键词 `"vision"` |
| 搜索 | 无 | 认 `"search"` / `"搜索"` |
| 图片 | 无 | 认 `"image"` / `"图片"` |
| 价格来源 | **全部写死**（3.0 / 15.0，gemini 0.5 / 3.0） | 读 `config.json` 的 `PRICE_*` |

后果：面板按用户填的价格算，Bot 按写死价格算 —— 「在设置页改了价格，Bot 仍按旧价计费」。另外面板侧只认英文关键词，中文来源（如「视频识别」）既不匹配 vision 也不匹配 search / image，一律落到对话价，设置页里「视觉模型」下的价格输入框形同虚设。

### 根因二：8 个 `PRICE_*` 键后端没声明

`chat.html` 引用了 8 个价格键（对话 / 视觉 / 搜索 / 图片 × 输入 / 输出），但 `config.py` 的 `_DEFAULTS` 里没有它们。`/api/config` 不下发未声明的键 → 面板价格输入框永远是空的。

### 根因三：成本账本的 `models` 字段只有面板侧写

面板的「当日调用次数」取自 `day["calls"]`，而明细 chips 取自 `day["models"]`。Bot 侧只累加 `calls` 不写 `models`，于是「N 次调用」与「明细之和」必然对不上。

**修复**：

| 项 | 说明 |
|------|------|
| `config.py` 声明 8 个 `PRICE_*` 默认键 | 默认 0，与面板提示语「留空 = 不计费」一致 |
| 新增 `config.resolve_model_price(source, model)` | 前后端共用的**单一实现**；匹配顺序为「来源关键词 → 模型关键词 → 对话兜底」。来源优先很关键：联网搜索走的是 gemini，先看模型名会被误判成视觉类 |
| `ai.py` / `local-chat.py` 的 `log_cost` 均改调该函数 | 删除两侧各自写死的匹配分支 |
| `ai.py` 的 `log_cost` 补写 `models` 明细 | key 口径与面板一致（模型名含 `/` 用模型名，否则用调用来源） |
| `config.example.json` 补齐 8 个键 | 新部署直接可用 |

验证方式：把两个进程的 `COST_LOG_FILE` 指向同一个临时文件，先由面板侧记一笔、再由 Bot 侧记两笔，断言 `明细次数之和 == calls`、`明细 token 之和 == 当日 token`、`明细金额之和 == total`，共 24 项断言全通过。

---

## 十一、User-Agent 不完整触发风控

B站写操作会拒收不完整的 User-Agent，返回 `code 30014`（`Token is invalid`）。

> **注意 `30014` 并非只表示凭证失效**，AI 中转站也会返回同名错误码，只看数字会误判。

`ai.py / config.py / private_messages.py / Proactive.py / dynamic.py` 的 UA 已补齐为完整 Chrome UA。

**区分错误来源的经验**：

| 来源 | 表现 |
|------|------|
| AI 中转站 | 抛 `openai.AuthenticationError` |
| B站 | 走 `requests`，不会抛这个类型 |

---

## 十二、部署注意事项

| # | 事项 | 要点 |
|:-:|------|------|
| 1 | `config.json` 不入库 | 已在 `.gitignore` 忽略。首次部署复制 `config.example.json` 为 `config.json` 后填入自己的密钥 |
| 2 | 休眠参数语义 | `SLEEP_START=24 / SLEEP_END=0` 表示全天活跃，**不要用 `0/0`**（等于永久休眠） |
| 3 | 行尾必须为 LF | 仓库通过 `.gitattributes` 的 `* text=auto eol=lf` 强制归一，CRLF 会导致 BusyBox `ash` / `procd` 等环境启动失败 |
| 4 | 配置热更新 | 主循环每 5 分钟 `reload_config()`（含休眠参数），只改 `config.json` 无需重启；改了 `.py` **或 `chat.html`** 必须重启服务 —— `debug=False` 下 Jinja 会缓存已编译模板，仅替换文件不生效 |
| 5 | 密钥管理 | 密钥、Cookie、Token 一律从 `config.json` 或环境变量读取，不要硬编码进代码或注释 |
| 6 | @ 回复时效 | `AT_REPLY_MAX_AGE`（秒，默认 3600）限制只回复「多久之内」的 @。B站消息流不读即不消且固定返回最新 N 条，**把它设得很大或关闭，重启后会对着历史 @ 一次性补发回复**；设为 0 或负数才表示不限时效 |
| 7 | 升级后清理旧视频缓存 | `data/video_memory.json` 里若存在以 `视频《…》，UP主：…，分区：` 开头的「分析」，那是旧版视觉分析失败时拼的元信息串（不含任何内容判断）。清掉（先备份）这些条目，对应视频下次被评论时会重新分析 |
| 8 | 模型通道配置 | 面板里四类模型（对话 / 视觉 / 搜索 / 图片）各有「主模型 / 备用模型 / 专用地址 / 专用 Key」。**视觉类留空时会自动回落到对话类**；若对话模型不支持图片输入，视觉任务仍会失败并退到元信息降级串 | `AT_REPLY_MAX_AGE`（秒，默认 3600）限制只回复「多久之内」的 @。B站消息流不读即不消且固定返回最新 N 条，**把它设得很大或关闭，重启后会对着历史 @ 一次性补发回复**；设为 0 或负数才表示不限时效 |

> 完整的部署步骤、systemd 单元、TLS 暴露方式与上线自检清单见 [DEPLOY.md](DEPLOY.md)。
