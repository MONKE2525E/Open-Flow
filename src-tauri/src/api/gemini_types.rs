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
