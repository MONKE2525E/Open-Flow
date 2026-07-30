export const DEFAULT_MODEL = "gemini-3.6-flash-high";
export const DEFAULT_FALLBACK_MODEL = "claude-sonnet-4.6";

const FALLBACK_ERROR_PATTERNS = [
  /\b(?:quota|rate[\s_-]?limit|too many requests|resource exhausted)\b/i,
  /\b429\b/i,
  /\b(?:model|engine)\b.{0,60}\b(?:not found|not available|unavailable|does not exist|unknown)\b/i,
  /\b(?:not found|not available|unavailable|does not exist|unknown)\b.{0,60}\b(?:model|engine)\b/i,
  /\bno available (?:model|engine)\b/i,
];
const SAFE_FAILURE_REASONS = new Set(["quota", "rate_limit", "model_unavailable", "preview_failed", "review_failed", "setup_failed", "provider_not_configured"]);

export function selectReviewModels({
  apiKey,
  primaryModel = DEFAULT_MODEL,
  fallbackModel = DEFAULT_FALLBACK_MODEL,
  additions = 0,
  deletions = 0,
} = {}) {
  if (!apiKey) return null;

  const models = [...new Set([primaryModel, fallbackModel].map((model) => String(model || "").trim()).filter(Boolean))];
  if (models.length === 0) return null;

  return {
    model: models[0],
    fallbackModel: models[1] || null,
    models,
    changedLines: (additions || 0) + (deletions || 0),
  };
}

export function fallbackReason(result) {
  if (!result || result.previewFailed || result.code === 0) return null;

  const output = `${result.stderr || ""}\n${result.stdout || ""}`;
  if (/\b(?:quota|resource exhausted)\b/i.test(output)) return "quota";
  if (/\b(?:rate[\s_-]?limit|too many requests|429)\b/i.test(output)) return "rate_limit";
  if (FALLBACK_ERROR_PATTERNS.slice(2).some((pattern) => pattern.test(output))) return "model_unavailable";
  return null;
}

export function shouldFallback(result, currentModel, fallbackModel) {
  return Boolean(fallbackModel && fallbackModel !== currentModel && fallbackReason(result));
}

export function failureCategory(result) {
  if (!result || result.code === 0) return null;
  if (result?.previewFailed) return "preview_failed";
  return fallbackReason(result) || "review_failed";
}

export function formatProgressSummary({ stage, model, fallbackModel, reason, mode, findings, headSha }) {
  const modelText = model ? ` with \`${model}\`` : "";
  const modeText = mode ? ` (${mode} mode)` : "";
  const shaText = headSha ? `\`${headSha.slice(0, 7)}\`` : "the current commit";

  switch (stage) {
    case "preparing":
      return `👀 Preparing Verenu AI review${modeText}...`;
    case "reviewing":
      return `🔍 Reviewing ${shaText}${modelText}...`;
    case "switching":
      return `🔁 ${model ? `\`${model}\`` : "Primary model"} ${reason === "model_unavailable" ? "unavailable" : reason === "rate_limit" ? "rate-limited" : "quota unavailable"}, switching to ${fallbackModel ? `\`${fallbackModel}\`` : "the fallback model"}...`;
    case "complete":
      return `✅ Verenu AI review complete${modelText}. Reviewed ${shaText} (${findings} finding${findings === 1 ? "" : "s"}).`;
    case "failed":
      return `❌ Verenu AI review failed (${SAFE_FAILURE_REASONS.has(reason) ? reason : "review_failed"}).`;
    default:
      return `👀 Verenu AI review in progress${modelText}...`;
  }
}
