import assert from "node:assert/strict";
import test from "node:test";

import {
  DEFAULT_FALLBACK_MODEL,
  DEFAULT_MODEL,
  failureCategory,
  fallbackReason,
  formatProgressSummary,
  normalizeReviewModel,
  reviewOutcome,
  selectReviewModels,
  shouldFallback,
} from "./verenu-ai-review-logic.mjs";

test("selectReviewModels returns Gemini then Claude defaults", () => {
  assert.deepEqual(
    selectReviewModels({ apiKey: "present" }),
    {
      model: DEFAULT_MODEL,
      fallbackModel: DEFAULT_FALLBACK_MODEL,
      models: [DEFAULT_MODEL, DEFAULT_FALLBACK_MODEL],
      changedLines: 0,
    },
  );
});

test("legacy configured model names migrate to the current review models", () => {
  assert.equal(normalizeReviewModel("gemini-3.6-flash-high", DEFAULT_MODEL), DEFAULT_MODEL);
  assert.equal(normalizeReviewModel("claude-sonnet-4.6", DEFAULT_FALLBACK_MODEL), DEFAULT_FALLBACK_MODEL);
  assert.deepEqual(selectReviewModels({
    apiKey: "present",
    primaryModel: "gemini-3.6-flash-high",
    fallbackModel: "claude-sonnet-4.6",
  }).models, [DEFAULT_MODEL, DEFAULT_FALLBACK_MODEL]);
});

test("selectReviewModels preserves configured ordering and removes duplicates", () => {
  assert.deepEqual(
    selectReviewModels({
      apiKey: "present",
      primaryModel: "custom-primary",
      fallbackModel: "custom-primary",
      additions: 4,
      deletions: 3,
    }),
    {
      model: "custom-primary",
      fallbackModel: null,
      models: ["custom-primary"],
      changedLines: 7,
    },
  );
});

test("selectReviewModels coerces numeric change counts", () => {
  assert.equal(selectReviewModels({ apiKey: "present", additions: "10", deletions: "5" }).changedLines, 15);
  assert.equal(selectReviewModels({ apiKey: "present", additions: "not-a-number", deletions: 5 }).changedLines, 5);
});

test("selectReviewModels rejects blank API keys", () => {
  assert.equal(selectReviewModels({ apiKey: "   " }), null);
});

test("quota, rate-limit, and model-unavailable failures are fallback eligible", () => {
  const cases = [
    [{ code: 1, stderr: "RESOURCE_EXHAUSTED: quota exceeded" }, "quota"],
    [{ code: 1, stderr: "status=RESOURCE_EXHAUSTED" }, "quota"],
    [{ code: 1, stderr: "reason=QUOTA_EXHAUSTED" }, "quota"],
    [{ code: 1, stderr: "insufficient_quota: daily limit reached" }, "quota"],
    [{ code: 1, stderr: "model_cooldown: all credentials are cooling down" }, "quota"],
    [{ code: 1, stderr: "HTTP 429 too many requests" }, "rate_limit"],
    [{ code: 1, stderr: "model gemini-3.7-flash-high not found" }, "model_unavailable"],
    [{ code: 1, stderr: "no available model for this request" }, "model_unavailable"],
    [{ code: 1, stderr: "model gemini-3.7-flash-high\nnot found" }, "model_unavailable"],
    [{ code: 1, stderr: "MODEL_NOT_FOUND: gemini-3.7-flash-high" }, "model_unavailable"],
  ];

  for (const [result, reason] of cases) {
    assert.equal(fallbackReason(result), reason);
    assert.equal(shouldFallback(result, DEFAULT_MODEL, DEFAULT_FALLBACK_MODEL), true);
  }
});

test("opaque review failures fallback, while infrastructure preview failures do not", () => {
  assert.equal(fallbackReason({ code: 1, stderr: "401 unauthorized" }), null);
  assert.equal(fallbackReason({ code: 1, stderr: "timeout" }), "review_failed");
  assert.equal(shouldFallback({ code: 1, stderr: "timeout" }, DEFAULT_MODEL, DEFAULT_FALLBACK_MODEL), true);
  assert.equal(shouldFallback({ code: 1, previewFailed: true, stderr: "429" }, DEFAULT_MODEL, DEFAULT_FALLBACK_MODEL), true);
  assert.equal(shouldFallback({ code: 1, previewFailed: true, stderr: "model unavailable" }, DEFAULT_MODEL, DEFAULT_FALLBACK_MODEL), true);
  assert.equal(shouldFallback({ code: 1, previewFailed: true, stderr: "fatal: bad object" }, DEFAULT_MODEL, DEFAULT_FALLBACK_MODEL), false);
  assert.equal(shouldFallback({ code: 0, stderr: "quota exceeded" }, DEFAULT_MODEL, DEFAULT_FALLBACK_MODEL), false);
  assert.equal(failureCategory({ code: 1, previewFailed: true }), "preview_failed");
  assert.equal(failureCategory({ code: 1, previewFailed: true, stderr: "model unavailable" }), "model_unavailable");
  assert.equal(failureCategory({ code: 1, stderr: "401 unauthorized" }), "review_failed");
  assert.equal(failureCategory({ code: 0, stderr: "success" }), null);
});

test("clean reviews pass and reviews with findings fail", () => {
  assert.deepEqual(reviewOutcome([]), { count: 0, hasFindings: false, exitCode: 0 });
  assert.deepEqual(reviewOutcome([{ message: "bug" }]), { count: 1, hasFindings: true, exitCode: 1 });
});

test("progress summaries expose the expected review stages without provider details", () => {
  assert.match(formatProgressSummary({ stage: "preparing", mode: "normal" }), /^👀 Preparing/);
  assert.match(formatProgressSummary({ stage: "reviewing", model: DEFAULT_MODEL, headSha: "1234567890abcdef" }), /🔍 Reviewing `1234567`/);
  assert.match(formatProgressSummary({ stage: "reviewing", headSha: 1234567 }), /current commit/);
  const quotaSwitch = formatProgressSummary({ stage: "switching", model: DEFAULT_MODEL, fallbackModel: DEFAULT_FALLBACK_MODEL, reason: "quota" });
  assert.ok(quotaSwitch.includes(`\`${DEFAULT_MODEL}\` quota unavailable`));
  assert.ok(quotaSwitch.includes(`switching to \`${DEFAULT_FALLBACK_MODEL}\``));
  assert.match(formatProgressSummary({ stage: "switching", model: "custom-primary", fallbackModel: "custom-fallback", reason: "model_unavailable" }), /`custom-primary` unavailable, switching to `custom-fallback`/);
  assert.match(formatProgressSummary({ stage: "complete", model: DEFAULT_MODEL, findings: 0, headSha: "1234567890abcdef" }), /0 findings/);
  assert.match(formatProgressSummary({ stage: "findings", model: DEFAULT_MODEL, findings: 2, headSha: "1234567890abcdef" }), /found 2 findings/);
  assert.match(formatProgressSummary({ stage: "complete", model: DEFAULT_FALLBACK_MODEL, findings: 2, headSha: "1234567890abcdef" }), /✅ .*2 findings/);
  assert.equal(formatProgressSummary({ stage: "failed", reason: "quota exceeded for secret-token" }), "❌ Verenu AI review failed (review_failed).");
});
