//! 记忆系统：JSON 存储、Embedding 语义检索、线程记忆、用户记忆压缩。
//! 与 ai.py 记忆模块对齐；未配置 Embedding 时自动降级为「最近记忆」。

use crate::config::Config;
use crate::error::Result;
use crate::llm::{log_cost, LlmClient};
use crate::util::{cosine_similarity, load_json, now_str, save_json};
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
        let text = format!("[{now}] 用户{user_id}({username})说：{content} | {bot_name}回复：{reply_text}", now = now_str());
        let embedding = self.get_embedding(&text).await;
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

    pub fn get_thread_memories(&self, memory: &[MemoryDoc], thread_id: &str) -> Vec<String> {
        let mut docs: Vec<&MemoryDoc> = memory.iter().filter(|m| m.thread_id == thread_id).collect();
        docs.sort_by_key(|m| m.time.clone());
        docs.iter().map(|m| m.text.clone()).collect()
    }

    pub async fn get_user_semantic_memories(&self, memory: &[MemoryDoc], user_id: &str, query_text: &str) -> Vec<String> {
        let user_mems: Vec<&MemoryDoc> = memory.iter().filter(|m| m.user_id == user_id).collect();
        if user_mems.is_empty() {
            return Vec::new();
        }
        let query_emb = self.get_embedding(query_text).await;
        if query_emb.is_empty() {
            return Vec::new();
        }
        let mut scored: Vec<(f32, &MemoryDoc)> = user_mems
            .iter()
            .map(|m| (cosine_similarity(&query_emb, &m.embedding), *m))
            .collect();
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        scored
            .into_iter()
            .take(3)
            .filter(|(s, _)| *s > 0.45)
            .map(|(_, m)| m.text.clone())
            .collect()
    }

    #[allow(dead_code)]
    pub fn get_recent_memories(&self, memory: &[MemoryDoc], limit: usize) -> Vec<String> {
        let mut docs = memory.to_vec();
        docs.sort_by_key(|m| m.time.clone());
        docs.iter().rev().take(limit).map(|m| m.text.clone()).collect()
    }

    /// 构建记忆上下文：线程记忆 + 用户语义记忆 + 用户档案。
    pub async fn build_memory_context(&self, memory: &[MemoryDoc], thread_id: &str, user_id: &str, query_text: &str) -> String {
        let mut parts: Vec<String> = Vec::new();
        let thread = self.get_thread_memories(memory, thread_id);
        if !thread.is_empty() {
            let tail: Vec<&String> = thread.iter().rev().take(6).collect();
            let mut joined = tail.iter().rev().map(|s| s.to_string()).collect::<Vec<_>>().join("\n");
            if joined.chars().count() > 1000 {
                joined = joined.chars().take(1000).collect::<String>();
            }
            parts.push(format!("【最近对话】\n{joined}"));
        }
        let semantic = self.get_user_semantic_memories(memory, user_id, query_text).await;
        if !semantic.is_empty() {
            parts.push(format!("【相关记忆】\n{}", semantic.join("\n")));
        }
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
/// 与 Python 兼容：条目为 {text, time} 对象数组，上限 20 条。
pub struct PermanentMemory {
    pub file: PathBuf,
}

pub const PERMANENT_MEMORY_LIMIT: usize = 20;

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
    pub fn remove_by_index(&self, index: usize) {
        let mut list = self.load();
        if index < list.len() {
            list.remove(index);
            let _ = save_json(&self.file, &list);
        }
    }
}

// 保留 log_cost 引用（压缩走 chat 通道的计费由调用方负责）
#[allow(dead_code)]
fn _keep_log_cost(cfg: &Arc<RwLock<Config>>, s: &str, i: i64, o: i64, m: &str, p: &std::path::Path) {
    log_cost(cfg, s, i, o, m, p);
}
