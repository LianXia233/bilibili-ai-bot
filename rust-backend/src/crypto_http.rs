//! 应用层加密通信协议 v1（HTTP + X25519 + HKDF-SHA256 + AES-256-GCM）。
//!
//! 目标：在明文 HTTP 之上，让被动网络抓包（Wireshark / tcpdump）无法直接读取
//! API 请求/响应正文。**不**提供 HTTPS 级别的身份认证，无法对抗主动中间人。
//!
//! 安全边界（对外说明用）：
//! - 浏览器端 JS 不是可信环境；主动篡改 HTML/JS 的攻击者可改写加密代码。
//! - 服务器身份锚点 = 服务器长期 X25519 静态公钥，浏览器通过「首次使用指纹核对
//!   （TOFU pinning）」建立信任；不预置公钥时无法防主动 MITM。
//! - 被动抓包者仍可见：IP / URL / HTTP Header / 时间 / 请求响应大小 / 频率。
//! - 拿不到会话密钥与随机 nonce 的被动抓包者，无法得到 AES-GCM 明文。

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce as GcmNonce};
use base64::Engine;
use hkdf::Hkdf;
use rand::{rngs::OsRng, RngCore};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use x25519_dalek::{EphemeralSecret, PublicKey, StaticSecret};

pub const PROTO_VERSION: u64 = 1;
const DOMAIN: &[u8] = b"bili-ai-bot-crypto-v1";
const DEFAULT_TTL_SECS: u64 = 900;
const MAX_SESSIONS: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CryptoError {
    UnknownSession,
    Expired,
    Replay,
}

/// 服务器长期静态身份（X25519 公钥作为身份锚点，持久化于
/// data/server_crypto_identity.bin，0600 权限）。
pub struct StaticIdentity {
    secret: StaticSecret,
    pub public: [u8; 32],
    /// SHA-256(public) 前 8 字节的 hex（16 字符），供用户在可信渠道核对。
    pub fingerprint: String,
}

impl StaticIdentity {
    pub fn load(base_dir: &str) -> StaticIdentity {
        let path = std::path::Path::new(base_dir)
            .join("data")
            .join("server_crypto_identity.bin");
        let secret: StaticSecret = match std::fs::read_to_string(&path) {
            Ok(text) => {
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(text.trim())
                    .unwrap_or_default();
                if bytes.len() == 32 {
                    StaticSecret::from(<[u8; 32]>::try_from(bytes).unwrap())
                } else {
                    Self::new_secret(&path)
                }
            }
            Err(_) => Self::new_secret(&path),
        };
        let public = PublicKey::from(&secret);
        Self::new(secret, public)
    }

    fn new_secret(path: &std::path::Path) -> StaticSecret {
        let mut raw = [0u8; 32];
        OsRng.fill_bytes(&mut raw);
        let s = StaticSecret::from(raw);
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let b64 = base64::engine::general_purpose::STANDARD.encode(s.to_bytes());
        let _ = std::fs::write(path, b64);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
        }
        s
    }

    fn new(secret: StaticSecret, public: PublicKey) -> StaticIdentity {
        let mut h = Sha256::new();
        h.update(public.as_bytes());
        let d = h.finalize();
        let fp: String = d.iter().take(8).map(|b| format!("{b:02x}")).collect();
        StaticIdentity {
            secret,
            public: public.to_bytes(),
            fingerprint: fp,
        }
    }
}

pub struct CryptoSession {
    pub c2s: [u8; 32],
    pub s2c: [u8; 32],
    pub expect: u64,
    pub authed: bool,
    #[allow(dead_code)]
    pub created: Instant,
    pub last: Instant,
}

pub struct CryptoState {
    identity: StaticIdentity,
    sessions: Mutex<HashMap<String, CryptoSession>>,
    pub ttl: Duration,
    pub max_sessions: usize,
}

impl CryptoState {
    pub fn new(base_dir: &str) -> CryptoState {
        let identity = StaticIdentity::load(base_dir);
        let ttl = std::env::var("CRYPTO_SESSION_TTL")
            .ok()
            .and_then(|v| v.parse().ok())
            .map(Duration::from_secs)
            .unwrap_or(Duration::from_secs(DEFAULT_TTL_SECS));
        CryptoState {
            identity,
            sessions: Mutex::new(HashMap::new()),
            ttl,
            max_sessions: MAX_SESSIONS,
        }
    }

    pub fn identity(&self) -> &StaticIdentity {
        &self.identity
    }

    fn cleanup_locked(map: &mut HashMap<String, CryptoSession>, ttl: Duration, max: usize) {
        let now = Instant::now();
        map.retain(|_, s| now.duration_since(s.last) < ttl);
        if map.len() > max {
            let mut keys: Vec<(String, Instant)> =
                map.iter().map(|(k, s)| (k.clone(), s.last)).collect();
            keys.sort_by_key(|(_, t)| *t);
            for (k, _) in keys.iter().take(map.len() - max) {
                map.remove(k);
            }
        }
    }

    /// 建立加密会话：客户端临时公钥 -> (session_id, server_eph_pub, ttl_secs)。
    /// 共享秘密 = X25519(eph, client) || X25519(static, client)，经 HKDF 派生双向密钥。
    pub fn create_session(&self, client_pub: [u8; 32]) -> (String, [u8; 32], u64) {
        let eph = EphemeralSecret::random_from_rng(OsRng);
        let eph_pub = PublicKey::from(&eph);
        let shared_eph = eph.diffie_hellman(&PublicKey::from(client_pub));
        let shared_static = self.identity.secret.diffie_hellman(&PublicKey::from(client_pub));

        let mut ikm = Vec::with_capacity(64);
        ikm.extend_from_slice(shared_eph.as_bytes());
        ikm.extend_from_slice(shared_static.as_bytes());

        let (c2s, s2c) = derive_keys(&ikm, &client_pub, &eph_pub.to_bytes(), &self.identity.public);

        let mut sid = [0u8; 16];
        OsRng.fill_bytes(&mut sid);
        let session_id =
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(sid);

        let now = Instant::now();
        let mut map = self.sessions.lock().unwrap();
        Self::cleanup_locked(&mut map, self.ttl, self.max_sessions);
        map.insert(
            session_id.clone(),
            CryptoSession {
                c2s,
                s2c,
                expect: 1,
                authed: false,
                created: now,
                last: now,
            },
        );
        (session_id, eph_pub.to_bytes(), self.ttl.as_secs())
    }

    /// 消费一个请求序号：锁内校验会话存在、未过期、counter == expect。
    /// 校验通过即视为已消费（expect += 1，刷新 last），返回会话密钥与授权态。
    /// 失败返回 CryptoError（调用方不得消耗 counter）。
    pub fn consume(&self, sid: &str, counter: u64) -> Result<([u8; 32], [u8; 32], bool), CryptoError> {
        let mut map = self.sessions.lock().unwrap();
        let now = Instant::now();
        let sess = match map.get_mut(sid) {
            Some(s) => s,
            None => return Err(CryptoError::UnknownSession),
        };
        if now.duration_since(sess.last) >= self.ttl {
            map.remove(sid);
            return Err(CryptoError::Expired);
        }
        if counter != sess.expect {
            return Err(CryptoError::Replay);
        }
        sess.expect += 1;
        sess.last = now;
        Ok((sess.c2s, sess.s2c, sess.authed))
    }

    pub fn mark_authed(&self, sid: &str) {
        if let Ok(mut map) = self.sessions.lock() {
            if let Some(s) = map.get_mut(sid) {
                s.authed = true;
            }
        }
    }

    pub fn destroy(&self, sid: &str) -> bool {
        if let Ok(mut map) = self.sessions.lock() {
            map.remove(sid).is_some()
        } else {
            false
        }
    }

    pub fn handshake_json(&self) -> Value {
        let id = self.identity();
        json!({
            "v": PROTO_VERSION,
            "server_static_pub": base64::engine::general_purpose::STANDARD.encode(id.public),
            "fingerprint": id.fingerprint,
            "ttl": self.ttl.as_secs(),
            "max_sessions": self.max_sessions,
        })
    }
}

/// HKDF-SHA256 派生双向会话密钥：
/// ikm = X25519(eph) || X25519(static)；
/// salt = DOMAIN || client_pub || server_eph_pub || server_static_pub（绑定双方临时公钥）；
/// info = "c2s" / "s2c"。
pub fn derive_keys(
    ikm: &[u8],
    client_pub: &[u8; 32],
    server_eph_pub: &[u8; 32],
    server_static_pub: &[u8; 32],
) -> ([u8; 32], [u8; 32]) {
    let mut salt = Vec::with_capacity(DOMAIN.len() + 96);
    salt.extend_from_slice(DOMAIN);
    salt.extend_from_slice(client_pub);
    salt.extend_from_slice(server_eph_pub);
    salt.extend_from_slice(server_static_pub);

    let hk = Hkdf::<Sha256>::new(Some(&salt), ikm);
    let mut c2s = [0u8; 32];
    let mut s2c = [0u8; 32];
    hk.expand(b"c2s", &mut c2s).expect("hkdf expand");
    hk.expand(b"s2c", &mut s2c).expect("hkdf expand");
    (c2s, s2c)
}

/// 生成 12 字节随机 nonce（AES-GCM 标准 nonce 长度，随机 96-bit；
/// 会话密钥唯一 + 每会话请求量远低于 2^32，碰撞概率可忽略）。
pub fn random_nonce() -> [u8; 12] {
    let mut n = [0u8; 12];
    OsRng.fill_bytes(&mut n);
    n
}

/// 构造 GCM AAD：v|session_id|counter|nonce_b64。
/// 把 envelope 外层可见字段绑定进认证数据，任何字段被篡改都会导致解密失败。
pub fn aad_bytes(v: u64, session_id: &str, counter: u64, nonce_b64: &str) -> Vec<u8> {
    format!("{v}|{session_id}|{counter}|{nonce_b64}").into_bytes()
}

pub fn encrypt_payload(key: &[u8; 32], nonce: &[u8; 12], aad: &[u8], plain: &[u8]) -> Option<Vec<u8>> {
    let cipher = Aes256Gcm::new_from_slice(key).ok()?;
    cipher
        .encrypt(GcmNonce::from_slice(nonce), Payload { msg: plain, aad })
        .ok()
}

pub fn decrypt_payload(
    key: &[u8; 32],
    nonce: &[u8; 12],
    aad: &[u8],
    ct: &[u8],
) -> Option<Vec<u8>> {
    let cipher = Aes256Gcm::new_from_slice(key).ok()?;
    cipher
        .decrypt(GcmNonce::from_slice(nonce), Payload { msg: ct, aad })
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(tag: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("bili-crypto-test-{tag}-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&p);
        p
    }

    #[test]
    fn handshake_identity_is_stable_and_fingerprinted() {
        let dir = tmp_dir("identity");
        let a = CryptoState::new(dir.to_str().unwrap());
        let fp = a.identity().fingerprint.clone();
        let pub1 = a.identity().public;
        drop(a);
        // 重新加载应得到同一静态身份（持久化）
        let b = CryptoState::new(dir.to_str().unwrap());
        assert_eq!(b.identity().public, pub1);
        assert_eq!(b.identity().fingerprint, fp);
        assert_eq!(fp.len(), 16);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn full_session_roundtrip() {
        let dir = tmp_dir("roundtrip");
        let st = CryptoState::new(dir.to_str().unwrap());

        // 模拟客户端：生成临时密钥对（StaticSecret 允许对同一私钥做多次 DH）
        let mut raw = [0u8; 32];
        OsRng.fill_bytes(&mut raw);
        let client_eph = StaticSecret::from(raw);
        let client_pub = PublicKey::from(&client_eph).to_bytes();

        // 建会话
        let (sid, server_eph_pub, ttl) = st.create_session(client_pub);
        assert_eq!(ttl, 900);

        // 客户端侧派生
        let shared_eph = client_eph.diffie_hellman(&PublicKey::from(server_eph_pub));
        let shared_static = client_eph.diffie_hellman(&PublicKey::from(st.identity().public));
        let mut ikm = Vec::with_capacity(64);
        ikm.extend_from_slice(shared_eph.as_bytes());
        ikm.extend_from_slice(shared_static.as_bytes());
        let (c2s, s2c) = derive_keys(&ikm, &client_pub, &server_eph_pub, &st.identity().public);

        // 请求：counter=1 加密 -> 服务端解密
        let counter = 1u64;
        let nonce = random_nonce();
        let nonce_b64 = base64::engine::general_purpose::STANDARD.encode(nonce);
        let aad = aad_bytes(PROTO_VERSION, &sid, counter, &nonce_b64);
        let plain = b"{\"path\":\"/api/stats\",\"method\":\"GET\",\"body\":{}}";
        let ct = encrypt_payload(&c2s, &nonce, &aad, plain).unwrap();

        let (sk_c2s, sk_s2c, authed) = st.consume(&sid, counter).unwrap();
        assert!(!authed);
        let out = decrypt_payload(&sk_c2s, &nonce, &aad, &ct).unwrap();
        assert_eq!(out, plain);

        // 响应加密
        let rnonce = random_nonce();
        let rnonce_b64 = base64::engine::general_purpose::STANDARD.encode(rnonce);
        let raad = aad_bytes(PROTO_VERSION, &sid, counter, &rnonce_b64);
        let rplain = b"{\"status\":200,\"body\":{}}";
        let rct = encrypt_payload(&sk_s2c, &rnonce, &raad, rplain).unwrap();
        let rout = decrypt_payload(&s2c, &rnonce, &raad, &rct).unwrap();
        assert_eq!(rout, rplain);

        // 重放：同一 counter 再次提交必须拒绝
        assert_eq!(st.consume(&sid, counter).unwrap_err(), CryptoError::Replay);
        // 下一个 counter 正常
        let nonce2 = random_nonce();
        let nonce2_b64 = base64::engine::general_purpose::STANDARD.encode(nonce2);
        let aad2 = aad_bytes(PROTO_VERSION, &sid, 2, &nonce2_b64);
        let ct2 = encrypt_payload(&c2s, &nonce2, &aad2, plain).unwrap();
        let (_, _, _) = st.consume(&sid, 2).unwrap();
        assert!(decrypt_payload(&c2s, &nonce2, &aad2, &ct2).is_some());

        // 篡改 AAD（换 counter）导致解密失败
        let aad3 = aad_bytes(PROTO_VERSION, &sid, 3, &nonce2_b64);
        assert!(decrypt_payload(&c2s, &nonce2, &aad3, &ct2).is_none());

        // destroy 后会话不存在
        assert!(st.destroy(&sid));
        assert_eq!(st.consume(&sid, 3).unwrap_err(), CryptoError::UnknownSession);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ttl_expiry() {
        let dir = tmp_dir("ttl");
        let st = CryptoState::new(dir.to_str().unwrap());
        let mut raw = [0u8; 32];
        OsRng.fill_bytes(&mut raw);
        let client_eph = StaticSecret::from(raw);
        let client_pub = PublicKey::from(&client_eph).to_bytes();
        let (sid, _, _) = st.create_session(client_pub);
        // 手动把 last 推到过期
        {
            let mut map = st.sessions.lock().unwrap();
            let s = map.get_mut(&sid).unwrap();
            s.last = Instant::now() - Duration::from_secs(st.ttl.as_secs() + 1);
        }
        assert_eq!(st.consume(&sid, 1).unwrap_err(), CryptoError::Expired);
        // 过期后会话被移除
        assert_eq!(st.consume(&sid, 1).unwrap_err(), CryptoError::UnknownSession);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
