//! 动态发布模块：AI 生成文案 + 生图 + B站图床上传 + 发布动态。
//! 与 dynamic.py 对齐（OpenAI 兼容 modalities:["image"] 生图）。

use crate::bot::Bot;
use crate::error::Result;
use crate::llm::truncate;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::process::Command;

/// 单次动态发布（每日调度触发）。
pub async fn run_once(bot: &Arc<Bot>) -> Result<()> {
    let cfg = bot.config.read().unwrap().clone();
    if !cfg.get_bool("ENABLE_DYNAMIC") {
        return Ok(());
    }
    tracing::info!("开始生成动态");

    let content = match generate_dynamic_content(bot).await {
        Some(c) if !c.is_empty() => c,
        _ => {
            tracing::warn!("动态文案生成失败，跳过本轮");
            return Ok(());
        }
    };

    // 生图（失败则发纯文字动态）
    let mut image_local: Option<PathBuf> = None;
    if cfg.get_bool("DYNAMIC_GENERATE_IMAGE") {
        let prompt = format!("为以下动态配一张氛围感插画，风格与内容匹配：\n{content}");
        match generate_image(bot, &prompt).await {
            Some(path) => image_local = Some(path),
            None => tracing::warn!("生图失败，改为纯文字动态"),
        }
    }

    let success = if let Some(local) = &image_local {
        match upload_image_to_bilibili(bot, local).await {
            Some(img_info) => post_dynamic_with_image(bot, &content, &img_info).await,
            None => post_dynamic_text(bot, &content).await,
        }
    } else {
        post_dynamic_text(bot, &content).await
    };

    if let Some(local) = &image_local {
        let _ = std::fs::remove_file(local);
    }

    if success {
        tracing::info!("动态发布成功");
        let mut log: Value = crate::util::load_json(&crate::util::data_path(&bot.base_dir, "dynamic_log.json"), json!([]));
        if let Some(arr) = log.as_array_mut() {
            arr.insert(0, json!({"content": content, "time": crate::util::now_str()}));
            arr.truncate(50);
        }
        let _ = crate::util::save_json(&crate::util::data_path(&bot.base_dir, "dynamic_log.json"), &log);
    } else {
        tracing::warn!("动态发布失败");
    }
    Ok(())
}

async fn generate_dynamic_content(bot: &Bot) -> Option<String> {
    let cfg = bot.config.read().unwrap().clone();
    let bot_name = cfg.get_str("BOT_NAME");
    // 与 Python dynamic.py / WebUI 面板键名对齐（PROMPT_DYNAMIC），面板上配置的自定义文案才能真正生效
    let custom = cfg.get_str("PROMPT_DYNAMIC");
    let prompt = if custom.is_empty() {
        format!(
            "你是{bot_name}，一个有独立人格的B站UP主。请写一条今天想发的动态（40-120字），自然口语化，有个人想法，不要营销腔。直接输出内容。"
        )
    } else {
        custom.clone()
    };
    let max_tok = cfg.max_tokens_of("dynamic");
    match bot.llm.compress("chat", &prompt, max_tok).await {
        Ok(c) if !c.is_empty() => Some(c),
        _ => None,
    }
}

/// OpenAI 兼容生图：请求 modalities:["image"]，解析响应中的图片。
async fn generate_image(bot: &Bot, prompt: &str) -> Option<PathBuf> {
    let candidates = {
        let cfg = bot.config.read().unwrap().clone();
        cfg.model_of("image")
    };
    let mut last_err = String::new();
    for (base_url, api_key, model) in candidates {
        let url = format!("{base_url}/chat/completions");
        let body = json!({
            "model": model,
            "messages": [{"role": "user", "content": prompt}],
            "modalities": ["text", "image"],
            "n": 1,
        });
        let client = reqwest::Client::new();
        let resp = match client
            .post(&url)
            .bearer_auth(&api_key)
            .json(&body)
            .timeout(std::time::Duration::from_secs(120))
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                last_err = format!("{e}");
                continue;
            }
        };
        let v: Value = match resp.json().await {
            Ok(v) => v,
            Err(_) => continue,
        };
        let images = v["choices"][0]["message"]["images"].as_array().cloned().unwrap_or_default();
        if images.is_empty() {
            last_err = "响应中无 images 字段".into();
            continue;
        }
        // 解析第一个图片（可能是 b64 或 URL）
        let first = &images[0];
        let dir = PathBuf::from(&bot.base_dir).join("data").join("dynamic_tmp");
        std::fs::create_dir_all(&dir).ok();
        let out = dir.join(format!("dyn_{}.png", crate::util::now_unix()));
        let url_val = first.get("url").and_then(|u| u.as_str()).unwrap_or("");
        let b64_val = first.get("b64_json").and_then(|b| b.as_str()).unwrap_or("");
        if !b64_val.is_empty() {
            if let Ok(bytes) = crate::util::b64_decode(b64_val) {
                if std::fs::write(&out, &bytes).is_ok() {
                    return Some(out);
                }
            }
        } else if !url_val.is_empty() {
            let data = match client.get(url_val).send().await {
                Ok(r) => r.bytes().await.unwrap_or_default().to_vec(),
                Err(_) => Vec::new(),
            };
            if !data.is_empty() && std::fs::write(&out, &data).is_ok() {
                return Some(out);
            }
        }
        last_err = "图片解析失败".into();
    }
    tracing::warn!("生图失败：{}", last_err);
    None
}

/// B站图床上传（/x/dynamic/feed/draw/upload_bfs）。
async fn upload_image_to_bilibili(bot: &Bot, image: &PathBuf) -> Option<Value> {
    let cfg = bot.config.read().unwrap().clone();
    let csrf = cfg.get_str("BILI_JCT");
    let bytes = std::fs::read(image).ok()?;
    let mime = if image.extension().map(|e| e == "png").unwrap_or(false) { "image/png" } else { "image/jpeg" };
    let filename = image.file_name()?.to_str()?.to_string();
    let client = reqwest::Client::new();
    let part = reqwest::multipart::Part::bytes(bytes).file_name(filename).mime_str(mime).ok()?;
    let form = reqwest::multipart::Form::new()
        .part("file_up", part)
        .text("category", "daily")
        .text("csrf", csrf);
    let resp = client
        .post("https://api.bilibili.com/x/dynamic/feed/draw/upload_bfs")
        .header("User-Agent", crate::bili_api::UA)
        .header("Referer", "https://www.bilibili.com/")
        .header("Cookie", bot.bili.cookie_header())
        .multipart(form)
        .send()
        .await
        .ok()?;
    let v: Value = resp.json().await.ok()?;
    if v.get("code").and_then(|c| c.as_i64()) != Some(0) {
        tracing::warn!("图床失败：{}", v);
        return None;
    }
    Some(v)
}

async fn post_dynamic_text(bot: &Bot, text: &str) -> bool {
    let cfg = bot.config.read().unwrap().clone();
    let csrf = cfg.get_str("BILI_JCT");
    let client = reqwest::Client::new();
    let resp = client
        .post("https://api.bilibili.com/x/dynamic/feed/create/dyn")
        .header("User-Agent", crate::bili_api::UA)
        .header("Referer", "https://www.bilibili.com/")
        .header("Cookie", bot.bili.cookie_header())
        .form(&[("dynamic_id", ""), ("type", "4"), ("rid", "0"), ("content", text), ("csrf", csrf.as_str())])
        .send()
        .await;
    match resp {
        Ok(r) => match r.json::<Value>().await {
            Ok(v) => {
                let code = v.get("code").and_then(|c| c.as_i64());
                if code == Some(0) {
                    true
                } else {
                    tracing::warn!("发动态失败：{}", truncate(&v.to_string(), 200));
                    false
                }
            }
            Err(_) => false,
        },
        Err(e) => {
            tracing::warn!("发动态网络失败：{e}");
            false
        }
    }
}

async fn post_dynamic_with_image(bot: &Bot, text: &str, img_info: &Value) -> bool {
    let cfg = bot.config.read().unwrap().clone();
    let csrf = cfg.get_str("BILI_JCT");
    let img_url = img_info["data"]["image_url"].as_str().unwrap_or("");
    if img_url.is_empty() {
        return post_dynamic_text(bot, text).await;
    }
    let pictures = json!([{"img_src": img_url, "img_width": 0, "img_height": 0}]);
    let pictures_s = pictures.to_string();
    let client = reqwest::Client::new();
    let resp = client
        .post("https://api.bilibili.com/x/dynamic/feed/create/dyn")
        .header("User-Agent", crate::bili_api::UA)
        .header("Referer", "https://www.bilibili.com/")
        .header("Cookie", bot.bili.cookie_header())
        .form(&[
            ("dynamic_id", ""),
            ("type", "0"),
            ("rid", "0"),
            ("content", text),
            ("pictures", pictures_s.as_str()),
            ("csrf", csrf.as_str()),
        ])
        .send()
        .await;
    match resp {
        Ok(r) => match r.json::<Value>().await {
            Ok(v) => {
                let code = v.get("code").and_then(|c| c.as_i64());
                if code == Some(0) {
                    true
                } else {
                    tracing::warn!("发图文动态失败：{}", truncate(&v.to_string(), 200));
                    false
                }
            }
            Err(_) => false,
        },
        Err(e) => {
            tracing::warn!("发图文动态网络失败：{e}");
            false
        }
    }
}

#[allow(dead_code)]
fn _unused(_: &mut Command) {}
