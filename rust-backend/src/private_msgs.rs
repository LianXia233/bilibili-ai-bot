//! B站私信：安全判定（危险链接/色情引流）+ 轮询客户端 + 发送。
//! 与 private_messages.py 对齐：仅处理个人会话纯文本与视频分享；危险链接只解析文本，不访问目标网址。

use crate::config::Config;
use crate::error::{AppError, Result};
use serde_json::{json, Value};
use std::net::IpAddr;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::{Arc, RwLock};
use unicode_normalization::UnicodeNormalization;
use url::Url;

const SESSIONS_URL: &str = "https://api.vc.bilibili.com/session_svr/v1/session_svr/get_sessions";
const MESSAGES_URL: &str = "https://api.vc.bilibili.com/svr_sync/v1/svr_sync/fetch_session_msgs";
const SEND_URL: &str = "https://api.vc.bilibili.com/web_im/v1/web_im/send_msg";

const STRONG_ADULT_MARKERS: [&str; 13] = [
    "裸聊", "约炮", "援交", "卖片", "色情网站", "黄色网站", "成人网站", "成人视频",
    "无码视频", "看片地址", "看片链接", "未成年", "私密视频",
];
const LINKED_ADULT_MARKERS: [&str; 11] = [
    "色情", "黄色", "成人", "福利姬", "福利群", "资源群", "私密视频", "色图", "涩图", "裸照", "看片",
];
const ADULT_DOMAIN_MARKERS: [&str; 10] = [
    "porn", "sex", "xxx", "hentai", "jav", "xvideo", "onlyfans", "91porn", "麻豆", "av",
];

/// 私信消息去重表保留上限（2026-09-15 从 1000 提升：多会话滚动时旧 key 会被挤出窗口，
/// 导致旧消息被当新消息重发；3000 与 Python PROCESSED_KEYS_LIMIT 对齐）。
const PROCESSED_KEYS_LIMIT: usize = 3000;

#[derive(Debug, Clone, Default)]
pub struct SafetyDecision {
    pub should_block: bool,
    pub reason: String,
    #[allow(dead_code)]
    pub urls: Vec<String>,
}

/// 归一化文本用于检测（NFKC、去零宽字符、hxxps 复原、全角点归一、汉字「点」→.）。
fn normalize_for_detection(text: &str) -> String {
    let mut value: String = text.nfkc().collect();
    value = value
        .chars()
        .filter(|c| !matches!(c, '\u{200b}'..='\u{200f}' | '\u{2060}' | '\u{feff}'))
        .collect();
    // hxxps:// → https://
    let re_hxxp = regex::Regex::new(r"(?i)\bhxxps?://").unwrap();
    value = re_hxxp
        .replace_all(&value, |caps: &regex::Captures| {
            if caps[0].to_lowercase().contains('s') {
                "https://".to_string()
            } else {
                "http://".to_string()
            }
        })
        .into_owned();
    // [.] → .
    let re_bracket = regex::Regex::new(r"[\[(\{]\s*\.\s*[\])\}]").unwrap();
    value = re_bracket.replace_all(&value, ".").into_owned();
    value = value.replace('。', ".").replace('．', ".").replace('｡', ".");
    // 汉字「点」在两段 ASCII 字母数字之间 → .
    value = replace_dian_as_dot(&value);
    value
}

/// 手工实现 Python 的 (?<=[A-Za-z0-9])点(?=[A-Za-z]{2,12}\b)（regex crate 不支持 look-around）。
fn replace_dian_as_dot(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let is_alnum = |c: char| c.is_ascii_alphanumeric();
    let n = chars.len();
    for (i, &c) in chars.iter().enumerate() {
        if c == '点' && i > 0 && i + 1 < n {
            let prev_ok = is_alnum(chars[i - 1]);
            let mut j = i + 1;
            let mut cnt = 0usize;
            while j < n && cnt < 13 && chars[j].is_ascii_alphabetic() {
                cnt += 1;
                j += 1;
            }
            let next_ok = cnt >= 2 && cnt <= 12 && (j >= n || !chars[j].is_alphanumeric());
            if prev_ok && next_ok {
                out.push('.');
                continue;
            }
        }
        out.push(c);
    }
    out
}

fn url_regex() -> regex::Regex {
    regex::Regex::new(
        r#"(?i)(?:https?|hxxps?)://[^\s<>"'，。！？、]+|www\.[^\s<>"'，。！？、]+|(?:[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?\.)+(?:com|net|org|cn|tv|cc|me|xyz|top|vip|site|link|app|io|info|live)(?:/[^\s<>"'，。！？、]*)?"#,
    )
    .unwrap()
}

pub fn extract_urls(text: &str) -> Vec<String> {
    let normalized = normalize_for_detection(text);
    let re = url_regex();
    let mut urls: Vec<String> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for m in re.find_iter(&normalized) {
        let mut candidate: String = m.as_str().trim_end_matches(|c: char| ".,;:!?)]}".contains(c)).to_string();
        let lower = candidate.to_lowercase();
        if lower.starts_with("www.") {
            candidate = format!("https://{candidate}");
        } else if !candidate.contains("://") {
            candidate = format!("https://{candidate}");
        }
        if !seen.contains(&candidate) {
            seen.insert(candidate.clone());
            urls.push(candidate);
        }
    }
    urls
}

fn hostname(url: &str) -> String {
    match Url::parse(url) {
        Ok(u) => u.host_str().unwrap_or("").trim_start_matches('.').to_lowercase(),
        Err(_) => String::new(),
    }
}

fn is_trusted_host(host: &str, trusted_domains: &[String]) -> bool {
    for item in trusted_domains {
        let trusted = item.trim().trim_start_matches('.').to_lowercase();
        if !trusted.is_empty() && (host == trusted || host.ends_with(&format!(".{trusted}"))) {
            return true;
        }
    }
    false
}

/// 私信安全判定：是否应因危险链接或色情引流直接拉黑。
pub fn assess_private_message(text: &str, trusted_domains: Option<&[String]>) -> SafetyDecision {
    let normalized = normalize_for_detection(text);
    let urls = extract_urls(&normalized);
    let trusted: Vec<String> = match trusted_domains {
        Some(t) if !t.is_empty() => t.to_vec(),
        _ => vec!["bilibili.com".to_string(), "b23.tv".to_string()],
    };

    if STRONG_ADULT_MARKERS.iter().any(|k| normalized.contains(k)) {
        return SafetyDecision {
            should_block: true,
            reason: "疑似色情或成人引流内容".into(),
            urls,
        };
    }

    for url in &urls {
        let host = hostname(url);
        if host.is_empty() {
            return SafetyDecision { should_block: true, reason: "无法识别目标域名的链接".into(), urls: urls.clone() };
        }
        if IpAddr::from_str(&host).is_ok() {
            return SafetyDecision { should_block: true, reason: format!("不可信 IP 链接：{host}"), urls: urls.clone() };
        }
        if ADULT_DOMAIN_MARKERS.iter().any(|m| host.contains(m)) {
            return SafetyDecision { should_block: true, reason: format!("疑似色情域名：{host}"), urls: urls.clone() };
        }
        if !is_trusted_host(&host, &trusted) {
            return SafetyDecision { should_block: true, reason: format!("未信任的外部链接：{host}"), urls: urls.clone() };
        }
    }

    if !urls.is_empty() && LINKED_ADULT_MARKERS.iter().any(|k| normalized.contains(k)) {
        return SafetyDecision { should_block: true, reason: "链接伴随疑似色情引流内容".into(), urls };
    }
    SafetyDecision { should_block: false, urls, ..Default::default() }
}

pub fn is_protected_sender(mid: &str, config: &Config) -> bool {
    let uid = mid.trim();
    if uid.is_empty() {
        return false;
    }
    let mut protected: std::collections::HashSet<String> = std::collections::HashSet::new();
    protected.insert(config.get_str("OWNER_MID"));
    protected.insert(config.get_str("DEDE_USER_ID"));
    for item in config.get_str_list("PRIVATE_MESSAGE_BLOCK_WHITELIST_UIDS") {
        protected.insert(item);
    }
    protected.remove("");
    protected.contains(uid)
}

pub fn reply_scope_allows(mid: &str, config: &Config) -> bool {
    let uid = mid.trim();
    let scope = config.get_str("PRIVATE_MESSAGE_REPLY_SCOPE").to_lowercase();
    if scope == "all" {
        return true;
    }
    let owner = config.get_str("OWNER_MID");
    if scope == "owner" {
        return !uid.is_empty() && uid == owner;
    }
    if scope == "whitelist" {
        if !uid.is_empty() && uid == owner {
            return true;
        }
        return config.get_str_list("PRIVATE_MESSAGE_REPLY_WHITELIST_UIDS").iter().any(|w| w == uid);
    }
    false
}

// ============ 私信客户端 ============

pub struct PrivateMessageClient {
    pub http: reqwest::Client,
    pub config: Arc<RwLock<Config>>,
    pub state_file: PathBuf,
}

impl PrivateMessageClient {
    pub fn new(config: Arc<RwLock<Config>>, base_dir: &str) -> Self {
        let http = reqwest::Client::builder().cookie_store(true).build().expect("私信 client 构建失败");
        PrivateMessageClient {
            http,
            config,
            state_file: crate::util::data_path(base_dir, "private_message_state.json"),
        }
    }

    fn headers(&self) -> reqwest::header::HeaderMap {
        let cfg = self.config.read().unwrap().clone();
        let mut h = reqwest::header::HeaderMap::new();
        h.insert("User-Agent", reqwest::header::HeaderValue::from_static(crate::bili_api::UA));
        h.insert("Referer", reqwest::header::HeaderValue::from_static("https://message.bilibili.com/"));
        h.insert("Origin", reqwest::header::HeaderValue::from_static("https://message.bilibili.com"));
        let cookie = format!(
            "SESSDATA={}; bili_jct={}; DedeUserID={}",
            cfg.get_str("SESSDATA"),
            cfg.get_str("BILI_JCT"),
            cfg.get_str("DEDE_USER_ID")
        );
        if let Ok(c) = reqwest::header::HeaderValue::from_str(&cookie) {
            h.insert("Cookie", c);
        }
        h
    }

    fn load_state(&self) -> Value {
        let mut state: Value = crate::util::load_json(&self.state_file, json!({}));
        if state.get("device_id").and_then(|v| v.as_str()).unwrap_or("").is_empty() {
            state["device_id"] = json!(crate::util::gen_token().to_uppercase());
            state["initialized"] = json!(false);
            state["sessions"] = json!({});
            state["processed_keys"] = json!([]);
        }
        state
    }

    fn save_state(&self, state: &Value) -> Result<()> {
        crate::util::save_json(&self.state_file, state)
    }

    async fn get_sessions(&self) -> Result<Vec<Value>> {
        let payload = self
            .http
            .get(SESSIONS_URL)
            .headers(self.headers())
            .query(&[
                ("session_type", "1"),
                ("group_fold", "1"),
                ("unfollow_fold", "0"),
                ("sort_rule", "2"),
                ("size", "100"),
                ("build", "0"),
                ("mobi_app", "web"),
            ])
            .send()
            .await?;
        let data: Value = payload.json().await?;
        if data.get("code").and_then(|c| c.as_i64()) != Some(0) {
            return Err(AppError::Api {
                code: data.get("code").and_then(|c| c.as_i64()).unwrap_or(-1),
                msg: data.get("message").and_then(|m| m.as_str()).unwrap_or("获取私信会话失败").to_string(),
            });
        }
        Ok(data["data"]["session_list"].as_array().cloned().unwrap_or_default())
    }

    async fn fetch_messages(&self, talker_id: i64, session_type: i64, begin_seqno: i64) -> Result<Value> {
        let payload = self
            .http
            .get(MESSAGES_URL)
            .headers(self.headers())
            .query(&[
                ("talker_id", talker_id.to_string()),
                ("session_type", session_type.to_string()),
                ("begin_seqno", begin_seqno.to_string()),
                ("size", "20".to_string()),
                ("sender_device_id", "1".to_string()),
                ("build", "0".to_string()),
                ("mobi_app", "web".to_string()),
            ])
            .send()
            .await?;
        let data: Value = payload.json().await?;
        if data.get("code").and_then(|c| c.as_i64()) != Some(0) {
            return Err(AppError::Api {
                code: data.get("code").and_then(|c| c.as_i64()).unwrap_or(-1),
                msg: data.get("message").and_then(|m| m.as_str()).unwrap_or("获取私信内容失败").to_string(),
            });
        }
        Ok(data.get("data").cloned().unwrap_or(json!({})))
    }

    /// 消息内容解析：纯文本 / 视频分享卡片。
    fn message_content(raw: &Value, msg_type: i64) -> (String, String) {
        let json_content = |v: &Value| -> String {
            if let Some(obj) = v.as_object() {
                return obj
                    .get("content")
                    .or_else(|| obj.get("text"))
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();
            }
            if let Some(s) = v.as_str() {
                if let Ok(parsed) = serde_json::from_str::<Value>(s) {
                    if let Some(obj) = parsed.as_object() {
                        return obj
                            .get("content")
                            .or_else(|| obj.get("text"))
                            .and_then(|x| x.as_str())
                            .unwrap_or("")
                            .trim()
                            .to_string();
                    }
                }
                return s.trim().to_string();
            }
            String::new()
        };
        match msg_type {
            1 => (json_content(raw), "text".into()),
            7 => {
                let parsed = if raw.is_object() {
                    raw.clone()
                } else if let Some(s) = raw.as_str() {
                    serde_json::from_str::<Value>(s).unwrap_or_else(|_| json!({}))
                } else {
                    json!({})
                };
                let bvid = parsed.get("bvid").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
                let aid = parsed.get("id").and_then(|v| v.as_str())
                    .or_else(|| parsed.get("aid").and_then(|v| v.as_str()))
                    .unwrap_or("")
                    .trim()
                    .to_string();
                let title = parsed.get("title").and_then(|v| v.as_str())
                    .or_else(|| parsed.get("headline").and_then(|v| v.as_str()))
                    .or_else(|| parsed.get("name").and_then(|v| v.as_str()))
                    .unwrap_or("")
                    .trim()
                    .to_string();
                let bvid_ok = bvid.len() == 12 && bvid.starts_with("BV");
                let video_url = if bvid_ok {
                    format!("https://www.bilibili.com/video/{bvid}")
                } else if !aid.is_empty() && aid.chars().all(|c| c.is_ascii_digit()) {
                    format!("https://www.bilibili.com/video/av{aid}")
                } else {
                    return (String::new(), String::new());
                };
                let prefix = if title.is_empty() {
                    "[B站视频分享]".to_string()
                } else {
                    format!("[B站视频分享] {title}")
                };
                (format!("{prefix}\n{video_url}"), "video_share".into())
            }
            _ => (String::new(), String::new()),
        }
    }

    /// 轮询新入站私信；首次启用只建游标不处理历史。
    pub async fn poll(&self) -> Vec<Value> {
        let cfg = self.config.read().unwrap().clone();
        let self_uid = cfg.get_str("DEDE_USER_ID");
        let sessions = match self.get_sessions().await {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("私信会话拉取失败: {e}");
                return vec![];
            }
        };
        let mut state = self.load_state();
        let previous_account = state.get("account_uid").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let account_changed = !previous_account.is_empty() && previous_account != self_uid;
        if previous_account != self_uid {
            state = json!({
                "initialized": false,
                "initialized_at": crate::util::now_unix(),
                "account_uid": self_uid,
                "device_id": crate::util::gen_token().to_uppercase(),
                "sessions": {},
                "processed_keys": []
            });
        }

        let mut session_state = state.get("sessions").cloned().unwrap_or_else(|| json!({}));
        let mut processed: Vec<String> = state.get("processed_keys").and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect()).unwrap_or_default();
        let mut processed_set: std::collections::HashSet<String> = processed.iter().cloned().collect();

        let initialized = state.get("initialized").and_then(|v| v.as_bool()).unwrap_or(false);
        if !initialized {
            if let Some(obj) = session_state.as_object_mut() {
                for session in &sessions {
                    let talker_id = session.get("talker_id").and_then(|v| v.as_i64()).unwrap_or(0);
                    let session_type = session.get("session_type").and_then(|v| v.as_i64()).unwrap_or(1);
                    if talker_id != 0 {
                        obj.insert(format!("{session_type}:{talker_id}"), json!(session.get("max_seqno").and_then(|v| v.as_i64()).unwrap_or(0)));
                    }
                }
            }
            state["initialized"] = json!(true);
            state["initialized_at"] = json!(crate::util::now_unix());
            state["sessions"] = session_state;
            let _ = self.save_state(&state);
            let reason = if account_changed { "账号已切换，已重置" } else { "首次启用" };
            tracing::info!("[私信] 监听初始化完成（{reason}）：已跳过现有历史消息");
            return vec![];
        }

        let max_age = cfg.get_i64("PRIVATE_MESSAGE_MAX_MESSAGE_AGE").max(60);
        let now = crate::util::now_unix();
        let message_limit = cfg.get_i64("PRIVATE_MESSAGE_MAX_PER_POLL").clamp(1, 20);
        let mut new_messages: Vec<Value> = Vec::new();

        for session in &sessions {
            if (new_messages.len() as i64) >= message_limit {
                break;
            }
            let talker_id = session.get("talker_id").and_then(|v| v.as_i64()).unwrap_or(0);
            let session_type = session.get("session_type").and_then(|v| v.as_i64()).unwrap_or(1);
            if talker_id == 0 || session_type != 1 {
                continue;
            }
            let key = format!("{session_type}:{talker_id}");
            let last_seqno = session_state.get(&key).and_then(|v| v.as_i64()).unwrap_or(0);
            let remote_max = session.get("max_seqno").and_then(|v| v.as_i64()).unwrap_or(0);
            if last_seqno != 0 && remote_max != 0 && remote_max <= last_seqno {
                continue;
            }
            let payload = match self.fetch_messages(talker_id, session_type, last_seqno).await {
                Ok(p) => p,
                Err(e) => {
                    tracing::warn!("[私信] 会话 {talker_id} 拉取失败：{e}");
                    continue;
                }
            };
            let messages = payload.get("messages").and_then(|v| v.as_array()).cloned().unwrap_or_default();
            let payload_max = payload.get("max_seqno").and_then(|v| v.as_i64()).unwrap_or(remote_max.max(last_seqno));

            for message in messages.iter().rev() {
                // msg_key / msg_seqno 可能是字符串或数字（B站私信 payload 为数字）
                let msg_key = message
                    .get("msg_key")
                    .and_then(|v| v.as_str().map(|s| s.to_string()))
                    .or_else(|| message.get("msg_key").and_then(|v| v.as_i64()).map(|n| n.to_string()))
                    .or_else(|| message.get("msg_seqno").and_then(|v| v.as_str().map(|s| s.to_string())))
                    .or_else(|| message.get("msg_seqno").and_then(|v| v.as_i64()).map(|n| n.to_string()))
                    .unwrap_or_default();
                let msg_seqno = message.get("msg_seqno").and_then(|v| v.as_i64()).unwrap_or(0);
                // sender_uid 可能是字符串或数字（与 Python str(message.get("sender_uid")) 对齐），
                // 否则数字型 sender_uid 会解析为空串，导致「自己发给自己的消息」无法被识别而自我回复
                let sender_uid = message
                    .get("sender_uid")
                    .and_then(|v| v.as_str().map(|s| s.to_string()))
                    .or_else(|| message.get("sender_uid").and_then(|v| v.as_i64()).map(|n| n.to_string()))
                    .unwrap_or_default();
                let msg_type = message.get("msg_type").and_then(|v| v.as_i64()).unwrap_or(0);
                let mut timestamp = message.get("timestamp").and_then(|v| v.as_i64()).unwrap_or(now);
                if timestamp > 10_000_000_000 {
                    timestamp /= 1000;
                }
                let skip = msg_key.is_empty()
                    || processed_set.contains(&msg_key)
                    || sender_uid == self_uid
                    || !(msg_type == 1 || msg_type == 7)
                    || (last_seqno != 0 && msg_seqno != 0 && msg_seqno <= last_seqno)
                    || now - timestamp > max_age;
                if skip {
                    continue;
                }
                let (content, content_type) = Self::message_content(&message["content"], msg_type);
                if content.is_empty() {
                    continue;
                }
                let account = session.get("account_info").cloned().unwrap_or(json!({}));
                let username = account.get("name").and_then(|v| v.as_str())
                    .or_else(|| account.get("uname").and_then(|v| v.as_str()))
                    .unwrap_or(&format!("UID {talker_id}"))
                    .to_string();
                new_messages.push(json!({
                    "msg_key": msg_key,
                    "msg_seqno": msg_seqno,
                    "talker_id": talker_id,
                    "session_type": session_type,
                    "sender_uid": if sender_uid.is_empty() { talker_id.to_string() } else { sender_uid },
                    "username": username,
                    "content": content,
                    "content_type": content_type,
                    "timestamp": timestamp,
                }));
                processed.push(msg_key.clone());
                processed_set.insert(msg_key);
                if (new_messages.len() as i64) >= message_limit {
                    break;
                }
            }

            // 游标无分支推进到远端最大值（与 Python 2026-09-15 修复对齐）：
            // 不再区分 reached_limit，一律推进，避免「最后取出的那一条」卡住后续消费；
            // max(last_seqno) 保证单调递增，远端 max_seqno 回退时不会重开已消费区间。
            let max_seq = messages.iter().filter_map(|m| m.get("msg_seqno").and_then(|v| v.as_i64())).fold(0i64, i64::max);
            let observed_max = last_seqno.max(remote_max).max(payload_max).max(max_seq);
            if let Some(obj) = session_state.as_object_mut() {
                obj.insert(key, json!(observed_max));
            }
        }

        if processed.len() > PROCESSED_KEYS_LIMIT {
            processed.drain(..processed.len() - PROCESSED_KEYS_LIMIT);
        }
        state["sessions"] = session_state;
        state["processed_keys"] = json!(processed);
        let _ = self.save_state(&state);
        new_messages
    }

    /// 发送纯文本私信。写操作不重试，避免重复发送。
    pub async fn send_text(&self, receiver_id: &str, text: &str) -> bool {
        let cfg = self.config.read().unwrap().clone();
        let sender_uid = cfg.get_str("DEDE_USER_ID");
        let csrf = cfg.get_str("BILI_JCT");
        let content = text.trim().to_string();
        if sender_uid.chars().all(|c| c.is_ascii_digit()) == false
            || receiver_id.chars().all(|c| c.is_ascii_digit()) == false
            || csrf.is_empty()
            || content.is_empty()
        {
            return false;
        }
        let state = self.load_state();
        let device_id = state.get("device_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
        if device_id.is_empty() {
            return false;
        }
        let now_ms = crate::util::now_unix() * 1000;
        let content_json = serde_json::to_string(&json!({"content": content})).unwrap_or_default();
        let form = [
            ("msg[sender_uid]", sender_uid),
            ("msg[receiver_id]", receiver_id.to_string()),
            ("msg[receiver_type]", "1".to_string()),
            ("msg[msg_type]", "1".to_string()),
            ("msg[msg_status]", "0".to_string()),
            ("msg[dev_id]", device_id),
            ("msg[timestamp]", now_ms.to_string()),
            ("msg[content]", content_json),
            ("msg[new_face_version]", "0".into()),
            ("from_firework", "0".into()),
            ("build", "0".into()),
            ("mobi_app", "web".into()),
            ("csrf_token", csrf.clone()),
            ("csrf", csrf),
        ];
        match self.http.post(SEND_URL).headers(self.headers()).form(&form).send().await {
            Ok(resp) => match resp.json::<Value>().await {
                Ok(result) => {
                    if result.get("code").and_then(|c| c.as_i64()) == Some(0) {
                        true
                    } else {
                        tracing::warn!("[私信] 发送失败 UID {receiver_id}: code={} {}", result.get("code").and_then(|c| c.as_i64()).unwrap_or(-1), result.get("message").and_then(|m| m.as_str()).unwrap_or(""));
                        false
                    }
                }
                Err(e) => {
                    tracing::warn!("[私信] 发送异常 UID {receiver_id}: {e}");
                    false
                }
            },
            Err(e) => {
                tracing::warn!("[私信] 发送异常 UID {receiver_id}: {e}");
                false
            }
        }
    }
}
