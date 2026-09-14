# 生产部署指南

<div align="center">

面向「一台 Linux 服务器长期跑 Bot + 面板」的场景

![Platform](https://img.shields.io/badge/Platform-Linux-4B5563?style=flat-square&logo=linux&logoColor=white)
![Init](https://img.shields.io/badge/Init-systemd-000000?style=flat-square&logo=systemd&logoColor=white)
![Python](https://img.shields.io/badge/Python-3.10%2B-3776AB?style=flat-square&logo=python&logoColor=white)
![TLS](https://img.shields.io/badge/TLS-required%20for%20public-0F9D58?style=flat-square&logo=letsencrypt&logoColor=white)

</div>

> **占位符约定**：本文所有地址、端口、账号、口令一律使用占位符（`<你的域名>`、`<强口令>` 等），请替换为你自己的值。**不要把真实值写进仓库。**

## 步骤索引

| 步骤 | 内容 | 是否必做 |
|:----:|------|:--------:|
| [1](#1-前置条件) | 前置条件 | — |
| [2](#2-落地目录) | 落地目录 | 必做 |
| [3](#3-依赖安装) | 依赖安装 | 必做 |
| [4](#4-配置) | 配置 | 必做 |
| [5](#5-systemd-服务) | systemd 服务 | 必做 |
| [6](#6-暴露到公网必须带-tls) | 暴露到公网（必须带 TLS） | 公网访问必做 |
| [7](#7-日志与轮转) | 日志与轮转 | 建议 |
| [8](#8-上线自检清单) | 上线自检清单 | 必做 |
| [9](#9-升级流程) | 升级流程 | 升级时 |
| [10](#10-安全加固清单) | 安全加固清单 | 建议 |

---

## 1. 前置条件

| 项 | 要求 |
|------|------|
| 系统 | 任意主流 Linux 发行版（本文以 systemd 为例） |
| Python | 3.10 以上（本项目在 3.13 上验证过） |
| 网络 | 服务器能直连 B站 API 与你的模型网关 |
| 权限 | 能创建 systemd 服务；若面板要暴露公网，还需域名或内网穿透账号 |

---

## 2. 落地目录

约定安装到 `/opt/bilibili-ai-bot`，与运行数据分离：

```
/opt/bilibili-ai-bot/          # 代码 + venv
/opt/bilibili-ai-bot/data/     # 运行时数据：记忆、好感度、安全日志、会话密钥
/var/log/bilibili-panel.log    # 面板日志
/var/log/bilibili-bot.log      # Bot 日志
```

```bash
sudo mkdir -p /opt/bilibili-ai-bot
cd /opt/bilibili-ai-bot
# 代码可以直接放进来，也可以用 git clone
```

---

## 3. 依赖安装

```bash
cd /opt/bilibili-ai-bot
python3 -m venv venv
./venv/bin/pip install -U pip
./venv/bin/pip install -r Requirements.txt
```

国内服务器可加镜像加速：

```bash
./venv/bin/pip install -r Requirements.txt \
  -i https://mirrors.cloud.tencent.com/pypi/simple/
```

---

## 4. 配置

```bash
cp config.example.json config.json
chmod 600 config.json          # 内含 Cookie 与 API Key
```

`config.json` 至少填这几项：

| 键 | 说明 |
|------|------|
| `SESSDATA` / `BILI_JCT` / `DEDE_USER_ID` | B站登录凭证 |
| `OWNER_MID` | 主人的 UID（好感度永远 100，且永不被拉黑） |
| `OR_API_KEY` / `OR_BASE_URL` / `OR_CHAT_MODEL` | 对话模型 |
| `EMBED_*` | 可选；不配则语义记忆检索自动降级为「最近记忆」 |

面板访问口令通过环境变量注入，**不要写进 `config.json`**：

```ini
# /etc/systemd/system/bilibili-panel.service 里的片段
Environment=CHAT_PASSWORD=<强口令>
```

口令优先级：

```
config.json 里的 CHAT_PASSWORD  >  环境变量 CHAT_PASSWORD  >  默认值 admin()
```

在面板「系统设置」里改过密码后，值会落到 `config.json`，此时环境变量不再生效。

> **首次部署务必确认面板没有停留在默认口令上**，可打开 `/api/auth_check` 看 `default_password` 是否为 `true`。

---

## 5. systemd 服务

### 5.1 面板

```ini
# /etc/systemd/system/bilibili-panel.service
[Unit]
Description=Bilibili AI Bot - Web Panel
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=root
WorkingDirectory=/opt/bilibili-ai-bot
Environment=PYTHONUNBUFFERED=1
Environment=CHAT_PASSWORD=<强口令>
ExecStart=/opt/bilibili-ai-bot/venv/bin/python /opt/bilibili-ai-bot/local-chat.py
Restart=always
RestartSec=5
StandardOutput=append:/var/log/bilibili-panel.log
StandardError=append:/var/log/bilibili-panel.log

[Install]
WantedBy=multi-user.target
```

### 5.2 Bot

```ini
# /etc/systemd/system/bilibili-bot.service
[Unit]
Description=Bilibili AI Bot - Worker
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=root
WorkingDirectory=/opt/bilibili-ai-bot
Environment=PYTHONUNBUFFERED=1
ExecStart=/opt/bilibili-ai-bot/venv/bin/python /opt/bilibili-ai-bot/ai.py
Restart=always
RestartSec=10
StandardOutput=append:/var/log/bilibili-bot.log
StandardError=append:/var/log/bilibili-bot.log

[Install]
WantedBy=multi-user.target
```

```bash
systemctl daemon-reload
systemctl enable --now bilibili-panel bilibili-bot
systemctl status bilibili-panel --no-pager
```

> `RestartSec` 面板用 5 秒、Bot 用 10 秒：Bot 重启会重新建连 B站，间隔太短容易触发风控。

---

## 6. 暴露到公网（必须带 TLS）

面板本身只监听明文 HTTP。这不是偷懒，而是刻意把 TLS 交给更专业的边界去终止；但**生产环境必须在面板前面放一层 TLS**，原因有二：

| 原因 | 后果 |
|------|------|
| 明文传输口令与会话 Cookie | 等于把面板交出去 |
| 浏览器 WebCrypto 只在安全上下文（`https://` 或 `localhost`）可用 | 没有 TLS 就没有前端口令密封，只能退回明文提交 |

<details open>
<summary><b>方式一：反向代理（Nginx / Caddy）</b></summary>

<br>

```nginx
server {
    listen 443 ssl;
    server_name <你的域名>;

    ssl_certificate     /path/to/fullchain.pem;
    ssl_certificate_key /path/to/privkey.pem;

    location / {
        proxy_pass http://127.0.0.1:5000;
        proxy_set_header Host              $host;
        proxy_set_header X-Real-IP         $remote_addr;
        proxy_set_header X-Forwarded-For   $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;   # 面板据此判定 TLS
        proxy_read_timeout 300s;                      # 聊天是长请求，别掐太早
    }
}
```

</details>

<details>
<summary><b>方式二：内网穿透隧道</b></summary>

<br>

适合没有域名、或服务器 443 不可用的场景。以 frp 类客户端为例：

```ini
[common]
user = <你的隧道账号>
server_addr = <你的隧道服务端>
server_port = <隧道服务端口>

[WebUI]
type = tcp
local_ip = 127.0.0.1
local_port = 5000
remote_port = <你的远程端口>
auto_https = auto          # 由隧道边缘生成证书，浏览器会提示自签警告
```

> `auto_https = auto` 用的是边缘自签证书，浏览器会报不受信任。想要不报警告，就在隧道侧绑定域名并使用受信任证书。

</details>

### 面板侧的反代感知

面板只在「请求来自回环地址」时才采信 `X-Forwarded-Proto`，因此反向代理 / 隧道必须与面板同机（或经由本机转发），否则 `tls` 判定不会生效 —— 这是为了避免伪造请求头骗过 HSTS 与 `Secure Cookie` 判定。

---

## 7. 日志与轮转

```ini
# /etc/logrotate.d/bilibili-ai-bot
/var/log/bilibili-panel.log /var/log/bilibili-bot.log {
    daily
    rotate 7
    missingok
    notifempty
    copytruncate
}
```

`copytruncate` 是**必需的**：服务用 `append:` 持有文件描述符直接写，不做 truncate 会一直写旧 inode。

---

## 8. 上线自检清单

```bash
# 1) 服务在跑
systemctl is-active bilibili-panel bilibili-bot

# 2) 端口在听
ss -ltnp | grep :5000

# 3) 未登录访问 API 必须 401，敏感路径必须 404
curl -s -o /dev/null -w '%{http_code}\n' http://127.0.0.1:5000/api/block_suggestions   # 401
curl -s -o /dev/null -w '%{http_code}\n' http://127.0.0.1:5000/data/images/x.jpg       # 404

# 4) 登录页可访问
curl -s -o /dev/null -w '%{http_code}\n' http://127.0.0.1:5000/                       # 200

# 5) 语法与关键文件就位
./venv/bin/python -m py_compile local-chat.py ai.py

# 6) 自动拉黑开关符合预期（默认应为 False）
./venv/bin/python -c "import json;c=json.load(open('config.json'));print(c.get('AUTO_BLOCK_ON_AFFECTION'), c.get('PRIVATE_MESSAGE_AUTO_BLOCK'))"
```

预期的自检结果：

| 检查项 | 期望值 |
|--------|--------|
| 服务状态 | `active` / `active` |
| `5000` 端口 | 处于 `LISTEN` |
| `/api/block_suggestions`（未登录） | `401` |
| `/data/images/x.jpg`（未登录） | `404` |
| `/`（登录页） | `200` |
| `py_compile` | 无输出（即无语法错误） |
| 自动拉黑开关 | `False False` |

浏览器侧再确认三件事：能登录、能发一条消息并收到回复、侧栏「安全中心」能打开。

---

## 9. 升级流程

```bash
cd /opt/bilibili-ai-bot
TS=$(date +%Y%m%d-%H%M%S)
for f in local-chat.py chat.html ai.py; do cp -a "$f" "$f.bak.$TS"; done

# 覆盖新文件（上传前确认行尾为 LF）
./venv/bin/python -m py_compile local-chat.py ai.py

systemctl restart bilibili-bot bilibili-panel
sleep 8
systemctl is-active bilibili-panel bilibili-bot
tail -n 20 /var/log/bilibili-panel.log
```

> **排查问题时先看日志里的真实异常**：面板把未预期异常收敛成 JSON 错误返回给前端，前端只会显示一句「出了点问题」，真实原因一定在 `/var/log/bilibili-panel.log` 里。

### 升级时的数据清理

从旧版本升级时有两处**运行数据**需要手工处理，否则改动看着生效、实际行为不变：

```bash
cd /opt/bilibili-ai-bot

# 1) 视频缓存里的「假分析」：旧版视觉分析 400 失败时拼的元信息串
#    识别特征：analysis 以「视频《…》，UP主：…，分区：」开头
./venv/bin/python - <<'PY'
import json, shutil, time
p = "data/video_memory.json"
d = json.load(open(p, encoding="utf-8"))
bad = [k for k, e in d.items()
       if (e.get("analysis") or "").startswith(
           "视频《%s》，UP主：%s，分区：" % (e.get("title", ""), e.get("owner_name", "")))]
if bad:
    shutil.copy2(p, "%s.bak.%s" % (p, time.strftime("%Y%m%d-%H%M%S")))
    for k in bad:
        d.pop(k, None)
    json.dump(d, open(p, "w", encoding="utf-8"), ensure_ascii=False, indent=2)
print("清理降级缓存 %d 条" % len(bad))
PY

# 2) config.json 补齐新增键（缺失时后端会走默认值，但补齐后设置页才能正确回显）
./venv/bin/python - <<'PY'
import json
p = "config.json"
d = json.load(open(p, encoding="utf-8"))
add = {"AT_REPLY_MAX_AGE": 3600}
for k in ("CHAT", "VISION", "SEARCH", "IMAGE"):
    add["PRICE_%s_INPUT" % k] = 0
    add["PRICE_%s_OUTPUT" % k] = 0
miss = {k: v for k, v in add.items() if k not in d}
d.update(miss)
json.dump(d, open(p, "w", encoding="utf-8"), ensure_ascii=False, indent=2)
print("补齐配置键 %d 个：%s" % (len(miss), sorted(miss)))
PY
```

> 清理视频缓存只影响**可再生的分析缓存**，不会动 Cookie、账号、好感度、记忆等任何不可再生数据。备份文件保留在 `data/` 下，可随时还原。

---

## 10. 安全加固清单

面板长期挂在公网（经隧道暴露），以下项均已落地。逐项的**根因与判定逻辑**见 [FIXES.md 第六节](FIXES.md#六面板安全加固)，此处只列部署侧需要知道的部分。

### 已完成

| 项 | 实现 | 为什么 / 部署注意 |
|----|------|-------------------|
| 鉴权 | 默认拒绝：`before_request` 兜底，未登录 `/api/*` 返回 `401`、其余 `404` | 白名单逐条列举而非前缀匹配，**新增路由自动受保护**，不需要维护者记得加装饰器 |
| 静态目录 | `static_folder="static"`，只暴露前端资源 | 历史写法 `static_folder="."` 会让 `/config.json`、`/ai.py`、`/data/**` 可直接下载 —— 升级时务必确认这一行 |
| 口令 | RSA-OAEP(SHA-256) 密封提交，明文仅作降级回退 | 隧道边缘终止 TLS，密封用于避免口令在中间段可读。依赖 `cryptography`，缺失时自动降级而非启动失败 |
| 会话 Cookie | `HttpOnly` + `SameSite=Lax`，`Secure` 按 TLS 动态判定 | `Secure` **不能**直接跟 `PANEL_TLS` 联动：证书缺失回落 HTTP 时，`Secure` Cookie 会被浏览器拒绝存储，表现为「登录成功但页面停在登录页」 |
| 会话密钥 | 持久化 `data/.secret_key`（`0600`），重启不掉线 | 若写成随机生成，每次重启都会让所有会话失效 |
| 反代感知 | 只采信回环地址（`127.0.0.1` / `::1`）来的 `X-Forwarded-Proto` | 该头可伪造；不限制来源时任何直连请求都能把面板骗成「HTTPS 上下文」 |
| 安全响应头 | `nosniff` / `X-Frame-Options: DENY` / `Referrer-Policy: no-referrer`；`/api/*` 加 `no-store`；HSTS 仅在安全上下文发 | 避免聊天记录与 UID 被中间层缓存 |
| 头像路由 | `/media/bot-avatar` 不接受任何文件名参数，只认配置指向的文件 | 「没有可控输入也就没有穿越面」；登录页需在鉴权前展示头像，因此单独放行 |
| 拉黑动作 | 只由人工在面板确认，Bot 不自动封人 | 避免误判导致不可逆操作 |
| 敏感文件 | `config.json` 与 `data/` 全程不入库（见 `.gitignore`） | 同时确认 `data/` 目录权限，其中含会话密钥与密封私钥 |

### 上线验证

权限问题的判据在**未登录**这一侧，不要只测「登录后能不能用」：

```bash
# 数据接口：必须 401
curl -s -o /dev/null -w "%{http_code}\n" http://127.0.0.1:5000/api/summary
# 敏感文件：必须 404（若为 200，说明静态根没收到 static/）
curl -s -o /dev/null -w "%{http_code}\n" http://127.0.0.1:5000/config.json
curl -s -o /dev/null -w "%{http_code}\n" http://127.0.0.1:5000/ai.py
# 登录前必须可达：必须 200
curl -s -o /dev/null -w "%{http_code}\n" http://127.0.0.1:5000/api/handshake
curl -s -o /dev/null -w "%{http_code}\n" http://127.0.0.1:5000/
```

### 建议补充

按当前代码实测，以下为已知但未处理项：

- [ ] **`/api/config` 会原样返回 `CHAT_PASSWORD`** —— 脱敏函数只匹配字段名含 `KEY` / `TOKEN` / `SESSDATA` / `JCT` 且长度 > 10 的项，口令字段不在其中。虽然该接口本身需要登录，但前端读取脱敏配置时应顺带把口令字段屏蔽（建议同时改 `config.get_config()` 的脱敏规则）
- [ ] **启动日志明文打印口令** —— `local-chat.py` 启动块里的 `print(f"访问密码：{AUTH_PASSWORD}")` 会写入 `/var/log/bilibili-panel.log`。建议改为仅在口令仍为默认值时提示「请尽快修改」，否则不回显
- [ ] **`yt-dlp` 的 `bvid` 拼接** —— `Proactive.py` 中 `f"https://www.bilibili.com/video/{bvid}"` 直接拼进子进程参数。当前 `bvid` 只来自 B站 API 响应、非用户输入，**不构成 SSRF 或命令注入**；但建议加一道 `^BV[0-9A-Za-z]{10}$` 白名单校验，避免将来引入用户输入路径时无声变成漏洞
- [ ] 面板登录失败限速（当前未实现，公网暴露时建议在反向代理层加限制）
- [ ] 反向代理层限制来源 IP 或叠加一层 Basic Auth
- [ ] 定期轮换 `CHAT_PASSWORD` 与 B站 Cookie
- [ ] 服务器防火墙只放行必要端口，管理端口限制来源
- [ ] 清理工作区根目录遗留的 `panel-ca.crt` 等调试产物，避免误入库
