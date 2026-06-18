use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

/// Deserialize a value that may be a string or a number into `Option<String>`.
/// V3 spec uses numeric timestamps for dates, but we normalize to String.
fn deserialize_string_or_number<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let v = Option::<Value>::deserialize(deserializer)?;
    match v {
        None => Ok(None),
        Some(Value::String(s)) => Ok(Some(s)),
        Some(Value::Number(n)) => Ok(Some(n.to_string())),
        Some(other) => Ok(Some(other.to_string())),
    }
}

// --- Enums ---

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum LoreBookMode {
    #[serde(rename = "multiple")]
    Multiple,
    #[serde(rename = "constant")]
    Constant,
    #[serde(rename = "normal")]
    Normal,
    #[serde(rename = "child")]
    Child,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TriggerType {
    #[serde(rename = "start")]
    Start,
    #[serde(rename = "manual")]
    Manual,
    #[serde(rename = "output")]
    Output,
    #[serde(rename = "input")]
    Input,
    #[serde(rename = "display")]
    Display,
    #[serde(rename = "request")]
    Request,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum FormatingOrderItem {
    #[serde(rename = "main")]
    Main,
    #[serde(rename = "jailbreak")]
    Jailbreak,
    #[serde(rename = "chats")]
    Chats,
    #[serde(rename = "lorebook")]
    Lorebook,
    #[serde(rename = "globalNote")]
    GlobalNote,
    #[serde(rename = "authorNote")]
    AuthorNote,
    #[serde(rename = "lastChat")]
    LastChat,
    #[serde(rename = "description")]
    Description,
    #[serde(rename = "postEverything")]
    PostEverything,
    #[serde(rename = "personaPrompt")]
    PersonaPrompt,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ChatRole {
    #[serde(rename = "user")]
    User,
    #[serde(rename = "bot")]
    Bot,
    #[serde(rename = "system")]
    System,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum CacheRole {
    #[serde(rename = "user")]
    User,
    #[serde(rename = "assistant")]
    Assistant,
    #[serde(rename = "system")]
    System,
    #[serde(rename = "all")]
    All,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum LLMFlags {
    #[serde(rename = "hasImageInput")]
    HasImageInput,
    #[serde(rename = "hasImageOutput")]
    HasImageOutput,
    #[serde(rename = "hasAudioInput")]
    HasAudioInput,
    #[serde(rename = "hasAudioOutput")]
    HasAudioOutput,
    #[serde(rename = "hasPrefill")]
    HasPrefill,
    #[serde(rename = "hasCache")]
    HasCache,
    #[serde(rename = "hasFullSystemPrompt")]
    HasFullSystemPrompt,
    #[serde(rename = "hasFirstSystemPrompt")]
    HasFirstSystemPrompt,
    #[serde(rename = "hasStreaming")]
    HasStreaming,
    #[serde(rename = "requiresAlternateRole")]
    RequiresAlternateRole,
    #[serde(rename = "mustStartWithUserInput")]
    MustStartWithUserInput,
    #[serde(rename = "poolSupported")]
    PoolSupported,
    #[serde(rename = "hasVideoInput")]
    HasVideoInput,
    #[serde(rename = "OAICompletionTokens")]
    OaiCompletionTokens,
    #[serde(rename = "DeveloperRole")]
    DeveloperRole,
    #[serde(rename = "geminiThinking")]
    GeminiThinking,
    #[serde(rename = "geminiBlockOff")]
    GeminiBlockOff,
    #[serde(rename = "deepSeekPrefix")]
    DeepSeekPrefix,
    #[serde(rename = "deepSeekThinkingInput")]
    DeepSeekThinkingInput,
    #[serde(rename = "deepSeekThinkingOutput")]
    DeepSeekThinkingOutput,
    #[serde(rename = "noCivilIntegrity")]
    NoCivilIntegrity,
    #[serde(rename = "claudeThinking")]
    ClaudeThinking,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum LLMFormat {
    OpenAICompatible,
    OpenAILegacyInstruct,
    Anthropic,
    AnthropicLegacy,
    Mistral,
    GoogleCloud,
    VertexAIGemini,
    NovelList,
    Cohere,
    NovelAI,
    WebLLM,
    OobaLegacy,
    Plugin,
    Ooba,
    Kobold,
    Ollama,
    Horde,
    AWSBedrockClaude,
}

// --- Core data types ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoreBook {
    pub key: String,
    pub secondkey: String,
    pub insertorder: i32,
    pub comment: String,
    pub content: String,
    pub mode: LoreBookMode,
    #[serde(rename = "alwaysActive")]
    pub always_active: bool,
    pub selective: bool,
    #[serde(rename = "extentions")]
    pub extensions: Option<LoreBookExtensions>,
    #[serde(rename = "activationPercent")]
    pub activation_percent: Option<f64>,
    #[serde(rename = "loreCache")]
    pub lore_cache: Option<LoreCache>,
    #[serde(rename = "useRegex")]
    pub use_regex: Option<bool>,
    #[serde(rename = "bookVersion")]
    pub book_version: Option<u32>,
    pub id: Option<String>,
    #[serde(flatten, default)]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoreBookExtensions {
    pub risu_case_sensitive: bool,
    #[serde(flatten, default)]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoreCache {
    pub key: String,
    pub data: Vec<String>,
    #[serde(flatten, default)]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomScript {
    pub comment: String,
    #[serde(rename = "in")]
    pub pattern: String,
    pub out: String,
    #[serde(rename = "type")]
    pub script_type: String,
    pub flag: Option<String>,
    #[serde(rename = "ableFlag")]
    pub able_flag: Option<bool>,
    #[serde(flatten, default)]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerScript {
    pub comment: String,
    #[serde(rename = "type")]
    pub trigger_type: TriggerType,
    pub conditions: Value,
    pub effect: Value,
    #[serde(rename = "lowLevelAccess")]
    pub low_level_access: Option<bool>,
    #[serde(flatten, default)]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RisuModule {
    pub name: String,
    pub description: String,
    pub lorebook: Option<Vec<LoreBook>>,
    pub regex: Option<Vec<CustomScript>>,
    pub cjs: Option<String>,
    pub trigger: Option<Vec<TriggerScript>>,
    pub id: String,
    #[serde(rename = "lowLevelAccess")]
    pub low_level_access: Option<bool>,
    #[serde(rename = "hideIcon")]
    pub hide_icon: Option<bool>,
    #[serde(rename = "backgroundEmbedding")]
    pub background_embedding: Option<String>,
    pub assets: Option<Vec<(String, String, String)>>,
    pub namespace: Option<String>,
    #[serde(rename = "customModuleToggle")]
    pub custom_module_toggle: Option<String>,
    #[serde(flatten, default)]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}

// --- Prompt template items ---

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum PromptItem {
    #[serde(rename = "plain")]
    Plain {
        type2: String,
        text: String,
        role: ChatRole,
        name: Option<String>,
    },
    #[serde(rename = "jailbreak")]
    Jailbreak {
        type2: String,
        text: String,
        role: ChatRole,
        name: Option<String>,
    },
    #[serde(rename = "cot")]
    Cot {
        type2: String,
        text: String,
        role: ChatRole,
        name: Option<String>,
    },
    #[serde(rename = "persona")]
    Persona {
        #[serde(rename = "innerFormat")]
        inner_format: Option<String>,
        name: Option<String>,
    },
    #[serde(rename = "description")]
    Description {
        #[serde(rename = "innerFormat")]
        inner_format: Option<String>,
        name: Option<String>,
    },
    #[serde(rename = "lorebook")]
    Lorebook {
        #[serde(rename = "innerFormat")]
        inner_format: Option<String>,
        name: Option<String>,
    },
    #[serde(rename = "postEverything")]
    PostEverything {
        #[serde(rename = "innerFormat")]
        inner_format: Option<String>,
        name: Option<String>,
    },
    #[serde(rename = "memory")]
    Memory {
        #[serde(rename = "innerFormat")]
        inner_format: Option<String>,
        name: Option<String>,
    },
    #[serde(rename = "authornote")]
    AuthorNote {
        #[serde(rename = "innerFormat")]
        inner_format: Option<String>,
        #[serde(rename = "defaultText")]
        default_text: Option<String>,
        name: Option<String>,
    },
    #[serde(rename = "chat")]
    Chat {
        #[serde(rename = "rangeStart")]
        range_start: i32,
        #[serde(rename = "rangeEnd")]
        range_end: RangeEnd,
        #[serde(rename = "chatAsOriginalOnSystem")]
        chat_as_original_on_system: Option<bool>,
        name: Option<String>,
    },
    #[serde(rename = "chatML")]
    ChatML { text: String, name: Option<String> },
    #[serde(rename = "cache")]
    Cache {
        name: String,
        depth: i32,
        role: CacheRole,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RangeEnd {
    Index(i32),
    End(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptSettings {
    #[serde(rename = "assistantPrefill")]
    pub assistant_prefill: String,
    #[serde(rename = "postEndInnerFormat")]
    pub post_end_inner_format: String,
    #[serde(rename = "sendChatAsSystem")]
    pub send_chat_as_system: bool,
    #[serde(rename = "sendName")]
    pub send_name: bool,
    #[serde(rename = "utilOverride")]
    pub util_override: bool,
    #[serde(rename = "customChainOfThought")]
    pub custom_chain_of_thought: Option<bool>,
    #[serde(rename = "maxThoughtTagDepth")]
    pub max_thought_tag_depth: Option<i32>,
    #[serde(flatten, default)]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SeparateParameters {
    pub temperature: Option<f64>,
    pub top_k: Option<f64>,
    pub repetition_penalty: Option<f64>,
    pub min_p: Option<f64>,
    pub top_a: Option<f64>,
    pub top_p: Option<f64>,
    pub frequency_penalty: Option<f64>,
    pub presence_penalty: Option<f64>,
    pub reasoning_effort: Option<f64>,
    pub thinking_tokens: Option<i32>,
    #[serde(flatten, default)]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SeparateParametersSet {
    #[serde(default)]
    pub memory: SeparateParameters,
    #[serde(default)]
    pub emotion: SeparateParameters,
    #[serde(default)]
    pub translate: SeparateParameters,
    #[serde(rename = "otherAx", default)]
    pub other_ax: SeparateParameters,
    #[serde(flatten, default)]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}

// --- Settings types ---

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OobaFormatting {
    #[serde(default)]
    pub header: String,
    #[serde(default, rename = "systemPrefix")]
    pub system_prefix: String,
    #[serde(default, rename = "userPrefix")]
    pub user_prefix: String,
    #[serde(default, rename = "assistantPrefix")]
    pub assistant_prefix: String,
    #[serde(default)]
    pub seperator: String,
    #[serde(default, rename = "useName")]
    pub use_name: bool,
    #[serde(flatten, default)]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OobaSettings {
    #[serde(default)]
    pub max_new_tokens: u32,
    #[serde(default)]
    pub do_sample: bool,
    #[serde(default)]
    pub temperature: f64,
    #[serde(default)]
    pub top_p: f64,
    #[serde(default)]
    pub typical_p: f64,
    #[serde(default)]
    pub repetition_penalty: f64,
    #[serde(default)]
    pub encoder_repetition_penalty: f64,
    #[serde(default)]
    pub top_k: u32,
    #[serde(default)]
    pub min_length: u32,
    #[serde(default)]
    pub no_repeat_ngram_size: u32,
    #[serde(default)]
    pub num_beams: u32,
    #[serde(default)]
    pub penalty_alpha: f64,
    #[serde(default)]
    pub length_penalty: f64,
    #[serde(default)]
    pub early_stopping: bool,
    #[serde(default)]
    pub seed: i64,
    #[serde(default)]
    pub add_bos_token: bool,
    #[serde(default)]
    pub truncation_length: u32,
    #[serde(default)]
    pub ban_eos_token: bool,
    #[serde(default)]
    pub skip_special_tokens: bool,
    #[serde(default)]
    pub top_a: f64,
    #[serde(default)]
    pub tfs: f64,
    #[serde(default)]
    pub epsilon_cutoff: f64,
    #[serde(default)]
    pub eta_cutoff: f64,
    #[serde(default)]
    pub formating: OobaFormatting,
    #[serde(flatten, default)]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AinSettings {
    #[serde(default)]
    pub top_p: f64,
    #[serde(default)]
    pub rep_pen: f64,
    #[serde(default)]
    pub top_a: f64,
    #[serde(default)]
    pub rep_pen_slope: f64,
    #[serde(default)]
    pub rep_pen_range: u32,
    #[serde(default)]
    pub typical_p: f64,
    #[serde(default)]
    pub badwords: String,
    #[serde(default)]
    pub stoptokens: String,
    #[serde(default)]
    pub top_k: u32,
    #[serde(flatten, default)]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NaiSettings {
    #[serde(rename = "topK")]
    pub top_k: u32,
    #[serde(rename = "topP")]
    pub top_p: f64,
    #[serde(rename = "topA")]
    pub top_a: f64,
    #[serde(rename = "tailFreeSampling")]
    pub tail_free_sampling: f64,
    #[serde(rename = "repetitionPenalty")]
    pub repetition_penalty: f64,
    #[serde(rename = "repetitionPenaltyRange")]
    pub repetition_penalty_range: u32,
    #[serde(rename = "repetitionPenaltySlope")]
    pub repetition_penalty_slope: f64,
    #[serde(rename = "repostitionPenaltyPresence")]
    pub reposition_penalty_presence: f64,
    pub seperator: String,
    #[serde(rename = "frequencyPenalty")]
    pub frequency_penalty: f64,
    #[serde(rename = "presencePenalty")]
    pub presence_penalty: f64,
    pub typicalp: f64,
    pub starter: String,
    pub mirostat_lr: Option<f64>,
    pub mirostat_tau: Option<f64>,
    pub cfg_scale: Option<f64>,
    #[serde(flatten, default)]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OobaChatCompletionParams {
    pub mode: String,
    pub turn_template: Option<String>,
    pub name1_instruct: Option<String>,
    pub name2_instruct: Option<String>,
    pub context_instruct: Option<String>,
    pub system_message: Option<String>,
    pub name1: Option<String>,
    pub name2: Option<String>,
    pub context: Option<String>,
    pub greeting: Option<String>,
    pub chat_instruct_command: Option<String>,
    pub preset: Option<String>,
    pub tokenizer: Option<String>,
    pub min_p: Option<f64>,
    pub top_k: Option<u32>,
    pub repetition_penalty: Option<f64>,
    pub repetition_penalty_range: Option<u32>,
    pub typical_p: Option<f64>,
    pub tfs: Option<f64>,
    pub top_a: Option<f64>,
    pub epsilon_cutoff: Option<f64>,
    pub eta_cutoff: Option<f64>,
    pub guidance_scale: Option<f64>,
    pub negative_prompt: Option<String>,
    pub penalty_alpha: Option<f64>,
    pub mirostat_mode: Option<u32>,
    pub mirostat_tau: Option<f64>,
    pub mirostat_eta: Option<f64>,
    pub temperature_last: Option<bool>,
    pub do_sample: Option<bool>,
    pub seed: Option<i64>,
    pub encoder_repetition_penalty: Option<f64>,
    pub no_repeat_ngram_size: Option<u32>,
    pub min_length: Option<u32>,
    pub num_beams: Option<u32>,
    pub length_penalty: Option<f64>,
    pub early_stopping: Option<bool>,
    pub truncation_length: Option<u32>,
    pub max_tokens_second: Option<u32>,
    pub custom_token_bans: Option<String>,
    pub auto_max_new_tokens: Option<bool>,
    pub ban_eos_token: Option<bool>,
    pub add_bos_token: Option<bool>,
    pub skip_special_tokens: Option<bool>,
    pub grammar_string: Option<String>,
    #[serde(flatten, default)]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}

// --- Bot preset ---

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BotPreset {
    #[serde(flatten)]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default, rename = "apiType")]
    pub api_type: Option<String>,
    #[serde(default, rename = "openAIKey")]
    pub openai_key: Option<String>,
    #[serde(default, rename = "mainPrompt")]
    pub main_prompt: String,
    #[serde(default)]
    pub jailbreak: String,
    #[serde(default, rename = "globalNote")]
    pub global_note: String,
    #[serde(default)]
    pub temperature: f64,
    #[serde(default, rename = "maxContext")]
    pub max_context: u32,
    #[serde(default, rename = "maxResponse")]
    pub max_response: u32,
    #[serde(default, rename = "frequencyPenalty")]
    pub frequency_penalty: f64,
    #[serde(default, rename = "PresensePenalty")]
    pub presence_penalty: f64,
    #[serde(default, rename = "formatingOrder")]
    pub formating_order: Vec<FormatingOrderItem>,
    #[serde(rename = "aiModel")]
    pub ai_model: Option<String>,
    #[serde(rename = "subModel")]
    pub sub_model: Option<String>,
    #[serde(rename = "currentPluginProvider")]
    pub current_plugin_provider: Option<String>,
    #[serde(rename = "textgenWebUIStreamURL")]
    pub textgen_stream_url: Option<String>,
    #[serde(rename = "textgenWebUIBlockingURL")]
    pub textgen_blocking_url: Option<String>,
    #[serde(rename = "forceReplaceUrl")]
    pub force_replace_url: Option<String>,
    #[serde(rename = "forceReplaceUrl2")]
    pub force_replace_url2: Option<String>,
    #[serde(default, rename = "promptPreprocess")]
    pub prompt_preprocess: bool,
    #[serde(default)]
    pub bias: Vec<(String, f64)>,
    #[serde(rename = "proxyKey")]
    pub proxy_key: Option<String>,
    #[serde(default)]
    pub ooba: OobaSettings,
    #[serde(default)]
    pub ainconfig: AinSettings,
    #[serde(rename = "koboldURL")]
    pub kobold_url: Option<String>,
    #[serde(rename = "NAISettings")]
    pub nai_settings: Option<NaiSettings>,
    #[serde(rename = "promptTemplate")]
    pub prompt_template: Option<Vec<PromptItem>>,
    #[serde(rename = "reverseProxyOobaArgs")]
    pub reverse_proxy_ooba_args: Option<OobaChatCompletionParams>,
    pub top_p: Option<f64>,
    #[serde(rename = "promptSettings")]
    pub prompt_settings: Option<PromptSettings>,
    pub repetition_penalty: Option<f64>,
    pub min_p: Option<f64>,
    pub top_a: Option<f64>,
    pub top_k: Option<i32>,
    #[serde(rename = "useInstructPrompt")]
    pub use_instruct_prompt: Option<bool>,
    pub regex: Option<Vec<CustomScript>>,
    #[serde(rename = "proxyRequestModel")]
    pub proxy_request_model: Option<String>,
    #[serde(rename = "openrouterRequestModel")]
    pub openrouter_request_model: Option<String>,
    #[serde(rename = "NAIadventure")]
    pub nai_adventure: Option<bool>,
    #[serde(rename = "NAIappendName")]
    pub nai_append_name: Option<bool>,
    #[serde(rename = "localStopStrings")]
    pub local_stop_strings: Option<Value>,
    #[serde(rename = "customProxyRequestModel")]
    pub custom_proxy_request_model: Option<String>,
    #[serde(rename = "autoSuggestPrompt")]
    pub auto_suggest_prompt: Option<String>,
    #[serde(rename = "autoSuggestPrefix")]
    pub auto_suggest_prefix: Option<String>,
    #[serde(rename = "autoSuggestClean")]
    pub auto_suggest_clean: Option<bool>,
    #[serde(rename = "openrouterProvider")]
    pub openrouter_provider: Option<Value>,
    #[serde(rename = "customPromptTemplateToggle")]
    pub custom_prompt_template_toggle: Option<String>,
    #[serde(rename = "templateDefaultVariables")]
    pub template_default_variables: Option<String>,
    #[serde(rename = "moduleIntergration")]
    pub module_integration: Option<String>,
    #[serde(rename = "instructChatTemplate")]
    pub instruct_chat_template: Option<String>,
    #[serde(rename = "JinjaTemplate")]
    pub jinja_template: Option<String>,
    #[serde(rename = "jsonSchemaEnabled")]
    pub json_schema_enabled: Option<bool>,
    #[serde(rename = "jsonSchema")]
    pub json_schema: Option<String>,
    #[serde(rename = "strictJsonSchema")]
    pub strict_json_schema: Option<bool>,
    #[serde(rename = "extractJson")]
    pub extract_json: Option<String>,
    #[serde(rename = "groupTemplate")]
    pub group_template: Option<String>,
    #[serde(rename = "groupOtherBotRole")]
    pub group_other_bot_role: Option<String>,
    #[serde(rename = "customAPIFormat")]
    pub custom_api_format: Option<serde_json::Value>,
    #[serde(rename = "systemContentReplacement")]
    pub system_content_replacement: Option<String>,
    #[serde(rename = "systemRoleReplacement")]
    pub system_role_replacement: Option<String>,
    #[serde(rename = "openAIPrediction")]
    pub openai_prediction: Option<String>,
    #[serde(rename = "seperateParametersEnabled")]
    pub separate_parameters_enabled: Option<bool>,
    #[serde(rename = "seperateParameters")]
    pub separate_parameters: Option<SeparateParametersSet>,
    #[serde(rename = "seperateModelsForAxModels")]
    pub separate_models_for_ax: Option<bool>,
    #[serde(rename = "seperateModels")]
    pub separate_models: Option<serde_json::Value>,
    #[serde(rename = "fallbackModels")]
    pub fallback_models: Option<serde_json::Value>,
    #[serde(rename = "fallbackWhenBlankResponse")]
    pub fallback_when_blank: Option<bool>,
    pub verbosity: Option<i32>,
    #[serde(rename = "modelTools")]
    pub model_tools: Option<Vec<String>>,
    #[serde(rename = "dynamicOutput")]
    pub dynamic_output: Option<serde_json::Value>,
    #[serde(rename = "enableCustomFlags")]
    pub enable_custom_flags: Option<bool>,
    #[serde(rename = "customFlags")]
    pub custom_flags: Option<Vec<serde_json::Value>>,
    pub image: Option<String>,
    #[serde(rename = "reasonEffort")]
    pub reason_effort: Option<f64>,
    #[serde(rename = "thinkingTokens")]
    pub thinking_tokens: Option<i32>,
}

// --- Character card types ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharBookEntry {
    pub keys: Vec<String>,
    pub content: String,
    pub extensions: Value,
    pub enabled: bool,
    pub insertion_order: i32,
    pub name: Option<String>,
    pub priority: Option<i32>,
    pub id: Option<i32>,
    pub comment: Option<String>,
    pub selective: Option<bool>,
    pub secondary_keys: Option<Vec<String>>,
    pub constant: Option<bool>,
    pub position: Option<String>,
    pub case_sensitive: Option<bool>,
    pub use_regex: Option<bool>,
    #[serde(flatten, default)]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterBook {
    pub name: Option<String>,
    pub description: Option<String>,
    pub scan_depth: Option<u32>,
    pub token_budget: Option<u32>,
    pub recursive_scanning: Option<bool>,
    pub extensions: Value,
    pub entries: Vec<CharBookEntry>,
    #[serde(flatten, default)]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RisuAiExtensions {
    pub emotions: Option<Vec<(String, String)>>,
    pub bias: Option<Vec<(String, f64)>>,
    #[serde(rename = "viewScreen")]
    pub view_screen: Option<Value>,
    #[serde(rename = "customScripts")]
    pub custom_scripts: Option<Vec<CustomScript>>,
    #[serde(rename = "utilityBot")]
    pub utility_bot: Option<bool>,
    #[serde(rename = "sdData")]
    pub sd_data: Option<Vec<(String, String)>>,
    #[serde(rename = "additionalAssets")]
    pub additional_assets: Option<Vec<(String, String, String)>>,
    #[serde(rename = "backgroundHTML")]
    pub background_html: Option<String>,
    pub license: Option<String>,
    pub triggerscript: Option<Vec<TriggerScript>>,
    pub private: Option<bool>,
    #[serde(rename = "additionalText")]
    pub additional_text: Option<String>,
    pub virtualscript: Option<String>,
    #[serde(rename = "largePortrait")]
    pub large_portrait: Option<bool>,
    #[serde(rename = "lorePlus")]
    pub lore_plus: Option<bool>,
    #[serde(rename = "inlayViewScreen")]
    pub inlay_view_screen: Option<bool>,
    #[serde(rename = "newGenData")]
    pub new_gen_data: Option<NewGenData>,
    pub vits: Option<std::collections::HashMap<String, String>>,
    #[serde(flatten, default)]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewGenData {
    pub prompt: String,
    pub negative: String,
    pub instructions: String,
    #[serde(rename = "emotionInstructions")]
    pub emotion_instructions: String,
    #[serde(flatten, default)]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepthPrompt {
    pub depth: u32,
    pub prompt: String,
    #[serde(flatten, default)]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CardExtensions {
    pub risuai: Option<RisuAiExtensions>,
    pub depth_prompt: Option<DepthPrompt>,
    #[serde(flatten, default)]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterCardV2Data {
    pub name: String,
    pub description: String,
    pub personality: String,
    pub scenario: String,
    pub first_mes: String,
    pub mes_example: String,
    pub creator_notes: String,
    pub system_prompt: String,
    pub post_history_instructions: String,
    pub alternate_greetings: Vec<String>,
    pub character_book: Option<CharacterBook>,
    pub tags: Vec<String>,
    pub creator: String,
    pub character_version: String,
    pub extensions: CardExtensions,
    // V3-specific fields (Option for V2 backward compatibility)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nickname: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_string_or_number"
    )]
    pub creation_date: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_string_or_number"
    )]
    pub modification_date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_only_greetings: Option<Vec<String>>,
    #[serde(flatten, default)]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterCardV2Risu {
    pub spec: String,
    pub spec_version: String,
    pub data: CharacterCardV2Data,
    #[serde(flatten, default)]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OldTavernChar {
    pub avatar: String,
    pub chat: String,
    pub create_date: String,
    pub description: String,
    pub first_mes: String,
    pub mes_example: String,
    pub name: String,
    pub personality: String,
    pub scenario: String,
    pub talkativeness: String,
    pub spec_version: Option<String>,
    #[serde(flatten, default)]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}

// --- Preset envelope (for .risup format) ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresetEnvelope {
    #[serde(rename = "presetVersion")]
    pub preset_version: u32,
    #[serde(rename = "type")]
    pub envelope_type: String,
    #[serde(with = "serde_bytes")]
    pub preset: Vec<u8>,
}

// --- RisuAI Message ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub data: String,
    pub time: Option<u64>,
    #[serde(rename = "chatId")]
    pub chat_id: Option<String>,
    #[serde(rename = "isPinned")]
    pub is_pinned: Option<bool>,
}
