//! B站 API 客户端：WBI 签名、消息流轮询、回复发送、用户屏蔽、视频信息。
//!
//! 与 Python ai.py / Proactive.py 的 B站 调用收敛到单一模块（bili_api.rs），
//! 后续接口变更只改这一处。

use crate::config::Config;
use crate::error::{AppError, Result};
use crate::util::md5_hex;
use serde_json::{json, Value};
use std::sync::atomic::AtomicI64;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

/// 与 ai.py 一致的屏蔽关键词。
pub const BLOCK_KEYWORDS: [&str; 7] = ["傻逼", "草泥马", "滚", "死", "废物", "智障", "脑残"];

pub const AT_EMPTY_CONTENT: &str = "（对方在评论里 @ 了我，但没有写别的内容）";

pub const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36";

/// 一条待处理评论/@。
#[derive(Debug, Clone)]
pub struct ReplyItem {
    pub rpid: i64,
    pub root_rpid: i64,
    pub oid: i64,
    pub thread_id: String,
    pub content_type: i64,
    pub content: String,
    #[allow(dead_code)]
    pub raw_content: String,
    pub username: String,
    pub mid: i64,
    pub via: String,
    pub no_content: bool,
}

// ============ WBI 签名 ============

#[allow(dead_code)]
const WBI_MIXIN_KEY_ENC_TAB: [usize; 64] = [
    46, 47, 18, 2, 53, 8, 23, 32, 15, 50, 10, 31, 58, 3, 45, 35, 27, 43, 5, 49, 33, 9, 42, 19,
    29, 28, 14, 39, 12, 38, 41, 13, 37, 48, 7, 16, 24, 55, 40, 61, 26, 17, 0, 1, 60, 51, 30, 4,
    22, 25, 54, 21, 56, 59, 6, 63, 57, 62, 11, 36, 20, 34, 44, 52,
];

fn mixin_key(raw_key: &str) -> String {
    let bytes: Vec<char> = raw_key.chars().collect();
    WBI_MIXIN_KEY_ENC_TAB
        .iter()
        .take(32)
        .filter_map(|&i| bytes.get(i))
        .collect()
}

struct WbiCache {
    img_key: String,
    sub_key: String,
    at: Instant,
}

/// B站 客户端。
pub struct BiliClient {
    pub http: reqwest::Client,
    config: Arc<RwLock<Config>>,
    wbi: RwLock<WbiCache>,
    /// 上一轮轮询到的 mid 集合，用于「用户在同一串内连续说话」判断（保持简单：不跨轮状态）。
    #[allow(dead_code)]
    pub last_poll_at: AtomicI64,
}

impl BiliClient {
    pub fn new(config: Arc<RwLock<Config>>) -> Self {
        let http = reqwest::Client::builder()
            .cookie_store(true)
            .timeout(Duration::from_secs(20))
            .build()
            .expect("reqwest client 构建失败");
        BiliClient {
            http,
            config,
            wbi: RwLock::new(WbiCache { img_key: String::new(), sub_key: String::new(), at: Instant::now() - Duration::from_secs(7200) }),
            last_poll_at: AtomicI64::new(0),
        }
    }

    pub fn cookie_header(&self) -> String {
        let cfg = self.config.read().unwrap();
        format!(
            "SESSDATA={}; bili_jct={}; DedeUserID={}",
            cfg.get_str("SESSDATA"),
            cfg.get_str("BILI_JCT"),
            cfg.get_str("DEDE_USER_ID")
        )
    }

    fn headers(&self, referer: &str) -> reqwest::header::HeaderMap {
        let mut h = reqwest::header::HeaderMap::new();
        h.insert("User-Agent", reqwest::header::HeaderValue::from_static(UA));
        if let Ok(r) = reqwest::header::HeaderValue::from_str(referer) {
            h.insert("Referer", r);
        }
        if let Ok(c) = reqwest::header::HeaderValue::from_str(&self.cookie_header()) {
            h.insert("Cookie", c);
        }
        h
    }

    /// 拉取 wbi 密钥（1 小时缓存），失败返回空串。
    async fn get_wbi_keys(&self) -> (String, String) {
        {
            let w = self.wbi.read().unwrap();
            if !w.img_key.is_empty() && w.at.elapsed() < Duration::from_secs(3600) {
                return (w.img_key.clone(), w.sub_key.clone());
            }
        }
        let url = "https://api.bilibili.com/x/web-interface/nav";
        let resp = self
            .http
            .get(url)
            .headers(self.headers("https://www.bilibili.com/"))
            .send()
            .await;
        let payload = match resp {
            Ok(r) => match r.json::<Value>().await {
                Ok(v) => v,
                Err(_) => return (String::new(), String::new()),
            },
            Err(_) => return (String::new(), String::new()),
        };
        if payload.get("code").and_then(|c| c.as_i64()) != Some(0) {
            return (String::new(), String::new());
        }
        let wbi_img = &payload["data"]["wbi_img"];
        let img_url = wbi_img.get("img_url").and_then(|v| v.as_str()).unwrap_or("");
        let sub_url = wbi_img.get("sub_url").and_then(|v| v.as_str()).unwrap_or("");
        if img_url.is_empty() || sub_url.is_empty() {
            return (String::new(), String::new());
        }
        let img_key = img_url.rsplit('/').next().unwrap_or("").split('.').next().unwrap_or("").to_string();
        let sub_key = sub_url.rsplit('/').next().unwrap_or("").split('.').next().unwrap_or("").to_string();
        {
            let mut w = self.wbi.write().unwrap();
            w.img_key = img_key.clone();
            w.sub_key = sub_key.clone();
            w.at = Instant::now();
        }
        (img_key, sub_key)
    }

    /// 给 params 加 wbi 签名（w_rid + wts）。拿不到密钥则原样返回。
    #[allow(dead_code)]
    pub async fn sign_wbi_params(&self, params: &mut Vec<(String, String)>) {
        let (img_key, sub_key) = self.get_wbi_keys().await;
        if img_key.is_empty() || sub_key.is_empty() {
            return;
        }
        let mixin = mixin_key(&format!("{img_key}{sub_key}"));
        let wts = chrono::Utc::now().timestamp();
        params.push(("wts".into(), wts.to_string()));
        let mut filtered: Vec<(String, String)> = params
            .iter()
            .map(|(k, v)| {
                let cleaned: String = v
                    .chars()
                    .filter(|c| !"!'()*".contains(*c))
                    .collect();
                (k.clone(), cleaned)
            })
            .collect();
        filtered.sort();
        let query: String = filtered
            .iter()
            .map(|(k, v)| format!("{k}={}", urlencode(v)))
            .collect::<Vec<_>>()
            .join("&");
        let w_rid = md5_hex(&format!("{query}{mixin}"));
        params.push(("w_rid".into(), w_rid));
    }

    // ---------- Cookie 检查 ----------
    pub async fn check_cookie(&self) -> (bool, String) {
        let url = "https://api.bilibili.com/x/web-interface/nav";
        match self
            .http
            .get(url)
            .headers(self.headers("https://www.bilibili.com/"))
            .send()
            .await
        {
            Ok(r) => match r.json::<Value>().await {
                Ok(payload) => {
                    let is_login = payload["data"]["isLogin"].as_bool().unwrap_or(false);
                    let uname = payload["data"]["uname"].as_str().unwrap_or("");
                    if payload.get("code").and_then(|c| c.as_i64()) == Some(0) && is_login {
                        (true, format!("Cookie 有效（{uname}）"))
                    } else {
                        let msg = payload.get("message").and_then(|m| m.as_str()).unwrap_or("未知原因");
                        (false, format!("Cookie 已失效（{msg}）"))
                    }
                }
                Err(_) => (false, "nav 接口解析失败".into()),
            },
            Err(e) => (false, format!("网络错误: {e}")),
        }
    }

    // ---------- 评论/AT 轮询 ----------
    pub async fn get_replies(&self) -> Vec<ReplyItem> {
        let url = "https://api.bilibili.com/x/msgfeed/reply";
        let params = vec![("ps".into(), "10".into()), ("pn".into(), "1".into())];
        let payload = match self.get_json(&url, &params, "https://www.bilibili.com/").await {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("回复流接口失败: {e}");
                return vec![];
            }
        };
        let items = payload["data"]["items"].as_array().cloned().unwrap_or_default();
        let mut out = Vec::new();
        for item in &items {
            let r = &item["item"];
            let user = &item["user"];
            let root_rpid = r.get("root_id").and_then(|v| v.as_i64()).unwrap_or(0)
                .max(r.get("source_id").and_then(|v| v.as_i64()).unwrap_or(0));
            let rpid = r.get("source_id").and_then(|v| v.as_i64()).unwrap_or(0);
            let oid = r.get("subject_id").and_then(|v| v.as_i64()).unwrap_or(0);
            if rpid == 0 || oid == 0 {
                continue;
            }
            let raw = r.get("source_content").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let at_list: &[Value] = r["at_details"].as_array().map(|a| a.as_slice()).unwrap_or(&[]);
            let stripped = strip_at_mentions(&strip_reply_prefix(&raw), at_list);
            let no_content = stripped.is_empty();
            out.push(ReplyItem {
                rpid,
                root_rpid,
                oid,
                thread_id: format!("{root_rpid}:{}", user.get("mid").and_then(|v| v.as_i64()).unwrap_or(0)),
                content_type: r.get("business_id").and_then(|v| v.as_i64()).unwrap_or(1),
                content: if stripped.is_empty() { AT_EMPTY_CONTENT.to_string() } else { stripped },
                raw_content: raw,
                username: user.get("nickname").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                mid: user.get("mid").and_then(|v| v.as_i64()).unwrap_or(0),
                via: "reply".into(),
                no_content,
            });
        }
        out
    }

    pub async fn get_at_replies(&self) -> Vec<ReplyItem> {
        let me = self.config.read().unwrap().get_str("DEDE_USER_ID");
        if me.is_empty() || me == "0" {
            tracing::warn!("DEDE_USER_ID 未配置，跳过 @ 消息");
            return vec![];
        }
        let url = "https://api.bilibili.com/x/msgfeed/at";
        let params = vec![("ps".into(), "20".into()), ("pn".into(), "1".into())];
        let payload = match self.get_json(&url, &params, "https://www.bilibili.com/").await {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("@消息接口失败: {e}");
                return vec![];
            }
        };
        let max_age = {
            let cfg = self.config.read().unwrap();
            let v = cfg.get_i64("AT_REPLY_MAX_AGE");
            if v > 0 { Some(v) } else { None }
        };
        let now_ts = crate::util::now_unix();
        let items = payload["data"]["items"].as_array().cloned().unwrap_or_default();
        let mut out = Vec::new();
        for item in &items {
            let r = &item["item"];
            let user = &item["user"];
            if r.get("business_id").and_then(|v| v.as_i64()) != Some(1) {
                continue;
            }
            // 时效判断
            if let Some(max_age) = max_age {
                if let Some(at_time) = item.get("at_time").and_then(|v| v.as_f64()) {
                    if now_ts as f64 - at_time > max_age as f64 {
                        continue;
                    }
                }
            }
            // 只处理确实 @ 到本账号的评论
            let at_details = r.get("at_details").and_then(|v| v.as_array()).cloned().unwrap_or_default();
            let mentions_me = at_details.iter().any(|u| {
                u.get("mid").and_then(|v| v.as_i64()).map(|m| m.to_string()) == Some(me.clone())
            });
            if !at_details.is_empty() && !mentions_me {
                continue;
            }
            let rpid = r.get("source_id").and_then(|v| v.as_i64()).unwrap_or(0);
            let oid = r.get("subject_id").and_then(|v| v.as_i64()).unwrap_or(0);
            if rpid == 0 || oid == 0 {
                continue;
            }
            let root_rpid = r.get("root_id").and_then(|v| v.as_i64()).unwrap_or(0);
            let root_rpid = if root_rpid == 0 { rpid } else { root_rpid };
            let raw = r.get("source_content").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let stripped = strip_at_mentions(&raw, &at_details);
            let no_content = stripped.is_empty();
            out.push(ReplyItem {
                rpid,
                root_rpid,
                oid,
                thread_id: format!("{root_rpid}:{}", user.get("mid").and_then(|v| v.as_i64()).unwrap_or(0)),
                content_type: 1,
                content: if stripped.is_empty() { AT_EMPTY_CONTENT.to_string() } else { stripped },
                raw_content: raw,
                username: user.get("nickname").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                mid: user.get("mid").and_then(|v| v.as_i64()).unwrap_or(0),
                via: "at".into(),
                no_content,
            });
        }
        out
    }

    /// 合并多个消息流并按 rpid 去重（靠前的流优先）。
    pub fn merge_pending(streams: Vec<Vec<ReplyItem>>) -> Vec<ReplyItem> {
        let mut seen = std::collections::HashSet::new();
        let mut merged = Vec::new();
        for stream in streams {
            for r in stream {
                if seen.contains(&r.rpid) {
                    continue;
                }
                seen.insert(r.rpid);
                merged.push(r);
            }
        }
        merged
    }

    // ---------- 发送回复 / 拉黑 ----------
    /// 返回新评论 rpid（拿不到返回 None）。-111/-101 属致命错误用 Err 上抛。
    pub async fn send_reply(&self, oid: i64, rpid: i64, content_type: i64, reply_text: &str, root_rpid: Option<i64>) -> Result<Option<i64>> {
        let url = "https://api.bilibili.com/x/v2/reply/add";
        let root = root_rpid.unwrap_or(rpid);
        let bili_jct = self.config.read().unwrap().get_str("BILI_JCT");
        let form = [
            ("oid", oid.to_string()),
            ("type", content_type.to_string()),
            ("root", root.to_string()),
            ("parent", rpid.to_string()),
            ("message", reply_text.to_string()),
            ("csrf", bili_jct),
        ];
        let mut headers = self.headers("https://www.bilibili.com/");
        headers.insert("Content-Type", reqwest::header::HeaderValue::from_static("application/x-www-form-urlencoded"));
        let resp = self.http.post(url).headers(headers).form(&form).send().await?;
        let payload: Value = resp.json().await?;
        let code = payload.get("code").and_then(|c| c.as_i64()).unwrap_or(-1);
        if code != 0 {
            let msg = payload.get("message").and_then(|m| m.as_str()).unwrap_or("").to_string();
            if code == -111 {
                return Err(AppError::Other("bili_jct 错误！请检查 Cookie 后重启".into()));
            }
            if code == -101 {
                return Err(AppError::Other("未登录！SESSDATA 失效，请更新 Cookie 后重启".into()));
            }
            tracing::warn!("发送回复失败: code={code} msg={msg}");
            return Ok(None);
        }
        Ok(payload.get("data").and_then(|d| d.get("rpid")).and_then(|v| v.as_i64()))
    }

    pub async fn block_user(&self, mid: i64) {
        let url = "https://api.bilibili.com/x/relation/modify";
        let bili_jct = self.config.read().unwrap().get_str("BILI_JCT");
        let form = [
            ("fid", mid.to_string()),
            ("act", "5".into()),
            ("reasons", "2".into()),
            ("csrf", bili_jct),
        ];
        let mut headers = self.headers("https://space.bilibili.com/");
        headers.insert("Content-Type", reqwest::header::HeaderValue::from_static("application/x-www-form-urlencoded"));
        if let Ok(resp) = self.http.post(url).headers(headers).form(&form).send().await {
            let _ = resp.json::<Value>().await;
        }
    }

    // ---------- 视频信息 ----------
    /// 按 aid(oid) 查询视频信息（与 Python get_video_info 一致）。
    pub async fn get_video_info(&self, oid: i64) -> Option<Value> {
        let url = "https://api.bilibili.com/x/web-interface/view";
        let params = vec![("aid".into(), oid.to_string())];
        let payload = self.get_json(&url, &params, "https://www.bilibili.com/").await.ok()?;
        if payload.get("code").and_then(|c| c.as_i64()) != Some(0) {
            return None;
        }
        let v = payload["data"].clone();
        Some(json!({
            "bvid": v.get("bvid").and_then(|x| x.as_str()).unwrap_or(""),
            "title": v.get("title").and_then(|x| x.as_str()).unwrap_or(""),
            "desc": v.get("desc").and_then(|x| x.as_str()).unwrap_or(""),
            "owner_name": v["owner"]["name"].as_str().unwrap_or(""),
            "owner_mid": v["owner"]["mid"].as_i64().map(|m| m.to_string()).unwrap_or_default(),
            "tname": v.get("tname").and_then(|x| x.as_str()).unwrap_or(""),
            "duration": v.get("duration").and_then(|x| x.as_i64()).unwrap_or(0),
            "pic": v.get("pic").and_then(|x| x.as_str()).unwrap_or(""),
        }))
    }

    /// 评论配图：拉取评论详情里的图片列表。
    pub async fn get_comment_images(&self, oid: i64, rpid: i64, content_type: i64) -> Vec<String> {
        let url = "https://api.bilibili.com/x/v2/reply/detail";
        let params = vec![
            ("oid".into(), oid.to_string()),
            ("type".into(), content_type.to_string()),
            ("root".into(), rpid.to_string()),
        ];
        let payload = match self.get_json(&url, &params, "https://www.bilibili.com/").await {
            Ok(v) => v,
            Err(_) => return vec![],
        };
        let pictures = payload["data"]["root"]["content"]["pictures"].as_array().cloned().unwrap_or_default();
        pictures
            .iter()
            .filter_map(|p| p.get("img_src").and_then(|v| v.as_str()).map(|s| s.to_string()))
            .collect()
    }

    // ---------- 通用 GET ----------
    pub async fn get_json(&self, url: &str, params: &[(String, String)], referer: &str) -> Result<Value> {
        let resp = self
            .http
            .get(url)
            .headers(self.headers(referer))
            .query(params)
            .send()
            .await?;
        let payload: Value = resp.json().await?;
        let code = payload.get("code").and_then(|c| c.as_i64()).unwrap_or(-1);
        if code != 0 {
            return Err(AppError::Api {
                code,
                msg: payload.get("message").and_then(|m| m.as_str()).unwrap_or("").to_string(),
            });
        }
        Ok(payload)
    }

    /// 生成带签名参数的 GET（wbi 需要的场景用）。
    #[allow(dead_code)]
    pub async fn get_json_signed(&self, url: &str, params: &mut Vec<(String, String)>, referer: &str) -> Result<Value> {
        self.sign_wbi_params(params).await;
        self.get_json(url, params, referer).await
    }

    /// 下载图片为 base64 data URL（供视觉模型用）。
    pub async fn download_image_b64(&self, url: &str) -> Option<String> {
        let resp = self.http.get(url).header("Referer", "https://www.bilibili.com").send().await.ok()?;
        if !resp.status().is_success() {
            return None;
        }
        let bytes = resp.bytes().await.ok()?;
        Some(format!("data:image/jpeg;base64,{}", crate::util::b64_encode(&bytes)))
    }
}

// ============ 文本处理 ============

/// 按 at_details 剥离 @昵称（昵称按长度倒序替换，避免短昵称切碎长昵称）。
pub fn strip_at_mentions(text: &str, at_details: &[Value]) -> String {
    let mut nicks: Vec<String> = at_details
        .iter()
        .filter_map(|u| u.get("nickname").and_then(|v| v.as_str()).map(|s| s.trim().to_string()))
        .filter(|s| !s.is_empty())
        .collect();
    nicks.sort_by_key(|n| std::cmp::Reverse(n.chars().count()));
    let mut out = text.to_string();
    for nick in &nicks {
        out = out.replace(&format!("@{nick}"), " ");
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// 去掉 B站 自动加的「回复 @昵称 :」前缀。
pub fn strip_reply_prefix(text: &str) -> String {
    let re = regex::Regex::new(r"^\s*回复\s*(?:@[^:：]{0,80})?\s*[:：]\s*").unwrap();
    re.replace(text, "").into_owned()
}

/// 是否命中屏蔽关键词。
pub fn is_blocked(text: &str) -> bool {
    BLOCK_KEYWORDS.iter().any(|kw| text.contains(kw))
}

// ============ av/bv 转换 ============

#[allow(dead_code)]
const BV_TABLE: &str = "fZodR9XQDSUm21yCkr6zBqiveYah8bt4xsWpHnJE7jL5VG3guMTKNPAwcF";
#[allow(dead_code)]
const BV_XOR: i64 = 177451812;
#[allow(dead_code)]
const BV_ADD: i64 = 8728348608;

/// oid(aid) → bvid；失败返回空串。
#[allow(dead_code)]
pub fn oid_to_bvid(oid: i64) -> String {
    let mut x = (oid ^ BV_XOR) + BV_ADD;
    let mut s = vec!['1'; 12];
    s[0] = 'B';
    s[1] = 'V';
    s[3] = '4';
    s[9] = 'a';
    let pos: [usize; 6] = [1, 2, 4, 5, 6, 8];
    let idx: [usize; 6] = [10, 7, 3, 6, 2, 9];
    for i in 0..6 {
        let r = (x % 58) as usize;
        x /= 58;
        s[pos[idx[i]]] = BV_TABLE.chars().nth(r).unwrap_or('1');
    }
    s.iter().collect()
}

/// 按场景限流（TPM 滑窗）。需要外部传入共享计数器。
#[derive(Default)]
pub struct TpmWindow {
    /// minute -> tokens
    pub buckets: RwLock<std::collections::HashMap<i64, i64>>,
}

impl TpmWindow {
    pub fn new() -> Self {
        Self::default()
    }
    /// 记录消耗；若当前分钟超出 tpm 上限则返回需要等待的秒数。
    pub fn record(&self, tokens: i64, tpm: i64) -> u64 {
        if tpm <= 0 {
            return 0;
        }
        let minute = chrono::Utc::now().timestamp() / 60;
        let mut b = self.buckets.write().unwrap();
        b.retain(|&k, _| k >= minute - 1);
        let used = *b.get(&minute).unwrap_or(&0);
        if used + tokens > tpm {
            let wait = 60 - (chrono::Utc::now().timestamp() % 60);
            return wait.max(1) as u64;
        }
        *b.entry(minute).or_insert(0) += tokens;
        0
    }
}

/// 构造对话消息（用户消息带图片时）。
#[allow(dead_code)]
fn content_with_images(text: &str, images: Vec<(String, String)>) -> Value {
    let mut content = Vec::new();
    for (name, data_url) in &images {
        let _ = name;
        content.push(json!({"type": "image_url", "image_url": {"url": data_url}}));
    }
    if !text.is_empty() {
        content.push(json!({"type": "text", "text": text}));
    }
    if content.is_empty() {
        content.push(json!({"type": "text", "text": ""}));
    }
    Value::Array(content)
}

pub fn urlencode(s: &str) -> String {
    percent_encoding::utf8_percent_encode(s, percent_encoding::NON_ALPHANUMERIC).to_string()
}

// 占位：避免未使用警告
#[allow(dead_code)]
fn _unused(_: &Value) -> Value {
    json!({})
}
