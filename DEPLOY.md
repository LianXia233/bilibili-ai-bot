# 生产部署指南

<div align="center">

面向「一台 Linux 服务器长期跑 Bot + 面板」的场景

![Platform](https://img.shields.io/badge/Platform-Linux-4B5563?style=flat-square&logo=linux&logoColor=white)
![Init](https://img.shields.io/badge/Init-systemd-000000?style=flat-square&logo=systemd&logoColor=white)
![Rust](https://img.shields.io/badge/Backend-Rust%20%2F%20Axum-DEA584?style=flat-square&logo=rust&logoColor=white)
![TLS](https://img.shields.io/badge/TLS-required%20for%20public-0F9D58?style=flat-square&logo=letsencrypt&logoColor=white)

</div>

> **占位符约定**：本文所有地址、端口、账号、口令一律使用占位符（`<你的域名>`、`<强口令>` 等），请替换为你自己的值。**不要把真实值写进仓库。**

> **后端形态**：仓库已移除 Python/Flask 版，当前唯一后端为 `rust-backend/`（Rust + Axum + tokio + reqwest）。单进程默认同时承担「Bot 主循环（评论/私信/主动行为/动态）」与「Web 面板」，无需再跑两个进程。

## 步骤索引

| 步骤 | 内容 | 是否必做 |
|:----:|------|:--------:|
| [1](#1-前置条件) | 前置条件 | — |
| [2](#2-构建) | 构建（或直接使用 release 二进制） | 必做 |
| [3](#3-落地目录) | 落地目录 | 必做 |
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
| 系统 | 任意主流 Linux 发行版（本文以 systemd 为例）；构建需 Rust 1.98+ |
| 构建 | 有 Rust 工具链的机器上 `cargo build --release`，或直接使用发布的 release 二进制（约 8MB，无运行时依赖） |
| 网络 | 服务器能直连 B站 API 与你的模型网关 |
| 权限 | 能创建 systemd 服务；若面板要暴露公网，还需域名或内网穿透账号 |
| 可选外部命令 | `yt-dlp`、`ffmpeg`（仅主动刷视频 / 动态生图需要；缺失时自动降级为「仅元信息分析 / 纯文字动态」） |

---

## 2. 构建

```bash
cd rust-backend
cargo build --release
# 产物：rust-backend/target/release/bilibili-ai-bot-rs（约 8MB）
```

把二进制与前端、配置一起拷到服务器（也可直接在服务器上 clone 构建）：

```bash
scp rust-backend/target/release/bilibili-ai-bot-rs <user>@<server>:/opt/bilibili-ai-bot-rs/
scp chat.html config.example.json <user>@<server>:/opt/bilibili-ai-bot-rs/
```

---

## 3. 落地目录

约定安装到 `/opt/bilibili-ai-bot-rs`，与运行数据分离：

```
/opt/bilibili-ai-bot-rs/          # 二进制 + 前端 + 配置
/opt/bilibili-ai-bot-rs/data/     # 运行时数据：记忆、好感度、安全日志、会话密钥、加密身份
/var/log/bilibili-rs.log          # 运行日志（Bot + 面板合一）
```

```bash
sudo mkdir -p /opt/bilibili-ai-bot-rs/data
cd /opt/bilibili-ai-bot-rs
# 二进制与 chat.html 放这里
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
# /etc/systemd/system/bilibili-rs.service 里的片段
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

单服务同时跑 Bot 主循环与 Web 面板：

```ini
# /etc/systemd/system/bilibili-rs.service
[Unit]
Description=Bilibili AI Bot - Rust Backend
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=root
WorkingDirectory=/opt/bilibili-ai-bot-rs
Environment=RUST_LOG=info
Environment=CHAT_PASSWORD=<强口令>
ExecStart=/opt/bilibili-ai-bot-rs/bilibili-ai-bot-rs --base-dir /opt/bilibili-ai-bot-rs --port 5000
Restart=always
RestartSec=5
StandardOutput=append:/var/log/bilibili-rs.log
StandardError=append:/var/log/bilibili-rs.log

[Install]
WantedBy=multi-user.target
```

```bash
systemctl daemon-reload
systemctl enable --now bilibili-rs
systemctl status bilibili-rs --no-pager
```

若想拆开跑（例如面板与 Bot 分开监控）：

```bash
# 只跑面板
bilibili-ai-bot-rs --base-dir /opt/bilibili-ai-bot-rs --port 5000 --no-bot
# 只跑 Bot 主循环
bilibili-ai-bot-rs --base-dir /opt/bilibili-ai-bot-rs --no-web
```

---

## 6. 暴露到公网（必须带 TLS）

面板本身只监听明文 HTTP。这不是偷懒，而是刻意把 TLS 交给更专业的边界去终止；但**生产环境必须在面板前面放一层 TLS**，原因有二：

1. 面板口令、Cookie、对话内容都走 HTTP 明文，抓包即得；
2. 应用层加密（`/api/crypto/*` + `/api/data` 网关，X25519 + HKDF-SHA256 + AES-256-GCM）能防**被动抓包直读正文**，但**不等价于 HTTPS**，无法对抗主动中间人。**公网访问必须套 TLS（Nginx/Caddy/反代），应用层加密只是纵深防御。**

### Nginx 反代示例（TLS 终止）

```nginx
server {
    listen 443 ssl http2;
    server_name <你的域名>;

    ssl_certificate     /etc/letsencrypt/live/<你的域名>/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/<你的域名>/privkey.pem;

    client_max_body_size 20m;

    location / {
        proxy_pass http://127.0.0.1:5000;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }
}

server {
    listen 80;
    server_name <你的域名>;
    return 301 https://$host$request_uri;
}
```

证书用 Certbot 申请：`certbot --nginx -d <你的域名>`。

### 面板侧的反代感知

面板通过 `X-Forwarded-Proto` 感知是否已由 TLS 反代终结：

- 直接 HTTP 访问时，前端登录页会显示「HTTP + 应用层加密（X25519 / AES-256-GCM）…不等价 HTTPS」提示与服务器身份指纹；
- 经 HTTPS 反代访问时，提示按 `tls: false` 逻辑展示安全边界说明。

---

## 7. 日志与轮转

```ini
# /etc/logrotate.d/bilibili-rs
/var/log/bilibili-rs.log {
    daily
    rotate 14
    compress
    delaycompress
    missingok
    notifempty
    copytruncate
}
```

日志级别由 `RUST_LOG` 控制（默认 `info`；排查用 `RUST_LOG=debug`）。

---

## 8. 上线自检清单

```bash
# 1) 服务在跑
systemctl is-active bilibili-rs

# 2) 端口在听
ss -tlnp | grep 5000

# 3) 健康检查
curl -s http://127.0.0.1:5000/api/health

# 4) 加密握手（应用层加密协议端点）
curl -s http://127.0.0.1:5000/api/crypto/handshake
# 应返回 {"v":1,"server_static_pub":"...","fingerprint":"...","max_sessions":...}

# 5) 明文业务端点已收口（预期 404）
curl -s -o /dev/null -w "%{http_code}\n" -X POST http://127.0.0.1:5000/api/login
# 404

# 6) 面板可登录（浏览器访问 http://<IP>:5000，输入 CHAT_PASSWORD）
#    登录框应显示「当前为 HTTP + 应用层加密…服务器身份指纹 …」
```

> 通过 HTTPS 反代访问时，第 5 项在反代层看到的是 404 或由反代拦截；第 4 项指纹应与浏览器登录页显示的指纹一致。

---

## 9. 升级流程

```bash
# 1) 备份当前二进制
cp /opt/bilibili-ai-bot-rs/bilibili-ai-bot-rs /opt/bilibili-ai-bot-rs/bilibili-ai-bot-rs.bak-$(date +%Y%m%d-%H%M%S)

# 2) 上传新二进制与前端
scp rust-backend/target/release/bilibili-ai-bot-rs <user>@<server>:/opt/bilibili-ai-bot-rs/
scp chat.html <user>@<server>:/opt/bilibili-ai-bot-rs/

# 3) 重启
systemctl restart bilibili-rs

# 4) 复检（第 8 节清单）
```

> 前端 `chat.html` 是静态文件、每次请求读盘，替换后无需重启；后端二进制替换后需 `systemctl restart`。
> 运行时数据（`data/`）不随升级覆盖；加密身份文件 `data/server_crypto_identity.bin` 持久化，重启不漂移，浏览器端指纹不变。

---

## 10. 安全加固清单

- [ ] 改掉默认口令（`admin()`），用环境变量注入强口令
- [ ] 公网必须套 TLS（Nginx/Caddy），应用层加密仅作纵深防御
- [ ] 服务器身份指纹（登录页显示）首次使用时在可信渠道核对（TOFU）
- [ ] `config.json` 权限 600；不要提交到仓库（`.gitignore` 已忽略）
- [ ] 生产机防火墙只放行必要端口（SSH / 面板 / 反代）
- [ ] 面板若无需公网访问，不开放 5000 端口（仅经本机反代访问，或由防火墙仅放行反代来源）
- [ ] 定期 `systemctl status bilibili-rs` + 查看 `/var/log/bilibili-rs.log` 异常
