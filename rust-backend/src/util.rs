//! 通用工具：原子 JSON 读写、时间、哈希、余弦相似度、HMAC。

use crate::error::{AppError, Result};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use chrono::Local;
use md5::{Digest, Md5};
use rand::RngCore;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;
use sha2::{Sha256, Sha512};
use std::fs;
use std::path::{Path, PathBuf};

/// 读取 JSON 文件；文件不存在或解析失败返回默认值（与 Python load_json 行为一致）。
pub fn load_json<T: DeserializeOwned>(path: &Path, default: T) -> T {
    match fs::read_to_string(path) {
        Ok(text) => match serde_json::from_str(&text) {
            Ok(v) => v,
            Err(_) => default,
        },
        Err(_) => default,
    }
}

/// 原子写 JSON：先写临时文件再 rename，避免读-改-写竞态留下半截文件。
pub fn save_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(dir).map_err(AppError::from)?;
    let tmp = path.with_extension("json.tmp");
    let text = serde_json::to_string_pretty(value).map_err(AppError::from)?;
    fs::write(&tmp, text).map_err(AppError::from)?;
    fs::rename(&tmp, path).map_err(AppError::from)?;
    Ok(())
}

/// data 目录下的文件路径。
pub fn data_path(base: &str, name: &str) -> PathBuf {
    Path::new(base).join("data").join(name)
}

/// 当前 Unix 秒。
pub fn now_unix() -> i64 {
    chrono::Utc::now().timestamp()
}

/// 当前本地时间字符串（YYYY-MM-DD HH:MM）。
pub fn now_str() -> String {
    Local::now().format("%Y-%m-%d %H:%M").to_string()
}

/// 今日日期字符串（YYYY-MM-DD）。
pub fn today_str() -> String {
    Local::now().format("%Y-%m-%d").to_string()
}

/// 随机 URL 安全 token（约 24 字节）。
pub fn gen_token() -> String {
    let mut buf = [0u8; 24];
    rand::thread_rng().fill_bytes(&mut buf);
    B64.encode(buf).trim_end_matches('=').to_string()
}

/// MD5 hex（WBI 签名用）。
pub fn md5_hex(data: &str) -> String {
    let mut h = Md5::new();
    h.update(data.as_bytes());
    format!("{:x}", h.finalize())
}

/// HMAC-SHA256 hex（会话 Cookie 签名）。
pub fn hmac_sha256_hex(secret: &[u8], data: &[u8]) -> String {
    
    const BLOCK: usize = 64;
    let mut key = vec![0u8; BLOCK];
    if secret.len() > BLOCK {
        let mut h = Sha256::new();
        h.update(secret);
        let d = h.finalize();
        key[..32].copy_from_slice(&d);
    } else {
        key[..secret.len()].copy_from_slice(secret);
    }
    let mut ipad = vec![0x36u8; BLOCK];
    let mut opad = vec![0x5cu8; BLOCK];
    for i in 0..BLOCK {
        ipad[i] ^= key[i];
        opad[i] ^= key[i];
    }
    let mut inner = Sha256::new();
    inner.update(&ipad);
    inner.update(data);
    let inner_d = inner.finalize();
    let mut outer = Sha256::new();
    outer.update(&opad);
    outer.update(inner_d);
    format!("{:x}", outer.finalize())
}

/// 对文本做 HMAC-SHA512 的摘要（保留函数名，供需要更长度摘要处使用）。
#[allow(dead_code)]
pub fn hmac_sha512_hex(secret: &[u8], data: &[u8]) -> String {
    
    const BLOCK: usize = 128;
    let mut key = vec![0u8; BLOCK];
    if secret.len() > BLOCK {
        let mut h = Sha512::new();
        h.update(secret);
        let d = h.finalize();
        key[..64].copy_from_slice(&d);
    } else {
        key[..secret.len()].copy_from_slice(secret);
    }
    let mut ipad = vec![0x36u8; BLOCK];
    let mut opad = vec![0x5cu8; BLOCK];
    for i in 0..BLOCK {
        ipad[i] ^= key[i];
        opad[i] ^= key[i];
    }
    let mut inner = Sha512::new();
    inner.update(&ipad);
    inner.update(data);
    let inner_d = inner.finalize();
    let mut outer = Sha512::new();
    outer.update(&opad);
    outer.update(inner_d);
    format!("{:x}", outer.finalize())
}

/// Value 转字符串工具。
pub fn v_str(v: &Value, default: &str) -> String {
    v.as_str().unwrap_or(default).to_string()
}

pub fn v_i64(v: &Value, default: i64) -> i64 {
    v.as_i64().unwrap_or(default)
}

pub fn v_f64(v: &Value, default: f64) -> f64 {
    v.as_f64().unwrap_or(default)
}

pub fn v_bool(v: &Value, default: bool) -> bool {
    v.as_bool().unwrap_or(default)
}

pub fn v_list(v: &Value) -> Vec<Value> {
    v.as_array().cloned().unwrap_or_default()
}

pub fn v_str_list(v: &Value) -> Vec<String> {
    v.as_array()
        .map(|arr| {
            arr.iter()
                .map(|x| x.as_str().unwrap_or("").to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// B64 编码。
pub fn b64_encode(data: &[u8]) -> String {
    B64.encode(data)
}

pub fn b64_decode(data: &str) -> std::result::Result<Vec<u8>, AppError> {
    Ok(B64.decode(data)?)
}

/// 归一化文本：只保留中英文与数字字符（去空白、标点、emoji），用于回复相似度比较。
pub fn normalize_chars(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric() || ('\u{4e00}'..='\u{9fff}').contains(c))
        .map(|c| c.to_lowercase().next().unwrap_or(c))
        .collect()
}

/// 字符集合 Jaccard 相似度（0.0~1.0）：两份归一化文本的字符交集 / 并集。
pub fn char_jaccard(a: &str, b: &str) -> f64 {
    use std::collections::HashSet;
    let sa: HashSet<char> = a.chars().collect();
    let sb: HashSet<char> = b.chars().collect();
    if sa.is_empty() && sb.is_empty() {
        return 1.0;
    }
    let inter = sa.intersection(&sb).count();
    let union = sa.union(&sb).count();
    if union == 0 {
        1.0
    } else {
        inter as f64 / union as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_keeps_cn_en_digits_only() {
        assert_eq!(normalize_chars("你好呀！Hello 123? 😊"), "你好呀hello123");
        assert_eq!(normalize_chars("   "), "");
        assert_eq!(normalize_chars(""), "");
    }

    #[test]
    fn char_jaccard_same_and_disjoint() {
        assert!((char_jaccard("你好世界", "你好世界") - 1.0).abs() < 1e-9);
        assert!(char_jaccard("abcdef", "ghijkl") < 0.2);
        // 近义改写：相似但不完全相同（约 0.67，低于去重阈值 0.85，不应误杀）
        assert!(char_jaccard("这装扮很好看呀", "这装扮看起来很好看") > 0.6);
        assert!(char_jaccard("这装扮很好看呀", "这装扮看起来很好看") < 0.9);
        // 一字不差 + 语气词：归一化后完全重复（真实调用链先 normalize 再 jaccard），应命中高阈值
        let na = normalize_chars("这装扮很好看呀");
        let nb = normalize_chars("这装扮很好看呀！");
        assert!((char_jaccard(&na, &nb) - 1.0).abs() < 1e-9);
        // 完全不同话题
        assert!(char_jaccard("今天天气怎么样", "推荐几首好听的歌") < 0.2);
    }
}
