# 生产部署指南

面向「一台 Linux 服务器长期跑 Bot + 面板」的场景。所有地址、端口、口令一律用占位符，
请替换为你自己的值；**不要把真实值写进仓库**。

---

## 1. 前置条件

| 项 | 要求 |
| --- | --- |
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
| --- | --- |
| `SESSDATA` / `BILI_JCT` / `DEDE_USER_ID` | B站登录凭证 |
| `OWNER_MID` | 主人的 UID（好感度永远 100，且永不被拉黑） |
| `OR_API_KEY` / `OR_BASE_URL` / `OR_CHAT_MODEL` | 对话模型 |
| `EMBED_*` | 可选；不配则语义记忆检索自动降级为「最近记忆」 |

面板访问口令通过环境变量注入，**不要写进 config.json**：

```ini
# /etc/systemd/system/bilibili-panel.service 里的片段
Environment=CHAT_PASSWORD=<强口令>
```

口令优先级：`config.json` 里的 `CHAT_PASSWORD` > 环境变量 `CHAT_PASSWORD` > 默认值 `admin()`。
在面板「系统设置」里改过密码后，值会落到 `config.json`，此时环境变量不再生效。

> 首次部署务必确认面板没有停留在默认口令上，
> 可打开 `/api/auth_check` 看 `default_password` 是否为 `true`。

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

---

## 6. 暴露到公网（必须带 TLS）

面板本身只监听明文 HTTP。这不是偷懒，而是刻意把 TLS 交给更专业的边界去终止；
但**生产环境必须在面板前面放一层 TLS**，原因有二：

1. 明文传输口令与会话 Cookie，等于把面板交出去
2. 浏览器的 WebCrypto 只在安全上下文（`https://` 或 `localhost`）可用，
   没有 TLS 就没有前端口令密封，只能退回明文提交

### 方式一：反向代理（Nginx / Caddy）

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

### 方式二：内网穿透隧道

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

> `auto_https = auto` 用的是边缘自签证书，浏览器会报不受信任。
> 想要不报警告，就在隧道侧绑定域名并使用受信任证书。

### 面板侧的反代感知

面板只在「请求来自回环地址」时才采信 `X-Forwarded-Proto`，
因此反向代理/隧道必须与面板同机（或经由本机转发），否则 `tls` 判定不会生效 ——
这是为了避免伪造请求头骗过 HSTS 与 `Secure Cookie` 判定。

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

`copytruncate` 是必需的：服务用 `append:` 持有文件描述符直接写，不做 truncate 会一直写旧 inode。

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

排查问题时先看日志里的真实异常：面板把未预期异常收敛成 JSON 错误返回给前端，
前端只会显示一句「出了点问题」，真实原因一定在 `/var/log/bilibili-panel.log` 里。

---

## 10. 安全加固清单

已完成：

- 默认拒绝鉴权，未登录 `/api/*` 401、其余 404
- 口令 RSA-OAEP 密封提交，明文仅作降级回退
- 会话 Cookie `HttpOnly` + `SameSite=Lax`，`Secure` 按 TLS 动态判定
- 会话密钥持久化 0600，重启不掉线
- `/media/bot-avatar` 不接受文件名参数，防目录穿越
- 拉黑动作只由人工在面板确认，Bot 不自动封人
- `config.json` 与 `data/` 全程不入库（见 `.gitignore`）

建议补充：

- 面板登录失败限速（当前未实现，公网暴露时建议在反向代理层加限制）
- 反向代理层限制来源 IP 或叠加一层 Basic Auth
- 定期轮换 `CHAT_PASSWORD` 与 B站 Cookie
- 服务器防火墙只放行必要端口，管理端口限制来源
