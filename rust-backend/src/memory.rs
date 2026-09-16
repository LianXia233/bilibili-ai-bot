//! 记忆系统：JSON 存储、Embedding 语义检索、线程记忆、用户记忆压缩。
//! 与 ai.py 记忆模块对齐；未配置 Embedding 时自动降级为「最近记忆」。

use crate::config::Config;
use crate::error::Result;
use crate::llm::{log_cost, LlmClient};
use crate::util::{load_json, now_str, save_json, v_str};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

pub const USER_MEMORY_COMPRESS_THRESHOLD: usize = 30;
pub const USER_MEMORY_KEEP_RECENT: usize = 10;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MemoryDoc {
    #[serde(default)]
    pub rpid: String,
    #[serde(default)]
    pub thread_id: String,
    #[serde(default)]
    pub user_id: String,
    #[serde(default)]
    pub time: String,
    #[serde(default)]
    pub text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub embedding: Vec<f32>,
}

pub struct MemoryStore {
    pub config: Arc<RwLock<Config>>,
    pub llm: Arc<LlmClient>,
    pub file: PathBuf,
    pub profile_file: PathBuf,
    pub security_file: PathBuf,
    embed_available: AtomicBool,
}

impl MemoryStore {
    pub fn new(config: Arc<RwLock<Config>>, llm: Arc<LlmClient>, base_dir: &str) -> Self {
        MemoryStore {
            config,
            llm,
            file: crate::util::data_path(base_dir, "memory.json"),
            profile_file: crate::util::data_path(base_dir, "user_profiles.json"),
            security_file: crate::util::data_path(base_dir, "security_log.json"),
            embed_available: AtomicBool::new(true),
        }
    }

    pub fn load(&self) -> Vec<MemoryDoc> {
        load_json(&self.file, Vec::new())
    }

    pub fn save(&self, memory: &[MemoryDoc]) -> Result<()> {
        save_json(&self.file, &memory)
    }

    // ---------- Embedding ----------
    pub async fn get_embedding(&self, text: &str) -> Vec<f32> {
        if !self.embed_available.load(Ordering::Relaxed) {
            return Vec::new();
        }
        let cfg = self.config.read().unwrap().clone();
        let base = cfg.get_str("EMBED_BASE_URL");
        let key = cfg.get_str("SILICON_API_KEY");
        let model = cfg.get_str("EMBED_MODEL");
        let model = if model.is_empty() { "BAAI/bge-m3" } else { &model };
        let url = format!("{}/embeddings", base.trim_end_matches('/'));
        let payload = json!({"model": model, "input": text});
        match self
            .llm
            .http
            .post(url)
            .header("Authorization", format!("Bearer {key}"))
            .json(&payload)
            .send()
            .await
        {
            Ok(resp) => match resp.json::<Value>().await {
                Ok(v) => {
                    if let Some(arr) = v["data"][0]["embedding"].as_array() {
                        return arr.iter().filter_map(|x| x.as_f64().map(|f| f as f32)).collect();
                    }
                    self.embed_available.store(false, Ordering::Relaxed);
                    Vec::new()
                }
                Err(_) => {
                    self.embed_available.store(false, Ordering::Relaxed);
                    Vec::new()
                }
            },
            Err(_) => {
                self.embed_available.store(false, Ordering::Relaxed);
                Vec::new()
            }
        }
    }

    // ---------- 记录与读取 ----------
    pub async fn save_record(
        &self,
        memory: &mut Vec<MemoryDoc>,
        rpid: &str,
        thread_id: &str,
        user_id: &str,
        username: &str,
        content: &str,
        reply_text: &str,
    ) -> Result<()> {
        let cfg = self.config.read().unwrap().clone();
        let bot_name = cfg.get_str("BOT_NAME");
        // 记忆里只保存 Bot 回复的前 N 字（长成品/长文不污染记忆），由 MEMORY_REPLY_CHARS 控制。
        let reply_keep: usize = {
            let n = cfg.get_i64("MEMORY_REPLY_CHARS");
            if n > 0 { n as usize } else { 300 }
        };
        let reply_saved: String = reply_text.chars().take(reply_keep).collect();
        let text = format!("[{now}] 用户{user_id}({username})说：{content} | {bot_name}回复：{reply_saved}", now = now_str());
        // embedding 以「用户侧内容」为主体，避免 Bot 自己回复的风格主导语义检索，造成召回偏差。
        let embed_text = format!("用户{user_id}({username})说：{content}");
        let embedding = self.get_embedding(&embed_text).await;
        memory.push(MemoryDoc {
            rpid: rpid.to_string(),
            thread_id: thread_id.to_string(),
            user_id: user_id.to_string(),
            time: now_str(),
            text,
            embedding,
        });
        self.save(memory)
    }

    /// 用户独立会话上下文：按 user_id 聚合该用户全部历史对话（评论/@/私信共享一份），
    /// **全部优先注入**（不按相关度挑拣——避免记忆被丢弃导致失忆/乱回），按时间正序排列；
    /// 预算 max_chars（默认 300K，见 CONTEXT_MAX_CHARS）作为安全上限，
    /// 超预算时保留「最近连续段」（倒序装入、装满即停、再反转回时间正序）。
    /// 每个用户只有自己的一份会话记忆，互不串台；回复前默认注入，作为连贯上下文的来源。
    pub async fn get_user_session_context(
        &self,
        memory: &[MemoryDoc],
        user_id: &str,
        max_chars: usize,
    ) -> Vec<String> {
        let mut user_mems: Vec<&MemoryDoc> = memory
            .iter()
            .filter(|m| m.user_id == user_id && m.thread_id != "compressed" && m.thread_id != "local")
            .collect();
        if user_mems.is_empty() {
            return Vec::new();
        }
        user_mems.sort_by_key(|m| m.time.clone());
        // 全部注入（时间正序）；超预算时保留最近连续段
        let mut rev: Vec<String> = Vec::new();
        let mut used = 0usize;
        for m in user_mems.iter().rev() {
            let t = m.text.clone();
            let len = t.chars().count();
            if used + len > max_chars && !rev.is_empty() {
                break;
            }
            used += len;
            rev.push(t);
            if used >= max_chars {
                break;
            }
        }
        rev.reverse();
        rev
    }

    #[allow(dead_code)]
    pub fn get_recent_memories(&self, memory: &[MemoryDoc], limit: usize) -> Vec<String> {
        let mut docs = memory.to_vec();
        docs.sort_by_key(|m| m.time.clone());
        docs.iter().rev().take(limit).map(|m| m.text.clone()).collect()
    }

    /// 构建记忆上下文：相关线程记忆 + 用户语义记忆 + 用户档案。
    pub async fn build_memory_context(&self, memory: &[MemoryDoc], user_id: &str) -> String {
        let cfg = self.config.read().unwrap().clone();
        let session_cap: usize = {
            let n = cfg.get_i64("CONTEXT_MAX_CHARS");
            if n > 0 { n as usize } else { 300_000 }
        };
        let mut parts: Vec<String> = Vec::new();
        // 用户独立会话上下文：默认注入（每个用户自己的连续对话历史，跨评论/@/私信聚合，
        // 全部优先注入、按时间正序；预算仅作安全上限，超限保留最近连续段）
        let session = self.get_user_session_context(memory, user_id, session_cap).await;
        if !session.is_empty() {
            let joined = session.join("\n");
            parts.push(format!("【与该用户的对话上下文（默认注入：先读这段历史理解对方是谁、聊过什么，再回复；据此保持连贯，禁止照搬复述历史发言）】\n{joined}"));
        }
        // 注：不再单独注入语义/线程检索段——会话上下文已全量包含该用户全部记忆，
        // 再挑拣只会造成重复；跨用户记忆一律不注入（用户会话相互独立）。
        let profile = self.get_user_profile_context(user_id);
        if !profile.is_empty() {
            parts.push(profile);
        }
        parts.join("\n\n")
    }

    // ---------- 用户档案 ----------
    pub fn get_user_profile_context(&self, user_id: &str) -> String {
        let profiles: Value = load_json(&self.profile_file, json!({}));
        let profile = profiles.get(user_id);
        let profile = match profile {
            Some(p) => p,
            None => return String::new(),
        };
        let mut parts = Vec::new();
        let impression = profile.get("impression").and_then(|v| v.as_str()).unwrap_or("");
        if !impression.is_empty() {
            parts.push(format!("印象：{impression}"));
        }
        let facts = profile.get("facts").and_then(|v| v.as_array()).cloned().unwrap_or_default();
        if !facts.is_empty() {
            let tail: Vec<String> = facts.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect();
            let tail = tail.into_iter().rev().take(10).collect::<Vec<_>>();
            let mut tail = tail;
            tail.reverse();
            parts.push(format!("已知信息：{}", tail.join("；")));
        }
        let tags = profile.get("tags").and_then(|v| v.as_array()).cloned().unwrap_or_default();
        if !tags.is_empty() {
            let t: Vec<String> = tags.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect();
            parts.push(format!("标签：{}", t.join("、")));
        }
        if parts.is_empty() {
            String::new()
        } else {
            format!("【对该用户的了解】\n{}", parts.join("\n"))
        }
    }

    pub fn update_user_profile(&self, user_id: &str, impression: Option<&str>, new_facts: Option<Vec<String>>, new_tags: Option<Vec<String>>) -> Result<()> {
        let mut profiles: Value = load_json(&self.profile_file, json!({}));
        let uid = user_id.to_string();
        if profiles.get(&uid).is_none() {
            profiles[&uid] = json!({"impression": "", "facts": [], "tags": []});
        }
        if let Some(imp) = impression {
            if !imp.is_empty() {
                profiles[&uid]["impression"] = json!(imp);
            }
        }
        if let Some(facts) = new_facts {
            let existing = profiles[&uid]["facts"].as_array().cloned().unwrap_or_default();
            let mut existing: Vec<String> = existing.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect();
            for fact in facts {
                let f = fact.trim().to_string();
                if !f.is_empty() && !existing.contains(&f) {
                    existing.push(f);
                }
            }
            if existing.len() > 20 {
                existing.drain(..existing.len() - 20);
            }
            profiles[&uid]["facts"] = json!(existing);
        }
        if let Some(tags) = new_tags {
            let existing = profiles[&uid]["tags"].as_array().cloned().unwrap_or_default();
            let mut existing: Vec<String> = existing.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect();
            for tag in tags {
                let t = tag.trim().to_string();
                if !t.is_empty() && !existing.contains(&t) {
                    existing.push(t);
                }
            }
            if existing.len() > 10 {
                existing.drain(..existing.len() - 10);
            }
            profiles[&uid]["tags"] = json!(existing);
        }
        save_json(&self.profile_file, &profiles)
    }

    // ---------- 记忆压缩 ----------
    pub async fn compress_user_memory(&self, memory: &mut Vec<MemoryDoc>, user_id: &str, username: &str) -> Result<()> {
        let user_mems: Vec<MemoryDoc> = memory.iter().filter(|m| m.user_id == user_id).cloned().collect();
        if user_mems.len() <= USER_MEMORY_COMPRESS_THRESHOLD {
            return Ok(());
        }
        let mut sorted = user_mems.clone();
        sorted.sort_by_key(|m| m.time.clone());
        let keep_count = sorted.len().saturating_sub(USER_MEMORY_KEEP_RECENT);
        let old_mems: Vec<MemoryDoc> = sorted.drain(..keep_count).collect();
        if old_mems.is_empty() {
            return Ok(());
        }
        let old_texts: String = old_mems.iter().map(|m| m.text.clone()).collect::<Vec<_>>().join("\n");
        let old_texts: String = old_texts.chars().take(3000).collect();
        let cfg = self.config.read().unwrap().clone();
        let bot_name = cfg.get_str("BOT_NAME");
        let prompt = format!(
            "你是{bot_name}，请根据以下与用户\"{username}\"的历史互动记录，完成以下任务：\n\n\
1. 写一段精炼的总结（100字以内），概括你和这个用户的关系、互动特点、重要事件\n\
2. 给这个用户打3-5个标签，描述ta的特点（如：常聊话题、性格、活跃时段等）\n\
3. 提取用户提到的个人信息（如：喜欢什么、做什么工作、多大年龄、在哪个城市、有什么习惯等），每条信息一句话\n\
4. 严格输出合法JSON，所有值中不要包含未转义的双引号。\n\n\
历史记录：\n{old_texts}\n\n\
请以JSON格式回复：\n\
{{\"summary\": \"总结内容\", \"tags\": [\"标签1\", \"标签2\"], \"user_facts\": [\"喜欢打游戏\", \"是大学生\"]}}\n\n\
user_facts：只提取用户明确说过的事实信息，不要瞎猜。没有就留空数组。"
        );
        let max_tokens = cfg.max_tokens_of("memory_compress");
        match self.llm.compress("chat", &prompt, max_tokens).await {
            Ok(text) => {
                let cleaned = text.replace("```json", "").replace("```", "").trim().to_string();
                let result: Value = parse_json_lenient(&cleaned).unwrap_or_else(|| json!({"summary": cleaned.chars().take(100).collect::<String>(), "tags": [], "user_facts": []}));
                let summary = result.get("summary").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let tags: Vec<String> = result.get("tags").and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect()).unwrap_or_default();
                let user_facts: Vec<String> = result.get("user_facts").and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect()).unwrap_or_default();
                if !summary.is_empty() {
                    self.update_user_profile(user_id, Some(&summary), Some(user_facts), Some(tags))?;
                }
                let now = now_str();
                let compressed = MemoryDoc {
                    rpid: format!("compressed_{}", crate::util::now_unix()),
                    thread_id: "compressed".into(),
                    user_id: user_id.to_string(),
                    time: now,
                    text: format!("[记忆压缩] {summary}"),
                    embedding: self.get_embedding(&summary).await,
                };
                let old_rpids: std::collections::HashSet<String> = old_mems.iter().map(|m| m.rpid.clone()).collect();
                memory.retain(|m| !old_rpids.contains(&m.rpid));
                memory.push(compressed);
                self.save(memory)?;
                tracing::info!("压缩完成：{} 条 → 1 条总结", old_mems.len());
            }
            Err(e) => {
                tracing::warn!("记忆压缩失败: {e}");
            }
        }
        Ok(())
    }

    // ---------- 安全日志 ----------
    pub fn log_security_event(&self, event_type: &str, mid: &str, username: &str, content: &str, detail: &str) {
        let mut logs: Vec<Value> = load_json(&self.security_file, Vec::new());
        let content_short: String = content.chars().take(200).collect();
        logs.push(json!({
            "time": now_str(),
            "type": event_type,
            "uid": mid,
            "username": username,
            "content": content_short,
            "detail": detail,
        }));
        if logs.len() > 500 {
            logs.drain(..logs.len() - 500);
        }
        let _ = save_json(&self.security_file, &logs);
    }

    // ---------- 本地聊天记忆（与 local-chat 共用 memory.json） ----------
    pub fn save_local_memory(&self, memory: &mut Vec<MemoryDoc>, user_msg: &str, reply: &str) {
        let text = format!("[{}] 用户说：{user_msg} | 回复：{reply}", now_str());
        memory.push(MemoryDoc {
            rpid: format!("local_{}", crate::util::now_unix()),
            thread_id: "local".into(),
            user_id: "local".into(),
            time: now_str(),
            text,
            embedding: Vec::new(),
        });
        let _ = self.save(memory);
    }
}

/// 宽容 JSON 解析：先整段解析，失败则抽取第一个 {} 块。
pub fn parse_json_lenient(text: &str) -> Option<Value> {
    if let Ok(v) = serde_json::from_str::<Value>(text) {
        return Some(v);
    }
    let re = regex::Regex::new(r"\{.*\}").ok()?;
    let m = re.find(text)?;
    serde_json::from_str(m.as_str()).ok()
}

/// 永久记忆（data/permanent_memory.json）。
/// 与 Python 兼容：条目为 {text, time} 对象数组，上限 40 条（2026-09-15 提升）。
pub struct PermanentMemory {
    pub file: PathBuf,
}

pub const PERMANENT_MEMORY_LIMIT: usize = 40;

/// 表情包池类条目的头部标记（与 Python EMOJI_POOL_MARKERS 对齐）。
const EMOJI_POOL_MARKERS: [&str; 4] = ["表情包池", "原始表情包池", "原样保留", "表情包清单"];

impl PermanentMemory {
    pub fn new(base_dir: &str) -> Self {
        PermanentMemory { file: crate::util::data_path(base_dir, "permanent_memory.json") }
    }
    /// 读取对象数组；兼容旧版纯字符串数组（读取时自动迁移为对象）。
    pub fn load(&self) -> Vec<Value> {
        let raw: Value = load_json(&self.file, json!([]));
        let mut out: Vec<Value> = Vec::new();
        if let Some(arr) = raw.as_array() {
            for item in arr {
                if let Some(s) = item.as_str() {
                    out.push(json!({"text": s, "time": ""}));
                } else {
                    out.push(item.clone());
                }
            }
        }
        out
    }
    pub fn add(&self, text: &str) {
        let text = text.trim().to_string();
        if text.is_empty() {
            return;
        }
        let mut list = self.load();
        if list.iter().any(|v| v.get("text").and_then(|t| t.as_str()) == Some(text.as_str())) {
            return;
        }
        if list.len() >= PERMANENT_MEMORY_LIMIT {
            return;
        }
        list.push(json!({"text": text, "time": crate::util::now_str()}));
        let _ = save_json(&self.file, &list);
    }
    /// 清空永久记忆（调用方负责备份）。
    pub fn clear(&self) -> usize {
        let before = self.load().len();
        let _ = save_json(&self.file, &Vec::<Value>::new());
        before
    }
    /// 按索引改写某条（整合长规则时不必先删再加）。
    pub fn update(&self, index: usize, text: &str) -> bool {
        let text = text.trim();
        if text.is_empty() {
            return false;
        }
        let mut list = self.load();
        if index >= list.len() {
            return false;
        }
        if let Some(obj) = list[index].as_object_mut() {
            obj.insert("text".to_string(), json!(text));
            obj.insert("time".to_string(), json!(crate::util::now_str()));
            obj.insert("source".to_string(), json!("manual"));
        }
        let _ = save_json(&self.file, &list);
        true
    }
    /// 整体替换（内部去重；超过上限返回 Err）。
    pub fn import(&self, items: Vec<Value>) -> std::result::Result<usize, String> {
        let mut cleaned: Vec<Value> = Vec::new();
        let mut seen: Vec<String> = Vec::new();
        for raw in items {
            let text = if let Some(obj) = raw.as_object() {
                v_str(obj.get("text").unwrap_or(&Value::Null), "")
            } else {
                v_str(&raw, "")
            };
            let text = text.trim().to_string();
            if text.is_empty() || seen.contains(&text) {
                continue;
            }
            seen.push(text.clone());
            cleaned.push(json!({"text": text, "time": crate::util::now_str(), "source": "manual"}));
        }
        if cleaned.len() > PERMANENT_MEMORY_LIMIT {
            return Err(format!(
                "整理后仍有 {} 条，超过上限 {}",
                cleaned.len(),
                PERMANENT_MEMORY_LIMIT
            ));
        }
        let before = self.load().len();
        let _ = save_json(&self.file, &cleaned);
        Ok(before)
    }
    pub fn remove_by_index(&self, index: usize) {
        let mut list = self.load();
        if index < list.len() {
            list.remove(index);
            let _ = save_json(&self.file, &list);
        }
    }
}

/// 判断一条永久记忆是不是「表情包池清单」类条目（素材索引，摘要注入）。
fn is_emoji_pool_entry(text: &str) -> bool {
    let head: String = text.chars().take(60).collect();
    EMOJI_POOL_MARKERS.iter().any(|m| head.contains(m))
}

/// 把表情包池清单压成「可用素材」摘要（样例 + 条数 + 硬约束）。
fn summarize_emoji_pool(entries: &[String]) -> String {
    let mut lines: Vec<String> = Vec::new();
    let mut total = 0usize;
    for text in entries {
        let mut names: Vec<String> = Vec::new();
        for seg in text.split('[') {
            if let Some(idx) = seg.find(']') {
                let n = seg[..idx].trim().to_string();
                if !n.is_empty() {
                    names.push(n);
                }
            }
        }
        total += names.len();
        if names.is_empty() {
            continue;
        }
        let sample = names.iter().take(6).map(|n| format!("[{n}]")).collect::<Vec<_>>().join("、");
        let tail = if names.len() > 6 { format!(" 等 {} 条", names.len()) } else { String::new() };
        lines.push(sample + &tail);
    }
    if lines.is_empty() {
        return String::new();
    }
    format!(
        "【可用表情包素材（索引摘要）】\n可用表情包池共约 {total} 条，按命名风格分布如下（样例，用的时候从同风格里挑）：\n{}",
        lines.iter().map(|l| format!("- {l}")).collect::<Vec<_>>().join("\n")
    ) + "\n使用约束：表情包名必须原样照抄，不得改名、缩写或自行创造；写进回复时用 [完整名称] 格式，多个连写不加分隔符。"
}

/// 给一条永久记忆规则定「装填优先级」，数字越小越先保（与 Python _rule_tier 对齐）。
fn rule_tier(text: &str) -> i64 {
    let head: String = text.chars().take(40).collect();
    if ["【身份】", "【人格", "行为原则", "【说话风格】"].iter().any(|k| head.contains(k)) {
        return 0;
    }
    if ["禁止", "底线", "【安全", "抗越狱", "冲突裁决", "优先级", "边界"].iter().any(|k| head.contains(k)) {
        return 1;
    }
    if ["不懂", "不确定", "无法理解", "理解用户", "今日心情", "短期状态"].iter().any(|k| head.contains(k)) {
        return 2;
    }
    if head.contains("表情包") {
        return 3;
    }
    9
}

/// 按字符预算组装「最高优先级规则」段落（分层装填 + 表情包池摘要）。
/// tier 0/1（人格/禁止/安全）无条件全量注入；budget 只约束 tier 2/3；
/// 组内「新的先保」，输出恢复原始书写顺序。
pub fn build_permanent_block(config: &Config, perm: &[Value]) -> String {
    if perm.is_empty() {
        return String::new();
    }
    let budget = {
        let b = config.get_i64("PERMANENT_MEMORY_CHAR_BUDGET");
        if b > 0 { b as usize } else { 2500 }
    };
    let inject = {
        let n = config.get_i64("PERMANENT_MEMORY_INJECT");
        if n > 0 { n as usize } else { 40 }
    };
    let start = perm.len().saturating_sub(inject);
    let items: Vec<Value> = perm[start..].to_vec();

    let mut rules: Vec<String> = Vec::new();
    let mut pools: Vec<String> = Vec::new();
    for entry in items {
        let text = entry.get("text").and_then(|t| t.as_str()).unwrap_or("").trim().to_string();
        if text.is_empty() {
            continue;
        }
        if is_emoji_pool_entry(&text) {
            pools.push(text);
        } else {
            rules.push(text);
        }
    }

    // 按层级分组，组内记录「后写的优先」（idx 大 = 新）
    let mut grouped: std::collections::BTreeMap<i64, Vec<(usize, &String)>> = Default::default();
    for (idx, text) in rules.iter().enumerate() {
        grouped.entry(rule_tier(text)).or_default().push((idx, text));
    }

    let mut kept_set: std::collections::HashSet<usize> = Default::default();
    let mut core_chars = 0usize;
    for (tier, mut entries) in grouped {
        entries.sort_by_key(|(idx, _)| std::cmp::Reverse(*idx)); // 组内新的先保
        if tier <= 1 {
            for (idx, text) in entries {
                kept_set.insert(idx);
                core_chars += text.chars().count() + 3;
            }
            continue;
        }
        let quota = budget.saturating_sub(core_chars);
        if tier == 3 && quota == 0 {
            continue;
        }
        let mut used = 0usize;
        for (idx, text) in entries {
            let cost = text.chars().count() + 3;
            // 首个条目无条件保留：预算极小时避免整层被跳过
            if used != 0 && used + cost > quota {
                continue;
            }
            kept_set.insert(idx);
            used += cost;
            core_chars += cost;
        }
    }

    // 输出恢复原始书写顺序
    let kept_list: Vec<String> = rules
        .iter()
        .enumerate()
        .filter(|(idx, _)| kept_set.contains(idx))
        .map(|(_, t)| t.clone())
        .collect();

    let mut blocks: Vec<String> = Vec::new();
    if !kept_list.is_empty() {
        blocks.push(
            "【最高优先级规则（人工设定，必须遵守）】\n以下是主人亲手写下的规则，优先级高于本提示词中的其他风格描述；若与【说话风格】【今日状态】等段落冲突，一律以本段为准。\n"
                .to_string()
                + &kept_list.iter().map(|t| format!("- {t}")).collect::<Vec<_>>().join("\n"),
        );
    }
    let pool_summary = summarize_emoji_pool(&pools);
    if !pool_summary.is_empty() {
        blocks.push(pool_summary);
    }
    if blocks.is_empty() {
        return String::new();
    }
    let dropped = rules.len() - kept_list.len();
    if dropped > 0 {
        blocks.push(format!(
            "（另有 {dropped} 条细节类规则（状态/表情包相关）因超出注入预算未展示；人格与禁令类规则已全部在上方列出。）"
        ));
    }
    blocks.join("\n\n")
}

// 保留 log_cost 引用（压缩走 chat 通道的计费由调用方负责）
#[allow(dead_code)]
fn _keep_log_cost(cfg: &Arc<RwLock<Config>>, s: &str, i: i64, o: i64, m: &str, p: &std::path::Path) {
    log_cost(cfg, s, i, o, m, p);
}

// ---------- 临时记忆清空（面板一键清空 / Bot 定时清空共用） ----------

/// 临时记忆文件清单（与 Python config.py TEMP_MEMORY_FILES 对齐）。
/// 临时 = 对话记忆 + 用户档案；长期（永久/好感度/视频缓存/性格/心情）不在此列。
pub fn temp_memory_files(base_dir: &str) -> Vec<(&'static str, PathBuf, &'static str)> {
    vec![
        ("dialog", crate::util::data_path(base_dir, "memory.json"), "对话记忆"),
        ("profile", crate::util::data_path(base_dir, "user_profiles.json"), "用户档案"),
    ]
}

/// 按保留天数裁剪记录列表（只对含 time 字段的列表生效）。
/// keep_days <= 0 表示全清；时间解析失败的单条按「保留」处理。
pub fn prune_old_records(records: &[Value], keep_days: i64) -> Vec<Value> {
    if keep_days <= 0 {
        return Vec::new();
    }
    let cutoff = chrono::Local::now() - chrono::Duration::days(keep_days);
    let cutoff_day = cutoff.format("%Y-%m-%d").to_string();
    records
        .iter()
        .filter(|r| {
            let ts = r.get("time").and_then(|t| t.as_str()).unwrap_or("");
            if ts.is_empty() {
                return true;
            }
            let day: String = ts.chars().take(10).collect();
            day >= cutoff_day
        })
        .cloned()
        .collect()
}

/// 备份数据文件为 `{path}.{tag}-{时间戳}.bak`（备份失败只打印不阻断）。
pub fn backup_data_file(path: &std::path::Path, tag: &str) {
    if !path.exists() {
        return;
    }
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let bak = path.with_file_name(format!("{name}.{tag}-{stamp}.bak"));
    match std::fs::read(path) {
        Ok(data) => {
            if let Err(e) = std::fs::write(&bak, data) {
                tracing::warn!("备份失败（{name}）：{e}");
            }
        }
        Err(e) => tracing::warn!("读取待备份文件失败（{name}）：{e}"),
    }
}

/// 清空临时记忆（对话记忆 + 用户档案），每个文件先备份。
/// 返回 (是否成功, 摘要文本, 明细列表)。
pub fn clear_temp_memory(base_dir: &str, keep_days: i64, tag: &str) -> (bool, String, Vec<Value>) {
    let mut cleared: Vec<Value> = Vec::new();
    for (key, path, label) in temp_memory_files(base_dir) {
        let before: Value = load_json(&path, json!([]));
        let count = before
            .as_array()
            .map(|a| a.len())
            .or_else(|| before.as_object().map(|o| o.len()))
            .unwrap_or(0);
        if count > 0 {
            backup_data_file(&path, tag);
        }
        let after: Value = if let Some(arr) = before.as_array() {
            json!(prune_old_records(arr, keep_days))
        } else if keep_days <= 0 {
            json!({})
        } else {
            before.clone()
        };
        let kept = after.as_array().map(|a| a.len()).or_else(|| after.as_object().map(|o| o.len())).unwrap_or(0);
        let _ = save_json(&path, &after);
        cleared.push(json!({"target": key, "label": label, "removed": count, "kept": kept}));
    }
    if cleared.is_empty() {
        return (false, "无可清空项".to_string(), cleared);
    }
    let msg = cleared
        .iter()
        .map(|c| {
            let kept = c.get("kept").and_then(|k| k.as_i64()).unwrap_or(0);
            let removed = c.get("removed").and_then(|r| r.as_i64()).unwrap_or(0);
            let label = c.get("label").and_then(|l| l.as_str()).unwrap_or("");
            if kept > 0 {
                format!("{label} 清除 {removed} 条（保留 {kept} 条）")
            } else {
                format!("{label} 清除 {removed} 条")
            }
        })
        .collect::<Vec<_>>()
        .join("；");
    (true, msg, cleared)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use std::path::PathBuf;

    fn test_mgr(tmp: &std::path::Path) -> (MemoryStore, Arc<RwLock<Config>>) {
        let cfg = Config::load(tmp.join("config.json")).unwrap();
        let cfg = Arc::new(RwLock::new(cfg));
        let llm = Arc::new(LlmClient::new(cfg.clone(), tmp.to_str().unwrap()));
        (MemoryStore::new(cfg.clone(), llm, tmp.to_str().unwrap()), cfg)
    }

    #[tokio::test]
    async fn user_session_isolated_and_budgeted() {
        let tmp = std::env::temp_dir().join(format!("mem_test_{}_{}", std::process::id(), uuid_suffix()));
        std::fs::create_dir_all(&tmp).unwrap();
        let (mgr, cfg) = test_mgr(&tmp);
        // 预算压到 60 字符，验证截断
        crate::config::update_config(&cfg, &json!({"CONTEXT_MAX_CHARS": 60})).unwrap();
        let mut memory: Vec<MemoryDoc> = Vec::new();
        // 用户 A：两条较长记忆
        mgr.save_record(&mut memory, "1", "t1", "uA", "甲", "今天一起看了星之卡比的新视频", "那个新作确实可爱喵").await.unwrap();
        mgr.save_record(&mut memory, "2", "t1", "uA", "甲", "我周末想试试新出的甜品店", "听起来不错喵").await.unwrap();
        // 用户 B：一条记忆（不应出现在 A 的会话里）
        mgr.save_record(&mut memory, "3", "t1", "uB", "乙", "我是另一个人的完全无关内容", "嗯嗯").await.unwrap();
        // 单独测用户 A 会话：只含 A，且总长度 ≤ 预算
        let sess = mgr.get_user_session_context(&memory, "uA", 60).await;
        assert!(!sess.is_empty(), "A 应有会话记忆");
        let total: usize = sess.iter().map(|s| s.chars().count()).sum();
        assert!(total <= 60, "会话注入应受预算约束, got {total}");
        for s in &sess {
            assert!(!s.contains("我是另一个人"), "会话串台: B 的记忆进入了 A 的上下文");
        }
        // 单独测用户 B：只有 B
        let sess_b = mgr.get_user_session_context(&memory, "uB", 100).await;
        assert_eq!(sess_b.len(), 1);
        assert!(sess_b[0].contains("我是另一个人"));
        // 全部注入：小预算下 A 只保留最近的连续片段，且该片段是 A 的最新对话
        let sess_all = mgr.get_user_session_context(&memory, "uA", 1000).await;
        let joined_all: String = sess_all.join("\n");
        assert!(joined_all.contains("甜品店"), "全部注入应包含 A 的全部记忆");
        assert!(joined_all.contains("星之卡比"), "全部注入应包含 A 的全部记忆");
        // 清理
        let _ = std::fs::remove_dir_all(&tmp);
    }

    fn uuid_suffix() -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos().to_string()
    }
}
