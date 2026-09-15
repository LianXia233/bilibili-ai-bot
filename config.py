"""
配置管理 — 支持动态热更新
配置存储在 config.json 中，可通过前端面板实时修改
"""
import json
import os
import time
import requests
import re
import hashlib
from datetime import datetime

CONFIG_FILE = os.path.join(os.path.dirname(os.path.abspath(__file__)), "config.json")

# ========== 默认配置（首次运行自动生成 config.json） ==========
_DEFAULTS = {
    # B站配置
    "SESSDATA": "",
    "BILI_JCT": "",
    "DEDE_USER_ID": "",
    "OWNER_MID": 0,
    "REFRESH_TOKEN": "",

    # API 配置（全局默认，各模型可单独覆盖）
    "OR_API_KEY": "",
    "OR_BASE_URL": "",

    # 对话模型
    "OR_CHAT_MODEL": "",
    "OR_CHAT_MODEL_FALLBACK": "",
    "OR_CHAT_URL": "",       # 留空=用全局 OR_BASE_URL
    "OR_CHAT_KEY": "",       # 留空=用全局 OR_API_KEY

    # ===== 对话模型池（多套 API/模型配置，面板一键切换）=====
    # 为什么要有池：对话是最常换模型的场景（不同网关的额度、限速、价格差别很大），
    # 只留一套配置时，换个模型就要手改 4 个输入框、改错了还不好回退。
    # 池里每条是 {"name","url","key","model","fallback"}；
    # CHAT_MODEL_ACTIVE 是激活项的 index，-1 表示不使用池（回落上面的单套配置）。
    # 这样旧配置继续可用，池只是可选的一层覆盖。
    "CHAT_MODEL_POOL": [],
    "CHAT_MODEL_ACTIVE": -1,

    # 视觉模型
    "OR_VISION_MODEL": "",
    "OR_VISION_MODEL_FALLBACK": "",
    "OR_VISION_URL": "",
    "OR_VISION_KEY": "",

    # 搜索模型
    "OR_SEARCH_MODEL": "",
    "OR_SEARCH_MODEL_FALLBACK": "",
    "OR_SEARCH_URL": "",
    "OR_SEARCH_KEY": "",

    # 图片生成模型
    "OR_IMAGE_MODEL": "",
    "OR_IMAGE_MODEL_FALLBACK": "",
    "OR_IMAGE_URL": "",
    "OR_IMAGE_KEY": "",

    # Embedding 模型（用于记忆语义检索）
    "SILICON_API_KEY": "",
    "EMBED_BASE_URL": "",
    "EMBED_MODEL": "",

    # ===== 计费价格（$/1M tokens）=====
    # 面板「费用统计」按这些值换算金额；前端提示语明确写着「留空=不计费」。
    # 四个类别与设置页的四组价格输入框一一对应：对话 / 视觉 / 搜索 / 图片。
    # 注意：这些键必须在这里声明，否则 /api/config 不会下发它们，
    # 面板的价格输入框永远是空的，且 Bot 侧无从读取用户填的价格。
    "PRICE_CHAT_INPUT": 0,
    "PRICE_CHAT_OUTPUT": 0,
    "PRICE_VISION_INPUT": 0,
    "PRICE_VISION_OUTPUT": 0,
    "PRICE_SEARCH_INPUT": 0,
    "PRICE_SEARCH_OUTPUT": 0,
    "PRICE_IMAGE_INPUT": 0,
    "PRICE_IMAGE_OUTPUT": 0,

    # ===== 功能开关 =====
    "ENABLE_WEB_SEARCH": True,
    "ENABLE_PROACTIVE": True,
    "ENABLE_DYNAMIC": True,
    "ENABLE_PERSONALITY_EVOLUTION": True,
    "ENABLE_MOOD": True,
    "ENABLE_AFFECTION": True,
    "ENABLE_PRIVATE_MESSAGES": False,
    "PRIVATE_MESSAGE_AUTO_REPLY": True,
    "PRIVATE_MESSAGE_AUTO_BLOCK": True,

    # ===== 私信参数 =====
    "PRIVATE_MESSAGE_REPLY_SCOPE": "all",  # all / owner / whitelist
    "PRIVATE_MESSAGE_REPLY_WHITELIST_UIDS": [],
    "PRIVATE_MESSAGE_BLOCK_WHITELIST_UIDS": [],
    "PRIVATE_MESSAGE_TRUSTED_DOMAINS": ["bilibili.com", "b23.tv"],
    "PRIVATE_MESSAGE_MAX_MESSAGE_AGE": 3600,
    "PRIVATE_MESSAGE_MAX_PER_POLL": 3,

    # ===== 评论参数 =====
    # 「@我的」消息的时效上限（秒）：超过这个时长的 @ 直接跳过、不回复。
    # 与私信侧的 PRIVATE_MESSAGE_MAX_MESSAGE_AGE 同理：B站消息流按时间倒序返回固定条数，
    # 且不读即不消，若不加时效判断，首次启用会对着历史 @ 一次性补发一批回复。
    "AT_REPLY_MAX_AGE": 3600,

    # ===== 主动行为开关 =====
    "PROACTIVE_LIKE": True,
    "PROACTIVE_COIN": False,
    "PROACTIVE_FAV": True,
    "PROACTIVE_FOLLOW": True,
    "PROACTIVE_COMMENT": True,

    # ===== 调度参数 =====
    "PROACTIVE_VIDEO_COUNT": 3,    # 每天刷几个视频
    "PROACTIVE_COMMENT_COUNT": 2,  # 每天评论几条
    "PROACTIVE_TIMES_COUNT": 2,    # 每天触发几次主动评论
    "DYNAMIC_ENABLED": True,
    "EVOLVE_HOUR": 1,              # 性格演化时间（0-23）
    "SLEEP_START": 24,              # 休眠开始（仅在启用休眠时生效）
    "SLEEP_END": 0,                # 休眠结束
    # 休眠总开关。默认 False = 不休眠，机器人全天在线。
    # 为什么要有这个开关：休眠窗口内主循环会直接 continue，既不拉取也不回复，
    # 而「@我的」消息有 AT_REPLY_MAX_AGE 时效，睡 8 小时会把夜间消息全部作废。
    # 要让机器人按 SLEEP_START/SLEEP_END 休息，把这里设为 True。
    "ENABLE_SLEEP": False,

    # ===== 权重参数 =====
    "MOOD_WEIGHT": 0.5,            # 心情对回复的影响程度 0-1

    # ===== Bot 基本信息 =====
    "BOT_NAME": "Bot",
    "BOT_AVATAR": "🤖",
    "USER_AVATAR": "🌙",
    "BOT_WELCOME": "你好，有什么想聊的？",
    "BOT_SUBTITLE": "AI 聊天助手",

    # ===== 人格系统 =====
    "ACTIVE_PERSONA": "default",

    # ===== 主动看视频的 UP主 UID 列表 =====
    "PROACTIVE_FOLLOW_UIDS": [],
    "PREFERRED_TIDS": [17, 160, 211, 3, 13, 167, 321, 36, 129],

    # ===== 自定义提示词（空=用默认） =====
    "PROMPT_DYNAMIC": "",
    "PROMPT_PROACTIVE_COMMENT": "",
    "PROMPT_VIDEO_EVALUATE": "",
    "PROMPT_PERSONALITY_EVOLVE": "",
    "PROMPT_SEARCH_PREFIX": "",
    "PROMPT_IMAGINE": "",
    "PROMPT_PRIVATE_MESSAGE": "",
    "DYNAMIC_TOPICS": [],

    # ===== 主人信息 =====
    "OWNER_NAME": "",
    "OWNER_BILI_NAME": "",

    # ===== Token 预算（面板可调）=====
    # 为什么这些要可配：推理型模型（reasoning）的 max_tokens 是「思考过程 + 正文」
    # 共用的预算。调用方按「短回复」估的 100~400 会被思考过程吃光，结果是正文为空、
    # finish_reason 停在 "length"，上层看到的就是「模型返回空正文」。把预算交给面板，
    # 换模型 / 换网关时不必改代码就能调大，是这条链路的根治手段。
    # 单位：token。数值越小越省 token 但越容易截断；调大只影响上限，不会凭空涨费用。
    # 数值取「一轮成功」而非「最省」：推理型模型被截断时会触发抬升重试，
    # 抬升意味着同一条要调用两次模型，既慢一倍又烧两倍 token。
    # 实测 spark-x2.5-4b：预算 400 必然截断（正文空），2000 一轮成功，8000 反而
    # 因模型「预算越大思考越久」慢到 100 秒以上 —— 所以对白类场景取 2000 附近，
    # 而不是无脑调大。
    "MAX_TOKENS_CHAT": 3000,             # 对话回复（Bot 对话 + 面板聊天/记忆总结）
    "MAX_TOKENS_REPLY": 3000,            # 评论回复 / 私信回复
    "MAX_TOKENS_MEMORY_COMPRESS": 3000,  # 记忆压缩（摘要 + 标签 + 用户事实）
    "MAX_TOKENS_THREAD_COMPRESS": 1000,  # 历史线程压缩（纯摘要）
    "MAX_TOKENS_EVOLVE": 3000,           # 性格演化（结构化 JSON）
    "MAX_TOKENS_SEARCH": 3000,           # 联网搜索
    # 视觉（OCR）类模型不走「思考过程 + 正文」那套推理预算，而是直接产出识别文本，
    # 且 xopdeepseekocr 在 250 预算下实测返回空正文（out_tokens=1, finish=stop）。
    # 注意不能给 8192：实测该网关在 max_tokens=8192 时直接返回 500
    # (server_error code 1001)，4096 才是这个模型的安全上界。
    "MAX_TOKENS_VISION": 4096,           # 视频 / 截图理解
    "MAX_TOKENS_RECOGNIZE": 4096,        # 评论配图识别（同一 OCR 模型）
    "MAX_TOKENS_DYNAMIC": 2000,          # 动态文案生成
    "MAX_TOKENS_PROACTIVE_COMMENT": 2000,# 主动评论 / 推荐语
    "MAX_TOKENS_IMAGE_PROMPT": 1000,     # 生图 prompt 精炼
    # 推理型模型被截断时的兜底抬升预算：正文为空且 finish_reason == "length" 时，
    # 用这个值对同一模型重试一轮。设 0 = 不抬升，完全按上面的场景预算执行。
    "MAX_TOKENS_REASONING_FLOOR": 6000,

    # ===== 模型速率限制（TPM = 每分钟 token 上限，0 = 不限） =====
    # 取值来自各模型在网关侧的实际配额：对白类（spark-x2.5-4b）1,000,000 TPM；
    # 视觉类（DeepSeek-OCR）不做限制 —— 它单次输出仅一两百 token，限流只会
    # 白白拖慢视频分析。真到超配额时表现为 429，届时再往下调。
    "RATE_LIMIT_CHAT_TPM": 1000000,
    "RATE_LIMIT_SEARCH_TPM": 1000000,
    "RATE_LIMIT_VISION_TPM": 0,
    "RATE_LIMIT_IMAGE_TPM": 0,

    # ===== 临时记忆的每日定时清空 =====
    # 「临时记忆」= 对话记忆（memory.json，含压缩摘要）+ 用户档案（user_profiles.json），
    # 也就是「Bot 记得跟我聊过什么、对我有什么印象」这一类会随时间不断堆积的内容。
    # 永久记忆（人格规则）、好感度、性格演化不在此列 —— 那些是长期资产，清掉等于回滚人格。
    #
    # 为什么要有定时清空：对话记忆与用户档案是持续增长的，长期不清理会带来两个问题：
    # 一是语义检索时旧记忆与新记忆混在一起，模型容易拿半年前的印象回答今天的话；
    # 二是 memory.json 越滚越大，每轮读盘 + embedding 比对的成本随之上升。
    # 交给面板配一个每天的低峰时间自动清，比人工记得来点更可靠。
    "TEMP_MEMORY_AUTO_CLEAR": False,   # 总开关，默认关闭（与 ENABLE_SLEEP 同理，不擅自改变现状）
    "TEMP_MEMORY_CLEAR_HOUR": 4,       # 每天几点清（0-23）
    "TEMP_MEMORY_CLEAR_MINUTE": 0,     # 几分清（0-59）
    "TEMP_MEMORY_KEEP_DAYS": 0,        # 保留最近 N 天的记录，0 = 全清（不清空更早的语义）
}

# ========== 加载/保存 ==========
def _load_config():
    """从 config.json 加载配置"""
    if os.path.exists(CONFIG_FILE):
        try:
            with open(CONFIG_FILE, "r", encoding="utf-8") as f:
                saved = json.load(f)
            # 合并：已保存的覆盖默认值，新增的字段用默认值补充
            merged = {**_DEFAULTS, **saved}
            return merged
        except Exception as e:
            print(f"⚠️ 读取 config.json 失败：{e}，使用默认配置")
    return dict(_DEFAULTS)

def _save_config(cfg):
    """保存配置到 config.json"""
    with open(CONFIG_FILE, "w", encoding="utf-8") as f:
        json.dump(cfg, f, ensure_ascii=False, indent=2)

def _ensure_config_file():
    """确保 config.json 存在，不存在则从旧硬编码配置迁移或创建默认"""
    if not os.path.exists(CONFIG_FILE):
        print("📝 首次运行，生成 config.json...")
        _save_config(_DEFAULTS)

_ensure_config_file()
_cfg = _load_config()

# ========== 导出变量（兼容 from config import *） ==========
SESSDATA = _cfg["SESSDATA"]
BILI_JCT = _cfg["BILI_JCT"]
DEDE_USER_ID = _cfg["DEDE_USER_ID"]
OWNER_MID = _cfg["OWNER_MID"]
REFRESH_TOKEN = _cfg.get("REFRESH_TOKEN", "")

OR_API_KEY = _cfg["OR_API_KEY"]
OR_BASE_URL = _cfg["OR_BASE_URL"]
OR_CHAT_MODEL = _cfg["OR_CHAT_MODEL"]
OR_CHAT_MODEL_FALLBACK = _cfg.get("OR_CHAT_MODEL_FALLBACK", "")
OR_CHAT_URL = _cfg.get("OR_CHAT_URL", "")
OR_CHAT_KEY = _cfg.get("OR_CHAT_KEY", "")
OR_SEARCH_MODEL = _cfg["OR_SEARCH_MODEL"]
OR_SEARCH_MODEL_FALLBACK = _cfg.get("OR_SEARCH_MODEL_FALLBACK", "")
OR_SEARCH_URL = _cfg.get("OR_SEARCH_URL", "")
OR_SEARCH_KEY = _cfg.get("OR_SEARCH_KEY", "")
OR_VISION_MODEL = _cfg["OR_VISION_MODEL"]
OR_VISION_MODEL_FALLBACK = _cfg.get("OR_VISION_MODEL_FALLBACK", "")
OR_VISION_URL = _cfg.get("OR_VISION_URL", "")
OR_VISION_KEY = _cfg.get("OR_VISION_KEY", "")
OR_IMAGE_MODEL = _cfg["OR_IMAGE_MODEL"]
OR_IMAGE_MODEL_FALLBACK = _cfg.get("OR_IMAGE_MODEL_FALLBACK", "")
OR_IMAGE_URL = _cfg.get("OR_IMAGE_URL", "")
OR_IMAGE_KEY = _cfg.get("OR_IMAGE_KEY", "")

SILICON_API_KEY = _cfg["SILICON_API_KEY"]

# 功能开关
ENABLE_WEB_SEARCH = _cfg.get("ENABLE_WEB_SEARCH", True)
ENABLE_PROACTIVE = _cfg.get("ENABLE_PROACTIVE", True)
ENABLE_DYNAMIC = _cfg.get("ENABLE_DYNAMIC", True)
ENABLE_PERSONALITY_EVOLUTION = _cfg.get("ENABLE_PERSONALITY_EVOLUTION", True)
ENABLE_MOOD = _cfg.get("ENABLE_MOOD", True)
ENABLE_AFFECTION = _cfg.get("ENABLE_AFFECTION", True)
ENABLE_PRIVATE_MESSAGES = _cfg.get("ENABLE_PRIVATE_MESSAGES", False)
PRIVATE_MESSAGE_AUTO_REPLY = _cfg.get("PRIVATE_MESSAGE_AUTO_REPLY", True)
PRIVATE_MESSAGE_AUTO_BLOCK = _cfg.get("PRIVATE_MESSAGE_AUTO_BLOCK", True)

# 主动行为开关
PROACTIVE_LIKE = _cfg.get("PROACTIVE_LIKE", True)
PROACTIVE_COIN = _cfg.get("PROACTIVE_COIN", False)
PROACTIVE_FAV = _cfg.get("PROACTIVE_FAV", True)
PROACTIVE_FOLLOW = _cfg.get("PROACTIVE_FOLLOW", True)
PROACTIVE_COMMENT = _cfg.get("PROACTIVE_COMMENT", True)

# 调度
PROACTIVE_VIDEO_COUNT = _cfg.get("PROACTIVE_VIDEO_COUNT", 3)
PROACTIVE_COMMENT_COUNT = _cfg.get("PROACTIVE_COMMENT_COUNT", 2)
PROACTIVE_TIMES_COUNT = _cfg.get("PROACTIVE_TIMES_COUNT", 2)
DYNAMIC_ENABLED = _cfg.get("DYNAMIC_ENABLED", True)
EVOLVE_HOUR = _cfg.get("EVOLVE_HOUR", 1)
SLEEP_START = _cfg.get("SLEEP_START", 24)
SLEEP_END = _cfg.get("SLEEP_END", 0)
ENABLE_SLEEP = _cfg.get("ENABLE_SLEEP", False)

# 权重
MOOD_WEIGHT = _cfg.get("MOOD_WEIGHT", 0.5)

# 人格
ACTIVE_PERSONA = _cfg.get("ACTIVE_PERSONA", "default")

# 自定义提示词
PROMPT_DYNAMIC = _cfg.get("PROMPT_DYNAMIC", "")
PROMPT_PROACTIVE_COMMENT = _cfg.get("PROMPT_PROACTIVE_COMMENT", "")
PROMPT_VIDEO_EVALUATE = _cfg.get("PROMPT_VIDEO_EVALUATE", "")
PROMPT_PERSONALITY_EVOLVE = _cfg.get("PROMPT_PERSONALITY_EVOLVE", "")
PROMPT_SEARCH_PREFIX = _cfg.get("PROMPT_SEARCH_PREFIX", "")
PROMPT_IMAGINE = _cfg.get("PROMPT_IMAGINE", "")
PROMPT_PRIVATE_MESSAGE = _cfg.get("PROMPT_PRIVATE_MESSAGE", "")

# ========== 动态更新函数 ==========
def reload_config():
    """重新加载配置（热更新）"""
    global SESSDATA, BILI_JCT, DEDE_USER_ID, OWNER_MID, REFRESH_TOKEN
    global OR_API_KEY, OR_BASE_URL, OR_CHAT_MODEL, OR_SEARCH_MODEL, OR_VISION_MODEL, OR_IMAGE_MODEL
    global OR_CHAT_MODEL_FALLBACK, OR_CHAT_URL, OR_CHAT_KEY
    global OR_SEARCH_MODEL_FALLBACK, OR_SEARCH_URL, OR_SEARCH_KEY
    global OR_VISION_MODEL_FALLBACK, OR_VISION_URL, OR_VISION_KEY
    global OR_IMAGE_MODEL_FALLBACK, OR_IMAGE_URL, OR_IMAGE_KEY
    global SILICON_API_KEY, _cfg
    global ENABLE_WEB_SEARCH, ENABLE_PROACTIVE, ENABLE_DYNAMIC
    global ENABLE_PERSONALITY_EVOLUTION, ENABLE_MOOD, ENABLE_AFFECTION
    global ENABLE_PRIVATE_MESSAGES, PRIVATE_MESSAGE_AUTO_REPLY, PRIVATE_MESSAGE_AUTO_BLOCK
    global PROACTIVE_LIKE, PROACTIVE_COIN, PROACTIVE_FAV, PROACTIVE_FOLLOW, PROACTIVE_COMMENT
    global PROACTIVE_VIDEO_COUNT, PROACTIVE_COMMENT_COUNT, PROACTIVE_TIMES_COUNT
    global DYNAMIC_ENABLED, EVOLVE_HOUR, SLEEP_START, SLEEP_END, ENABLE_SLEEP
    global MOOD_WEIGHT, ACTIVE_PERSONA
    global PROMPT_DYNAMIC, PROMPT_PROACTIVE_COMMENT, PROMPT_VIDEO_EVALUATE
    global PROMPT_PERSONALITY_EVOLVE, PROMPT_SEARCH_PREFIX, PROMPT_IMAGINE
    global PROMPT_PRIVATE_MESSAGE

    _cfg = _load_config()
    SESSDATA = _cfg["SESSDATA"]
    BILI_JCT = _cfg["BILI_JCT"]
    DEDE_USER_ID = _cfg["DEDE_USER_ID"]
    OWNER_MID = _cfg["OWNER_MID"]
    REFRESH_TOKEN = _cfg.get("REFRESH_TOKEN", "")
    OR_API_KEY = _cfg["OR_API_KEY"]
    OR_BASE_URL = _cfg["OR_BASE_URL"]
    OR_CHAT_MODEL = _cfg["OR_CHAT_MODEL"]
    OR_CHAT_MODEL_FALLBACK = _cfg.get("OR_CHAT_MODEL_FALLBACK", "")
    OR_CHAT_URL = _cfg.get("OR_CHAT_URL", "")
    OR_CHAT_KEY = _cfg.get("OR_CHAT_KEY", "")
    OR_SEARCH_MODEL = _cfg["OR_SEARCH_MODEL"]
    OR_SEARCH_MODEL_FALLBACK = _cfg.get("OR_SEARCH_MODEL_FALLBACK", "")
    OR_SEARCH_URL = _cfg.get("OR_SEARCH_URL", "")
    OR_SEARCH_KEY = _cfg.get("OR_SEARCH_KEY", "")
    OR_VISION_MODEL = _cfg["OR_VISION_MODEL"]
    OR_VISION_MODEL_FALLBACK = _cfg.get("OR_VISION_MODEL_FALLBACK", "")
    OR_VISION_URL = _cfg.get("OR_VISION_URL", "")
    OR_VISION_KEY = _cfg.get("OR_VISION_KEY", "")
    OR_IMAGE_MODEL = _cfg["OR_IMAGE_MODEL"]
    OR_IMAGE_MODEL_FALLBACK = _cfg.get("OR_IMAGE_MODEL_FALLBACK", "")
    OR_IMAGE_URL = _cfg.get("OR_IMAGE_URL", "")
    OR_IMAGE_KEY = _cfg.get("OR_IMAGE_KEY", "")
    SILICON_API_KEY = _cfg["SILICON_API_KEY"]
    ENABLE_WEB_SEARCH = _cfg.get("ENABLE_WEB_SEARCH", True)
    ENABLE_PROACTIVE = _cfg.get("ENABLE_PROACTIVE", True)
    ENABLE_DYNAMIC = _cfg.get("ENABLE_DYNAMIC", True)
    ENABLE_PERSONALITY_EVOLUTION = _cfg.get("ENABLE_PERSONALITY_EVOLUTION", True)
    ENABLE_MOOD = _cfg.get("ENABLE_MOOD", True)
    ENABLE_AFFECTION = _cfg.get("ENABLE_AFFECTION", True)
    ENABLE_PRIVATE_MESSAGES = _cfg.get("ENABLE_PRIVATE_MESSAGES", False)
    PRIVATE_MESSAGE_AUTO_REPLY = _cfg.get("PRIVATE_MESSAGE_AUTO_REPLY", True)
    PRIVATE_MESSAGE_AUTO_BLOCK = _cfg.get("PRIVATE_MESSAGE_AUTO_BLOCK", True)
    PROACTIVE_LIKE = _cfg.get("PROACTIVE_LIKE", True)
    PROACTIVE_COIN = _cfg.get("PROACTIVE_COIN", False)
    PROACTIVE_FAV = _cfg.get("PROACTIVE_FAV", True)
    PROACTIVE_FOLLOW = _cfg.get("PROACTIVE_FOLLOW", True)
    PROACTIVE_COMMENT = _cfg.get("PROACTIVE_COMMENT", True)
    PROACTIVE_VIDEO_COUNT = _cfg.get("PROACTIVE_VIDEO_COUNT", 3)
    PROACTIVE_COMMENT_COUNT = _cfg.get("PROACTIVE_COMMENT_COUNT", 2)
    PROACTIVE_TIMES_COUNT = _cfg.get("PROACTIVE_TIMES_COUNT", 2)
    DYNAMIC_ENABLED = _cfg.get("DYNAMIC_ENABLED", True)
    EVOLVE_HOUR = _cfg.get("EVOLVE_HOUR", 1)
    SLEEP_START = _cfg.get("SLEEP_START", 24)
    SLEEP_END = _cfg.get("SLEEP_END", 0)
    ENABLE_SLEEP = _cfg.get("ENABLE_SLEEP", False)
    MOOD_WEIGHT = _cfg.get("MOOD_WEIGHT", 0.5)
    ACTIVE_PERSONA = _cfg.get("ACTIVE_PERSONA", "default")
    PROMPT_DYNAMIC = _cfg.get("PROMPT_DYNAMIC", "")
    PROMPT_PROACTIVE_COMMENT = _cfg.get("PROMPT_PROACTIVE_COMMENT", "")
    PROMPT_VIDEO_EVALUATE = _cfg.get("PROMPT_VIDEO_EVALUATE", "")
    PROMPT_PERSONALITY_EVOLVE = _cfg.get("PROMPT_PERSONALITY_EVOLVE", "")
    PROMPT_SEARCH_PREFIX = _cfg.get("PROMPT_SEARCH_PREFIX", "")
    PROMPT_IMAGINE = _cfg.get("PROMPT_IMAGINE", "")
    PROMPT_PRIVATE_MESSAGE = _cfg.get("PROMPT_PRIVATE_MESSAGE", "")
    return _cfg

def update_config(updates: dict):
    """更新部分配置并保存"""
    cfg = _load_config()
    cfg.update(updates)
    _save_config(cfg)
    reload_config()
    return cfg

def get_config():
    """获取当前配置（脱敏版，隐藏密钥中间部分）"""
    cfg = _load_config()
    safe = {}
    for k, v in cfg.items():
        if isinstance(v, str) and ("KEY" in k or "TOKEN" in k or "SESSDATA" in k or "JCT" in k) and len(v) > 10:
            safe[k] = v[:6] + "***" + v[-4:]
        else:
            safe[k] = v
    # 模型池是列表，上面只按「顶层字符串键名」脱敏，管道进不来 ——
    # 池里的 api key 会被原样下发到前端（面板任何登录用户都能在响应里读到明文 key）。
    # 这里对池单独做一次逐条脱敏。
    if isinstance(safe.get("CHAT_MODEL_POOL"), list):
        pooled = []
        for item in safe["CHAT_MODEL_POOL"]:
            if not isinstance(item, dict):
                continue
            it = dict(item)
            key = str(it.get("key", "") or "")
            if len(key) > 10:
                it["key"] = key[:4] + "***" + key[-4:]
            elif key:
                it["key"] = "***"
            it["has_key"] = bool(key)
            pooled.append(it)
        safe["CHAT_MODEL_POOL"] = pooled
    return safe

def get_raw_config():
    """获取原始配置（不脱敏，仅后端使用）"""
    return _load_config()

# ========== 计费价格解析（前后端共用的单一实现） ==========
# 为什么放在这里：ai.py（Bot 工作进程）与 local-chat.py（面板进程）都要按价格换算金额。
# 两处各写一套匹配规则时，会出现「在面板里改了价格，但 Bot 的调用仍按写死的价格计费」——
# 面板显示与后端真实行为不一致。收敛成一份实现，两边的换算口径才不会再漂移。
_PRICE_SOURCE_KEYWORDS = (
    # 先按「调用来源」判定：来源比模型名更能说明这次调用实际在做什么
    ("PRICE_VISION", ("视频", "识别", "视觉")),
    ("PRICE_SEARCH", ("搜索",)),
    ("PRICE_IMAGE", ("图片", "画图", "绘图")),
)
_PRICE_MODEL_KEYWORDS = (
    # 来源无法判定时，再按模型名兜底
    ("PRICE_VISION", ("vision", "gemini")),
    ("PRICE_IMAGE", ("image", "dall")),
    ("PRICE_SEARCH", ("search",)),
)

def _price_of(cfg, prefix):
    """读取某类别的（输入价, 输出价），单位 $/1M tokens。缺失或非法一律按 0 处理。"""
    out = []
    for suffix in ("_INPUT", "_OUTPUT"):
        try:
            v = float(cfg.get(prefix + suffix, 0) or 0)
        except (TypeError, ValueError):
            v = 0.0
        out.append(max(0.0, v))
    return tuple(out)

def resolve_model_price(source, model=""):
    """按调用来源/模型名解析出 (输入价, 输出价)，单位 $/1M tokens。

    匹配顺序：来源关键词 -> 模型关键词 -> 对话类兜底。
    来源优先很关键：联网搜索走的是 gemini，若先看模型名会被误判成视觉类。
    """
    cfg = get_raw_config()
    src = str(source or "").lower()
    mdl = str(model or "").lower()
    for prefix, kws in _PRICE_SOURCE_KEYWORDS:
        if any(k in src for k in kws):
            return _price_of(cfg, prefix)
    for prefix, kws in _PRICE_MODEL_KEYWORDS:
        if any(k in mdl for k in kws):
            return _price_of(cfg, prefix)
    return _price_of(cfg, "PRICE_CHAT")

# ========== Token 预算解析（面板可调，改完即时生效） ==========
# 与计费价格同理：ai.py（Bot 进程）与 local-chat.py（面板进程）都要读同一份预算，
# 两处各写一套默认值必然漂移。这里收成单一实现。
# 场景名 -> _DEFAULTS 里的键名 / 兜底默认值（键缺失时用）。
_MAX_TOKENS_DEFAULT = {
    "chat": 3000,
    "reply": 3000,
    "memory_compress": 3000,
    "thread_compress": 1000,
    "evolve": 3000,
    "search": 3000,
    "vision": 4096,
    "recognize": 4096,
    "dynamic": 2000,
    "proactive_comment": 2000,
    "image_prompt": 1000,
    "reasoning_floor": 6000,
}

def get_max_tokens(reason, minimum=1):
    """读取某场景的 max_tokens 预算（单位 token）。

    每次读盘而非用模块级快照：面板改完预算后 Bot 进程无需重启即生效，
    与 resolve_model_price 的证据口径一致。

    reason: chat / reply / memory_compress / thread_compress / evolve /
            search / vision / recognize / dynamic / proactive_comment /
            image_prompt / reasoning_floor
    minimum: 下限。默认 1（预算至少 1 token）；reasoning_floor 场景传 0，
             表示允许「不抬升」这一语义。
    """
    key = str(reason or "chat")
    fallback = _MAX_TOKENS_DEFAULT.get(key, 300)
    raw = get_raw_config().get(f"MAX_TOKENS_{key.upper()}", fallback)
    # 注意：不能用 `or fallback`——0 是合法取值（reasoning_floor=0 表示不抬升），
    # 而 `0 or fallback` 会把它替换成兜底值，语义被悄悄改掉。只把真正的空值视为缺失。
    if raw is None or (isinstance(raw, str) and not raw.strip()):
        raw = fallback
    try:
        v = int(float(raw))
    except (TypeError, ValueError):
        v = fallback
    return max(minimum, v)

# ========== 模型速率限制解析（面板可调，改完即时生效） ==========
# 与 Token 预算同理：ai.py（Bot 进程）与 local-chat.py（面板进程）读同一份配置，
# 每次都读盘而不是缓存快照，面板改完无需重启即生效。
# 0 表示「不限」—— 视觉/OCR 类默认不限，因为单次输出只有一两百 token，
# 给它限流只会让视频分析平白多等一轮窗口。
_RATE_LIMIT_DEFAULT = {
    "chat": 1000000,
    "search": 1000000,
    "vision": 0,
    "image": 0,
}

def get_rate_limit(scene):
    """读取某场景的 TPM 上限（每分钟 token 数）。返回 0 表示不限。

    scene: chat / search / vision / image
    """
    key = str(scene or "chat").lower()
    fallback = _RATE_LIMIT_DEFAULT.get(key, 0)
    raw = get_raw_config().get("RATE_LIMIT_%s_TPM" % key.upper(), fallback)
    # 与 get_max_tokens 同款：0 是合法值（不限），不能用 `or fallback` 把它吃掉
    if raw is None or (isinstance(raw, str) and not raw.strip()):
        raw = fallback
    try:
        v = int(float(raw))
    except (TypeError, ValueError):
        v = fallback
    return max(0, v)

# ========== 对话模型池 ==========
# 池的存在意义：对话模型换得最勤（网关额度/限速/价格差异大），单套配置换个模型
# 要手改 4 个输入框，改错了不好回退。池把每套配置存成一条，切换只改一个下标。
# 语义是「可选覆盖」：CHAT_MODEL_ACTIVE = -1 表示不用池，走 OR_CHAT_* 单套配置。
MODEL_POOL_MAX = 20

def get_chat_pool():
    """读取对话模型池（原始结构，含 key，仅供后端使用）。"""
    pool = get_raw_config().get("CHAT_MODEL_POOL", [])
    if not isinstance(pool, list):
        return []
    out = []
    for item in pool:
        if not isinstance(item, dict):
            continue
        out.append({
            "name": str(item.get("name", "") or "").strip(),
            "url": str(item.get("url", "") or "").strip(),
            "key": str(item.get("key", "") or "").strip(),
            "model": str(item.get("model", "") or "").strip(),
            "fallback": str(item.get("fallback", "") or "").strip(),
        })
    return out[:MODEL_POOL_MAX]

def get_chat_pool_masked():
    """池的脱敏版（喂给前端）：key 只留首尾各 4 位。"""
    out = []
    for item in get_chat_pool():
        k = item["key"]
        masked = (k[:4] + "***" + k[-4:]) if len(k) > 10 else ("***" if k else "")
        out.append(dict(item, key=masked, has_key=bool(k)))
    return out

def get_active_chat_model():
    """取当前激活的池条目；未激活或下标越界时返回 None。

    每次读盘而非缓存：面板点「启用」后 Bot 进程无需重启即生效，
    与 get_max_tokens / get_rate_limit 的证据口径一致。
    """
    cfg = get_raw_config()
    try:
        idx = int(cfg.get("CHAT_MODEL_ACTIVE", -1))
    except (TypeError, ValueError):
        idx = -1
    if idx < 0:
        return None
    pool = get_chat_pool()
    if idx >= len(pool):
        # 池被改小（删条目）后下标会越界，此时视为未激活，避免读到错误配置
        return None
    item = pool[idx]
    if not (item["model"] or item["url"] or item["key"]):
        return None
    return item

# ========== 获取各模型的 API 配置 ==========
def get_model_config(model_type):
    """
    获取指定模型类型的 (base_url, api_key, model_id, fallback_model)
    model_type: "chat" / "vision" / "search" / "image"
    每个模型可以有独立的 URL 和 Key，留空则用全局的

    chat 类额外支持「模型池」：若面板启用了池中某条，该条整体覆盖
    OR_CHAT_URL / OR_CHAT_KEY / OR_CHAT_MODEL / OR_CHAT_MODEL_FALLBACK。
    覆盖语义是「整条取代」而不是逐字段合并 —— 池条目里留空的字段
    表示「用全局默认」，若与单套配置逐字段混合，会出现「切换了模型但
    Key 还是上一套的」这种极难排查的状态。
    """
    cfg = _load_config()
    prefix = f"OR_{model_type.upper()}"
    base_url = cfg.get(f"{prefix}_URL", "") or cfg.get("OR_BASE_URL", "")
    api_key = cfg.get(f"{prefix}_KEY", "") or cfg.get("OR_API_KEY", "")
    model_id = cfg.get(f"{prefix}_MODEL", "")
    fallback = cfg.get(f"{prefix}_MODEL_FALLBACK", "")

    if model_type == "chat":
        active = get_active_chat_model()
        if active:
            base_url = active["url"] or cfg.get("OR_BASE_URL", "")
            api_key = active["key"] or cfg.get("OR_API_KEY", "")
            model_id = active["model"]
            fallback = active["fallback"]

    return base_url, api_key, model_id, fallback

# ========== B站 Cookie 有效性检查 ==========
def check_bili_cookie():
    """检查B站cookie是否有效，返回 (valid: bool, info: str)"""
    if not SESSDATA:
        return False, "SESSDATA 为空"
    try:
        url = "https://api.bilibili.com/x/web-interface/nav"
        h = {
            "Cookie": f"SESSDATA={SESSDATA}; bili_jct={BILI_JCT}; DedeUserID={DEDE_USER_ID}",
            "User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36"
        }
        resp = requests.get(url, headers=h, timeout=10)
        data = resp.json()
        if data["code"] == 0:
            uname = data["data"].get("uname", "未知")
            mid = data["data"].get("mid", "")
            level = data["data"].get("level_info", {}).get("current_level", 0)
            return True, f"有效 | {uname} (UID:{mid}) LV{level}"
        else:
            return False, f"Cookie 已失效 (code: {data['code']})"
    except Exception as e:
        return False, f"检查失败: {e}"

# ========== B站 Cookie 自动刷新（完整实现） ==========

# B站 RSA 公钥（用于生成 correspondPath）
_BILI_RSA_PUBLIC_KEY = """-----BEGIN PUBLIC KEY-----
MIGfMA0GCSqGSIb3DQEBAQUAA4GNADCBiQKBgQDLgd2OAkcGVtoE3ThUREbio0Eg
Uc/prcajMKXvkCKFCWhJYJcLkcM2DKKcSeFpD/j6Boy538YXnR6VhcuUJOhH2x71
nzPjfdTcqMz7djHum0qSZA0AyCBDABUqCrfNgCiJ00Ra7GmRj+YCK1NJEuewlb40
JNrRuoEUXpabUzGB8QIDAQAB
-----END PUBLIC KEY-----"""


def _generate_correspond_path(timestamp_ms: int) -> str:
    """
    用 RSA 公钥加密 'refresh_{timestamp_ms}'，生成 correspondPath
    使用 OAEP 填充（SHA-256）
    """
    from cryptography.hazmat.primitives.asymmetric import padding
    from cryptography.hazmat.primitives import hashes, serialization

    # 加载公钥
    public_key = serialization.load_pem_public_key(_BILI_RSA_PUBLIC_KEY.encode())

    # 加密明文
    plaintext = f"refresh_{timestamp_ms}".encode()
    ciphertext = public_key.encrypt(
        plaintext,
        padding.OAEP(
            mgf=padding.MGF1(algorithm=hashes.SHA256()),
            algorithm=hashes.SHA256(),
            label=None
        )
    )

    # 转为十六进制字符串
    return ciphertext.hex()


def check_need_refresh() -> tuple:
    """
    检查 Cookie 是否需要刷新
    返回 (need_refresh: bool, message: str)
    """
    cfg = _load_config()
    sessdata = cfg.get("SESSDATA", "")
    bili_jct = cfg.get("BILI_JCT", "")

    if not sessdata:
        return False, "SESSDATA 为空，无法检查"

    try:
        url = "https://passport.bilibili.com/x/passport-login/web/cookie/info"
        params = {"csrf": bili_jct}
        headers = {
            "Cookie": f"SESSDATA={sessdata}; bili_jct={bili_jct}",
            "User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36"
        }
        resp = requests.get(url, params=params, headers=headers, timeout=10)
        data = resp.json()

        if data["code"] != 0:
            return False, f"检查失败: code={data['code']}, {data.get('message', '')}"

        refresh = data["data"].get("refresh", False)
        timestamp = data["data"].get("timestamp", 0)

        if refresh:
            return True, f"需要刷新 (服务器时间戳: {timestamp})"
        else:
            return False, "Cookie 仍然有效，暂不需要刷新"
    except Exception as e:
        return False, f"检查出错: {e}"


def refresh_bili_cookie():
    """
    完整的 B站 Cookie 刷新流程：
    1. 检查是否需要刷新
    2. RSA 加密生成 correspondPath
    3. 获取 refresh_csrf
    4. 调用刷新接口
    5. 确认更新（用旧 refresh_token）
    返回 (success: bool, message: str)
    """
    cfg = _load_config()
    sessdata = cfg.get("SESSDATA", "")
    bili_jct = cfg.get("BILI_JCT", "")
    rt = cfg.get("REFRESH_TOKEN", "")

    if not rt:
        return False, "没有 REFRESH_TOKEN，无法自动刷新。请在面板中填入 refresh_token（登录时从浏览器 localStorage 的 ac_time_value 或登录接口获取）"

    if not sessdata:
        return False, "SESSDATA 为空，请先手动登录获取 Cookie"

    headers_base = {
        "Cookie": f"SESSDATA={sessdata}; bili_jct={bili_jct}",
        "User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
        "Referer": "https://www.bilibili.com"
    }

    try:
        # === 第1步：检查是否需要刷新 ===
        info_url = "https://passport.bilibili.com/x/passport-login/web/cookie/info"
        info_resp = requests.get(info_url, params={"csrf": bili_jct}, headers=headers_base, timeout=10)
        info_data = info_resp.json()

        if info_data["code"] != 0:
            return False, f"检查刷新状态失败: {info_data.get('message', str(info_data['code']))}"

        need_refresh = info_data["data"].get("refresh", False)
        if not need_refresh:
            return True, "Cookie 仍然有效，无需刷新"

        print("🔄 Cookie 需要刷新，开始刷新流程...")

        # === 第2步：RSA 加密生成 correspondPath ===
        timestamp_ms = int(time.time() * 1000)
        try:
            correspond_path = _generate_correspond_path(timestamp_ms)
        except ImportError:
            return False, "缺少 cryptography 库，请运行: pip install cryptography"
        except Exception as e:
            return False, f"生成 correspondPath 失败: {e}"

        # === 第3步：获取 refresh_csrf ===
        correspond_url = f"https://www.bilibili.com/correspond/1/{correspond_path}"
        csrf_resp = requests.get(correspond_url, headers=headers_base, timeout=10)

        if csrf_resp.status_code != 200:
            return False, f"获取 refresh_csrf 页面失败: HTTP {csrf_resp.status_code}"

        # 从 HTML 中提取 <div id="1-name">xxx</div> 里的 refresh_csrf
        match = re.search(r'<div\s+id="1-name"\s*>([^<]+)</div>', csrf_resp.text)
        if not match:
            return False, f"无法从页面提取 refresh_csrf，页面内容可能已变更"

        refresh_csrf = match.group(1).strip()
        print(f"✅ 获取到 refresh_csrf: {refresh_csrf[:8]}...")

        # === 第4步：调用刷新接口 ===
        refresh_url = "https://passport.bilibili.com/x/passport-login/web/cookie/refresh"
        refresh_data = {
            "csrf": bili_jct,
            "refresh_csrf": refresh_csrf,
            "source": "main_web",
            "refresh_token": rt,
        }
        refresh_resp = requests.post(refresh_url, headers=headers_base, data=refresh_data, timeout=10)
        refresh_result = refresh_resp.json()

        if refresh_result["code"] != 0:
            msg = refresh_result.get("message", str(refresh_result["code"]))
            if refresh_result["code"] == 86095:
                return False, f"刷新失败(86095): refresh_csrf 或 refresh_token 与 cookie 不匹配，可能需要重新登录"
            return False, f"刷新接口返回错误: {msg}"

        # 提取新的 refresh_token
        new_rt = refresh_result["data"].get("refresh_token", "")

        # 从响应 Set-Cookie 中提取新的 SESSDATA 和 bili_jct
        updates = {}
        if new_rt:
            updates["REFRESH_TOKEN"] = new_rt

        for cookie in refresh_resp.cookies:
            if cookie.name == "SESSDATA":
                updates["SESSDATA"] = cookie.value
            elif cookie.name == "bili_jct":
                updates["BILI_JCT"] = cookie.value
            elif cookie.name == "DedeUserID":
                updates["DEDE_USER_ID"] = cookie.value

        if "SESSDATA" not in updates:
            return False, "刷新响应中未找到新的 SESSDATA Cookie"

        # 先保存新 Cookie
        update_config(updates)
        print(f"✅ 新 Cookie 已保存: SESSDATA={updates['SESSDATA'][:8]}...")

        # === 第5步：确认更新（用新 Cookie + 旧 refresh_token） ===
        try:
            confirm_url = "https://passport.bilibili.com/x/passport-login/web/confirm/refresh"
            confirm_headers = {
                "Cookie": f"SESSDATA={updates['SESSDATA']}; bili_jct={updates.get('BILI_JCT', bili_jct)}",
                "User-Agent": headers_base["User-Agent"],
                "Referer": "https://www.bilibili.com"
            }
            confirm_data = {
                "csrf": updates.get("BILI_JCT", bili_jct),
                "refresh_token": rt,  # 注意：这里用的是【旧的】refresh_token
            }
            confirm_resp = requests.post(confirm_url, headers=confirm_headers, data=confirm_data, timeout=10)
            confirm_result = confirm_resp.json()

            if confirm_result["code"] == 0:
                print("✅ 刷新确认成功，旧 refresh_token 已失效")
            else:
                print(f"⚠️ 刷新确认返回: {confirm_result.get('message', confirm_result['code'])}（Cookie 已更新，不影响使用）")
        except Exception as e:
            print(f"⚠️ 刷新确认步骤出错: {e}（Cookie 已更新，不影响使用）")

        return True, f"Cookie 刷新成功！新 SESSDATA: {updates['SESSDATA'][:8]}..."

    except Exception as e:
        return False, f"刷新出错: {e}"

# ========== 临时记忆：分类与清空（面板与 Bot 共用的单一实现） ==========
# 为什么收敛到这里：面板（local-chat.py）要提供「一键清空临时记忆」按钮，
# Bot（ai.py）要按计划每日清空，二者清的东西必须完全一致。
# 各写一套文件清单是最典型的漂移源 —— 面板清了两份、Bot 清了三份，
# 用户看到「清空了但 Bot 还记得」却查不出原因。
#
# 临时 vs 长期的分界（用户定义）：
#   临时 = 对话记忆（memory.json）+ 用户档案（user_profiles.json）
#          —— 随时间自然堆积、清掉只是「忘掉聊过什么」，不影响人格
#   长期 = 永久记忆（人工规则）、好感度、性格演化、视频缓存、当日心情
#          —— 清掉会改变 Bot 的自我认知或主人的关系定位，不在自动清理范围
TEMP_MEMORY_FILES = (
    ("dialog",  "data/memory.json",         [], "对话记忆"),
    ("profile", "data/user_profiles.json",  {}, "用户档案"),
)

# 长期记忆的清单（供面板展示与「全部清空」使用，定时任务永不触碰）
LONG_MEMORY_FILES = (
    ("permanent",   "data/permanent_memory.json",       [], "永久记忆"),
    ("affection",   "data/affection.json",             {}, "好感度"),
    ("video",       "data/video_memory.json",          {}, "视频分析缓存"),
    ("personality", "data/personality_evolution.json",  {}, "性格演化"),
    ("mood",        "data/mood.json",                   {}, "当日心情"),
)

def _load_json_safe(path, empty):
    """读取 JSON，缺失或损坏时返回初值。"""
    import json as _json
    if not os.path.exists(path):
        return empty
    try:
        with open(path, "r", encoding="utf-8") as f:
            return _json.load(f)
    except Exception:
        return empty

def _save_json_safe(path, value):
    """原子写 JSON：先写 .tmp 再替换，避免清空到一半进程被杀导致文件半截。"""
    import json as _json
    d = os.path.dirname(path)
    if d and not os.path.exists(d):
        os.makedirs(d, exist_ok=True)
    tmp = path + ".tmp"
    with open(tmp, "w", encoding="utf-8") as f:
        _json.dump(value, f, ensure_ascii=False, indent=2)
    os.replace(tmp, path)

def _prune_old_records(records, keep_days):
    """按保留天数裁剪记录列表（只对含 time 字段的列表生效）。

    keep_days <= 0 表示全清 —— 这是默认值，语义上等于「临时记忆就该每天归零」。
    设为正数时保留最近 N 天：需要「Bot 有短期记忆但别无限堆积」的场景。
    时间解析失败的单条记录按「保留」处理：宁可多留一条，也不因格式异常误删。
    """
    if not isinstance(records, list):
        return records
    if not keep_days or keep_days <= 0:
        return []
    from datetime import datetime as _dt, timedelta as _td
    cutoff = _dt.now() - _td(days=int(keep_days))
    kept = []
    for r in records:
        if not isinstance(r, dict):
            continue
        ts = str(r.get("time", "") or "").strip()
        if not ts:
            kept.append(r)
            continue
        parsed = None
        for fmt in ("%Y-%m-%d %H:%M", "%Y-%m-%d %H:%M:%S", "%Y-%m-%d"):
            try:
                parsed = _dt.strptime(ts[:len(fmt) + 2].strip(), fmt)
                break
            except ValueError:
                continue
        if parsed is None or parsed >= cutoff:
            kept.append(r)
    return kept

def clear_temp_memory(keep_days=0, backup=True, tag="tempclear"):
    """清空临时记忆。返回 (是否成功, 摘要文本, 明细列表)。

    backup=True 时每个文件先落一份带时间戳的 .bak 再清 —— 定时任务同样要备份：
    「反正有备份」是用户在误清后唯一的回退手段，不能因为是自动执行就省掉。
    """
    from datetime import datetime as _dt
    cleared = []
    for name, path, empty, label in TEMP_MEMORY_FILES:
        before = _load_json_safe(path, empty)
        count = len(before) if isinstance(before, (list, dict)) else 0
        if backup and count:
            try:
                stamp = _dt.now().strftime("%Y%m%d-%H%M%S")
                if os.path.exists(path):
                    with open(path, "rb") as src:
                        data = src.read()
                    with open(f"{path}.{tag}-{stamp}.bak", "wb") as dst:
                        dst.write(data)
            except Exception as e:
                print(f"⚠️ 临时记忆备份失败（{label}）：{e}")
        if isinstance(before, list):
            after = _prune_old_records(before, keep_days)
            if after == before:
                kept = 0
            else:
                kept = len(after)
        else:
            after = {} if not keep_days else before
            kept = len(after) if isinstance(after, dict) else 0
        _save_json_safe(path, after)
        cleared.append({"target": name, "label": label, "removed": count, "kept": kept})
    if not cleared:
        return False, "无可清空项", []
    msg = "；".join(
        f"{c['label']} 清除 {c['removed']} 条" + (f"（保留 {c['kept']} 条）" if c.get("kept") else "")
        for c in cleared
    )
    return True, msg, cleared

def get_temp_clear_plan():
    """读取定时清空配置，返回给面板用的结构（含下次执行时间）。"""
    cfg = get_raw_config()
    try:
        hour = int(cfg.get("TEMP_MEMORY_CLEAR_HOUR", 4))
    except (TypeError, ValueError):
        hour = 4
    try:
        minute = int(cfg.get("TEMP_MEMORY_CLEAR_MINUTE", 0))
    except (TypeError, ValueError):
        minute = 0
    hour = min(23, max(0, hour))
    minute = min(59, max(0, minute))
    try:
        keep_days = int(cfg.get("TEMP_MEMORY_KEEP_DAYS", 0))
    except (TypeError, ValueError):
        keep_days = 0
    keep_days = max(0, keep_days)
    enabled = bool(cfg.get("TEMP_MEMORY_AUTO_CLEAR", False))

    # 下次执行时间：今天该时刻若已过，顺延到明天。
    from datetime import datetime as _dt, timedelta as _td
    now = _dt.now()
    today_at = now.replace(hour=hour, minute=minute, second=0, microsecond=0)
    nxt = today_at if today_at > now else today_at + _td(days=1)
    return {
        "enabled": enabled,
        "hour": hour,
        "minute": minute,
        "keep_days": keep_days,
        "next_run": nxt.strftime("%Y-%m-%d %H:%M") if enabled else "",
        "time_str": f"{hour:02d}:{minute:02d}",
    }
