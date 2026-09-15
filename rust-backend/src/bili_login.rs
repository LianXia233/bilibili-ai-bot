//! B站二维码登录（与 bili_login.py 对齐）。
//!
//! 二维码在本机生成；登录 Cookie 保留在会话中，成功后由调用方写入配置，
//! 不把凭证明文返回给浏览器。

use crate::config::Config;
use crate::error::{AppError, Result};
use crate::util::gen_token;
use qrcode::QrCode;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use url::Url;

const QR_GENERATE_URL: &str = "https://passport.bilibili.com/x/passport-login/web/qrcode/generate";
const QR_POLL_URL: &str = "https://passport.bilibili.com/x/passport-login/web/qrcode/poll";
const NAV_URL: &str = "https://api.bilibili.com/x/web-interface/nav";

pub const QR_WAITING: i64 = 86101;
pub const QR_SCANNED: i64 = 86090;
pub const QR_EXPIRED: i64 = 86038;
pub const QR_CONFIRMED: i64 = 0;

struct QrSession {
    http: reqwest::Client,
    qrcode_key: String,
    created_at: Instant,
}

pub struct BiliQrLoginManager {
    sessions: RwLock<HashMap<String, QrSession>>,
    ttl: Duration,
}

fn qr_headers() -> reqwest::header::HeaderMap {
    let mut h = reqwest::header::HeaderMap::new();
    h.insert("User-Agent", reqwest::header::HeaderValue::from_static(crate::bili_api::UA));
    h.insert("Referer", reqwest::header::HeaderValue::from_static("https://www.bilibili.com/"));
    h
}

fn qr_svg_data_url(login_url: &str) -> String {
    let code = QrCode::new(login_url.as_bytes());
    match code {
        Ok(code) => {
            let svg = code
                .render::<qrcode::render::svg::Color>()
                .min_dimensions(4, 4)
                .quiet_zone(true)
                .build();
            format!("data:image/svg+xml;base64,{}", crate::util::b64_encode(svg.as_bytes()))
        }
        Err(_) => String::new(),
    }
}

impl BiliQrLoginManager {
    pub fn new(ttl_secs: u64) -> Self {
        BiliQrLoginManager {
            sessions: RwLock::new(HashMap::new()),
            ttl: Duration::from_secs(ttl_secs.max(60)),
        }
    }

    fn cleanup(&self) {
        let mut s = self.sessions.write().unwrap();
        s.retain(|_, v| v.created_at.elapsed() < self.ttl);
    }

    pub async fn start(&self) -> Result<Value> {
        self.cleanup();
        let client = reqwest::Client::builder().cookie_store(true).build()?;
        let resp = client
            .get(QR_GENERATE_URL)
            .headers(qr_headers())
            .query(&[
                ("source", "main-fe-header"),
                ("go_url", "https://www.bilibili.com/"),
                ("web_location", "333.1007"),
            ])
            .send()
            .await?;
        let _set_cookies: Vec<String> = resp
            .headers()
            .get_all("set-cookie")
            .iter()
            .filter_map(|sc| sc.to_str().ok().map(|s| s.to_string()))
            .collect();
        let payload: Value = resp.json().await?;
        if payload.get("code").and_then(|c| c.as_i64()) != Some(0) {
            return Err(AppError::Api {
                code: payload.get("code").and_then(|c| c.as_i64()).unwrap_or(-1),
                msg: payload.get("message").and_then(|m| m.as_str()).unwrap_or("获取登录二维码失败").to_string(),
            });
        }
        let login_url = payload["data"]["url"].as_str().unwrap_or("").to_string();
        let qrcode_key = payload["data"]["qrcode_key"].as_str().unwrap_or("").to_string();
        if login_url.is_empty() || qrcode_key.is_empty() {
            return Err(AppError::Api { code: -1, msg: "获取登录二维码失败: 返回值缺少 url 或 qrcode_key".into() });
        }
        let token = gen_token();
        self.sessions.write().unwrap().insert(
            token.clone(),
            QrSession { http: client, qrcode_key, created_at: Instant::now() },
        );
        Ok(json!({
            "token": token,
            "image": qr_svg_data_url(&login_url),
            "expires_in": self.ttl.as_secs(),
        }))
    }

    pub async fn poll(&self, token: &str) -> Result<Value> {
        self.cleanup();
        let item = match self.sessions.read().unwrap().get(token) {
            Some(v) => {
                // 需要可变：取出后 clone client 与 key
                (v.http.clone(), v.qrcode_key.clone())
            }
            None => {
                return Ok(json!({"status": "expired", "message": "二维码已过期，请重新获取"}));
            }
        };
        let (client, qrcode_key) = item;
        let resp = client
            .get(QR_POLL_URL)
            .headers(qr_headers())
            .query(&[("qrcode_key", qrcode_key.as_str()), ("source", "main-fe-header")])
            .send()
            .await?;
        let set_cookies: Vec<String> = resp
            .headers()
            .get_all("set-cookie")
            .iter()
            .filter_map(|sc| sc.to_str().ok().map(|s| s.to_string()))
            .collect();
        let payload: Value = resp.json().await?;
        if payload.get("code").and_then(|c| c.as_i64()) != Some(0) {
            return Err(AppError::Api {
                code: payload.get("code").and_then(|c| c.as_i64()).unwrap_or(-1),
                msg: payload.get("message").and_then(|m| m.as_str()).unwrap_or("查询扫码状态失败").to_string(),
            });
        }
        let data = &payload["data"];
        let code = data.get("code").and_then(|c| c.as_i64()).unwrap_or(-1);
        match code {
            QR_WAITING => return Ok(json!({"status": "waiting", "message": "等待扫码"})),
            QR_SCANNED => return Ok(json!({"status": "scanned", "message": "已扫码，请在 App 内确认"})),
            QR_EXPIRED => {
                self.sessions.write().unwrap().remove(token);
                return Ok(json!({"status": "expired", "message": "二维码已过期，请重新获取"}));
            }
            _ => {}
        }
        if code != QR_CONFIRMED {
            return Ok(json!({"status": "waiting", "message": data.get("message").and_then(|m| m.as_str()).unwrap_or(&format!("等待确认（{code}）"))}));
        }

        let redirect_url = data.get("url").and_then(|v| v.as_str()).unwrap_or("").to_string();
        if !redirect_url.is_empty() {
            let _ = client.get(&redirect_url).headers(qr_headers()).send().await;
        }

        // 从响应 Cookie + 跳转 URL 查询参数收集凭据
        let mut cookies: HashMap<String, String> = HashMap::new();
        for sc in set_cookies {
            if let Some((k, v)) = sc.split_once('=') {
                let k = k.trim().to_string();
                let v = v.split(';').next().unwrap_or("").to_string();
                cookies.insert(k, v);
            }
        }
        if let Ok(u) = Url::parse(&redirect_url) {
            for (k, v) in u.query_pairs() {
                if matches!(k.as_ref(), "SESSDATA" | "bili_jct" | "DedeUserID") {
                    cookies.insert(k.into_owned(), v.into_owned());
                }
            }
        }
        let sessdata = cookies.get("SESSDATA").cloned().unwrap_or_default();
        let bili_jct = cookies.get("bili_jct").cloned().unwrap_or_default();
        if sessdata.is_empty() || bili_jct.is_empty() {
            return Err(AppError::Api { code: -1, msg: "扫码已确认，但响应中没有完整 Cookie".into() });
        }

        // nav 校验登录态
        let nav = client
            .get(NAV_URL)
            .headers(qr_headers())
            .header("Cookie", format!("SESSDATA={sessdata}; bili_jct={bili_jct}"))
            .send()
            .await?;
        let nav_payload: Value = nav.json().await?;
        let is_login = nav_payload["data"]["isLogin"].as_bool().unwrap_or(false);
        if nav_payload.get("code").and_then(|c| c.as_i64()) != Some(0) || !is_login {
            return Err(AppError::Api { code: -1, msg: "扫码成功，但登录状态校验失败".into() });
        }
        let account = &nav_payload["data"];
        let dede_user_id = if cookies.get("DedeUserID").map(|v| !v.is_empty()).unwrap_or(false) {
            cookies.get("DedeUserID").cloned().unwrap_or_default()
        } else {
            account.get("mid").map(|m| m.to_string()).unwrap_or_default()
        };

        self.sessions.write().unwrap().remove(token);
        Ok(json!({
            "status": "confirmed",
            "message": "登录成功",
            "config": {
                "SESSDATA": sessdata,
                "BILI_JCT": bili_jct,
                "DEDE_USER_ID": dede_user_id,
                "REFRESH_TOKEN": data.get("refresh_token").and_then(|v| v.as_str()).unwrap_or(""),
            },
            "account": {
                "mid": account.get("mid").map(|m| m.to_string()).unwrap_or_default(),
                "name": account.get("uname").and_then(|v| v.as_str()).unwrap_or(""),
                "level": account["level_info"]["current_level"].as_i64().unwrap_or(0),
            }
        }))
    }
}

// 供 Web 面板使用：登录成功后写入配置
#[allow(dead_code)]
pub async fn apply_qr_login_config(config: &Arc<RwLock<Config>>, patch: &Value) {
    let _ = crate::config::update_config(config, patch);
}
