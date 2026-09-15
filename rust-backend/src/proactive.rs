//! 主动行为模块：刷视频、AI 分析、点赞/投币/收藏/关注、主动评论。
//! 与 Proactive.py 对齐；yt-dlp / ffmpeg 通过 tokio 子进程调用。

use crate::bot::Bot;
use crate::error::{AppError, Result};
use crate::llm::{log_cost, truncate};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::process::Command;

const WBI_MIXIN_TAB: [usize; 64] = [
    46, 47, 18, 2, 53, 8, 23, 32, 15, 50, 10, 31, 58, 3, 45, 35, 27, 43, 5, 49, 33, 9, 42, 19,
    29, 28, 14, 39, 12, 38, 41, 13, 37, 48, 7, 16, 24, 55, 40, 61, 26, 17, 0, 1, 60, 51, 30, 4,
    22, 25, 54, 21, 56, 59, 6, 63, 57, 62, 11, 36, 20, 34, 44, 52,
];

/// 生成 yt-dlp cookies.txt（每次启动重建）。
fn generate_cookies_file(bot: &Bot) -> Result<PathBuf> {
    let cfg = bot.config.read().unwrap().clone();
    let path = PathBuf::from(&bot.base_dir).join("data").join("cookies.txt");
    let content = format!(
        "# Netscape HTTP Cookie File\n.bilibili.com\tTRUE\t/\tTRUE\t0\tSESSDATA\t{}\n.bilibili.com\tTRUE\t/\tTRUE\t0\tbili_jct\t{}\n.bilibili.com\tTRUE\t/\tTRUE\t0\tDedeUserID\t{}\n",
        cfg.get_str("SESSDATA"),
        cfg.get_str("BILI_JCT"),
        cfg.get_str("DEDE_USER_ID")
    );
    std::fs::write(&path, content)?;
    Ok(path)
}

/// WBI 签名（复用 Proactive.py 算法）。
async fn sign_wbi(bot: &Bot, params: &mut Vec<(String, String)>) {
    let (ik, sk) = get_wbi_keys_inline(bot).await;
    if ik.is_empty() || sk.is_empty() {
        return;
    }
    let mixin: String = (ik + &sk)
        .chars()
        .enumerate()
        .filter_map(|(i, c)| WBI_MIXIN_TAB.get(i).map(|_| c))
        .take(32)
        .collect();
    let wts = crate::util::now_unix();
    params.push(("wts".into(), wts.to_string()));
    let mut filtered: Vec<(String, String)> = params
        .iter()
        .map(|(k, v)| {
            let cleaned: String = v.chars().filter(|c| !"!'()*".contains(*c)).collect();
            (k.clone(), cleaned)
        })
        .collect();
    filtered.sort();
    let query = filtered
        .iter()
        .map(|(k, v)| format!("{k}={}", crate::bili_api::urlencode(v)))
        .collect::<Vec<_>>()
        .join("&");
    let w_rid = crate::util::md5_hex(&format!("{query}{mixin}"));
    params.push(("w_rid".into(), w_rid));
}

async fn get_wbi_keys_inline(bot: &Bot) -> (String, String) {
    let url = "https://api.bilibili.com/x/web-interface/nav";
    let resp = bot.bili.http.get(url).send().await;
    if let Ok(r) = resp {
        if let Ok(v) = r.json::<Value>().await {
            if v.get("code").and_then(|c| c.as_i64()) == Some(0) {
                let img_url = v["data"]["wbi_img"]["img_url"].as_str().unwrap_or("");
                let sub_url = v["data"]["wbi_img"]["sub_url"].as_str().unwrap_or("");
                let ik = img_url.rsplit('/').next().unwrap_or("").split('.').next().unwrap_or("").to_string();
                let sk = sub_url.rsplit('/').next().unwrap_or("").split('.').next().unwrap_or("").to_string();
                return (ik, sk);
            }
        }
    }
    (String::new(), String::new())
}

/// 主动行为单次执行（每日调度触发）。
pub async fn run_once(bot: &Arc<Bot>) -> Result<()> {
    let cfg = bot.config.read().unwrap().clone();
    if !cfg.get_bool("ENABLE_PROACTIVE") {
        return Ok(());
    }
    tracing::info!("主动行为开始");
    let _cookies = generate_cookies_file(bot)?;

    // 1. 收集候选视频：关注的 UP 主最新视频 + 热门视频
    let mut candidates: Vec<Value> = Vec::new();
    for uid in cfg.get_i64_list("PROACTIVE_FOLLOW_UIDS").iter().take(20) {
        if let Some(v) = get_up_latest_video(bot, *uid).await {
            candidates.push(v);
        }
    }
    for tid in cfg.get_i64_list("PREFERRED_TIDS").iter().take(4) {
        if let Some(mut list) = get_hot_videos_by_tid(bot, *tid).await {
            candidates.append(&mut list);
        }
    }
    if candidates.is_empty() {
        tracing::warn!("没有候选视频，跳过本轮");
        return Ok(());
    }
    // 去重
    let mut seen = std::collections::HashSet::new();
    candidates.retain(|v| {
        let aid = v.get("aid").map(|a| a.to_string()).unwrap_or_default();
        seen.insert(aid)
    });
    candidates.truncate(cfg.get_i64("PROACTIVE_VIDEO_COUNT").max(1) as usize);

    // 2. 逐个处理
    let comment_count = cfg.get_i64("PROACTIVE_COMMENT_COUNT").max(0);
    let mut comments_sent = 0i64;
    for video in &candidates {
        let aid = video.get("aid").and_then(|v| v.as_i64()).unwrap_or(0);
        let bvid = video.get("bvid").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let title = video.get("title").and_then(|v| v.as_str()).unwrap_or("未知").to_string();
        if aid == 0 || bvid.is_empty() {
            continue;
        }
        tracing::info!("处理视频《{}》（{}）", truncate(&title, 40), bvid);

        // 下载 + 抽帧（失败降级为仅元信息）
        let mut analysis = String::new();
        if let Some(local) = download_video(bot, &bvid).await {
            if let Some(frames) = extract_frames(bot, &local).await {
                analysis = analyze_video_frames(bot, &title, &frames).await;
            }
            let _ = std::fs::remove_file(&local);
        }
        if analysis.is_empty() {
            let desc = video.get("desc").and_then(|v| v.as_str()).unwrap_or("无").to_string();
            analysis = format!("视频《{title}》。简介：{}", desc.chars().take(100).collect::<String>());
        }

        // 3. 互动：点赞/投币/收藏/关注
        if cfg.get_bool("PROACTIVE_LIKE") {
            let _ = like_video(bot, aid).await;
        }
        if cfg.get_bool("PROACTIVE_COIN") {
            let _ = coin_video(bot, aid, 1).await;
        }
        if cfg.get_bool("PROACTIVE_FAV") {
            let _ = fav_video(bot, aid).await;
        }
        if cfg.get_bool("PROACTIVE_FOLLOW") {
            if let Some(up_mid) = video.get("owner").and_then(|o| o.get("mid")).and_then(|m| m.as_i64()) {
                let _ = follow_user(bot, up_mid).await;
            }
        }

        // 4. 主动评论
        if cfg.get_bool("PROACTIVE_COMMENT") && comments_sent < comment_count {
            if let Some(comment) = generate_proactive_comment(bot, &title, &analysis).await {
                if !comment.is_empty() {
                    let ok = send_comment(bot, aid, &comment).await;
                    if ok {
                        comments_sent += 1;
                        save_external_memory(bot, &bvid, video, &analysis, &comment).await;
                    }
                }
            }
        }
    }
    tracing::info!("主动行为完成，共评论 {comments_sent} 条");
    Ok(())
}

// ---------- 数据获取 ----------
async fn get_up_latest_video(bot: &Bot, mid: i64) -> Option<Value> {
    let url = "https://api.bilibili.com/x/space/wbi/arc/search";
    let mut params = vec![("mid".into(), mid.to_string()), ("ps".into(), "1".to_string()), ("pn".into(), "1".to_string()), ("order".into(), "pubdate".to_string())];
    sign_wbi(bot, &mut params).await;
    let resp = bot.bili.http.get(url).query(&params).send().await.ok()?;
    let v: Value = resp.json().await.ok()?;
    v["data"]["list"]["vlist"].as_array().and_then(|a| a.first().cloned())
}

async fn get_hot_videos_by_tid(bot: &Bot, tid: i64) -> Option<Vec<Value>> {
    let url = "https://api.bilibili.com/x/web-interface/popular";
    let mut params = vec![("ps".into(), "20".to_string()), ("pn".into(), "1".to_string()), ("rid".into(), tid.to_string())];
    sign_wbi(bot, &mut params).await;
    let resp = bot.bili.http.get(url).query(&params).send().await.ok()?;
    let v: Value = resp.json().await.ok()?;
    Some(v["data"]["list"].as_array().cloned().unwrap_or_default())
}

// ---------- 下载与抽帧 ----------
async fn download_video(bot: &Bot, bvid: &str) -> Option<PathBuf> {
    let cookies = generate_cookies_file(bot).ok()?;
    let out = PathBuf::from(&bot.base_dir).join("data").join("proactive_tmp").join(format!("{bvid}.mp4"));
    std::fs::create_dir_all(out.parent()?).ok()?;
    let out_str = out.to_str()?.to_string();
    let url = format!("https://www.bilibili.com/video/{bvid}");
    let status = Command::new("yt-dlp")
        .args(["-f", "mp4", "--cookies", cookies.to_str()?, "-o", &out_str, "-q", &url])
        .status()
        .await
        .ok()?;
    if status.success() && out.exists() {
        Some(out)
    } else {
        None
    }
}

async fn extract_frames(_bot: &Bot, video: &PathBuf) -> Option<Vec<PathBuf>> {
    let dir = video.parent()?.join("frames");
    std::fs::create_dir_all(&dir).ok()?;
    let mut frames = Vec::new();
    let counts = [3usize, 8, 14];
    for &sec in &counts {
        let frame = dir.join(format!("f{sec}.jpg"));
        let status = Command::new("ffmpeg")
            .args(["-y", "-ss", &sec.to_string(), "-i", video.to_str()?, "-frames:v", "1", "-q:v", "2", frame.to_str()?])
            .status()
            .await
            .ok()?;
        if status.success() && frame.exists() {
            frames.push(frame);
        }
    }
    if frames.is_empty() {
        return None;
    }
    Some(frames)
}

async fn analyze_video_frames(bot: &Bot, title: &str, frames: &[PathBuf]) -> String {
    let mut images = Vec::new();
    for f in frames.iter().take(3) {
        if let Ok(bytes) = std::fs::read(f) {
            images.push((f.to_string_lossy().to_string(), format!("data:image/jpeg;base64,{}", crate::util::b64_encode(&bytes))));
        }
    }
    if images.is_empty() {
        return String::new();
    }
    let max_tok = bot.config.read().unwrap().clone().max_tokens_of("vision");
    let ocr_prompt = "请逐张描述这些视频画面：画面里有什么、有没有文字，用中文简洁说明。";
    let ocr = match bot.llm.chat_with_images("vision", ocr_prompt, images, max_tok).await {
        Ok(r) => {
            log_cost(&bot.config, "视频识别", r.input_tokens, r.output_tokens, &r.model, &bot.llm.cost_log);
            r.text
        }
        Err(e) => {
            tracing::warn!("画面识别失败: {e}");
            String::new()
        }
    };
    let max_tok2 = bot.config.read().unwrap().clone().max_tokens_of("chat");
    let prompt = format!(
        "请根据以下B站视频信息与画面描述，用中文写一段简洁的内容概括（150字以内），包括：这个视频大概在讲什么、是什么类型/风格、可能的受众。\n\n标题：{title}\n画面描述：{}\n\n直接输出概括内容，不要加前缀。",
        if ocr.is_empty() { "（未能识别）" } else { &ocr }
    );
    bot.llm.compress("chat", &prompt, max_tok2).await.unwrap_or_default()
}

// ---------- 互动 ----------
async fn like_video(bot: &Bot, aid: i64) -> bool {
    post_form(bot, "https://api.bilibili.com/x/web-interface/archive/like", &[("aid", aid.to_string()), ("like", "1".to_string())]).await
}

async fn coin_video(bot: &Bot, aid: i64, num: i64) -> bool {
    post_form(bot, "https://api.bilibili.com/x/web-interface/coin/add", &[("aid", aid.to_string()), ("multiply", num.to_string()), ("select_like", "0".to_string())]).await
}

async fn fav_video(bot: &Bot, aid: i64) -> bool {
    let cfg = bot.config.read().unwrap().clone();
    let url = "https://api.bilibili.com/x/v3/fav/folder/created/list-all";
    let params = [("up_mid", cfg.get_str("DEDE_USER_ID")), ("type", "2".to_string())];
    let resp = match bot.bili.http.get(url).query(&params).send().await {
        Ok(r) => r,
        Err(_) => return false,
    };
    let v: Value = match resp.json().await {
        Ok(v) => v,
        Err(_) => return false,
    };
    let fav_id = v["data"]["list"][0]["id"].as_i64().unwrap_or(0);
    if fav_id == 0 {
        return false;
    }
    post_form(bot, "https://api.bilibili.com/x/v3/fav/resource/deal", &[
        ("rid", aid.to_string()),
        ("type", "2".into()),
        ("add_media_ids", fav_id.to_string()),
    ])
    .await
}

async fn follow_user(bot: &Bot, mid: i64) -> bool {
    post_form(bot, "https://api.bilibili.com/x/relation/modify", &[("fid", mid.to_string()), ("act", "1".into()), ("reasons", "".into())]).await
}

async fn post_form(bot: &Bot, url: &str, extra: &[(&str, String)]) -> bool {
    let cfg = bot.config.read().unwrap().clone();
    let csrf = cfg.get_str("BILI_JCT");
    let mut form: Vec<(&str, String)> = extra.to_vec();
    form.push(("csrf", csrf));
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert("User-Agent", reqwest::header::HeaderValue::from_static(crate::bili_api::UA));
    headers.insert("Referer", reqwest::header::HeaderValue::from_static("https://www.bilibili.com/"));
    headers.insert("Content-Type", reqwest::header::HeaderValue::from_static("application/x-www-form-urlencoded"));
    let resp = match bot.bili.http.post(url).headers(headers).form(&form).send().await {
        Ok(r) => r,
        Err(_) => return false,
    };
    match resp.json::<Value>().await {
        Ok(v) => v.get("code").and_then(|c| c.as_i64()) == Some(0),
        Err(_) => false,
    }
}

async fn send_comment(bot: &Bot, oid: i64, comment: &str) -> bool {
    let cfg = bot.config.read().unwrap().clone();
    let csrf = cfg.get_str("BILI_JCT");
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert("User-Agent", reqwest::header::HeaderValue::from_static(crate::bili_api::UA));
    headers.insert("Referer", reqwest::header::HeaderValue::from_static("https://www.bilibili.com/"));
    headers.insert("Content-Type", reqwest::header::HeaderValue::from_static("application/x-www-form-urlencoded"));
    let form = [
        ("oid", oid.to_string()),
        ("type", "1".into()),
        ("message", comment.to_string()),
        ("csrf", csrf),
    ];
    let resp = match bot.bili.http.post("https://api.bilibili.com/x/v2/reply/add").headers(headers).form(&form).send().await {
        Ok(r) => r,
        Err(_) => return false,
    };
    match resp.json::<Value>().await {
        Ok(v) => v.get("code").and_then(|c| c.as_i64()) == Some(0),
        Err(_) => false,
    }
}

// ---------- 评论生成与记忆 ----------
async fn generate_proactive_comment(bot: &Bot, title: &str, analysis: &str) -> Option<String> {
    let cfg = bot.config.read().unwrap().clone();
    let bot_name = cfg.get_str("BOT_NAME");
    let custom = cfg.get_str("PROMPT_PROACTIVE_COMMENT");
    let prompt = if custom.is_empty() {
        format!(
            "你是{bot_name}，正在B站刷视频。请根据下面视频信息，写一条自然、有个人风格的评论（15-40字），不要套话，不要夸得太假。\n\n视频标题：{title}\n内容概括：{analysis}\n\n直接输出评论内容。"
        )
    } else {
        format!("{custom}\n\n视频标题：{title}\n内容概括：{analysis}")
    };
    let max_tok = cfg.max_tokens_of("proactive_comment");
    match bot.llm.compress("chat", &prompt, max_tok).await {
        Ok(c) if !c.is_empty() => Some(c),
        _ => None,
    }
}

async fn save_external_memory(bot: &Bot, bvid: &str, video: &Value, analysis: &str, comment: &str) {
    let mut external: Value = crate::util::load_json(&crate::util::data_path(&bot.base_dir, "external_memory.json"), json!({}));
    external[bvid] = json!({
        "title": video.get("title").and_then(|v| v.as_str()).unwrap_or(""),
        "owner_name": video["owner"]["name"].as_str().unwrap_or(""),
        "analysis": analysis,
        "comment": comment,
        "time": crate::util::now_str(),
    });
    let _ = crate::util::save_json(&crate::util::data_path(&bot.base_dir, "external_memory.json"), &external);
}

#[allow(dead_code)]
fn _unused(_: &AppError) -> String {
    String::new()
}
