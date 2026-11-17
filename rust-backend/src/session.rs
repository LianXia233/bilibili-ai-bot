//! 多用户会话隔离：按 B 站用户标识（mid）维护互不干扰的对话会话，
//! 并把「全局固定人设」与「该用户的会话内容」组装成一次回复的上下文。
//!
//! ## 存储模型（单一数据源，不复制对话正文）
//! - 对话内容仍存 `data/memory.json` 的 [`MemoryDoc`]，`user_id` 即会话键：
//!   一个用户一份会话，评论 / @ / 私信都归入同一份（同一用户跨渠道连续对话）。
//! - `data/sessions.json` 只存**会话索引**（建立时间、最近活跃、最近渠道、人设快照、轮次统计），
//!   供读取 / 清理 / 排障使用；索引删掉不会丢对话内容，可从 memory.json 重建。
//!
//! 两份数据不重复存正文，避免「索引与对话各写一份」导致的不一致；
//! 所有按用户过滤的逻辑集中在本模块，调用方不再自行拼 `user_id` 条件。
//!
//! ## 与私信会话的区别
//! 这里的「会话」指**对话上下文**（一个 B 站用户一份）；[`crate::private_msgs`] 里的
//! `sessions` 指 B 站私信接口的会话游标，两者无关。

use crate::error::Result;
use crate::memory::{backup_data_file, MemoryDoc, MemoryStore};
use crate::util::{load_json, now_str, save_json};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// 会话索引上限（超出按最近活跃淘汰，只淘汰索引，不动对话正文）。
pub const SESSION_INDEX_LIMIT: usize = 5000;
/// 拿不到用户标识时的兜底键：单独归入一份会话，绝不与真实用户混用。
pub const UNKNOWN_USER_ID: &str = "unknown";
/// 本地面板对话在 memory.json 中的 user_id，不属于 B 站用户会话。
pub const LOCAL_USER_ID: &str = "local";

/// 会话元数据（索引项）。正文以 memory.json 为准，这里只存检索/统计用的字段。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SessionMeta {
    #[serde(default)]
    pub user_id: String,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
    /// 最近一次交互渠道：comment / at / private。
    #[serde(default)]
    pub channel: String,
    /// 最近一次回复使用的人设标识（ACTIVE_PERSONA），便于排查「换了人设但会话没变」。
    #[serde(default)]
    pub persona: String,
    /// 已记录轮次（统计用；真实正文条数以 memory.json 该用户记录数为准）。
    #[serde(default)]
    pub turns: usize,
}

/// 一次回复的上下文：固定人设 + 该用户独立会话。
///
/// 两段来自不同存储、渲染时按固定顺序拼接，互不写入对方：
/// - `persona`：personas.json / 永久规则，所有用户完全一致；
/// - `user_context`：按 `user_id` 过滤出的该用户会话 + 用户档案，跨用户不可见。
#[derive(Debug, Clone, Default)]
pub struct ReplyContext {
    pub user_id: String,
    pub username: String,
    pub persona: String,
    pub user_context: String,
}

impl ReplyContext {
    pub fn new(user_id: &str, username: &str, persona: &str, user_context: &str) -> Self {
        ReplyContext {
            user_id: SessionStore::normalize_user_id(user_id),
            username: username.trim().to_string(),
            persona: persona.trim().to_string(),
            user_context: user_context.trim().to_string(),
        }
    }

    /// 固定人设段（全局同一份，不含任何用户数据）。
    pub fn persona_section(&self) -> String {
        self.persona.trim().to_string()
    }

    /// 该用户独立会话段（带隔离说明，语气与旧版记忆段保持一致）。
    pub fn user_section(&self) -> String {
        if self.user_context.trim().is_empty() {
            return String::new();
        }
        let who = if self.username.is_empty() { "当前用户" } else { self.username.as_str() };
        format!(
            "\n\n【本用户会话（记忆，按 B 站用户 ID 隔离）】\n以下只属于当前用户「{who}」（UID {uid}）的历史对话/互动记录。回复前先读懂：对方是谁、之前聊过什么、关系如何。\n回复须与这段历史连贯（记得的事要接得上），但禁止照搬复述历史发言，禁止把记忆里的旧话题当成当前要回应的话题；记忆里没有的信息不要编造，不知道就自然承认；严禁使用或提及任何其他用户的信息。\n{body}",
            uid = self.user_id,
            body = self.user_context.trim()
        )
    }

    /// 完整渲染（人设在前、会话在后），主要供测试与排障查看实际注入结构。
    #[allow(dead_code)]
    pub fn render(&self) -> String {
        let persona = self.persona_section();
        let user = self.user_section();
        match (persona.is_empty(), user.is_empty()) {
            (true, true) => String::new(),
            (true, false) => user.trim_start().to_string(),
            (false, true) => persona,
            (false, false) => format!("{persona}{user}"),
        }
    }
}

/// 会话存储：一个 B 站用户一份会话（对话正文在 memory.json，索引在 sessions.json）。
#[derive(Debug, Clone)]
pub struct SessionStore {
    #[allow(dead_code)]
    pub base_dir: String,
    pub file: PathBuf,
}

impl SessionStore {
    pub fn new(base_dir: &str) -> Self {
        SessionStore {
            base_dir: base_dir.to_string(),
            file: crate::util::data_path(base_dir, "sessions.json"),
        }
    }

    /// 规范化用户标识：去空白、去控制字符、限长；空值归入 `unknown` 单独一份会话。
    pub fn normalize_user_id(raw: &str) -> String {
        let cleaned: String = raw
            .trim()
            .chars()
            .filter(|c| !c.is_control())
            .take(64)
            .collect();
        if cleaned.is_empty() {
            UNKNOWN_USER_ID.to_string()
        } else {
            cleaned
        }
    }

    // ---------- 索引读写 ----------
    fn entries(&self) -> BTreeMap<String, SessionMeta> {
        let raw: Value = load_json(&self.file, json!({}));
        let mut out: BTreeMap<String, SessionMeta> = BTreeMap::new();
        if let Some(obj) = raw.as_object() {
            for (key, value) in obj {
                let mut meta: SessionMeta = serde_json::from_value(value.clone()).unwrap_or_default();
                if meta.user_id.is_empty() {
                    meta.user_id = key.clone();
                }
                out.insert(key.clone(), meta);
            }
        }
        out
    }

    fn save_entries(&self, entries: &BTreeMap<String, SessionMeta>) -> Result<()> {
        let mut obj = serde_json::Map::new();
        for (key, meta) in entries {
            obj.insert(key.clone(), serde_json::to_value(meta).map_err(crate::error::AppError::from)?);
        }
        save_json(&self.file, &Value::Object(obj))
    }

    /// 索引超限时淘汰最久未活跃的条目（只删索引，不删对话）。
    fn trim(entries: &mut BTreeMap<String, SessionMeta>) {
        if entries.len() <= SESSION_INDEX_LIMIT {
            return;
        }
        let mut keys: Vec<(String, String)> = entries
            .iter()
            .map(|(k, m)| (k.clone(), m.updated_at.clone()))
            .collect();
        keys.sort_by(|a, b| a.1.cmp(&b.1));
        let drop_n = entries.len() - SESSION_INDEX_LIMIT;
        for (key, _) in keys.into_iter().take(drop_n) {
            entries.remove(&key);
        }
    }

    // ---------- 创建 / 读取 ----------
    /// 取会话，不存在则创建（新用户第一条消息即建立会话）。
    /// 已存在时刷新 `updated_at` 与昵称，`created_at` 保持不变。
    pub fn ensure(&self, user_id: &str, username: &str) -> SessionMeta {
        let uid = Self::normalize_user_id(user_id);
        let now = now_str();
        let mut entries = self.entries();
        let meta = entries.entry(uid.clone()).or_insert_with(|| SessionMeta {
            user_id: uid.clone(),
            username: username.trim().to_string(),
            created_at: now.clone(),
            updated_at: now.clone(),
            ..Default::default()
        });
        if meta.user_id.is_empty() {
            meta.user_id = uid.clone();
        }
        if meta.created_at.is_empty() {
            meta.created_at = now.clone();
        }
        if !username.trim().is_empty() {
            meta.username = username.trim().to_string();
        }
        meta.updated_at = now;
        let meta = meta.clone();
        Self::trim(&mut entries);
        if let Err(e) = self.save_entries(&entries) {
            tracing::warn!("会话索引写入失败（{uid}）：{e}");
        }
        meta
    }

    /// 只读获取（不创建、不刷新）。供面板 / 运维查询单用户会话。
    #[allow(dead_code)]
    pub fn get(&self, user_id: &str) -> Option<SessionMeta> {
        self.entries().remove(&Self::normalize_user_id(user_id))
    }

    /// 列出全部会话，按最近活跃倒序。供面板 / 运维查看会话列表。
    #[allow(dead_code)]
    pub fn list(&self) -> Vec<SessionMeta> {
        let mut list: Vec<SessionMeta> = self.entries().into_values().collect();
        list.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        list
    }

    /// 记录一轮成功交互（回复已发出后调用），用于统计与最近活跃时间。
    pub fn record_turn(&self, user_id: &str, username: &str, channel: &str, persona: &str) -> Result<()> {
        let uid = Self::normalize_user_id(user_id);
        let now = now_str();
        let mut entries = self.entries();
        let meta = entries.entry(uid.clone()).or_insert_with(|| SessionMeta {
            user_id: uid.clone(),
            created_at: now.clone(),
            ..Default::default()
        });
        if meta.user_id.is_empty() {
            meta.user_id = uid;
        }
        if meta.created_at.is_empty() {
            meta.created_at = now.clone();
        }
        if !username.trim().is_empty() {
            meta.username = username.trim().to_string();
        }
        meta.updated_at = now;
        meta.turns = meta.turns.saturating_add(1);
        if !channel.is_empty() {
            meta.channel = channel.to_string();
        }
        if !persona.is_empty() {
            meta.persona = persona.to_string();
        }
        Self::trim(&mut entries);
        self.save_entries(&entries)
    }

    // ---------- 上下文组装 ----------
    /// 读取某用户的独立会话正文：会话档案 + 该用户历史对话 + 用户档案。
    /// 只按 `user_id` 过滤，没有任何跨用户检索分支；无历史且无档案时返回空串。
    pub async fn render_user_context(
        &self,
        memory: &MemoryStore,
        docs: &[MemoryDoc],
        user_id: &str,
        username: &str,
    ) -> String {
        let uid = Self::normalize_user_id(user_id);
        let meta = self.ensure(&uid, username);
        let cap: usize = {
            let n = memory.config.read().unwrap().get_i64("CONTEXT_MAX_CHARS");
            if n > 0 { n as usize } else { 300_000 }
        };
        let history = memory.get_user_session_context(docs, &uid, cap).await;
        let profile = memory.get_user_profile_context(&uid);
        if history.is_empty() && profile.is_empty() {
            return String::new();
        }
        let name = if meta.username.is_empty() { format!("UID {uid}") } else { meta.username.clone() };
        let mut parts: Vec<String> = vec![format!(
            "【会话档案】用户：{name}（UID {uid}）｜建立：{}｜最近活跃：{}｜已记录 {} 轮",
            meta.created_at.if_empty_or("未知"),
            meta.updated_at.if_empty_or("未知"),
            meta.turns
        )];
        if !history.is_empty() {
            parts.push(format!("【本用户历史对话（时间正序，仅属于该用户）】\n{}", history.join("\n")));
        }
        if !profile.is_empty() {
            parts.push(profile);
        }
        parts.join("\n")
    }

    // ---------- 清理 ----------
    /// 清理单个用户的会话：对话正文 + 用户档案 + 索引项，清理前备份。
    /// 其他用户的会话、永久记忆、好感度、性格演化均不受影响。返回删除的对话条数。
    /// 供面板 / 运维按用户单独清理（全局清空仍走 [`crate::memory::clear_temp_memory`]）。
    #[allow(dead_code)]
    pub fn clear_user(
        &self,
        memory: &MemoryStore,
        docs: &mut Vec<MemoryDoc>,
        user_id: &str,
    ) -> Result<usize> {
        let uid = Self::normalize_user_id(user_id);
        let memory_file = crate::util::data_path(&self.base_dir, "memory.json");
        let profile_file = crate::util::data_path(&self.base_dir, "user_profiles.json");

        let before = docs.len();
        docs.retain(|m| m.user_id != uid);
        let removed = before - docs.len();
        if removed > 0 {
            backup_data_file(&memory_file, "sessionclear");
            memory.save(docs)?;
        }

        let mut profiles: Value = load_json(&profile_file, json!({}));
        let profile_removed = profiles
            .as_object_mut()
            .map(|obj| obj.remove(&uid).is_some())
            .unwrap_or(false);
        if profile_removed {
            backup_data_file(&profile_file, "sessionclear");
            save_json(&profile_file, &profiles)?;
        }

        let mut entries = self.entries();
        if entries.remove(&uid).is_some() {
            self.save_entries(&entries)?;
        }
        tracing::info!("已清理用户 {uid} 的会话：对话 {removed} 条，档案{}", if profile_removed { "已删除" } else { "无" });
        Ok(removed)
    }

    /// 以 memory.json 为准重建索引：补出历史用户、修正轮次与活跃时间，
    /// 丢弃已无正文的条目（保留既有会话的 `created_at` / `persona`）。返回索引条目数。
    pub fn rebuild_from_memory(&self, memory: &[MemoryDoc]) -> usize {
        let mut existing = self.entries();
        let mut grouped: BTreeMap<String, Vec<&MemoryDoc>> = BTreeMap::new();
        for doc in memory {
            if doc.user_id.is_empty() || doc.user_id == LOCAL_USER_ID || doc.thread_id == LOCAL_USER_ID {
                continue;
            }
            grouped.entry(doc.user_id.clone()).or_default().push(doc);
        }
        let mut fresh: BTreeMap<String, SessionMeta> = BTreeMap::new();
        for (uid, mut docs) in grouped {
            docs.sort_by_key(|m| m.time.clone());
            let mut meta = existing.remove(&uid).unwrap_or_default();
            meta.user_id = uid.clone();
            if meta.username.is_empty() {
                if let Some(last) = docs.last() {
                    if let Some(name) = username_from_doc(last) {
                        meta.username = name;
                    }
                }
            }
            if meta.created_at.is_empty() {
                meta.created_at = docs.first().map(|m| m.time.clone()).unwrap_or_default();
            }
            meta.updated_at = docs.last().map(|m| m.time.clone()).unwrap_or_default();
            meta.turns = docs.len();
            if meta.channel.is_empty() {
                if let Some(last) = docs.last() {
                    meta.channel = channel_of(last);
                }
            }
            fresh.insert(uid, meta);
        }
        Self::trim(&mut fresh);
        let count = fresh.len();
        if let Err(e) = self.save_entries(&fresh) {
            tracing::warn!("会话索引重建失败：{e}");
        }
        count
    }
}

/// 从记忆正文里取回昵称：`[时间] 用户{uid}({昵称})说：...`。
fn username_from_doc(doc: &MemoryDoc) -> Option<String> {
    let marker = format!("用户{}(", doc.user_id);
    let start = doc.text.find(&marker)? + marker.len();
    let rest = &doc.text[start..];
    let end = rest.find(')')?;
    let name = rest[..end].trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

/// 从记录推断渠道：私信记录的 thread_id 为 `dm:{mid}` / `private:{mid}`。
fn channel_of(doc: &MemoryDoc) -> String {
    if doc.thread_id.starts_with("dm:") || doc.thread_id.starts_with("private:") {
        "private".to_string()
    } else {
        "comment".to_string()
    }
}

/// 空串回退（与 bot.rs 的 IfEmpty 语义一致，避免跨模块依赖）。
trait IfEmpty {
    fn if_empty_or(&self, default: &str) -> String;
}
impl IfEmpty for String {
    fn if_empty_or(&self, default: &str) -> String {
        if self.is_empty() { default.to_string() } else { self.clone() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::llm::LlmClient;
    use std::sync::{Arc, RwLock};

    struct Fixture {
        dir: std::path::PathBuf,
        store: SessionStore,
        memory: MemoryStore,
        docs: Vec<MemoryDoc>,
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn fixture(tag: &str) -> Fixture {
        let dir = std::env::temp_dir().join(format!(
            "session_test_{tag}_{}_{}",
            std::process::id(),
            crate::util::now_unix()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let cfg = Config::load(dir.join("config.json")).unwrap();
        let cfg = Arc::new(RwLock::new(cfg));
        let llm = Arc::new(LlmClient::new(cfg.clone(), dir.to_str().unwrap()));
        Fixture {
            store: SessionStore::new(dir.to_str().unwrap()),
            memory: MemoryStore::new(cfg, llm, dir.to_str().unwrap()),
            docs: Vec::new(),
            dir,
        }
    }

    #[test]
    fn normalize_blank_user_id_is_separate_bucket() {
        assert_eq!(SessionStore::normalize_user_id(""), UNKNOWN_USER_ID);
        assert_eq!(SessionStore::normalize_user_id("  \t "), UNKNOWN_USER_ID);
        assert_eq!(SessionStore::normalize_user_id(" 12345 "), "12345");
        assert_eq!(SessionStore::normalize_user_id("123\u{0007}"), "123");
    }

    #[tokio::test]
    async fn sessions_are_isolated_and_persona_is_shared() {
        let mut fx = fixture("isolated");
        fx.memory
            .save_record(&mut fx.docs, "1", "t1", "uA", "甲", "今天一起看了星之卡比的新视频", "那个新作确实可爱喵")
            .await
            .unwrap();
        fx.memory
            .save_record(&mut fx.docs, "2", "t2", "uB", "乙", "我是另一个人的完全无关内容", "嗯嗯")
            .await
            .unwrap();

        let persona = "【人设（固定设定）】\n测试人格\n\n【说话风格】\n简洁";
        let user_a = fx.store.render_user_context(&fx.memory, &fx.docs, "uA", "甲").await;
        let user_b = fx.store.render_user_context(&fx.memory, &fx.docs, "uB", "乙").await;
        let ctx_a = ReplyContext::new("uA", "甲", persona, &user_a);
        let ctx_b = ReplyContext::new("uB", "乙", persona, &user_b);

        assert!(ctx_a.render().contains("星之卡比"), "A 会话应包含自己的历史");
        assert!(!ctx_a.render().contains("另一个人"), "A 会话串入了 B 的内容");
        assert!(ctx_b.render().contains("另一个人"), "B 会话应包含自己的历史");
        assert!(!ctx_b.render().contains("星之卡比"), "B 会话串入了 A 的内容");
        // 人设对每个用户完全一致，且排在会话之前
        assert_eq!(ctx_a.persona_section(), ctx_b.persona_section());
        let rendered = ctx_a.render();
        assert!(rendered.starts_with("【人设"), "人设必须稳定排在最前");
        assert!(
            rendered.find("【人设").unwrap() < rendered.find("本用户会话").unwrap(),
            "人设应先于用户会话注入"
        );
        // 人设不写进任何用户会话存储
        assert!(!user_a.contains("测试人格") && !user_b.contains("测试人格"));
        // 两个用户各有一份会话索引
        assert_eq!(fx.store.list().len(), 2);
    }

    #[tokio::test]
    async fn clear_one_user_keeps_others() {
        let mut fx = fixture("clear");
        fx.memory
            .save_record(&mut fx.docs, "1", "t1", "uA", "甲", "A 的消息", "A 的回复")
            .await
            .unwrap();
        fx.memory
            .save_record(&mut fx.docs, "2", "t2", "uB", "乙", "B 的消息", "B 的回复")
            .await
            .unwrap();
        fx.memory.update_user_profile("uA", Some("A 的印象"), None, None).unwrap();
        fx.memory.update_user_profile("uB", Some("B 的印象"), None, None).unwrap();
        // 会话索引在第一条入站消息到达时建立（对应 bot 的 build_reply_context → ensure）
        fx.store.ensure("uA", "甲");
        fx.store.ensure("uB", "乙");

        let removed = fx.store.clear_user(&fx.memory, &mut fx.docs, "uA").unwrap();
        assert_eq!(removed, 1);
        assert_eq!(fx.docs.len(), 1);
        assert_eq!(fx.docs[0].user_id, "uB");
        assert!(fx.store.get("uA").is_none(), "A 的会话索引应删除");
        assert!(fx.store.get("uB").is_some(), "B 的会话索引必须保留");
        assert!(fx.memory.get_user_profile_context("uA").is_empty(), "A 的档案应删除");
        assert!(fx.memory.get_user_profile_context("uB").contains("B 的印象"), "B 的档案必须保留");
        // 清理结果已落盘
        let reloaded = fx.memory.load();
        assert_eq!(reloaded.len(), 1);
        assert_eq!(reloaded[0].user_id, "uB");
    }

    #[tokio::test]
    async fn ensure_creates_once_and_records_turns() {
        let fx = fixture("lifecycle");
        let first = fx.store.ensure("uA", "甲");
        assert_eq!(first.turns, 0);
        assert_eq!(fx.store.list().len(), 1);

        let second = fx.store.ensure("uA", "甲改名");
        assert_eq!(second.created_at, first.created_at, "重复 ensure 不得重置建立时间");
        assert_eq!(second.username, "甲改名");
        assert_eq!(fx.store.list().len(), 1, "同一用户只能有一份会话");

        fx.store.record_turn("uA", "甲改名", "private", "default").unwrap();
        let meta = fx.store.get("uA").unwrap();
        assert_eq!(meta.turns, 1);
        assert_eq!(meta.channel, "private");
        assert_eq!(meta.persona, "default");
        assert!(fx.store.get("uB").is_none(), "不得为未交互用户凭空建会话");
    }

    #[tokio::test]
    async fn rebuild_index_from_memory_keeps_history_migratable() {
        let mut fx = fixture("rebuild");
        fx.memory
            .save_record(&mut fx.docs, "1", "t1", "uA", "甲", "评论内容", "评论回复")
            .await
            .unwrap();
        fx.docs.push(MemoryDoc {
            rpid: "dm_1".into(),
            thread_id: "dm:uB".into(),
            user_id: "uB".into(),
            time: now_str(),
            text: "[时间] 用户uB(乙)说：私信内容 | 回复：私信回复".into(),
            embedding: Vec::new(),
        });
        fx.memory.save_local_memory(&mut fx.docs, "面板内容", "面板回复");

        let count = fx.store.rebuild_from_memory(&fx.docs);
        assert_eq!(count, 2, "本地面板对话不应计入 B 站用户会话");
        let a = fx.store.get("uA").unwrap();
        assert_eq!(a.username, "甲");
        assert_eq!(a.turns, 1);
        let b = fx.store.get("uB").unwrap();
        assert_eq!(b.username, "乙");
        assert_eq!(b.channel, "private");
        assert!(fx.store.get(LOCAL_USER_ID).is_none());
    }
}
