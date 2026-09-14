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
| [五](#五移动端汉堡按钮随面板消失) | 切面板后无法回到侧栏 | 按钮被放在单个面板内部 | 逐面板遍历断言按钮可见性 |
| [六](#六面板安全加固) | 面板公网暴露的加固 | `static_folder="."` 暴露根目录、逐路由鉴权易漏 | 以未登录身份打接口断言响应码 |
| [七](#七user-agent-不完整触发风控) | User-Agent 不完整触发风控 | UA 缺版本号 | 看错误码 + 异常类型 |
| [八](#八部署注意事项) | 部署注意事项 | — | — |

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

## 七、User-Agent 不完整触发风控

B站写操作会拒收不完整的 User-Agent，返回 `code 30014`（`Token is invalid`）。

> **注意 `30014` 并非只表示凭证失效**，AI 中转站也会返回同名错误码，只看数字会误判。

`ai.py / config.py / private_messages.py / Proactive.py / dynamic.py` 的 UA 已补齐为完整 Chrome UA。

**区分错误来源的经验**：

| 来源 | 表现 |
|------|------|
| AI 中转站 | 抛 `openai.AuthenticationError` |
| B站 | 走 `requests`，不会抛这个类型 |

---

## 八、部署注意事项

| # | 事项 | 要点 |
|:-:|------|------|
| 1 | `config.json` 不入库 | 已在 `.gitignore` 忽略。首次部署复制 `config.example.json` 为 `config.json` 后填入自己的密钥 |
| 2 | 休眠参数语义 | `SLEEP_START=24 / SLEEP_END=0` 表示全天活跃，**不要用 `0/0`**（等于永久休眠） |
| 3 | 行尾必须为 LF | 仓库通过 `.gitattributes` 的 `* text=auto eol=lf` 强制归一，CRLF 会导致 BusyBox `ash` / `procd` 等环境启动失败 |
| 4 | 配置热更新 | 主循环每 5 分钟 `reload_config()`（含休眠参数），只改 `config.json` 无需重启；改了 `.py` **或 `chat.html`** 必须重启服务 —— `debug=False` 下 Jinja 会缓存已编译模板，仅替换文件不生效 |
| 5 | 密钥管理 | 密钥、Cookie、Token 一律从 `config.json` 或环境变量读取，不要硬编码进代码或注释 |

> 完整的部署步骤、systemd 单元、TLS 暴露方式与上线自检清单见 [DEPLOY.md](DEPLOY.md)。
