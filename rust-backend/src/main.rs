//! bilibili-ai-bot Rust 后端入口。
//! 用法: bilibili-ai-bot-rs [--base-dir <目录>] [--port <端口>] [--no-bot] [--no-web]

mod bili_api;
mod bili_login;
mod bot;
mod config;
mod dynamic;
mod error;
mod llm;
mod memory;
mod personality;
mod private_msgs;
mod proactive;
mod util;
mod web;

use config::Config;
use std::sync::{Arc, RwLock};

fn main() {
    // 每个新 shell 需要 PATH；这里显式补一次
    if let Ok(home) = std::env::var("HOME") {
        let cargo_bin = format!("{home}/.cargo/bin");
        let path = std::env::var("PATH").unwrap_or_default();
        if !path.split(':').any(|p| p == cargo_bin) {
            std::env::set_var("PATH", format!("{cargo_bin}:{path}"));
        }
    }

    let args: Vec<String> = std::env::args().collect();
    let mut base_dir = std::env::var("BOT_DIR").unwrap_or_else(|_| ".".to_string());
    let mut port: u16 = 5000;
    let mut run_bot = true;
    let mut run_web = true;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--base-dir" => {
                i += 1;
                if i < args.len() {
                    base_dir = args[i].clone();
                }
            }
            "--port" => {
                i += 1;
                if i < args.len() {
                    port = args[i].parse().unwrap_or(5000);
                }
            }
            "--no-bot" => run_bot = false,
            "--no-web" => run_web = false,
            _ => {}
        }
        i += 1;
    }

    // 日志
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .with_target(false)
        .compact()
        .init();

    // 配置
    let cfg = match Config::load(std::path::PathBuf::from(&base_dir).join("config.json")) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("配置加载失败: {e}（请确认 {base_dir}/config.json 存在）");
            std::process::exit(1);
        }
    };
    let config = Arc::new(RwLock::new(cfg));
    tracing::info!("📂 数据目录: {base_dir}");

    // 客户端
    let bili = Arc::new(bili_api::BiliClient::new(config.clone()));
    let llm = Arc::new(llm::LlmClient::new(config.clone(), &base_dir));

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime 创建失败");

    runtime.block_on(async {
        // 主循环
        if run_bot {
            let bot = bot::Bot::new(config.clone(), bili.clone(), llm.clone(), &base_dir);
            tokio::spawn(async move {
                bot.run().await;
            });
        }

        // Web 面板
        if run_web {
            let seal = web::load_seal_key(&base_dir);
            if seal.is_some() {
                tracing::info!("🔐 RSA 口令密封密钥已就绪（data/.seal_key.pem）");
            }
            let secret = web::load_secret_key(&base_dir);
            let ctx = Arc::new(web::WebCtx {
                base_dir: base_dir.clone(),
                config: config.clone(),
                bili: bili.clone(),
                llm: llm.clone(),
                memory: Arc::new(memory::MemoryStore::new(config.clone(), llm.clone(), &base_dir)),
                personality: Arc::new(personality::Personality::new(config.clone(), llm.clone(), &base_dir)),
                qr: Arc::new(bili_login::BiliQrLoginManager::new(180)),
                permanent: memory::PermanentMemory::new(&base_dir),
                secret_key: secret,
                seal: std::sync::RwLock::new(seal),
            });
            web::serve(ctx, port).await;
        } else {
            // 只跑主循环时挂住
            std::future::pending::<()>().await;
        }
    });
}
