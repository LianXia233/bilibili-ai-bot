//! Axum Web 管理面板：登录认证（RSA 口令密封 + 签名 Cookie 会话）、
//! 聊天、记忆、配置、成本、人格、黑名单等核心 /api/* 路由。
//! 与 local-chat.py 的 API 契约对齐，前端 chat.html 无需改动。

use crate::bili_api::BiliClient;
use crate::error::Result;
use crate::bili_login::BiliQrLoginManager;
use crate::config::Config;
use crate::llm::{log_cost, LlmClient};
use crate::memory::{MemoryStore, PermanentMemory};
use crate::personality::{PersonaStore, Personality};
use crate::util::{b64_decode, hmac_sha256_hex, load_json, now_str, save_json};
use axum::body::Body;
use axum::extract::{Multipart, Path as AxumPath, State};
use axum::http::{header, HeaderValue, Request, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::Engine;
use rsa::pkcs8::{DecodePrivateKey, EncodePrivateKey};
use rsa::pkcs8::EncodePublicKey;
use rsa::{Oaep, RsaPrivateKey};
use serde_json::{json, Value};
use sha2::Sha256;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

const DEFAULT_AUTH_PASSWORD: &str = "admin()";
const COOKIE_NAME: &str = "rs_session";

pub struct WebCtx {
    pub base_dir: String,
    pub config: Arc<RwLock<Config>>,
    pub bili: Arc<BiliClient>,
    pub llm: Arc<LlmClient>,
    pub memory: Arc<MemoryStore>,
    pub personality: Arc<Personality>,
    pub qr: Arc<BiliQrLoginManager>,
    pub permanent: PermanentMemory,
    pub secret_key: String,
    pub seal: RwLock<Option<RsaPrivateKey>>,
}

impl WebCtx {
    pub fn path(&self, name: &str) -> PathBuf {
        crate::util::data_path(&self.base_dir, name)
    }

    fn auth_password(&self) -> String {
        let env = std::env::var("CHAT_PASSWORD").unwrap_or_default();
        if !env.is_empty() {
            return env;
        }
        let cfg = self.config.read().unwrap().clone();
        let saved = cfg.get_str("CHAT_PASSWORD");
        if !saved.is_empty() {
            saved
        } else {
            DEFAULT_AUTH_PASSWORD.to_string()
        }
    }
}

// ---------- 会话 Cookie ----------
fn session_cookie(ctx: &WebCtx, authed: bool) -> String {
    let payload = format!("authed={authed}&ts={}", crate::util::now_unix());
    let b64 = base64::engine::general_purpose::STANDARD.encode(payload.as_bytes());
    let sig = hmac_sha256_hex(ctx.secret_key.as_bytes(), b64.as_bytes());
    format!("{b64}.{sig}")
}

fn verify_session(ctx: &WebCtx, cookie: Option<&str>) -> bool {
    let Some(cookie) = cookie else { return false };
    let Some((b64, sig)) = cookie.rsplit_once('.') else { return false };
    let expect = hmac_sha256_hex(ctx.secret_key.as_bytes(), b64.as_bytes());
    // 常量时间比较，避免通过时序差异探测签名
    if !constant_time_eq(sig, &expect) {
        return false;
    }
    if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(b64) {
        if let Ok(payload) = String::from_utf8(bytes) {
            let authed = payload.split('&').any(|kv| kv == "authed=true");
            let ts: i64 = payload
                .split('&')
                .filter_map(|kv| kv.strip_prefix("ts="))
                .next()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            let fresh = crate::util::now_unix() - ts < 30 * 24 * 3600;
            return authed && fresh;
        }
    }
    false
}

fn cookie_from_headers(headers: &axum::http::HeaderMap) -> Option<String> {
    headers
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(|all| {
            all.split(';')
                .find_map(|kv| kv.trim().strip_prefix(&format!("{COOKIE_NAME}=")))
                .map(|s| s.to_string())
        })
}

/// 常量时间字符串比较（防止签名校验的时序侧信道）。
fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.bytes().zip(b.bytes()) {
        diff |= x ^ y;
    }
    diff == 0
}

async fn auth_middleware(
    State(ctx): State<Arc<WebCtx>>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let path = req.uri().path().to_string();
    let public = path == "/api/auth_check"
        || path == "/api/handshake"
        || path == "/api/login"
        || path == "/api/branding"
        || path == "/api/health"
        || path == "/media/bot-avatar"
        || path.starts_with("/data/images/")
        || path == "/";
    if public {
        return next.run(req).await;
    }
    let cookie = cookie_from_headers(req.headers());
    if verify_session(&ctx, cookie.as_deref()) {
        return next.run(req).await;
    }
    (StatusCode::UNAUTHORIZED, Json(json!({"error": "未登录"}))).into_response()
}

// ---------- JSON 帮助 ----------
fn json_resp(code: StatusCode, v: Value) -> Response {
    (code, Json(v)).into_response()
}

// ---------- 登录 / 认证 ----------
async fn api_login(State(ctx): State<Arc<WebCtx>>, Json(body): Json<Value>) -> Response {
    let sealed = body.get("sealed").and_then(|v| v.as_str()).unwrap_or("");
    let (pwd, channel) = if !sealed.is_empty() {
        let raw = seal_decrypt(&ctx, sealed);
        match raw {
            Some(bytes) => (String::from_utf8_lossy(&bytes).into_owned(), "sealed"),
            None => return json_resp(StatusCode::BAD_REQUEST, json!({"error": "密文无法解密，请刷新页面重试"})),
        }
    } else {
        (body.get("password").and_then(|v| v.as_str()).unwrap_or("").to_string(), "plain")
    };
    if !pwd.is_empty() && pwd == ctx.auth_password() {
        let cookie = session_cookie(&ctx, true);
        let mut resp = Json(json!({"ok": true, "channel": channel})).into_response();
        if let Ok(v) = HeaderValue::from_str(&format!(
            "{COOKIE_NAME}={cookie}; Path=/; HttpOnly; SameSite=Lax; Max-Age={}",
            30 * 24 * 3600
        )) {
            resp.headers_mut().insert(header::SET_COOKIE, v);
        }
        resp
    } else {
        json_resp(StatusCode::FORBIDDEN, json!({"error": "密码错误"}))
    }
}

async fn api_auth_check(State(ctx): State<Arc<WebCtx>>, req: Request<Body>) -> Response {
    let cookie = cookie_from_headers(req.headers());
    let authed = verify_session(&ctx, cookie.as_deref());
    let is_default = ctx.auth_password() == DEFAULT_AUTH_PASSWORD;
    Json(json!({
        "authed": authed,
        "default_password": is_default,
        "tls": false,
        "seal": ctx.seal.read().unwrap().is_some(),
    }))
    .into_response()
}

async fn api_handshake(State(ctx): State<Arc<WebCtx>>) -> Response {
    let seal = ctx.seal.read().unwrap();
    match seal.as_ref() {
        Some(key) => {
            let der = key
                .to_public_key()
                .to_public_key_der()
                .map_err(|e| e.to_string())
                .ok();
            match der {
                Some(der) => Json(json!({
                    "alg": "RSA-OAEP-256",
                    "pubkey": crate::util::b64_encode(&der.as_bytes()),
                    "usage": "对文本做 RSA-OAEP(SHA-256) 加密后 base64，作为 /api/login 的 sealed 字段",
                }))
                .into_response(),
                None => Json(json!({"alg": Value::Null, "pubkey": ""})).into_response(),
            }
        }
        None => Json(json!({"alg": Value::Null, "pubkey": ""})).into_response(),
    }
}

fn seal_decrypt(ctx: &WebCtx, token: &str) -> Option<Vec<u8>> {
    let seal = ctx.seal.read().unwrap();
    let key = seal.as_ref()?;
    let raw = b64_decode(token).ok()?;
    if raw.len() != 256 {
        return None;
    }
    key.decrypt(Oaep::new::<sha2::Sha256>(), &raw).ok()
}

async fn api_change_password(State(ctx): State<Arc<WebCtx>>, Json(body): Json<Value>) -> Response {
    let old = body.get("old").and_then(|v| v.as_str()).unwrap_or("");
    let new = body.get("new").and_then(|v| v.as_str()).unwrap_or("");
    if old != ctx.auth_password() {
        return json_resp(StatusCode::FORBIDDEN, json!({"error": "原密码错误"}));
    }
    if new.is_empty() || new.chars().count() < 3 {
        return json_resp(StatusCode::BAD_REQUEST, json!({"error": "新密码太短"}));
    }
    let _ = crate::config::update_config(&ctx.config, &json!({"CHAT_PASSWORD": new}));
    Json(json!({"ok": true})).into_response()
}

// ---------- 品牌 ----------
async fn api_branding(State(ctx): State<Arc<WebCtx>>) -> Response {
    let cfg = ctx.config.read().unwrap().clone();
    let avatar = cfg.get_str("BOT_AVATAR");
    Json(json!({
        "bot_name": cfg.get_str("BOT_NAME"),
        "bot_avatar": if avatar.starts_with("/data/images/") { "/media/bot-avatar" } else { &avatar },
        "user_avatar": cfg.get_str("USER_AVATAR"),
        "bot_welcome": cfg.get_str("BOT_WELCOME"),
        "bot_subtitle": cfg.get_str("BOT_SUBTITLE"),
    }))
    .into_response()
}

// ---------- 配置 ----------
async fn api_config(State(ctx): State<Arc<WebCtx>>) -> Response {
    let cfg = ctx.config.read().unwrap().clone();
    Json(cfg.raw).into_response()
}

async fn api_config_raw(State(ctx): State<Arc<WebCtx>>) -> Response {
    let cfg = ctx.config.read().unwrap().clone();
    Json(cfg.raw).into_response()
}

async fn api_config_update(State(ctx): State<Arc<WebCtx>>, Json(body): Json<Value>) -> Response {
    let _cfg = ctx.config.read().unwrap().clone();
    match crate::config::update_config(&ctx.config, &body) {
        Ok(_) => Json(json!({"ok": true})).into_response(),
        Err(e) => json_resp(StatusCode::BAD_REQUEST, json!({"error": e.to_string()})),
    }
}

async fn api_config_check_cookie(State(ctx): State<Arc<WebCtx>>) -> Response {
    let (valid, info) = ctx.bili.check_cookie().await;
    Json(json!({"valid": valid, "info": info})).into_response()
}

async fn api_config_refresh_cookie(State(ctx): State<Arc<WebCtx>>, Json(body): Json<Value>) -> Response {
    let _cfg = ctx.config.read().unwrap().clone();
    let patch = json!({
        "SESSDATA": body.get("SESSDATA").and_then(|v| v.as_str()).unwrap_or(""),
        "BILI_JCT": body.get("BILI_JCT").and_then(|v| v.as_str()).unwrap_or(""),
        "DEDE_USER_ID": body.get("DEDE_USER_ID").and_then(|v| v.as_str()).unwrap_or(""),
    });
    let _ = crate::config::update_config(&ctx.config, &patch);
    Json(json!({"ok": true})).into_response()
}

async fn api_qr_login_start(State(ctx): State<Arc<WebCtx>>) -> Response {
    match ctx.qr.start().await {
        Ok(v) => Json(v).into_response(),
        Err(e) => json_resp(StatusCode::BAD_REQUEST, json!({"error": e.to_string()})),
    }
}

async fn api_qr_login_poll(State(ctx): State<Arc<WebCtx>>, Json(body): Json<Value>) -> Response {
    let token = body.get("token").and_then(|v| v.as_str()).unwrap_or("");
    match ctx.qr.poll(token).await {
        Ok(mut v) => {
            if v.get("status").and_then(|s| s.as_str()) == Some("confirmed") {
                if let Some(patch) = v.get("config").cloned() {
                    let _ = crate::config::update_config(&ctx.config, &patch);
                }
                v["applied"] = json!(true);
            }
            Json(v).into_response()
        }
        Err(e) => json_resp(StatusCode::BAD_REQUEST, json!({"error": e.to_string()})),
    }
}

// ---------- 聊天 ----------
async fn api_chat(State(ctx): State<Arc<WebCtx>>, Json(body): Json<Value>) -> Response {
    let user_msg = body.get("message").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    let image_filename = body.get("image").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    let truncate_text = body.get("truncate_text").and_then(|v| v.as_str()).unwrap_or("").to_string();
    if user_msg.is_empty() && image_filename.is_empty() {
        return json_resp(StatusCode::BAD_REQUEST, json!({"error": "空消息"}));
    }

    // 编辑截断：按原始文本反查位置
    if !truncate_text.is_empty() {
        let mut history: Vec<Value> = load_json(&ctx.path("local_chat.json"), Vec::new());
        let cut_index = history.iter().rposition(|m| {
            m.get("role").and_then(|r| r.as_str()) == Some("user")
                && m.get("content").and_then(|c| c.as_str()) == Some(&truncate_text)
        });
        if let Some(i) = cut_index {
            let removed: Vec<Value> = history.split_off(i);
            save_json(&ctx.path("local_chat.json"), &history).ok();
            let removed_texts: Vec<String> = removed
                .iter()
                .filter(|m| m.get("role").and_then(|r| r.as_str()) == Some("user"))
                .filter_map(|m| m.get("content").and_then(|c| c.as_str()).map(|s| s.to_string()))
                .collect();
            let mut memory = ctx.memory.load();
            memory.retain(|m| !removed_texts.iter().any(|rt| m.text.contains(rt.as_str())));
            let _ = ctx.memory.save(&memory);
        }
    }

    let cfg = ctx.config.read().unwrap().clone();
    let history: Vec<Value> = load_json(&ctx.path("local_chat.json"), Vec::new());
    let mut llm_messages: Vec<Value> = Vec::new();
    let persona = PersonaStore::new(&ctx.base_dir).active_system_prompt(&cfg.get_str("ACTIVE_PERSONA"));
    if !persona.is_empty() {
        llm_messages.push(json!({"role": "system", "content": persona}));
    }
    let bot_name = cfg.get_str("BOT_NAME");
    llm_messages.push(json!({"role": "system", "content": format!(
        "你是{bot_name}，一个有独立人格的存在。有自己的想法和态度，不敷衍不套话，说话自然像真人聊天。"
    )}));
    for m in history.iter().rev().take(12).rev() {
        let role = m.get("role").and_then(|r| r.as_str()).unwrap_or("user");
        let content = m.get("content").and_then(|c| c.as_str()).unwrap_or("");
        llm_messages.push(json!({"role": role, "content": content}));
    }
    let mut user_entry = if image_filename.is_empty() {
        json!({"role": "user", "content": user_msg})
    } else {
        json!({"role": "user", "content": user_msg, "image": image_filename})
    };
    // 组装 LLM 消息：图片 + 文本合并为单条 content 数组（与 Python _generate_reply 对齐）；
    // 图片缺失/不可读时降级为纯文本，绝不丢掉用户文字消息。
    let mut content: Vec<Value> = Vec::new();
    if !image_filename.is_empty() {
        let img_path = ctx.path("images").join(&image_filename);
        match std::fs::read(&img_path) {
            Ok(bytes) => {
                let mime = image_mime(&image_filename);
                content.push(json!({
                    "type": "image_url",
                    "image_url": {"url": format!("data:{mime};base64,{}", crate::util::b64_encode(&bytes))}
                }));
            }
            Err(_) => {
                tracing::warn!("图片不存在或不可读，降级为纯文本消息: {image_filename}");
            }
        }
    }
    if !user_msg.is_empty() {
        content.push(json!({"type": "text", "text": user_msg}));
    }
    if content.is_empty() {
        content.push(json!({"type": "text", "text": "（发送了一张图片）"}));
    }
    llm_messages.push(json!({"role": "user", "content": Value::Array(content)}));

    let max_tokens = cfg.max_tokens_of("chat");
    match ctx.llm.complete("chat", json!(llm_messages), max_tokens).await {
        Ok(r) => {
            if r.text.is_empty() {
                return json_resp(StatusCode::BAD_GATEWAY, json!({"error": "模型返回空正文"}));
            }
            log_cost(&ctx.config, "面板对话", r.input_tokens, r.output_tokens, &r.model, &ctx.llm.cost_log);
            let now = now_str();
            let mut history: Vec<Value> = load_json(&ctx.path("local_chat.json"), Vec::new());
            user_entry["time"] = json!(now);
            history.push(user_entry);
            history.push(json!({"role": "assistant", "content": r.text, "time": now}));
            save_json(&ctx.path("local_chat.json"), &history).ok();
            let mut memory = ctx.memory.load();
            let mem_text = if user_msg.is_empty() { "（发送了一张图片）" } else { &user_msg };
            ctx.memory.save_local_memory(&mut memory, mem_text, &r.text);
            Json(json!({"reply": r.text})).into_response()
        }
        Err(e) => json_resp(StatusCode::BAD_GATEWAY, json!({"error": format!("生成失败：{e}")})),
    }
}

async fn api_chat_history(State(ctx): State<Arc<WebCtx>>) -> Response {
    let history: Vec<Value> = load_json(&ctx.path("local_chat.json"), Vec::new());
    Json(json!({"history": history})).into_response()
}

async fn api_chat_clear(State(ctx): State<Arc<WebCtx>>) -> Response {
    let _ = save_json(&ctx.path("local_chat.json"), &Vec::<Value>::new());
    let mut memory = ctx.memory.load();
    memory.retain(|m| m.thread_id != "local");
    let _ = ctx.memory.save(&memory);
    Json(json!({"ok": true})).into_response()
}

// ---------- 记忆 ----------
async fn api_memory_list(State(ctx): State<Arc<WebCtx>>) -> Response {
    let memory = ctx.memory.load();
    let list: Vec<Value> = memory
        .iter()
        .rev()
        .take(200)
        .map(|m| json!({
            "rpid": m.rpid,
            "thread_id": m.thread_id,
            "user_id": m.user_id,
            "time": m.time,
            "text": m.text,
        }))
        .collect();
    Json(json!({"memory": list, "count": memory.len()})).into_response()
}

async fn api_memory_delete(State(ctx): State<Arc<WebCtx>>, Json(body): Json<Value>) -> Response {
    let rpid = body.get("rpid").and_then(|v| v.as_str()).unwrap_or("");
    let mut memory = ctx.memory.load();
    memory.retain(|m| m.rpid != rpid);
    let _ = ctx.memory.save(&memory);
    Json(json!({"ok": true})).into_response()
}

// ---------- 人格 ----------
async fn api_personality(State(ctx): State<Arc<WebCtx>>) -> Response {
    let evo: Value = load_json(&ctx.path("personality_evolution.json"), json!({}));
    Json(evo).into_response()
}

// ---------- 用户 ----------
async fn api_users(State(ctx): State<Arc<WebCtx>>) -> Response {
    let profiles: Value = load_json(&ctx.path("user_profiles.json"), json!({}));
    let affection: Value = load_json(&ctx.path("affection.json"), json!({}));
    let users: Vec<Value> = profiles
        .as_object()
        .map(|obj| {
            obj.iter()
                .map(|(uid, profile)| {
                    let score = affection.get(uid).and_then(|v| v.as_i64()).unwrap_or(0);
                    let level = ctx.personality.get_level(score, Some(uid));
                    json!({
                        "uid": uid,
                        "impression": profile.get("impression").cloned().unwrap_or(json!("")),
                        "facts": profile.get("facts").cloned().unwrap_or(json!([])),
                        "tags": profile.get("tags").cloned().unwrap_or(json!([])),
                        "affection": score,
                        "level": level,
                        "level_name": crate::personality::Personality::level_name(level),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Json(json!({"users": users})).into_response()
}

async fn api_user_detail(State(ctx): State<Arc<WebCtx>>, AxumPath(uid): AxumPath<String>) -> Response {
    let profiles: Value = load_json(&ctx.path("user_profiles.json"), json!({}));
    let profile = profiles.get(&uid).cloned().unwrap_or(json!({}));
    let affection: Value = load_json(&ctx.path("affection.json"), json!({}));
    let score = affection.get(&uid).and_then(|v| v.as_i64()).unwrap_or(0);
    let level = ctx.personality.get_level(score, Some(&uid));
    Json(json!({
        "uid": uid,
        "profile": profile,
        "affection": score,
        "level": level,
        "level_name": crate::personality::Personality::level_name(level),
    }))
    .into_response()
}

// ---------- 成本 ----------
async fn api_cost_stats(State(ctx): State<Arc<WebCtx>>) -> Response {
    let cost: Value = load_json(&ctx.path("cost_log.json"), json!({}));
    let today = crate::util::today_str();
    let today_entry = cost.get(&today).cloned().unwrap_or(json!({}));
    Json(json!({
        "by_day": cost,
        "today": today_entry,
        "date": today,
    }))
    .into_response()
}

// ---------- 安全中心 ----------
async fn api_blocklist(State(ctx): State<Arc<WebCtx>>) -> Response {
    let block_log: Value = load_json(&ctx.path("block_log.json"), json!({}));
    Json(block_log).into_response()
}

async fn api_security_list(State(ctx): State<Arc<WebCtx>>) -> Response {
    let logs: Vec<Value> = load_json(&ctx.path("security_log.json"), Vec::new());
    Json(json!({"logs": logs})).into_response()
}

/// 聚合待人工确认的拉黑建议（对齐 build_block_suggestions）。
fn build_block_suggestions(ctx: &WebCtx) -> Vec<Value> {
    let logs: Vec<Value> = load_json(&ctx.path("security_log.json"), Vec::new());
    let block_log: Value = load_json(&ctx.path("block_log.json"), json!({}));
    let dismissed: std::collections::HashSet<String> = load_json(&ctx.path("block_suggestion_dismissed.json"), Vec::<String>::new())
        .iter()
        .map(|x| x.to_string())
        .collect();
    let owner = ctx.config.read().unwrap().clone().get_str("OWNER_MID");
    let mut agg: Vec<Value> = Vec::new();
    for ev in logs.iter() {
        let etype = ev.get("type").and_then(|t| t.as_str()).unwrap_or("");
        if etype != "auto_block_suppressed" && etype != "private_message_quarantined" {
            continue;
        }
        let uid = ev.get("uid").and_then(|u| u.as_str()).unwrap_or("").to_string();
        if uid.is_empty() || uid == "0" || uid == "None" || uid == "null" {
            continue;
        }
        if uid == owner || block_log.get(&uid).is_some() || dismissed.contains(&uid) {
            continue;
        }
        let time = ev.get("time").and_then(|t| t.as_str()).unwrap_or("").to_string();
        let username = ev.get("username").and_then(|u| u.as_str()).unwrap_or("未知").to_string();
        let reason = ev.get("detail").and_then(|d| d.as_str()).unwrap_or("").to_string();
        let content = ev.get("content").and_then(|c| c.as_str()).unwrap_or("").to_string();
        let source = if etype == "private_message_quarantined" { "私信" } else { "评论" };
        let mut item = agg.iter_mut().find(|i| i.get("uid").and_then(|u| u.as_str()) == Some(uid.as_str()));
        match &mut item {
            Some(it) => {
                it["hits"] = json!(it.get("hits").and_then(|h| h.as_i64()).unwrap_or(0) + 1);
                if time > it.get("last_time").and_then(|t| t.as_str()).unwrap_or("").to_string() {
                    it["last_time"] = json!(time);
                    it["username"] = json!(username);
                    it["reason"] = json!(reason);
                    it["source"] = json!(source);
                }
                if let Some(samples) = it["samples"].as_array_mut() {
                    if !content.is_empty() && !samples.iter().any(|s| s.as_str() == Some(content.as_str())) && samples.len() < 3 {
                        samples.push(json!(content.chars().take(120).collect::<String>()));
                    }
                }
            }
            None => {
                agg.push(json!({
                    "uid": uid,
                    "username": username,
                    "reason": reason,
                    "source": source,
                    "hits": 1,
                    "first_time": time,
                    "last_time": time,
                    "samples": if content.is_empty() { vec![] } else { vec![json!(content.chars().take(120).collect::<String>())] },
                }));
            }
        }
    }
    agg.sort_by(|a, b| {
        b.get("last_time").and_then(|t| t.as_str()).unwrap_or("").cmp(a.get("last_time").and_then(|t| t.as_str()).unwrap_or(""))
    });
    agg
}

async fn api_block_suggestions(State(ctx): State<Arc<WebCtx>>) -> Response {
    let items = build_block_suggestions(&ctx);
    Json(json!({"suggestions": items, "total": items.len()})).into_response()
}

async fn api_block_suggestion_dismiss(State(ctx): State<Arc<WebCtx>>, Json(body): Json<Value>) -> Response {
    let uid = body.get("uid").and_then(|v| v.as_str()).unwrap_or("").to_string();
    if uid.is_empty() {
        return json_resp(StatusCode::BAD_REQUEST, json!({"error": "缺少UID"}));
    }
    let mut kept: Vec<String> = load_json(&ctx.path("block_suggestion_dismissed.json"), Vec::<String>::new());
    kept.retain(|x| x != &uid);
    if !body.get("undo").and_then(|v| v.as_bool()).unwrap_or(false) {
        kept.push(uid);
    }
    kept.truncate(1000);
    let _ = save_json(&ctx.path("block_suggestion_dismissed.json"), &kept);
    let total = build_block_suggestions(&ctx).len();
    Json(json!({"ok": true, "total": total})).into_response()
}

async fn api_block_user(State(ctx): State<Arc<WebCtx>>, Json(body): Json<Value>) -> Response {
    let uid = body.get("uid").and_then(|v| v.as_str()).unwrap_or("").to_string();
    if uid.is_empty() {
        return json_resp(StatusCode::BAD_REQUEST, json!({"error": "缺少UID"}));
    }
    let username = body.get("username").and_then(|v| v.as_str()).unwrap_or("未知").to_string();
    let reason = body.get("reason").and_then(|v| v.as_str()).unwrap_or("前端手动拉黑").to_string();
    let mut block_log: Value = load_json(&ctx.path("block_log.json"), json!({}));
    block_log[&uid] = json!({
        "username": username,
        "reason": reason,
        "last_comment": "",
        "score": 0,
        "time": now_str(),
    });
    let _ = save_json(&ctx.path("block_log.json"), &block_log);
    if let Ok(mid) = uid.parse::<i64>() {
        ctx.bili.block_user(mid).await;
    }
    let mut kept: Vec<String> = load_json(&ctx.path("block_suggestion_dismissed.json"), Vec::<String>::new());
    kept.retain(|x| x != &uid);
    kept.truncate(1000);
    let _ = save_json(&ctx.path("block_suggestion_dismissed.json"), &kept);
    Json(json!({"ok": true, "msg": format!("已拉黑UID:{uid}")})).into_response()
}

// ---------- 功能开关 ----------
async fn api_features(State(ctx): State<Arc<WebCtx>>) -> Response {
    let cfg = ctx.config.read().unwrap().clone();
    let keys = [
        "ENABLE_WEB_SEARCH", "ENABLE_PROACTIVE", "ENABLE_DYNAMIC",
        "ENABLE_PERSONALITY_EVOLUTION", "ENABLE_MOOD", "ENABLE_AFFECTION",
        "ENABLE_PRIVATE_MESSAGES", "PRIVATE_MESSAGE_AUTO_REPLY",
        "PRIVATE_MESSAGE_AUTO_BLOCK", "PROACTIVE_LIKE", "PROACTIVE_COIN",
        "PROACTIVE_FAV", "PROACTIVE_FOLLOW", "PROACTIVE_COMMENT", "DYNAMIC_ENABLED",
    ];
    let mut out = serde_json::Map::new();
    for k in keys {
        out.insert(k.to_string(), json!(cfg.get_bool(k)));
    }
    Json(Value::Object(out)).into_response()
}

async fn api_features_update(State(ctx): State<Arc<WebCtx>>, Json(body): Json<Value>) -> Response {
    let allowed = [
        "ENABLE_WEB_SEARCH", "ENABLE_PROACTIVE", "ENABLE_DYNAMIC",
        "ENABLE_PERSONALITY_EVOLUTION", "ENABLE_MOOD", "ENABLE_AFFECTION",
        "ENABLE_PRIVATE_MESSAGES", "PRIVATE_MESSAGE_AUTO_REPLY",
        "PRIVATE_MESSAGE_AUTO_BLOCK", "PROACTIVE_LIKE", "PROACTIVE_COIN",
        "PROACTIVE_FAV", "PROACTIVE_FOLLOW", "PROACTIVE_COMMENT", "DYNAMIC_ENABLED",
    ];
    let mut updates = serde_json::Map::new();
    if let Some(obj) = body.as_object() {
        for k in allowed {
            if let Some(v) = obj.get(k) {
                updates.insert(k.to_string(), v.clone());
            }
        }
    }
    if updates.is_empty() {
        return json_resp(StatusCode::BAD_REQUEST, json!({"error": "无有效字段"}));
    }
    let _ = crate::config::update_config(&ctx.config, &Value::Object(updates.clone()));
    Json(json!({"ok": true, "updated": updates.keys().collect::<Vec<_>>()})).into_response()
}

// ---------- 调度 ----------
async fn api_schedule(State(ctx): State<Arc<WebCtx>>) -> Response {
    let cfg = ctx.config.read().unwrap().clone();
    Json(json!({
        "PROACTIVE_VIDEO_COUNT": cfg.get_i64("PROACTIVE_VIDEO_COUNT"),
        "PROACTIVE_COMMENT_COUNT": cfg.get_i64("PROACTIVE_COMMENT_COUNT"),
        "PROACTIVE_TIMES_COUNT": cfg.get_i64("PROACTIVE_TIMES_COUNT"),
        "EVOLVE_HOUR": cfg.get_i64("EVOLVE_HOUR"),
        "SLEEP_START": cfg.get_i64("SLEEP_START"),
        "SLEEP_END": cfg.get_i64("SLEEP_END"),
        "ENABLE_SLEEP": cfg.get_bool("ENABLE_SLEEP"),
        "MOOD_WEIGHT": cfg.get_f64("MOOD_WEIGHT"),
    }))
    .into_response()
}

async fn api_schedule_today(State(ctx): State<Arc<WebCtx>>) -> Response {
    let sched: Value = load_json(&ctx.path("schedule_today.json"), json!({}));
    Json(sched).into_response()
}

async fn api_schedule_update(State(ctx): State<Arc<WebCtx>>, Json(body): Json<Value>) -> Response {
    let allowed = [
        "PROACTIVE_VIDEO_COUNT", "PROACTIVE_COMMENT_COUNT", "PROACTIVE_TIMES_COUNT",
        "EVOLVE_HOUR", "SLEEP_START", "SLEEP_END", "MOOD_WEIGHT", "ENABLE_SLEEP",
    ];
    let mut updates = serde_json::Map::new();
    if let Some(obj) = body.as_object() {
        for k in allowed {
            if let Some(v) = obj.get(k) {
                updates.insert(k.to_string(), v.clone());
            }
        }
    }
    if updates.is_empty() {
        return json_resp(StatusCode::BAD_REQUEST, json!({"error": "无有效字段"}));
    }
    let _ = crate::config::update_config(&ctx.config, &Value::Object(updates));
    Json(json!({"ok": true})).into_response()
}

// ---------- 自定义提示词 ----------
async fn api_prompts(State(ctx): State<Arc<WebCtx>>) -> Response {
    let cfg = ctx.config.read().unwrap().clone();
    Json(json!({
        "PROMPT_DYNAMIC": cfg.get_str("PROMPT_DYNAMIC"),
        "PROMPT_PROACTIVE_COMMENT": cfg.get_str("PROMPT_PROACTIVE_COMMENT"),
        "PROMPT_VIDEO_EVALUATE": cfg.get_str("PROMPT_VIDEO_EVALUATE"),
        "PROMPT_PERSONALITY_EVOLVE": cfg.get_str("PROMPT_PERSONALITY_EVOLVE"),
        "PROMPT_SEARCH_PREFIX": cfg.get_str("PROMPT_SEARCH_PREFIX"),
        "PROMPT_IMAGINE": cfg.get_str("PROMPT_IMAGINE"),
        "PROMPT_PRIVATE_MESSAGE": cfg.get_str("PROMPT_PRIVATE_MESSAGE"),
        "DYNAMIC_TOPICS": cfg.get_str_list("DYNAMIC_TOPICS"),
    }))
    .into_response()
}

async fn api_prompts_update(State(ctx): State<Arc<WebCtx>>, Json(body): Json<Value>) -> Response {
    let allowed = [
        "PROMPT_DYNAMIC", "PROMPT_PROACTIVE_COMMENT", "PROMPT_VIDEO_EVALUATE",
        "PROMPT_PERSONALITY_EVOLVE", "PROMPT_SEARCH_PREFIX", "PROMPT_IMAGINE",
        "PROMPT_PRIVATE_MESSAGE", "DYNAMIC_TOPICS",
    ];
    let mut updates = serde_json::Map::new();
    if let Some(obj) = body.as_object() {
        for k in allowed {
            if let Some(v) = obj.get(k) {
                updates.insert(k.to_string(), v.clone());
            }
        }
    }
    if updates.is_empty() {
        return json_resp(StatusCode::BAD_REQUEST, json!({"error": "无有效字段"}));
    }
    let _ = crate::config::update_config(&ctx.config, &Value::Object(updates));
    Json(json!({"ok": true})).into_response()
}

// ---------- 心情 ----------
async fn api_mood(State(ctx): State<Arc<WebCtx>>) -> Response {
    let mood: Value = load_json(&ctx.path("mood.json"), json!({}));
    Json(mood).into_response()
}

async fn api_mood_update(State(ctx): State<Arc<WebCtx>>, Json(body): Json<Value>) -> Response {
    let mut mood: Value = load_json(&ctx.path("mood.json"), json!({}));
    if let Some(m) = body.get("mood") {
        mood["mood"] = m.clone();
    }
    if let Some(p) = body.get("mood_prompt") {
        mood["mood_prompt"] = p.clone();
    }
    if mood.get("date").is_none() {
        mood["date"] = json!(crate::util::today_str());
    }
    let _ = save_json(&ctx.path("mood.json"), &mood);
    Json(json!({"ok": true})).into_response()
}

// ---------- 好感度 ----------
async fn api_affection_update(State(ctx): State<Arc<WebCtx>>, Json(body): Json<Value>) -> Response {
    let uid = body.get("uid").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let score = body.get("score").and_then(|v| v.as_i64());
    if uid.is_empty() || score.is_none() {
        return json_resp(StatusCode::BAD_REQUEST, json!({"error": "缺少参数"}));
    }
    let mut affection: Value = load_json(&ctx.path("affection.json"), json!({}));
    affection[&uid] = json!(score.unwrap());
    let _ = save_json(&ctx.path("affection.json"), &affection);
    Json(json!({"ok": true})).into_response()
}

// ---------- 性格演化编辑 ----------
async fn api_personality_delete_trait(State(ctx): State<Arc<WebCtx>>, Json(body): Json<Value>) -> Response {
    delete_evo_item(&ctx, "evolved_traits", body).await
}
async fn api_personality_delete_habit(State(ctx): State<Arc<WebCtx>>, Json(body): Json<Value>) -> Response {
    delete_evo_item(&ctx, "speech_habits", body).await
}
async fn api_personality_delete_opinion(State(ctx): State<Arc<WebCtx>>, Json(body): Json<Value>) -> Response {
    delete_evo_item(&ctx, "opinions", body).await
}
async fn delete_evo_item(ctx: &WebCtx, key: &str, body: Value) -> Response {
    let idx = body.get("index").and_then(|v| v.as_i64()).unwrap_or(-1);
    let mut evo: Value = load_json(&ctx.path("personality_evolution.json"), json!({}));
    let mut arr: Vec<Value> = evo.get(key).and_then(|v| v.as_array()).cloned().unwrap_or_default();
    if idx >= 0 && (idx as usize) < arr.len() {
        arr.remove(idx as usize);
        evo[key] = json!(arr);
        let _ = save_json(&ctx.path("personality_evolution.json"), &evo);
        return Json(json!({"ok": true})).into_response();
    }
    json_resp(StatusCode::BAD_REQUEST, json!({"error": "索引无效"}))
}

async fn api_personality_clear(State(ctx): State<Arc<WebCtx>>) -> Response {
    let _ = save_json(&ctx.path("personality_evolution.json"), &json!({}));
    Json(json!({"ok": true, "msg": "成长日志已清空"})).into_response()
}

// ---------- personas 管理 ----------
async fn api_personas_delete(State(ctx): State<Arc<WebCtx>>, Json(body): Json<Value>) -> Response {
    let name = body.get("name").and_then(|v| v.as_str()).unwrap_or("");
    if name == "default" {
        return json_resp(StatusCode::BAD_REQUEST, json!({"error": "不能删除默认人格"}));
    }
    let store = PersonaStore::new(&ctx.base_dir);
    if !store.exists(name) {
        return json_resp(StatusCode::NOT_FOUND, json!({"error": "人格不存在"}));
    }
    let _ = store.delete(name);
    let cfg = ctx.config.read().unwrap().clone();
    if cfg.get_str("ACTIVE_PERSONA") == name {
        let _ = crate::config::update_config(&ctx.config, &json!({"ACTIVE_PERSONA": "default"}));
    }
    Json(json!({"ok": true})).into_response()
}

async fn api_personas_reset(State(ctx): State<Arc<WebCtx>>) -> Response {
    let _ = PersonaStore::new(&ctx.base_dir).reset();
    let _ = crate::config::update_config(&ctx.config, &json!({"ACTIVE_PERSONA": "default"}));
    let _ = save_json(&ctx.path("personality_evolution.json"), &json!({}));
    let _ = save_json(&ctx.path("mood.json"), &json!({}));
    let _ = save_json(&ctx.path("permanent_memory.json"), &json!([]));
    Json(json!({"ok": true, "msg": "已重置为默认人设，性格演化/永久记忆/心情已清空"})).into_response()
}

// ---------- 数据导出 ----------
async fn api_export(State(ctx): State<Arc<WebCtx>>) -> Response {
    let export = json!({
        "memory": load_json(&ctx.path("memory.json"), Vec::<Value>::new()),
        "affection": load_json(&ctx.path("affection.json"), json!({})),
        "chat_history": load_json(&ctx.path("local_chat.json"), Vec::<Value>::new()),
        "permanent_memory": load_json(&ctx.path("permanent_memory.json"), Vec::<Value>::new()),
        "personality": load_json(&ctx.path("personality_evolution.json"), json!({})),
        "user_profiles": load_json(&ctx.path("user_profiles.json"), json!({})),
        "personas": PersonaStore::new(&ctx.base_dir).list(),
        "mood": load_json(&ctx.path("mood.json"), json!({})),
        "export_time": now_str(),
    });
    Json(export).into_response()
}

// ---------- 模型测试 ----------
async fn api_model_test(State(ctx): State<Arc<WebCtx>>, Json(body): Json<Value>) -> Response {
    let model_type = body.get("type").and_then(|v| v.as_str()).unwrap_or("chat");
    if !matches!(model_type, "chat" | "vision" | "search" | "image") {
        return json_resp(StatusCode::BAD_REQUEST, json!({"ok": false, "error": format!("未知模型类型: {model_type}")}));
    }
    let cfg = ctx.config.read().unwrap().clone();
    let (base_url, api_key, candidates) = cfg.model_of(model_type);
    let model = candidates.first().cloned().unwrap_or_default();
    let start = std::time::Instant::now();
    let client = reqwest::Client::new();
    let url = format!("{base_url}/chat/completions");
    let test_payload = if model_type == "image" {
        json!({"model": model, "messages": [{"role": "user", "content": "test pixel"}], "modalities": ["image"], "max_tokens": 1})
    } else {
        json!({"model": model, "messages": [{"role": "user", "content": "hi"}], "max_tokens": 5})
    };
    let resp = match client
        .post(&url)
        .bearer_auth(&api_key)
        .json(&test_payload)
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            let latency = start.elapsed().as_millis() as i64;
            let err_str = e.to_string();
            let msg = if err_str.to_lowercase().contains("timeout") {
                "连接超时，检查 Base URL 是否可达".to_string()
            } else if err_str.contains("<!DOCTYPE") || err_str.contains("<html") {
                "API返回了HTML页面而非JSON，Base URL 或模型ID可能配置错误".to_string()
            } else {
                err_str.chars().take(200).collect()
            };
            return Json(json!({"ok": false, "error": msg, "model": model, "latency": latency})).into_response();
        }
    };
    let latency = start.elapsed().as_millis() as i64;
    let status = resp.status();
    if status == 200 {
        if model_type == "image" {
            return Json(json!({"ok": true, "model": model, "latency": latency, "msg": "图片模型可用"})).into_response();
        }
        let v: Value = resp.json().await.unwrap_or(json!({}));
        let reply = v["choices"][0]["message"]["content"].as_str().unwrap_or("").trim().to_string();
        return Json(json!({"ok": true, "model": model, "latency": latency, "msg": format!("收到回复: {}", reply.chars().take(50).collect::<String>())})).into_response();
    }
    if status == 404 {
        return Json(json!({"ok": false, "error": format!("模型不存在或已下线: {model}"), "model": model})).into_response();
    }
    if status == 401 {
        return Json(json!({"ok": false, "error": "API Key 无效或过期", "model": model})).into_response();
    }
    if status == 429 {
        return Json(json!({"ok": true, "model": model, "latency": latency, "msg": "连接正常（限流中）"})).into_response();
    }
    let body_txt = resp.text().await.unwrap_or_default();
    let body = if body_txt.contains("<!DOCTYPE") || body_txt.contains("<html") {
        "API返回了HTML页面，Base URL 可能配置错误".to_string()
    } else {
        body_txt.chars().take(200).collect()
    };
    Json(json!({"ok": false, "error": format!("HTTP {status}: {body}"), "model": model})).into_response()
}

// ---------- 聊天重新生成 / 生图 ----------
async fn api_chat_regenerate(State(ctx): State<Arc<WebCtx>>) -> Response {
    let mut history: Vec<Value> = load_json(&ctx.path("local_chat.json"), Vec::new());
    if history.len() < 2 {
        return json_resp(StatusCode::BAD_REQUEST, json!({"error": "没有可重新生成的消息"}));
    }
    if history.last().and_then(|m| m.get("role").and_then(|r| r.as_str())) != Some("assistant") {
        return json_resp(StatusCode::BAD_REQUEST, json!({"error": "最后一条不是Bot的回复"}));
    }
    history.pop();
    let last_user = match history.last() {
        Some(m) if m.get("role").and_then(|r| r.as_str()) == Some("user") => m.clone(),
        _ => return json_resp(StatusCode::BAD_REQUEST, json!({"error": "找不到对应的用户消息"})),
    };
    let user_msg = last_user.get("content").and_then(|c| c.as_str()).unwrap_or("").to_string();
    let image_filename = last_user.get("image").and_then(|c| c.as_str()).unwrap_or("").to_string();

    let reply = match generate_chat_reply(&ctx, &user_msg, &image_filename, &history[..history.len().saturating_sub(1)]).await {
        Ok(r) => r,
        Err(e) => return json_resp(StatusCode::BAD_GATEWAY, json!({"error": format!("生成失败：{e}")})),
    };
    history.push(json!({"role": "assistant", "content": reply, "time": now_str()}));
    let _ = save_json(&ctx.path("local_chat.json"), &history);
    let mut memory = ctx.memory.load();
    let mem_text = if user_msg.is_empty() { "（发送了一张图片）".to_string() } else if !image_filename.is_empty() { format!("{user_msg}（附带图片）") } else { user_msg.clone() };
    ctx.memory.save_local_memory(&mut memory, &mem_text, &reply);
    Json(json!({"reply": reply})).into_response()
}

async fn api_chat_imagine(State(ctx): State<Arc<WebCtx>>, Json(body): Json<Value>) -> Response {
    let prompt = body.get("prompt").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    if prompt.is_empty() {
        return json_resp(StatusCode::BAD_REQUEST, json!({"error": "请描述想要生成的图片"}));
    }
    let cfg = ctx.config.read().unwrap().clone();
    let bot_name = cfg.get_str("BOT_NAME");
    let permanent: Vec<Value> = load_json(&ctx.path("permanent_memory.json"), Vec::new());
    let perm_section = if permanent.is_empty() {
        String::new()
    } else {
        let texts: Vec<String> = permanent.iter().rev().take(10).filter_map(|p| p.get("text").and_then(|t| t.as_str()).map(|s| s.to_string())).collect();
        format!("\n你的自我认知：{}", texts.join("；"))
    };
    let persona_brief: String = PersonaStore::new(&ctx.base_dir).active_system_prompt(&cfg.get_str("ACTIVE_PERSONA")).chars().take(200).collect();
    let custom = cfg.get_str("PROMPT_IMAGINE");
    let refine_prompt = if !custom.is_empty() {
        custom
            .replace("{prompt}", &prompt)
            .replace("{bot_name}", &bot_name)
            .replace("{persona}", &persona_brief)
            .replace("{perm_section}", &perm_section)
    } else {
        format!(
            "你是{bot_name}。{persona_brief}{perm_section}\n\n用户请你画一张图，描述是：「{prompt}」\n\n请根据你的审美和人设，将用户的描述转化为一段详细的英文图片生成 prompt。要求：\n1. 风格偏二次元/插画/唯美，适合 AI 生图\n2. 融入你的审美偏好（冰蓝色调、冷色系、氛围感）\n3. 加入具体的画面细节（光影、构图、色彩、氛围）\n4. 保留用户原始意图，但让描述更丰富专业\n5. 只输出英文 prompt，不加任何解释，不超过100词"
        )
    };
    let max_tok = cfg.max_tokens_of("image_prompt");
    let refined = match ctx.llm.compress("chat", &refine_prompt, max_tok).await {
        Ok(r) if !r.is_empty() => r,
        _ => prompt.clone(),
    };
    // 生图（与 dynamic 同款 OpenAI 兼容 modalities）
    let (base_url, api_key, candidates) = cfg.model_of("image");
    let model = candidates.first().cloned().unwrap_or_default();
    let client = reqwest::Client::new();
    let payload = json!({
        "model": model,
        "messages": [{"role": "user", "content": refined}],
        "modalities": ["text", "image"],
        "n": 1,
    });
    match client
        .post(format!("{base_url}/chat/completions"))
        .bearer_auth(&api_key)
        .json(&payload)
        .timeout(std::time::Duration::from_secs(120))
        .send()
        .await
    {
        Ok(resp) => {
            let v: Value = resp.json().await.unwrap_or(json!({}));
            let images = v["choices"][0]["message"]["images"].as_array().cloned().unwrap_or_default();
            if let Some(img) = images.first() {
                let url = img.get("url").and_then(|u| u.as_str()).unwrap_or("");
                let b64 = img.get("b64_json").and_then(|b| b.as_str()).unwrap_or("");
                if !url.is_empty() {
                    return Json(json!({"ok": true, "url": url})).into_response();
                }
                if !b64.is_empty() {
                    return Json(json!({"ok": true, "b64": b64})).into_response();
                }
            }
            json_resp(StatusCode::BAD_GATEWAY, json!({"ok": false, "error": "生图模型未返回图片"}))
        }
        Err(e) => json_resp(StatusCode::BAD_GATEWAY, json!({"ok": false, "error": format!("生图失败：{e}")})),
    }
}

/// 面板聊天核心：组装上下文并调用 LLM。
async fn generate_chat_reply(ctx: &WebCtx, user_msg: &str, image_filename: &str, history: &[Value]) -> Result<String> {
    let cfg = ctx.config.read().unwrap().clone();
    let mut llm_messages: Vec<Value> = Vec::new();
    let persona = PersonaStore::new(&ctx.base_dir).active_system_prompt(&cfg.get_str("ACTIVE_PERSONA"));
    if !persona.is_empty() {
        llm_messages.push(json!({"role": "system", "content": persona}));
    }
    llm_messages.push(json!({"role": "system", "content": format!("你是{}，一个有独立人格的存在。有自己的想法和态度，不敷衍不套话，说话自然像真人聊天。", cfg.get_str("BOT_NAME"))}));
    for m in history.iter().rev().take(12).rev() {
        let role = m.get("role").and_then(|r| r.as_str()).unwrap_or("user");
        let content = m.get("content").and_then(|c| c.as_str()).unwrap_or("");
        llm_messages.push(json!({"role": role, "content": content}));
    }
    if !image_filename.is_empty() {
        let img_path = ctx.path("images").join(image_filename);
        if let Ok(bytes) = std::fs::read(&img_path) {
            let mime = image_mime(image_filename);
            llm_messages.push(json!({
                "role": "user",
                "content": [{"type": "image_url", "image_url": {"url": format!("data:{mime};base64,{}", crate::util::b64_encode(&bytes))}}]
            }));
        } else {
            tracing::warn!("图片不存在或不可读，降级为纯文本消息: {image_filename}");
        }
    }
    llm_messages.push(json!({"role": "user", "content": user_msg}));
    let max_tokens = cfg.max_tokens_of("chat");
    let r = ctx.llm.complete("chat", json!(llm_messages), max_tokens).await?;
    if r.text.is_empty() {
        return Err(crate::error::AppError::Llm("模型返回空正文".into()));
    }
    log_cost(&ctx.config, "面板对话", r.input_tokens, r.output_tokens, &r.model, &ctx.llm.cost_log);
    Ok(r.text)
}

// ---------- 成本 ----------
async fn api_cost_add(State(ctx): State<Arc<WebCtx>>, Json(body): Json<Value>) -> Response {
    let amount = body.get("amount").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let calls = body.get("calls").and_then(|v| v.as_i64()).unwrap_or(1);
    if amount <= 0.0 {
        return json_resp(StatusCode::BAD_REQUEST, json!({"error": "金额无效"}));
    }
    let today = crate::util::today_str();
    let mut logs: Value = load_json(&ctx.path("cost_log.json"), json!({}));
    // 账本文件损坏为非对象时兜底为 {}，避免 as_object_mut().unwrap() 触发 500
    if !logs.is_object() {
        logs = json!({});
    }
    let entry = logs
        .as_object_mut()
        .map(|obj| obj.entry(today).or_insert(json!({"total": 0.0, "calls": 0, "input_tokens": 0, "output_tokens": 0})))
        .unwrap();
    entry["total"] = json!(entry.get("total").and_then(|v| v.as_f64()).unwrap_or(0.0) + amount);
    entry["calls"] = json!(entry.get("calls").and_then(|v| v.as_i64()).unwrap_or(0) + calls);
    let _ = save_json(&ctx.path("cost_log.json"), &logs);
    Json(json!({"ok": true})).into_response()
}

// ---------- 记录列表 ----------
async fn api_dynamic_list(State(ctx): State<Arc<WebCtx>>) -> Response {
    let mut logs: Vec<Value> = load_json(&ctx.path("dynamic_log.json"), Vec::new());
    logs.sort_by(|a, b| b.get("time").and_then(|t| t.as_str()).unwrap_or("").cmp(a.get("time").and_then(|t| t.as_str()).unwrap_or("")));
    logs.truncate(50);
    Json(json!({"logs": logs})).into_response()
}

async fn api_proactive_list(State(ctx): State<Arc<WebCtx>>) -> Response {
    let mut logs: Vec<Value> = load_json(&ctx.path("proactive_log.json"), Vec::new());
    logs.sort_by(|a, b| b.get("time").and_then(|t| t.as_str()).unwrap_or("").cmp(a.get("time").and_then(|t| t.as_str()).unwrap_or("")));
    logs.truncate(50);
    Json(json!({"logs": logs})).into_response()
}

async fn api_watchlog_list(State(ctx): State<Arc<WebCtx>>) -> Response {
    let mut logs: Vec<Value> = load_json(&ctx.path("watch_log.json"), Vec::new());
    logs.sort_by(|a, b| b.get("time").and_then(|t| t.as_str()).unwrap_or("").cmp(a.get("time").and_then(|t| t.as_str()).unwrap_or("")));
    Json(json!({"logs": logs})).into_response()
}

async fn api_permanent_list(State(ctx): State<Arc<WebCtx>>) -> Response {
    Json(json!({"items": ctx.permanent.load()})).into_response()
}

async fn api_permanent_add(State(ctx): State<Arc<WebCtx>>, Json(body): Json<Value>) -> Response {
    let text = body.get("text").and_then(|v| v.as_str()).unwrap_or("").trim();
    if text.is_empty() {
        return json_resp(StatusCode::BAD_REQUEST, json!({"error": "内容为空"}));
    }
    if ctx.permanent.load().len() >= crate::memory::PERMANENT_MEMORY_LIMIT {
        return json_resp(StatusCode::BAD_REQUEST, json!({"error": "永久记忆已满20条，请先删除旧的"}));
    }
    ctx.permanent.add(text);
    Json(json!({"ok": true})).into_response()
}

async fn api_permanent_delete(State(ctx): State<Arc<WebCtx>>, Json(body): Json<Value>) -> Response {
    let index = body.get("index").and_then(|v| v.as_i64()).unwrap_or(-1);
    if index >= 0 {
        ctx.permanent.remove_by_index(index as usize);
    }
    Json(json!({"ok": true})).into_response()
}

// ---------- 摘要 ----------
async fn api_summary(State(ctx): State<Arc<WebCtx>>) -> Response {
    let summary: Value = load_json(&ctx.path("summary.json"), json!({}));
    Json(summary).into_response()
}

async fn api_summary_save(State(ctx): State<Arc<WebCtx>>, Json(body): Json<Value>) -> Response {
    let _ = save_json(&ctx.path("summary.json"), &body);
    Json(json!({"ok": true})).into_response()
}

// ---------- personas ----------
async fn api_personas(State(ctx): State<Arc<WebCtx>>) -> Response {
    let cfg = ctx.config.read().unwrap().clone();
    Json(json!({
        "personas": PersonaStore::new(&ctx.base_dir).list(),
        "active": cfg.get_str("ACTIVE_PERSONA"),
    }))
    .into_response()
}

async fn api_personas_create(State(ctx): State<Arc<WebCtx>>, Json(body): Json<Value>) -> Response {
    let name = body.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let prompt = body.get("system_prompt").and_then(|v| v.as_str()).unwrap_or("");
    if name.is_empty() {
        return json_resp(StatusCode::BAD_REQUEST, json!({"error": "名称不能为空"}));
    }
    let display_name = body.get("display_name").and_then(|v| v.as_str()).unwrap_or("");
    match PersonaStore::new(&ctx.base_dir).create(name, display_name, prompt) {
        Ok(persona) => Json(json!({"ok": true, "persona": persona})).into_response(),
        Err(e) => json_resp(StatusCode::BAD_REQUEST, json!({"error": e.to_string()})),
    }
}

async fn api_personas_update(State(ctx): State<Arc<WebCtx>>, Json(body): Json<Value>) -> Response {
    let name = body.get("name").and_then(|v| v.as_str()).unwrap_or("");
    if name.is_empty() {
        return json_resp(StatusCode::BAD_REQUEST, json!({"error": "缺少名称"}));
    }
    match PersonaStore::new(&ctx.base_dir).update(name, &body) {
        Ok(_) => Json(json!({"ok": true})).into_response(),
        Err(e) => json_resp(StatusCode::NOT_FOUND, json!({"error": e.to_string()})),
    }
}

async fn api_personas_switch(State(ctx): State<Arc<WebCtx>>, Json(body): Json<Value>) -> Response {
    let name = body.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let store = PersonaStore::new(&ctx.base_dir);
    if !store.exists(name) {
        return json_resp(StatusCode::NOT_FOUND, json!({"error": "人格不存在"}));
    }
    // 与 Python 一致：切换即更新配置 ACTIVE_PERSONA
    let _ = crate::config::update_config(&ctx.config, &json!({"ACTIVE_PERSONA": name}));
    Json(json!({"ok": true, "active": name})).into_response()
}

// ---------- 图片上传 ----------
async fn api_upload_image(State(ctx): State<Arc<WebCtx>>, mut multipart: Multipart) -> Response {
    const MAX_UPLOAD_BYTES: usize = 10 * 1024 * 1024;
    let img_dir = ctx.path("images");
    std::fs::create_dir_all(&img_dir).ok();
    loop {
        let field = match multipart.next_field().await {
            Ok(Some(f)) => f,
            Ok(None) => break,
            Err(e) => {
                tracing::warn!("multipart 解析失败: {e}");
                break;
            }
        };
        let name = field.file_name().unwrap_or("upload.jpg").to_string();
        let ext = name.rsplit('.').next().unwrap_or("jpg").to_lowercase();
        if !matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "gif" | "webp") {
            continue;
        }
        let data = match field.bytes().await {
            Ok(d) => d,
            Err(_) => continue,
        };
        if data.is_empty() {
            continue;
        }
        if data.len() > MAX_UPLOAD_BYTES {
            return json_resp(StatusCode::PAYLOAD_TOO_LARGE, json!({"error": "图片超过 10MB 上限"}));
        }
        // 时间戳 + 随机后缀，避免同一秒内两次上传互相覆盖
        let fname = format!("{}_{}.{}", crate::util::now_unix(), crate::util::gen_token().chars().take(6).collect::<String>(), ext);
        let target = img_dir.join(&fname);
        if std::fs::write(&target, &data).is_ok() {
            return Json(json!({"ok": true, "filename": fname, "url": format!("/data/images/{fname}")})).into_response();
        }
    }
    json_resp(StatusCode::BAD_REQUEST, json!({"error": "上传失败"}))
}

// ---------- 静态文件 ----------
async fn serve_index(State(ctx): State<Arc<WebCtx>>) -> Response {
    serve_file(&ctx, "chat.html")
}

async fn serve_avatar(State(ctx): State<Arc<WebCtx>>) -> Response {
    let cfg = ctx.config.read().unwrap().clone();
    let avatar = cfg.get_str("BOT_AVATAR");
    let name = avatar.strip_prefix("/data/images/").unwrap_or("");
    if name.is_empty() || name.contains('/') || name.contains('\\') || name.contains("..") {
        return StatusCode::NOT_FOUND.into_response();
    }
    serve_file(&ctx, &format!("data/images/{name}"))
}

async fn serve_image(State(ctx): State<Arc<WebCtx>>, AxumPath(name): AxumPath<String>) -> Response {
    if name.contains('/') || name.contains('\\') || name.contains("..") {
        return StatusCode::NOT_FOUND.into_response();
    }
    serve_file(&ctx, &format!("data/images/{name}"))
}

async fn api_health() -> Response {
    Json(json!({"ok": true, "name": "bilibili-ai-bot-rs", "time": now_str()})).into_response()
}

/// 按扩展名推断图片 MIME（与 Python get_image_media_type 对齐）。
fn image_mime(name: &str) -> &'static str {
    match name.rsplit('.').next().unwrap_or("").to_lowercase().as_str() {
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => "image/jpeg",
    }
}

fn serve_file(ctx: &WebCtx, rel: &str) -> Response {
    let path = Path::new(&ctx.base_dir).join(rel);
    match std::fs::read(&path) {
        Ok(bytes) => {
            let mime = if rel.ends_with(".html") {
                "text/html; charset=utf-8"
            } else if rel.ends_with(".webp") {
                "image/webp"
            } else if rel.ends_with(".png") {
                "image/png"
            } else if rel.ends_with(".jpg") || rel.ends_with(".jpeg") {
                "image/jpeg"
            } else if rel.ends_with(".gif") {
                "image/gif"
            } else {
                "application/octet-stream"
            };
            ([(header::CONTENT_TYPE, mime)], bytes).into_response()
        }
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

// ---------- 建图与启动 ----------
pub fn build_router(ctx: Arc<WebCtx>) -> Router {
    Router::new()
        .route("/", get(serve_index))
        .route("/api/health", get(api_health))
        .route("/api/login", post(api_login))
        .route("/api/auth_check", get(api_auth_check))
        .route("/api/handshake", get(api_handshake))
        .route("/api/change_password", post(api_change_password))
        .route("/api/branding", get(api_branding))
        .route("/api/config", get(api_config))
        .route("/api/config/raw", get(api_config_raw))
        .route("/api/config/update", post(api_config_update))
        .route("/api/config/check_cookie", get(api_config_check_cookie))
        .route("/api/config/refresh_cookie", post(api_config_refresh_cookie))
        .route("/api/config/qr_login/start", post(api_qr_login_start))
        .route("/api/config/qr_login/poll", post(api_qr_login_poll))
        .route("/api/chat", post(api_chat))
        .route("/api/chat/history", get(api_chat_history))
        .route("/api/chat/clear", post(api_chat_clear))
        .route("/api/memory/list", get(api_memory_list))
        .route("/api/memory/delete", post(api_memory_delete))
        .route("/api/personality", get(api_personality))
        .route("/api/users", get(api_users))
        .route("/api/user/:uid", get(api_user_detail))
        .route("/api/cost/stats", get(api_cost_stats))
        .route("/api/blocklist", get(api_blocklist))
        .route("/api/security/list", get(api_security_list))
        .route("/api/block_suggestions", get(api_block_suggestions))
        .route("/api/block_suggestion/dismiss", post(api_block_suggestion_dismiss))
        .route("/api/block_user", post(api_block_user))
        .route("/api/features", get(api_features))
        .route("/api/features/update", post(api_features_update))
        .route("/api/schedule", get(api_schedule))
        .route("/api/schedule/today", get(api_schedule_today))
        .route("/api/schedule/update", post(api_schedule_update))
        .route("/api/prompts", get(api_prompts))
        .route("/api/prompts/update", post(api_prompts_update))
        .route("/api/mood", get(api_mood))
        .route("/api/mood/update", post(api_mood_update))
        .route("/api/affection/update", post(api_affection_update))
        .route("/api/personality/delete_trait", post(api_personality_delete_trait))
        .route("/api/personality/delete_habit", post(api_personality_delete_habit))
        .route("/api/personality/delete_opinion", post(api_personality_delete_opinion))
        .route("/api/personality/clear", post(api_personality_clear))
        .route("/api/personas/delete", post(api_personas_delete))
        .route("/api/personas/reset", post(api_personas_reset))
        .route("/api/export", get(api_export))
        .route("/api/model/test", post(api_model_test))
        .route("/api/chat/regenerate", post(api_chat_regenerate))
        .route("/api/chat/imagine", post(api_chat_imagine))
        .route("/api/cost/add", post(api_cost_add))
        .route("/api/dynamic/list", get(api_dynamic_list))
        .route("/api/proactive/list", get(api_proactive_list))
        .route("/api/watchlog/list", get(api_watchlog_list))
        .route("/api/permanent/list", get(api_permanent_list))
        .route("/api/permanent/add", post(api_permanent_add))
        .route("/api/permanent/delete", post(api_permanent_delete))
        .route("/api/summary", get(api_summary))
        .route("/api/summary/save", post(api_summary_save))
        .route("/api/personas", get(api_personas))
        .route("/api/personas/create", post(api_personas_create))
        .route("/api/personas/update", post(api_personas_update))
        .route("/api/personas/switch", post(api_personas_switch))
        .route("/api/upload_image", post(api_upload_image))
        .route("/media/bot-avatar", get(serve_avatar))
        .route("/data/images/:name", get(serve_image))
        .layer(middleware::from_fn_with_state(ctx.clone(), auth_middleware))
        .with_state(ctx)
}

/// 生成或加载密封密钥对（data/.seal_key.pem，PKCS8 PEM）。
pub fn load_seal_key(base_dir: &str) -> Option<RsaPrivateKey> {
    let path = Path::new(base_dir).join("data").join(".seal_key.pem");
    if let Ok(text) = std::fs::read_to_string(&path) {
        if let Ok(key) = RsaPrivateKey::from_pkcs8_pem(&text) {
            return Some(key);
        }
    }
    let key = RsaPrivateKey::new(&mut rand::rngs::OsRng, 2048).ok()?;
    let pem = key.to_pkcs8_pem(rsa::pkcs8::LineEnding::LF).ok()?;
    std::fs::create_dir_all(path.parent()?).ok()?;
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::write(&path, pem.as_bytes());
    let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    Some(key)
}

/// 加载或生成会话密钥（data/.secret_key）。
pub fn load_secret_key(base_dir: &str) -> String {
    let env = std::env::var("SECRET_KEY").unwrap_or_default();
    if !env.is_empty() {
        return env;
    }
    let path = Path::new(base_dir).join("data").join(".secret_key");
    if let Ok(text) = std::fs::read_to_string(&path) {
        let k = text.trim().to_string();
        if k.len() >= 32 {
            return k;
        }
    }
    let k = format!("{}{}", crate::util::gen_token(), crate::util::gen_token());
    std::fs::create_dir_all(path.parent().unwrap_or(Path::new("."))).ok();
    let _ = std::fs::write(&path, &k);
    k
}

/// 启动 Web 服务（0.0.0.0:port）。
pub async fn serve(ctx: Arc<WebCtx>, port: u16) {
    if ctx.auth_password() == DEFAULT_AUTH_PASSWORD {
        tracing::warn!("面板正在使用默认口令，请尽快通过环境变量 CHAT_PASSWORD 或配置项 CHAT_PASSWORD 修改（默认口令不可用于公网部署）");
    }
    let app = build_router(ctx);
    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));
    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            tracing::error!("端口 {port} 绑定失败: {e}");
            return;
        }
    };
    tracing::info!("Web 面板已启动 -> http://localhost:{port}");
    if let Err(e) = axum::serve(listener, app).await {
        tracing::error!("Web 服务退出: {e}");
    }
}

// 保留未使用引用
#[allow(dead_code)]
fn _keep(_: &mut Sha256, _: &Personality) {}
