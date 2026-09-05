use serde::Deserialize;

/// Shared Gemini generateContent response types used by both transcription.rs and cleanup.rs.
#[derive(Deserialize, Debug)]
pub struct GeminiResp {
    pub candidates: Option<Vec<GeminiCandidate>>,
    #[serde(rename = "promptFeedback")]
    pub prompt_feedback: Option<serde_json::Value>,
}

#[derive(Deserialize, Debug)]
pub struct GeminiCandidate {
    pub content: Option<GeminiContent>,
    #[serde(rename = "finishReason")]
    pub finish_reason: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct GeminiContent {
    pub parts: Vec<GeminiPart>,
}

#[derive(Deserialize, Debug)]
pub struct GeminiPart {
    pub text: Option<String>,
}

use serde::Serialize;

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GeminiTranscribeReq {
    pub contents: Vec<GeminiReqContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generation_config: Option<GeminiGenConfig>,
}

/// Request body for Google's dedicated audio transcription model. Unlike
/// general Gemini audio understanding, this model is exposed through the
/// Interactions API rather than generateContent.
#[derive(Serialize, Debug)]
pub struct GeminiInteractionTranscribeReq {
    pub model: String,
    pub input: Vec<GeminiInteractionInput>,
}

#[derive(Serialize, Debug)]
#[serde(tag = "type")]
pub enum GeminiInteractionInput {
    #[serde(rename = "audio")]
    Audio { data: String, mime_type: String },
    #[serde(rename = "text")]
    Text { text: String },
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GeminiGenerateReq {
    pub contents: Vec<GeminiReqContent>,
    pub system_instruction: GeminiReqContent,
    pub generation_config: GeminiGenConfig,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GeminiReqContent {
    pub parts: Vec<GeminiReqPart>,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GeminiReqPart {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inline_data: Option<GeminiInlineData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GeminiInlineData {
    pub mime_type: String,
    pub data: String,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GeminiGenConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_config: Option<GeminiThinkingConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GeminiThinkingConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_budget: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_level: Option<String>,
}
