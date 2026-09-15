//! 配置系统：JSON 文件读写、默认值、类型化取值、热重载。
//!
//! 与 Python config.py 对齐：config.json 位于仓库根目录，缺省字段用默认值补齐，
//! 面板可随时写入任意键；bot 任务每 300s 重读一次实现热更新。

use crate::error::{AppError, Result};
use crate::util::{v_bool, v_f64, v_i64, v_list, v_str, v_str_list};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock, RwLock};

/// 与 config.py `_DEFAULTS` 对齐的默认值表。
pub fn defaults() -> HashMap<&'static str, Value> {
    let mut m = HashMap::new();
    let mut set = |k: &'static str, v: Value| {
        m.insert(k, v);
    };
    // B站配置
    set("SESSDATA", json!(""));
    set("BILI_JCT", json!(""));
    set("DEDE_USER_ID", json!(""));
    set("OWNER_MID", json!(0));
    set("REFRESH_TOKEN", json!(""));
    // API 全局
    set("OR_API_KEY", json!(""));
    set("OR_BASE_URL", json!(""));
    set("OR_CHAT_MODEL", json!(""));
    set("OR_CHAT_MODEL_FALLBACK", json!(""));
    set("OR_CHAT_URL", json!(""));
    set("OR_CHAT_KEY", json!(""));
    set("OR_VISION_MODEL", json!(""));
    set("OR_VISION_MODEL_FALLBACK", json!(""));
    set("OR_VISION_URL", json!(""));
    set("OR_VISION_KEY", json!(""));
    set("OR_SEARCH_MODEL", json!(""));
    set("OR_SEARCH_MODEL_FALLBACK", json!(""));
    set("OR_SEARCH_URL", json!(""));
    set("OR_SEARCH_KEY", json!(""));
    set("OR_IMAGE_MODEL", json!(""));
    set("OR_IMAGE_MODEL_FALLBACK", json!(""));
    set("OR_IMAGE_URL", json!(""));
    set("OR_IMAGE_KEY", json!(""));
    set("SILICON_API_KEY", json!(""));
    set("EMBED_BASE_URL", json!(""));
    set("EMBED_MODEL", json!(""));
    // 计费价格
    for k in [
        "PRICE_CHAT_INPUT", "PRICE_CHAT_OUTPUT", "PRICE_VISION_INPUT", "PRICE_VISION_OUTPUT",
        "PRICE_SEARCH_INPUT", "PRICE_SEARCH_OUTPUT", "PRICE_IMAGE_INPUT", "PRICE_IMAGE_OUTPUT",
    ] {
        set(k, json!(0));
    }
    // 功能开关
    set("ENABLE_WEB_SEARCH", json!(true));
    set("ENABLE_PROACTIVE", json!(true));
    set("ENABLE_DYNAMIC", json!(true));
    set("ENABLE_PERSONALITY_EVOLUTION", json!(true));
    set("ENABLE_MOOD", json!(true));
    set("ENABLE_AFFECTION", json!(true));
    set("ENABLE_PRIVATE_MESSAGES", json!(false));
    set("PRIVATE_MESSAGE_AUTO_REPLY", json!(true));
    set("PRIVATE_MESSAGE_AUTO_BLOCK", json!(true));
    // 私信参数
    set("PRIVATE_MESSAGE_REPLY_SCOPE", json!("all"));
    set("PRIVATE_MESSAGE_REPLY_WHITELIST_UIDS", json!([]));
    set("PRIVATE_MESSAGE_BLOCK_WHITELIST_UIDS", json!([]));
    set("PRIVATE_MESSAGE_TRUSTED_DOMAINS", json!(["bilibili.com", "b23.tv"]));
    set("PRIVATE_MESSAGE_MAX_MESSAGE_AGE", json!(3600));
    set("PRIVATE_MESSAGE_MAX_PER_POLL", json!(3));
    // 评论参数
    set("AT_REPLY_MAX_AGE", json!(3600));
    // 主动行为开关
    set("PROACTIVE_LIKE", json!(true));
    set("PROACTIVE_COIN", json!(false));
    set("PROACTIVE_FAV", json!(true));
    set("PROACTIVE_FOLLOW", json!(true));
    set("PROACTIVE_COMMENT", json!(true));
    // 调度
    set("PROACTIVE_VIDEO_COUNT", json!(3));
    set("PROACTIVE_COMMENT_COUNT", json!(2));
    set("PROACTIVE_TIMES_COUNT", json!(2));
    set("DYNAMIC_ENABLED", json!(true));
    set("EVOLVE_HOUR", json!(1));
    set("SLEEP_START", json!(24));
    set("SLEEP_END", json!(0));
    set("ENABLE_SLEEP", json!(false));
    set("MOOD_WEIGHT", json!(0.5));
    // Bot 信息
    set("BOT_NAME", json!("Bot"));
    set("BOT_AVATAR", json!("🤖"));
    set("USER_AVATAR", json!("🌙"));
    set("BOT_WELCOME", json!("你好，有什么想聊的？"));
    set("BOT_SUBTITLE", json!("AI 聊天助手"));
    set("ACTIVE_PERSONA", json!("default"));
    set("PROACTIVE_FOLLOW_UIDS", json!([]));
    set("PREFERRED_TIDS", json!([17, 160, 211, 3, 13, 167, 321, 36, 129]));
    // 自定义提示词
    for k in [
        "PROMPT_DYNAMIC", "PROMPT_PROACTIVE_COMMENT", "PROMPT_VIDEO_EVALUATE",
        "PROMPT_PERSONALITY_EVOLVE", "PROMPT_SEARCH_PREFIX", "PROMPT_IMAGINE",
        "PROMPT_PRIVATE_MESSAGE",
    ] {
        set(k, json!(""));
    }
    set("DYNAMIC_TOPICS", json!([]));
    set("OWNER_NAME", json!(""));
    set("OWNER_BILI_NAME", json!(""));
    // Token 预算
    set("MAX_TOKENS_CHAT", json!(3000));
    set("MAX_TOKENS_REPLY", json!(3000));
    set("MAX_TOKENS_MEMORY_COMPRESS", json!(3000));
    set("MAX_TOKENS_THREAD_COMPRESS", json!(1000));
    set("MAX_TOKENS_EVOLVE", json!(3000));
    set("MAX_TOKENS_SEARCH", json!(3000));
    set("MAX_TOKENS_VISION", json!(4096));
    set("MAX_TOKENS_RECOGNIZE", json!(4096));
    set("MAX_TOKENS_DYNAMIC", json!(2000));
    set("MAX_TOKENS_PROACTIVE_COMMENT", json!(2000));
    set("MAX_TOKENS_IMAGE_PROMPT", json!(1000));
    set("MAX_TOKENS_REASONING_FLOOR", json!(6000));
    // 速率限制 TPM
    set("RATE_LIMIT_CHAT_TPM", json!(1000000));
    set("RATE_LIMIT_SEARCH_TPM", json!(1000000));
    set("RATE_LIMIT_VISION_TPM", json!(0));
    set("RATE_LIMIT_IMAGE_TPM", json!(0));
    // 面板
    set("CHAT_PASSWORD", json!(""));
    m
}

static DEFAULTS: OnceLock<HashMap<&'static str, Value>> = OnceLock::new();

pub fn default_of(key: &str) -> Value {
    let map = DEFAULTS.get_or_init(defaults);
    map.get(key).cloned().unwrap_or(Value::Null)
}

#[derive(Clone, Debug)]
pub struct Config {
    pub file: PathBuf,
    pub raw: Value,
}

impl Config {
    /// 从磁盘加载；文件缺失时用默认值并回写。
    pub fn load(file: PathBuf) -> Result<Self> {
        let mut cfg = Config { file, raw: json!({}) };
        cfg.reload()?;
        Ok(cfg)
    }

    pub fn reload(&mut self) -> Result<()> {
        match std::fs::read_to_string(&self.file) {
            Ok(text) => {
                let v: Value = serde_json::from_str(&text)
                    .map_err(|e| AppError::Config(format!("config.json 解析失败: {e}")))?;
                if v.is_object() {
                    self.raw = v;
                }
            }
            Err(_) => {
                // 文件不存在：写一份默认配置
                self.raw = json!({});
                self.save()?;
            }
        }
        Ok(())
    }

    pub fn save(&self) -> Result<()> {
        let mut merged = Map::new();
        let def = DEFAULTS.get_or_init(defaults);
        for (k, v) in def {
            merged.insert(k.to_string(), v.clone());
        }
        if let Some(obj) = self.raw.as_object() {
            for (k, v) in obj {
                merged.insert(k.clone(), v.clone());
            }
        }
        let v = Value::Object(merged);
        crate::util::save_json(&self.file, &v)
    }

    // ---------- 类型化取值（带默认值） ----------
    pub fn get(&self, key: &str) -> Value {
        self.raw.get(key).cloned().unwrap_or_else(|| default_of(key))
    }

    pub fn get_str(&self, key: &str) -> String {
        v_str(&self.get(key), "")
    }

    pub fn get_i64(&self, key: &str) -> i64 {
        v_i64(&self.get(key), 0)
    }

    pub fn get_f64(&self, key: &str) -> f64 {
        v_f64(&self.get(key), 0.0)
    }

    pub fn get_bool(&self, key: &str) -> bool {
        v_bool(&self.get(key), false)
    }

    pub fn get_list(&self, key: &str) -> Vec<Value> {
        v_list(&self.get(key))
    }

    pub fn get_str_list(&self, key: &str) -> Vec<String> {
        v_str_list(&self.get(key))
    }

    pub fn get_i64_list(&self, key: &str) -> Vec<i64> {
        self.get_list(key)
            .iter()
            .filter_map(|v| v.as_i64())
            .collect()
    }

    // ---------- 模型场景解析 ----------
    /// 场景相关 base_url / api_key / model 与候选回退模型。
    pub fn model_of(&self, scene: &str) -> (String, String, Vec<String>) {
        let (model_k, url_k, key_k, fb_k) = match scene {
            "chat" => ("OR_CHAT_MODEL", "OR_CHAT_URL", "OR_CHAT_KEY", "OR_CHAT_MODEL_FALLBACK"),
            "vision" => ("OR_VISION_MODEL", "OR_VISION_URL", "OR_VISION_KEY", "OR_VISION_MODEL_FALLBACK"),
            "search" => ("OR_SEARCH_MODEL", "OR_SEARCH_URL", "OR_SEARCH_KEY", "OR_SEARCH_MODEL_FALLBACK"),
            "image" => ("OR_IMAGE_MODEL", "OR_IMAGE_URL", "OR_IMAGE_KEY", "OR_IMAGE_MODEL_FALLBACK"),
            _ => ("OR_CHAT_MODEL", "OR_CHAT_URL", "OR_CHAT_KEY", "OR_CHAT_MODEL_FALLBACK"),
        };
        let base = self.get_str("OR_BASE_URL");
        let url = self.get_str(url_k);
        let base_url = if url.is_empty() { base } else { url };
        let key = {
            let k = self.get_str(key_k);
            if k.is_empty() {
                self.get_str("OR_API_KEY")
            } else {
                k
            }
        };
        let mut candidates = vec![self.get_str(model_k)];
        let fb = self.get_str(fb_k);
        if !fb.is_empty() {
            candidates.push(fb);
        }
        candidates.retain(|m| !m.is_empty());
        (base_url, key, candidates)
    }

    pub fn max_tokens_of(&self, scene: &str) -> i64 {
        let key = match scene {
            "chat" => "MAX_TOKENS_CHAT",
            "reply" => "MAX_TOKENS_REPLY",
            "memory_compress" => "MAX_TOKENS_MEMORY_COMPRESS",
            "thread_compress" => "MAX_TOKENS_THREAD_COMPRESS",
            "evolve" => "MAX_TOKENS_EVOLVE",
            "search" => "MAX_TOKENS_SEARCH",
            "vision" => "MAX_TOKENS_VISION",
            "recognize" => "MAX_TOKENS_RECOGNIZE",
            "dynamic" => "MAX_TOKENS_DYNAMIC",
            "proactive_comment" => "MAX_TOKENS_PROACTIVE_COMMENT",
            "image_prompt" => "MAX_TOKENS_IMAGE_PROMPT",
            _ => "MAX_TOKENS_CHAT",
        };
        self.get_i64(key)
    }

    pub fn rate_limit_tpm(&self, scene: &str) -> i64 {
        let key = match scene {
            "chat" | "reply" => "RATE_LIMIT_CHAT_TPM",
            "search" => "RATE_LIMIT_SEARCH_TPM",
            "vision" | "recognize" => "RATE_LIMIT_VISION_TPM",
            "image" => "RATE_LIMIT_IMAGE_TPM",
            _ => "RATE_LIMIT_CHAT_TPM",
        };
        self.get_i64(key)
    }
}

/// 更新配置（合并到 raw 并回写磁盘）。
/// 合并 patch 到共享配置并持久化（Python update_config 语义：读全局→合并→写盘）。
pub fn update_config(config: &Arc<RwLock<Config>>, patch: &Value) -> Result<()> {
    if let Some(obj) = patch.as_object() {
        let mut cfg = config.write().unwrap();
        let mut raw = cfg.raw.as_object().cloned().unwrap_or_default();
        for (k, v) in obj {
            raw.insert(k.clone(), v.clone());
        }
        cfg.raw = Value::Object(raw);
        cfg.save()?;
    }
    Ok(())
}
