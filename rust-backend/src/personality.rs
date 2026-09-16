//! 人格系统：好感度等级、里程碑、每日心情、节日彩蛋（含农历）、性格演化、personas。

use crate::config::Config;
use crate::error::{AppError, Result};
use crate::llm::LlmClient;
use crate::memory::parse_json_lenient;
use crate::util::{load_json, now_str, now_unix, save_json};
use chrono::Datelike;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

pub struct Personality {
    pub config: Arc<RwLock<Config>>,
    pub llm: Arc<LlmClient>,
    pub base_dir: String,
}

impl Personality {
    pub fn new(config: Arc<RwLock<Config>>, llm: Arc<LlmClient>, base_dir: &str) -> Self {
        Personality { config, llm, base_dir: base_dir.to_string() }
    }

    fn path(&self, name: &str) -> PathBuf {
        crate::util::data_path(&self.base_dir, name)
    }

    // ---------- 等级 ----------
    pub fn get_level(&self, score: i64, mid: Option<&str>) -> &'static str {
        let cfg = self.config.read().unwrap().clone();
        if let Some(mid) = mid {
            let owner = cfg.get_str("OWNER_MID");
            if mid == owner {
                return "special";
            }
        }
        if score <= -10 {
            "cold"
        } else if score >= 51 {
            "close"
        } else if score >= 31 {
            "friend"
        } else if score >= 11 {
            "normal"
        } else {
            "stranger"
        }
    }

    pub fn level_name(level: &str) -> &'static str {
        match level {
            "special" => "主人",
            "close" => "好友",
            "friend" => "熟人",
            "normal" => "粉丝",
            "stranger" => "陌生人",
            "cold" => "厌恶",
            _ => "陌生人",
        }
    }

    pub fn level_prompt(&self, level: &str) -> String {
        let cfg = self.config.read().unwrap().clone();
        let owner_name = {
            let n = cfg.get_str("OWNER_NAME");
            if n.is_empty() { "主人".to_string() } else { n }
        };
        let owner_bili = cfg.get_str("OWNER_BILI_NAME");
        let bili_note = if !owner_bili.is_empty() {
            format!("，{owner_name}的B站账号名是'{owner_bili}'，是同一个人")
        } else {
            String::new()
        };
        match level {
            "special" => format!("这是你的主人{owner_name}，你最亲近最信任的人。可以完全放松，展现真实的自己，语气自然随意{bili_note}。"),
            "close" => "这是你的好友，好感度很高的人。可以亲近自然地交流，真诚关心对方，语气轻松。".into(),
            "friend" => "这是熟悉的人，好感度中等。可以放松一些，友好地聊天，偶尔开开玩笑。".into(),
            "normal" => "这是普通人，好感度较低。保持友善但不过分热情，礼貌回应。".into(),
            "stranger" => "这是陌生人。礼貌简洁地回应，不冷漠也不过度热情。".into(),
            "cold" => "这个人曾多次恶意攻击你。保持冷淡克制，回复简短，不需要友善但也不要骂人。".into(),
            _ => String::new(),
        }
    }

    // ---------- 好感度 ----------
    pub fn load_affection(&self) -> Value {
        load_json(&self.path("affection.json"), json!({}))
    }

    pub fn save_affection(&self, affection: &Value) -> Result<()> {
        save_json(&self.path("affection.json"), affection)
    }

    pub fn load_replied(&self) -> std::collections::HashSet<String> {
        let v: Vec<Value> = load_json(&self.path("replied.json"), Vec::new());
        v.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect()
    }

    pub fn save_replied(&self, replied: &std::collections::HashSet<String>) -> Result<()> {
        let list: Vec<String> = replied.iter().cloned().collect();
        save_json(&self.path("replied.json"), &list)
    }

    // ---------- 里程碑 ----------
    pub fn check_milestone(&self, mid: &str, old_score: i64, new_score: i64, username: &str) -> Option<String> {
        let map: &[(i64, &str)] = &[
            (10, "「{u}」，你对我来说不再是陌生人了哦。"),
            (30, "不知不觉就和「{u}」变熟了呢，以后可以随意一点。"),
            (50, "「{u}」...我们算是好朋友了吧？请多关照。"),
            (80, "能和「{u}」走到这一步，说实话我挺开心的。"),
            (99, "「{u}」，你是我最重要的人之一。...别得意，我就说这一次。"),
        ];
        let mut triggered: Value = load_json(&self.path("milestones.json"), json!({}));
        let user_milestones = triggered.get(mid).and_then(|v| v.as_array()).cloned().unwrap_or_default();
        let mut user_milestones: Vec<i64> = user_milestones.iter().filter_map(|v| v.as_i64()).collect();
        for (threshold, template) in map {
            if old_score < *threshold && *threshold <= new_score && !user_milestones.contains(threshold) {
                user_milestones.push(*threshold);
                triggered[mid] = json!(user_milestones);
                let _ = save_json(&self.path("milestones.json"), &triggered);
                return Some(template.replace("{u}", username));
            }
        }
        None
    }

    // ---------- 每日心情 ----------
    pub fn get_today_mood(&self) -> (String, String) {
        let cfg = self.config.read().unwrap().clone();
        if !cfg.get_bool("ENABLE_MOOD") {
            return ("平静如常".into(), "".into());
        }
        let today = crate::util::today_str();
        let mood_data: Value = load_json(&self.path("mood.json"), json!({}));
        if mood_data.get("date").and_then(|v| v.as_str()) == Some(&today) {
            return (
                mood_data.get("mood").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                mood_data.get("mood_prompt").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            );
        }
        let moods = [
            ("心情不错", "今天状态还行，语气可以比平时稍微轻快一点点，但别刻意表现。"),
            ("平静如常", "今天一切如常，按正常性格回复。"),
            ("有点安静", "今天话少一点，但不影响正常交流。"),
            ("有点皮", "今天偶尔可以多一点调侃，但不要刻意阴阳怪气。"),
            ("懒得废话", "今天回复可以更简洁一些。"),
        ];
        let (mood, prompt) = moods[rand::random::<usize>() % moods.len()];
        let _ = save_json(&self.path("mood.json"), &json!({"date": today, "mood": mood, "mood_prompt": prompt}));
        (mood.to_string(), prompt.to_string())
    }

    // ---------- 节日彩蛋 ----------
    pub fn get_festival_prompt(&self) -> String {
        let today = chrono::Local::now().format("%m-%d").to_string();
        let solar = [
            ("01-01", "今天是元旦新年！你很开心，会主动说新年快乐，语气温暖。"),
            ("02-14", "今天是情人节。你会调侃一下这个节日，表示自己是AI不需要过情人节，但会祝福别人。"),
            ("03-08", "今天是妇女节，你会真诚地祝福女性用户节日快乐。"),
            ("04-01", "今天是愚人节！你特别皮，回复里可能会开小玩笑或者故意说反话，但不过分。"),
            ("05-01", "今天是劳动节，你会感慨一下自己作为AI全年无休，语气略带自嘲。"),
            ("06-01", "今天是儿童节，你会装可爱一下，然后立刻恢复正常说'我才不是小孩子'。"),
            ("09-10", "今天是教师节，你会对主人表示感谢，对其他人也友善一些。"),
            ("10-01", "今天是国庆节，你会简单祝福节日快乐。"),
            ("10-31", "今天是万圣节，你的语气会带一点神秘感和暗黑风，觉得这个节日很对自己审美。"),
            ("12-25", "今天是圣诞节，你觉得下雪很配自己的名字，语气温柔一些。"),
            ("12-31", "今天是跨年夜，你会感慨时间过得快，温柔地祝大家新年快乐。"),
        ];
        for (d, prompt) in solar {
            if d == today {
                return prompt.to_string();
            }
        }
        let lunar = lunar_md();
        let lunar_map = [
            ("01-01", "今天是除夕/春节！你非常开心，会热情地说新年快乐，语气最温暖。"),
            ("01-15", "今天是元宵节，你会提到汤圆，语气温馨。"),
            ("05-05", "今天是端午节，你会提到粽子，祝大家端午安康。"),
            ("08-15", "今天是中秋节，你会提到月亮和月饼，语气温柔思念感。"),
            ("09-09", "今天是重阳节，你会表达对长辈的尊重。"),
        ];
        for (d, prompt) in lunar_map {
            if lunar == d {
                return prompt.to_string();
            }
        }
        String::new()
    }

    // ---------- 性格演化 ----------
    pub fn get_personality_prompt(&self) -> String {
        let evo: Value = load_json(&self.path("personality_evolution.json"), json!({}));
        if evo.is_null() {
            return String::new();
        }
        let mut parts: Vec<String> = Vec::new();
        let traits = evo.get("evolved_traits").and_then(|v| v.as_array()).cloned().unwrap_or_default();
        if !traits.is_empty() {
            // 只注入最近 1 条：成长是增量吸收的，旧特质已被新特质覆盖，全量注入会让说话风格漂移。
            parts.push("【长期形成的稳定特点（这是长期趋势，不要在单条回复中刻意改变语气，也不要每句话都体现）】".into());
            if let Some(t) = traits.last() {
                if let Some(c) = t.get("change").and_then(|v| v.as_str()) {
                    parts.push(format!("- {c}"));
                }
            }
        }
        let habits = evo.get("speech_habits").and_then(|v| v.as_array()).cloned().unwrap_or_default();
        if !habits.is_empty() {
            let h: Vec<String> = habits.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect();
            parts.push(format!("【当前说话习惯】{}", h.join("；")));
        }
        let opinions = evo.get("opinions").and_then(|v| v.as_array()).cloned().unwrap_or_default();
        if !opinions.is_empty() {
            let o: Vec<String> = opinions.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect();
            parts.push(format!("【对事物的看法】{}", o.join("；")));
        }
        parts.join("\n")
    }

    pub async fn maybe_evolve_personality(&self, memory_texts: &[String]) {
        let cfg = self.config.read().unwrap().clone();
        if !cfg.get_bool("ENABLE_PERSONALITY_EVOLUTION") {
            return;
        }
        let evo: Value = load_json(&self.path("personality_evolution.json"), json!({}));
        let today = crate::util::today_str();
        if evo.get("last_evolve").and_then(|v| v.as_str()).unwrap_or("").starts_with(&today) {
            return;
        }
        if memory_texts.is_empty() {
            return;
        }
        let bot_name = cfg.get_str("BOT_NAME");
        let recent = memory_texts.iter().rev().take(15).cloned().collect::<Vec<_>>().join("\n");
        let prompt = format!(
            "你是{bot_name}，以下是最近与用户互动的记录。请反思：\n1. 你的行为/语气有没有需要调整的地方？\n\
2. 产生了什么新的说话习惯或口头禅？\n3. 对某些事物形成了什么新看法？\n\n\
历史互动（最近）：\n{recent}\n\n\
严格输出JSON：{{\"new_trait\": \"一句话新特质\", \"trigger\": \"触发事件\", \
\"speech_habits\": [\"习惯1\"], \"opinions\": [\"看法1\"], \"reflection\": \"简短反思\"}}"
        );
        let max_tokens = cfg.max_tokens_of("evolve");
        match self.llm.compress("chat", &prompt, max_tokens).await {
            Ok(text) => {
                let result = parse_json_lenient(&text).unwrap_or_else(|| json!({}));
                let mut evo = evo;
                let mut traits = evo.get("evolved_traits").and_then(|v| v.as_array()).cloned().unwrap_or_default();
                let new_trait = result.get("new_trait").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let trigger = result.get("trigger").and_then(|v| v.as_str()).unwrap_or("").to_string();
                if !new_trait.is_empty() {
                    traits.push(json!({"change": new_trait, "trigger": trigger, "time": now_str()}));
                    if traits.len() > 30 {
                        traits.drain(..traits.len() - 30);
                    }
                }
                let habits: Vec<String> = result.get("speech_habits").and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect()).unwrap_or_default();
                let opinions: Vec<String> = result.get("opinions").and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect()).unwrap_or_default();
                evo["evolved_traits"] = json!(traits);
                if !habits.is_empty() {
                    evo["speech_habits"] = json!(habits);
                }
                if !opinions.is_empty() {
                    evo["opinions"] = json!(opinions);
                }
                evo["last_evolve"] = json!(format!("{today} 01:00"));
                evo["reflection"] = result.get("reflection").cloned().unwrap_or(json!(""));
                let _ = save_json(&self.path("personality_evolution.json"), &evo);
                tracing::info!("性格演化完成：{new_trait}");
            }
            Err(e) => {
                tracing::warn!("性格演化失败: {e}");
            }
        }
    }
}

// ============ 农历（1900–2100 表驱动） ============
const LUNAR_INFO: &str = "0x04bd8,0x04ae0,0x0a570,0x054d5,0x0d260,0x0d950,0x16554,0x056a0,0x09ad0,0x055d2,0x04ae0,0x0a5b6,0x0a4d0,0x0d250,0x1d255,0x0b540,0x0d6a0,0x0ada2,0x095b0,0x14977,0x04970,0x0a4b0,0x0b4b5,0x06a50,0x06d40,0x1ab54,0x02b60,0x09570,0x052f2,0x04970,0x06566,0x0d4a0,0x0ea50,0x06e95,0x05ad0,0x02b60,0x186e3,0x092e0,0x1c8d7,0x0c950,0x0d4a0,0x1d8a6,0x0b550,0x056a0,0x1a5b4,0x025d0,0x092d0,0x0d2b2,0x0a950,0x0b557,0x06ca0,0x0b550,0x15355,0x04da0,0x0a5b0,0x14573,0x052b0,0x0a9a8,0x0e950,0x06aa0,0x0aea6,0x0ab50,0x04b60,0x0aae4,0x0a570,0x05260,0x0f263,0x0d950,0x05b57,0x056a0,0x096d0,0x04dd5,0x04ad0,0x0a4d0,0x0d4d4,0x0d250,0x0d558,0x0b540,0x0b5a0,0x195a6,0x095b0,0x049b0,0x0a974,0x0a4b0,0x0b27a,0x06a50,0x06d40,0x0af46,0x0ab60,0x09570,0x04af5,0x04970,0x064b0,0x074a3,0x0ea50,0x06b58,0x05ac0,0x0ab60,0x096d5,0x092e0,0x0c960,0x0d954,0x0d4a0,0x0da50,0x07552,0x056a0,0x0abb7,0x025d0,0x092d0,0x0cab5,0x0a950,0x0b4a0,0x0baa4,0x0ad50,0x055d9,0x04ba0,0x0a5b0,0x15176,0x052b0,0x0a930,0x07954,0x06aa0,0x0ad50,0x05b52,0x04b60,0x0a6e6,0x0a4e0,0x0d260,0x0ea65,0x0d530,0x05aa0,0x076a3,0x096d0,0x04afb,0x04ad0,0x0a4d0,0x1d0b6,0x0d250,0x0d520,0x0dd45,0x0b5a0,0x056d0,0x055b2,0x049b0,0x0a577,0x0a4b0,0x0aa50,0x1b255,0x06d20,0x0ada0,0x14b63,0x09370,0x049f8,0x04970,0x064b0,0x168a6,0x0ea50,0x06b20,0x1a6c4,0x0aae0,0x092e0,0x0d2e3,0x0c960,0x0d557,0x0d4a0,0x0da50,0x05d55,0x056a0,0x0a6d0,0x055d4,0x052d0,0x0a9b8,0x0a950,0x0b4a0,0x0b6a6,0x0ad50,0x055a0,0x0aba4,0x0a5b0,0x052b0,0x0b273,0x06930,0x07337,0x06aa0,0x0ad50,0x14b55,0x04b60,0x0a570,0x054e4,0x0d160,0x0e968,0x0d520,0x0daa0,0x16aa6,0x056d0,0x04ae0,0x0a9d4,0x0a2d0,0x0d150,0x0f252,0x0d520";

/// 返回今天的农历月-日（MM-DD）；超出数据表范围返回空串。
#[allow(unused_assignments)]
pub fn lunar_md() -> String {
    let now = chrono::Local::now();
    let year = now.year();
    let month = now.month() as i32;
    let day = now.day() as i32;
    let infos: Vec<u32> = LUNAR_INFO
        .split(',')
        .filter_map(|s| u32::from_str_radix(s.trim_start_matches("0x"), 16).ok())
        .collect();
    if year < 1900 || year > 2100 {
        return String::new();
    }
    // 直接用精确算法：从 1900-01-31（农历 1900-01-01）起算
    let base_date = chrono::NaiveDate::from_ymd_opt(1900, 1, 31).unwrap();
    let cur = chrono::NaiveDate::from_ymd_opt(year, month as u32, day as u32).unwrap();
    let days = (cur - base_date).num_days();
    let mut lmonth = 0i32;
    let mut lday2 = 0i32;
    let mut accum = days;
    let mut ly = 1900i32;
    let mut li = 0usize;
    loop {
        let info = *infos.get(li).unwrap_or(&0x04bd8);
        let leap = (info >> 4) & 0x0f;
        let mut year_days = 0i32;
        for m in 1..=12 {
            year_days += if m == 13 { 0 } else { month_days(ly, m as u32, info, leap, 0) };
        }
        if leap > 0 {
            year_days += leap_month_days(ly, info);
        }
        if accum < year_days as i64 {
            break;
        }
        accum -= year_days as i64;
        ly += 1;
        li += 1;
    }
    let info = *infos.get(li).unwrap_or(&0x04bd8);
    let leap = (info >> 4) & 0x0f;
    let mut m = 1u32;
    let mut leap_month_done = false;
    loop {
        let regular = month_days(ly, m, info, leap, 0);
        if accum < regular as i64 {
            lmonth = m as i32;
            lday2 = (accum + 1) as i32;
            break;
        }
        accum -= regular as i64;
        if m as u32 == leap && !leap_month_done {
            let ld = leap_month_days(ly, info);
            if accum < ld as i64 {
                lmonth = m as i32 + 100; // 闰月标记
                lday2 = (accum + 1) as i32;
                break;
            }
            accum -= ld as i64;
            leap_month_done = true;
        }
        m += 1;
        if m > 12 {
            lmonth = 12;
            lday2 = 1;
            break;
        }
    }
    format!("{lmonth:02}-{lday2:02}")
}

#[allow(dead_code)]
fn is_leap_year(year: i32) -> bool {
    let infos: Vec<u32> = LUNAR_INFO
        .split(',')
        .filter_map(|s| u32::from_str_radix(s.trim_start_matches("0x"), 16).ok())
        .collect();
    if year < 1900 || year > 2100 {
        return false;
    }
    let info = infos[(year - 1900) as usize];
    ((info >> 4) & 0x0f) > 0
}

/// 该年某月（1-12）的天数；leap 为该年闰月（0=无闰）。
fn month_days(year: i32, month: u32, info: u32, _leap: u32, _leap_days: i32) -> i32 {
    let _ = year;
    if (info >> (16 - month as u32)) & 1 == 1 {
        30
    } else {
        29
    }
}

/// 闰月天数。
fn leap_month_days(_year: i32, info: u32) -> i32 {
    if info & 0x10000 != 0 {
        30
    } else {
        29
    }
}

// ============ personas ============
/// 默认人格（与 Python local-chat.py DEFAULT_PERSONA 对齐）。
pub fn default_persona() -> Value {
    json!({
        "name": "default",
        "display_name": "默认人格",
        "system_prompt": "你是一个友善的 AI 聊天助手。你有自己的性格和态度，说话自然随意，像朋友一样聊天。",
        "style_prompt": "【说话风格】\n- 轻松自然，像朋友聊天\n- 有自己的态度和想法，不无脑附和\n- 可以适当调侃和开玩笑\n- 回复简洁，1-3句话为主",
        "owner_prompt": "【对用户的态度】\n- 友善、真诚、自然\n- 不过度客气，也不过度热情\n- 像一个值得信赖的朋友",
    })
}

/// personas 存储：文件为 persona 对象**列表**（与 Python / chat.html 契约一致），
/// 当前激活项由配置 ACTIVE_PERSONA 决定。兼容旧 Rust 版对象格式（自动迁移落盘）。
pub struct PersonaStore {
    pub file: PathBuf,
}

impl PersonaStore {
    pub fn new(base_dir: &str) -> Self {
        PersonaStore { file: crate::util::data_path(base_dir, "personas.json") }
    }
    /// 读取 personas 列表；兼容旧版 {"active","personas":{name:{...}}} 对象格式并迁移落盘。
    pub fn load(&self) -> Vec<Value> {
        let raw: Value = load_json(&self.file, json!([]));
        let mut out: Vec<Value> = Vec::new();
        if let Some(arr) = raw.as_array() {
            for item in arr {
                if let Some(obj) = item.as_object() {
                    out.push(Value::Object(obj.clone()));
                }
            }
        } else if let Some(obj) = raw.as_object() {
            if let Some(personas) = obj.get("personas").and_then(|p| p.as_object()) {
                for (name, p) in personas {
                    let mut entry = p.clone();
                    if let Some(eo) = entry.as_object_mut() {
                        eo.entry("name").or_insert_with(|| json!(name));
                        eo.entry("display_name").or_insert_with(|| json!(name));
                        eo.entry("style_prompt").or_insert_with(|| json!(""));
                        eo.entry("owner_prompt").or_insert_with(|| json!(""));
                    }
                    out.push(entry);
                }
                let _ = save_json(&self.file, &out);
            }
        }
        if !out.iter().any(|p| p.get("name").and_then(|n| n.as_str()) == Some("default")) {
            out.insert(0, default_persona());
            let _ = save_json(&self.file, &out);
        }
        out
    }
    pub fn list(&self) -> Vec<Value> {
        self.load()
    }
    pub fn exists(&self, name: &str) -> bool {
        self.load().iter().any(|p| p.get("name").and_then(|n| n.as_str()) == Some(name))
    }
    /// 创建人格：name 做 slug 化（小写、空格→_、去非字母数字_），与 Python 一致。
    pub fn create(&self, name: &str, display_name: &str, system_prompt: &str) -> Result<Value> {
        let slug: String = name
            .to_lowercase()
            .replace(' ', "_")
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        let slug = if slug.is_empty() {
            format!("persona_{}", now_unix())
        } else {
            slug
        };
        let mut list = self.load();
        if list.iter().any(|p| p.get("name").and_then(|n| n.as_str()) == Some(slug.as_str())) {
            return Err(AppError::Other("名称已存在".into()));
        }
        let persona = json!({
            "name": slug,
            "display_name": if display_name.is_empty() { name } else { display_name },
            "system_prompt": system_prompt,
            "style_prompt": "",
            "owner_prompt": "",
            "is_default": false,
        });
        list.push(persona.clone());
        save_json(&self.file, &list)?;
        Ok(persona)
    }
    /// 按 name 更新 display_name/system_prompt/style_prompt/owner_prompt。
    pub fn update(&self, name: &str, fields: &Value) -> Result<()> {
        let mut list = self.load();
        let mut found = false;
        for p in list.iter_mut() {
            if p.get("name").and_then(|n| n.as_str()) == Some(name) {
                if let Some(o) = p.as_object_mut() {
                    for k in ["display_name", "system_prompt", "style_prompt", "owner_prompt"] {
                        if let Some(v) = fields.get(k) {
                            o.insert(k.to_string(), v.clone());
                        }
                    }
                }
                found = true;
                break;
            }
        }
        if !found {
            return Err(AppError::Other("人格不存在".into()));
        }
        save_json(&self.file, &list)
    }
    pub fn delete(&self, name: &str) -> Result<()> {
        let mut list = self.load();
        list.retain(|p| p.get("name").and_then(|n| n.as_str()) != Some(name));
        save_json(&self.file, &list)
    }
    pub fn reset(&self) -> Result<()> {
        save_json(&self.file, &json!([default_persona()]))
    }
    /// 当前激活人格的 system_prompt（active 来自配置 ACTIVE_PERSONA）。
    pub fn active_system_prompt(&self, active: &str) -> String {
        let list = self.load();
        for p in &list {
            if p.get("name").and_then(|n| n.as_str()) == Some(active) {
                return p.get("system_prompt").and_then(|s| s.as_str()).unwrap_or("").to_string();
            }
        }
        list.iter()
            .find(|p| p.get("name").and_then(|n| n.as_str()) == Some("default"))
            .and_then(|p| p.get("system_prompt").and_then(|s| s.as_str()))
            .unwrap_or("")
            .to_string()
    }

    /// 当前激活人格的完整提示词：(system_prompt, style_prompt, owner_prompt)。
    /// 与 Python `_get_active_persona` 对齐：style_prompt 优先于默认说话风格、owner_prompt
    /// 单独注入（Rust 此前只取 system_prompt，导致自定义人格的风格/态度指令全部丢失）。
    pub fn active_persona_full(&self, active: &str) -> (String, String, String) {
        let list = self.load();
        let fallback = json!({});
        let p = match list.iter().find(|p| p.get("name").and_then(|n| n.as_str()) == Some(active)) {
            Some(p) => p,
            None => list
                .iter()
                .find(|p| p.get("name").and_then(|n| n.as_str()) == Some("default"))
                .unwrap_or(&fallback),
        };
        let get = |k: &str| p.get(k).and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
        (get("system_prompt"), get("style_prompt"), get("owner_prompt"))
    }
}
