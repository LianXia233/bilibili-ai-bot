//! LLM 客户端：OpenAI 兼容 chat/completions，四类场景（chat/vision/search/image），
//! 候选模型回退、推理截断抬升预算、TPM 限流、成本记账。

use crate::bili_api::TpmWindow;
use crate::config::Config;
use crate::error::{AppError, Result};
use crate::util::{save_json, v_i64};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::Duration;

#[derive(Debug, Clone, Default)]
pub struct LlmResult {
    pub text: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub model: String,
    pub finish_reason: String,
}

pub struct LlmClient {
    pub http: reqwest::Client,
    pub config: Arc<RwLock<Config>>,
    pub rate: TpmWindow,
    pub cost_log: PathBuf,
}

/// 触发联网搜索的关键词（与 ai.py 对齐）。
pub const SEARCH_KEYWORDS: &[&str] = &[
    "最近", "最新", "今天", "昨天", "现在", "目前", "当前", "新闻", "热搜", "热门",
    "发生了什么", "怎么回事", "什么时候", "多少钱", "价格", "股价", "天气", "谁赢了",
    "比分", "比赛", "选举", "发布", "上映", "更新", "版本", "公告", "通知", "真的吗",
    "是真的吗", "听说", "搜一下", "查一下", "帮我查",
];

pub fn needs_search(text: &str) -> bool {
    if SEARCH_KEYWORDS.iter().any(|kw| text.contains(kw)) {
        return true;
    }
    if text.contains('?') || text.contains('？') {
        for p in ["多少", "几点", "哪里", "什么时候", "谁是", "有没有"] {
            if text.contains(p) {
                return true;
            }
        }
    }
    false
}

impl LlmClient {
    pub fn new(config: Arc<RwLock<Config>>, base_dir: &str) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(180))
            .build()
            .expect("llm client 构建失败");
        LlmClient {
            http,
            config,
            rate: TpmWindow::new(),
            cost_log: crate::util::data_path(base_dir, "cost_log.json"),
        }
    }

    /// 核心调用：按场景取候选模型依次尝试，任一返回非空正文即采纳。
    /// 推理模型被截断（finish_reason=length 且正文为空）时按 MAX_TOKENS_REASONING_FLOOR 抬升重试一次。
    pub async fn complete(
        &self,
        scene: &str,
        messages: Value,
        max_tokens: i64,
    ) -> Result<LlmResult> {
        let cfg = self.config.read().unwrap().clone();
        let (base_url, api_key, candidates) = cfg.model_of(scene);
        if base_url.is_empty() || api_key.is_empty() || candidates.is_empty() {
            return Err(AppError::Llm(format!("场景 {scene} 未配置模型（base_url/api_key/model）")));
        }
        let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
        let tpm = cfg.rate_limit_tpm(scene);
        let reasoning_floor = cfg.get_i64("MAX_TOKENS_REASONING_FLOOR");

        let mut last_err = String::new();
        for model in candidates {
            let est = (max_tokens as f64 * 0.6) as i64 + 64;
            let wait = self.rate.record(est, tpm);
            if wait > 0 {
                tracing::info!("TPM 限流，等待 {wait}s（场景 {scene}）");
                tokio::time::sleep(Duration::from_secs(wait)).await;
                self.rate.record(est, tpm);
            }
            match self.call_once(&url, &api_key, &model, &messages, max_tokens).await {
                Ok(mut r) => {
                    // 截断抬升：正文为空且因 length 截断，用推理下限预算重试
                    if r.text.is_empty() && r.finish_reason == "length" && reasoning_floor > max_tokens {
                        tracing::info!("模型 {model} 被截断，抬升预算到 {reasoning_floor} 重试");
                        match self.call_once(&url, &api_key, &model, &messages, reasoning_floor).await {
                            Ok(r2) => r = r2,
                            Err(_) => {}
                        }
                    }
                    if !r.text.is_empty() {
                        return Ok(r);
                    }
                    last_err = format!("模型 {model} 返回空正文");
                }
                Err(e) => {
                    last_err = format!("模型 {model}: {e}");
                    tracing::warn!("{last_err}");
                }
            }
        }
        Err(AppError::Llm(format!("候选模型全部失败：{last_err}")))
    }

    async fn call_once(
        &self,
        url: &str,
        api_key: &str,
        model: &str,
        messages: &Value,
        max_tokens: i64,
    ) -> Result<LlmResult> {
        let payload = json!({
            "model": model,
            "messages": messages,
            "max_tokens": max_tokens,
        });
        let resp = self
            .http
            .post(url)
            .header("Authorization", format!("Bearer {api_key}"))
            .json(&payload)
            .send()
            .await?;
        let status = resp.status();
        let body: Value = resp.json().await?;
        if !status.is_success() {
            let msg = body.get("error").map(|e| e.to_string()).unwrap_or_else(|| format!("HTTP {status}"));
            return Err(AppError::Llm(msg));
        }
        let choice = &body["choices"][0];
        let text = choice["message"]["content"].as_str().unwrap_or("").trim().to_string();
        let finish_reason = choice["finish_reason"].as_str().unwrap_or("").to_string();
        let usage = &body["usage"];
        Ok(LlmResult {
            text,
            input_tokens: v_i64(&usage["prompt_tokens"], 0),
            output_tokens: v_i64(&usage["completion_tokens"], 0),
            model: body["model"].as_str().unwrap_or(model).to_string(),
            finish_reason,
        })
    }

    /// 纯文本对话。
    pub async fn chat(&self, scene: &str, prompt: &str, max_tokens: i64) -> Result<LlmResult> {
        let messages = json!([{"role": "user", "content": prompt}]);
        self.complete(scene, messages, max_tokens).await
    }

    /// 带图片的多模态对话。
    pub async fn chat_with_images(
        &self,
        scene: &str,
        text: &str,
        images: Vec<(String, String)>,
        max_tokens: i64,
    ) -> Result<LlmResult> {
        let mut content: Vec<Value> = Vec::new();
        for (name, data_url) in images {
            let _ = name;
            content.push(json!({"type": "image_url", "image_url": {"url": data_url}}));
        }
        if !text.is_empty() {
            content.push(json!({"type": "text", "text": text}));
        }
        if content.is_empty() {
            content.push(json!({"type": "text", "text": ""}));
        }
        let messages = json!([{"role": "user", "content": Value::Array(content)}]);
        self.complete(scene, messages, max_tokens).await
    }

    /// 联网搜索：走 search 场景模型（需支持联网）。
    pub async fn web_search(&self, query: &str) -> String {
        let cfg = self.config.read().unwrap().clone();
        let prefix = cfg.get_str("PROMPT_SEARCH_PREFIX");
        let prompt = if prefix.is_empty() {
            format!("请搜索并简要回答（200字以内，中文）：{query}")
        } else {
            format!("{prefix}{query}")
        };
        let max_tokens = cfg.max_tokens_of("search");
        match self.chat("search", &prompt, max_tokens).await {
            Ok(r) => {
                if r.text.is_empty() {
                    tracing::warn!("联网搜索失败：候选通道都没返回内容");
                    return String::new();
                }
                log_cost(&self.config, "联网搜索", r.input_tokens, r.output_tokens, &r.model, &self.cost_log);
                r.text
            }
            Err(e) => {
                tracing::warn!("联网搜索失败: {e}");
                String::new()
            }
        }
    }

    /// 记忆压缩等结构化任务（与 chat 相同通道，scene 区分预算）。
    pub async fn compress(&self, scene: &str, prompt: &str, max_tokens: i64) -> Result<String> {
        let r = self.chat(scene, prompt, max_tokens).await?;
        Ok(r.text)
    }
}

/// 成本记账（data/cost_log.json），结构与 Python log_cost 对齐：
/// day-keyed 对象 {日期: {total, calls, input_tokens, output_tokens, details, models}}。
pub fn log_cost(
    config: &Arc<RwLock<Config>>,
    source: &str,
    input_tokens: i64,
    output_tokens: i64,
    model: &str,
    cost_log_path: &std::path::Path,
) {
    let cfg = config.read().unwrap().clone();
    let price_key = |prefix: &str, out: bool| -> f64 {
        let k = if out {
            format!("PRICE_{}_OUTPUT", prefix)
        } else {
            format!("PRICE_{}_INPUT", prefix)
        };
        cfg.get_f64(&k)
    };
    // 按来源判断计费类别（与 Python resolve_model_price 简化对齐）
    let (input_price, output_price) = if source.contains("识别") || source.contains("视频") || source.contains("图片理解") {
        (price_key("VISION", false), price_key("VISION", true))
    } else if source.contains("搜索") {
        (price_key("SEARCH", false), price_key("SEARCH", true))
    } else if source.contains("生图") || source.contains("图片生成") {
        (price_key("IMAGE", false), price_key("IMAGE", true))
    } else {
        (price_key("CHAT", false), price_key("CHAT", true))
    };
    let cost = input_tokens as f64 / 1_000_000.0 * input_price + output_tokens as f64 / 1_000_000.0 * output_price;

    let path = cost_log_path;
    let mut log: Value = crate::util::load_json(path, json!({}));
    if !log.is_object() {
        log = json!({});
    }
    let today = crate::util::today_str();
    if !log.get(&today).map(|v| v.is_object()).unwrap_or(false) {
        log[&today] = json!({"total": 0.0, "calls": 0, "input_tokens": 0, "output_tokens": 0, "details": [], "models": {}});
    }
    let day = log.as_object_mut().unwrap().get_mut(&today).unwrap();
    let day_obj = day.as_object_mut().unwrap();
    for k in ["total", "calls", "input_tokens", "output_tokens"] {
        if !day_obj.contains_key(k) {
            day_obj.insert(k.to_string(), json!(0));
        }
    }
    if !day_obj.contains_key("details") {
        day_obj.insert("details".to_string(), json!([]));
    }
    if !day_obj.contains_key("models") {
        day_obj.insert("models".to_string(), json!({}));
    }
    let prev_total = day_obj.get("total").and_then(|v| v.as_f64()).unwrap_or(0.0);
    day_obj.insert("total".to_string(), json!(((prev_total + cost) * 1e6).round() / 1e6));
    day_obj.insert("calls".to_string(), json!(day_obj.get("calls").and_then(|v| v.as_i64()).unwrap_or(0) + 1));
    day_obj.insert("input_tokens".to_string(), json!(day_obj.get("input_tokens").and_then(|v| v.as_i64()).unwrap_or(0) + input_tokens));
    day_obj.insert("output_tokens".to_string(), json!(day_obj.get("output_tokens").and_then(|v| v.as_i64()).unwrap_or(0) + output_tokens));
    let time_hm: String = crate::util::now_str().chars().skip(11).take(5).collect();
    if let Some(details) = day_obj.get_mut("details").and_then(|d| d.as_array_mut()) {
        details.push(json!({
            "time": time_hm,
            "source": source,
            "in": input_tokens,
            "out": output_tokens,
            "cost": (cost * 1e6).round() / 1e6,
        }));
    }
    let model_key = if model.contains('/') { model.to_string() } else { source.to_string() };
    if let Some(models) = day_obj.get_mut("models").and_then(|ms| ms.as_object_mut()) {
        let entry = models
            .entry(model_key)
            .or_insert_with(|| json!({"calls": 0, "input_tokens": 0, "output_tokens": 0, "cost": 0.0}));
        if let Some(eo) = entry.as_object_mut() {
            eo.insert("calls".to_string(), json!(eo.get("calls").and_then(|v| v.as_i64()).unwrap_or(0) + 1));
            eo.insert("input_tokens".to_string(), json!(eo.get("input_tokens").and_then(|v| v.as_i64()).unwrap_or(0) + input_tokens));
            eo.insert("output_tokens".to_string(), json!(eo.get("output_tokens").and_then(|v| v.as_i64()).unwrap_or(0) + output_tokens));
            let prev_mcost = eo.get("cost").and_then(|v| v.as_f64()).unwrap_or(0.0);
            eo.insert("cost".to_string(), json!(((prev_mcost + cost) * 1e6).round() / 1e6));
        }
    }
    // 保留最近 30 天
    if let Some(obj) = log.as_object_mut() {
        let mut keys: Vec<String> = obj.keys().cloned().collect();
        keys.sort();
        if keys.len() > 30 {
            for k in keys.iter().take(keys.len() - 30) {
                obj.remove(k);
            }
        }
    }
    let _ = save_json(path, &log);
}

/// 字符串安全截断（日志用）。
pub fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        s.chars().take(n).collect()
    }
}

// 保留 v_str 引用避免未使用警告
#[allow(dead_code)]
fn _keep(v: &Value) -> String {
    crate::util::v_str(v, "")
}
