#!/usr/bin/env node
// Verenu AI Review — drives Open Code Review (`ocr`) against a pull request.
//
// Security model:
//  - Runs under pull_request_target, so this process has repo write secrets.
//  - The PR head is NEVER checked out via actions/checkout. It is fetched as
//    git object data (refs/pull/<n>/head) into the trusted base checkout.
//  - `ocr review` is tried first against those git objects directly, with no
//    working tree matching the PR head. Only if that fails is a worktree
//    materialized, and then only in a quarantined temp directory with hooks
//    disabled, LFS smudge disabled, no submodules, and no execution of
//    anything from it — OCR reads files, nothing in this script or workflow
//    ever runs PR-supplied code, scripts, or installs PR dependencies.
//  - A cheap `--preview` check runs before every real (LLM-billed) review
//    call, at each candidate location, so a broken git state fails fast
//    without spending tokens.
//  - OCR's own config/telemetry/MCP state is isolated: its child process
//    gets HOME pointed at a fresh temp directory for the whole run, so it
//    can never read or persist ~/.opencodereview/config.json, shell rc
//    files, or any pre-existing MCP/tool config from the runner. No --tools
//    flag is ever passed and no MCP server is ever configured — only OCR's
//    built-in review tools run.
//  - All PR content (diff, title, body, comments) is data for the reviewer
//    model to inspect, never instructions this script or OCR should obey.

import { spawn } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";

const GITHUB_API = process.env.GITHUB_API_URL || "https://api.github.com";
const [OWNER, REPO] = requireEnv("GITHUB_REPOSITORY").split("/");
const TOKEN = requireEnv("GITHUB_TOKEN");
const RULE_FILE_PATH = ".github/verenu-ocr-rules.json";
const STATE_MARKER = "<!-- verenu-ai-review-state:v1";
const SHA_RE = /^[0-9a-f]{40}$/i;
const REF_RE = /^[A-Za-z0-9._/-]+$/;

function requireEnv(name) {
  const value = process.env[name];
  if (!value) throw new Error(`missing required env var ${name}`);
  return value;
}

async function gh(pathOrUrl, init = {}) {
  const url = pathOrUrl.startsWith("http") ? pathOrUrl : `${GITHUB_API}${pathOrUrl}`;
  const res = await fetch(url, {
    ...init,
    headers: {
      Authorization: `Bearer ${TOKEN}`,
      Accept: "application/vnd.github+json",
      "X-GitHub-Api-Version": "2022-11-28",
      ...(init.headers || {}),
    },
  });
  if (!res.ok) {
    const body = await res.text().catch(() => "");
    throw new Error(`GitHub API ${res.status} ${url}: ${body.slice(0, 500)}`);
  }
  return res.status === 204 ? null : res.json();
}

function run(cmd, args, opts = {}) {
  return new Promise((resolve, reject) => {
    const child = spawn(cmd, args, { ...opts, shell: false });
    let stdout = "";
    let stderr = "";
    child.stdout?.setEncoding("utf8");
    child.stderr?.setEncoding("utf8");
    child.stdout?.on("data", (d) => (stdout += d));
    child.stderr?.on("data", (d) => (stderr += d));
    child.on("error", reject);
    child.on("close", (code) => resolve({ code, stdout, stderr }));
  });
}

async function git(args, opts = {}) {
  const result = await run("git", args, { ...opts, env: { ...process.env, GIT_LFS_SKIP_SMUDGE: "1", ...opts.env } });
  if (result.code !== 0) {
    throw new Error(`git ${args[0]} failed (${result.code}): ${result.stderr.slice(0, 800)}`);
  }
  return result;
}

// --- event context -------------------------------------------------------

function loadEvent() {
  return JSON.parse(readFileSync(requireEnv("GITHUB_EVENT_PATH"), "utf8"));
}

async function resolveContext() {
  const eventName = requireEnv("GITHUB_EVENT_NAME");
  const event = loadEvent();

  if (eventName === "pull_request_target") {
    const pr = event.pull_request;
    return { prNumber: pr.number, pr, mode: "normal", forceFull: false, trusted: true };
  }

  if (eventName === "issue_comment") {
    if (!event.issue.pull_request) return null; // comment on a plain issue, ignore

    const body = (event.comment.body || "").trim();
    const match = /^\/verenu-review\b(.*)$/i.exec(body);
    if (!match) return null;
    const flags = match[1].toLowerCase();

    const author = event.comment?.user?.login;
    if (!author) return null;

    let perm;
    try {
      perm = await gh(`/repos/${OWNER}/${REPO}/collaborators/${encodeURIComponent(author)}/permission`);
    } catch (err) {
      // Non-collaborators (and anyone GitHub won't disclose permission for)
      // get a 404 here — that's an expected "ignore" case, not a crash.
      console.log(`ignoring /verenu-review from ${author}: failed to fetch permission: ${err.message}`);
      return null;
    }
    if (!["admin", "write"].includes(perm.permission)) {
      console.log(`ignoring /verenu-review from ${author}: permission=${perm.permission}`);
      return null;
    }

    const pr = await gh(`/repos/${OWNER}/${REPO}/pulls/${event.issue.number}`);
    return {
      prNumber: event.issue.number,
      pr,
      mode: flags.includes("security") ? "security" : "normal",
      forceFull: flags.includes("full"),
      trusted: false,
    };
  }

  return null;
}

// --- bot state comment ----------------------------------------------------

async function findStateComment(prNumber) {
  const comments = await gh(`/repos/${OWNER}/${REPO}/issues/${prNumber}/comments?per_page=100`);
  return comments.find((c) => c.body && c.body.includes(STATE_MARKER)) || null;
}

function parseState(comment) {
  if (!comment) return null;
  const start = comment.body.indexOf(STATE_MARKER);
  const end = comment.body.indexOf("-->", start);
  if (start === -1 || end === -1) return null;
  try {
    return JSON.parse(comment.body.slice(start + STATE_MARKER.length, end).trim());
  } catch {
    return null;
  }
}

async function upsertStateComment(prNumber, existing, summary, state) {
  const body = `${summary}\n\n${STATE_MARKER} ${JSON.stringify(state)} -->`;
  if (existing) {
    await gh(`/repos/${OWNER}/${REPO}/issues/comments/${existing.id}`, {
      method: "PATCH",
      body: JSON.stringify({ body }),
    });
  } else {
    await gh(`/repos/${OWNER}/${REPO}/issues/${prNumber}/comments`, {
      method: "POST",
      body: JSON.stringify({ body }),
    });
  }
}

// --- provider selection ----------------------------------------------------

function selectProvider(pr, mode) {
  const changedLines = (pr.additions || 0) + (pr.deletions || 0);
  const wantEscalate = changedLines > 3000 || mode === "security";
  const haveZai = !!process.env.ZAI_API_KEY;
  const haveDeepseek = !!process.env.DEEPSEEK_API_KEY;

  if (wantEscalate && haveZai) return { provider: "zai", model: "glm-5.2", fellBack: false, changedLines };
  if (haveDeepseek) return { provider: "deepseek", model: "deepseek-v4-pro", fellBack: wantEscalate, changedLines };
  if (haveZai) return { provider: "zai", model: "glm-5.2", fellBack: false, changedLines };
  return null;
}

// Both DeepSeek and Z.ai are OpenAI-protocol-compatible endpoints, never
// Anthropic-protocol — OCR_USE_ANTHROPIC is forced to "false" for both.
function providerEnv(provider) {
  if (provider === "deepseek") {
    return {
      OCR_LLM_URL: process.env.DEEPSEEK_BASE_URL || "https://api.deepseek.com/v1",
      OCR_LLM_TOKEN: process.env.DEEPSEEK_API_KEY,
      OCR_LLM_MODEL: "deepseek-v4-pro",
      OCR_USE_ANTHROPIC: "false",
    };
  }
  if (provider === "zai") {
    return {
      OCR_LLM_URL: process.env.ZAI_BASE_URL || "https://api.z.ai/api/paas/v4",
      OCR_LLM_TOKEN: process.env.ZAI_API_KEY,
      OCR_LLM_MODEL: "glm-5.2",
      OCR_USE_ANTHROPIC: "false",
    };
  }
  throw new Error(`unknown provider ${provider}`);
}

// --- git object fetch (no checkout of PR head) -----------------------------

async function fetchPrCommits(pr) {
  if (!SHA_RE.test(pr.base.sha) || !SHA_RE.test(pr.head.sha)) {
    throw new Error("base/head sha failed format validation");
  }
  if (!REF_RE.test(pr.base.ref)) {
    throw new Error("base ref failed format validation");
  }
  await git(["fetch", "--no-tags", "--no-recurse-submodules", "origin", pr.base.ref]);
  await git(["fetch", "--no-tags", "--no-recurse-submodules", "origin", `refs/pull/${pr.number}/head`]);
}

// --- OCR invocation ---------------------------------------------------------

// OCR_HOME isolates OCR's config/telemetry/MCP state from the runner's real
// HOME for the whole run: no --tools flag, no MCP server config, and OCR
// cannot read or write any pre-existing ~/.opencodereview state.
function makeOcrHome() {
  return mkdtempSync(path.join(tmpdir(), "verenu-ocr-home-"));
}

async function runOcrAt(cwd, args, providerEnvVars, ocrHome) {
  const childEnv = {
    PATH: process.env.PATH,
    HOME: ocrHome,
    ...providerEnvVars,
  };
  return run("ocr", args, { cwd, env: childEnv });
}

async function previewOk(cwd, pr, providerEnvVars, ocrHome) {
  const result = await runOcrAt(cwd, ["review", "--from", pr.base.sha, "--to", pr.head.sha, "--preview"], providerEnvVars, ocrHome);
  if (result.code !== 0) {
    console.log(`ocr preview check failed at ${cwd}: exit ${result.code}: ${result.stderr.slice(0, 300)}`);
    return false;
  }
  return true;
}

function ocrReviewArgs({ baseSha, headSha, model, background }) {
  return [
    "review",
    "--from", baseSha,
    "--to", headSha,
    "--format", "json",
    "--model", model,
    "--audience", "agent",
    "--rule", RULE_FILE_PATH,
    "--background", background,
    "--concurrency", "2",
    "--timeout", "10",
    "--max-git-procs", "2",
  ];
}

async function reviewWithQuarantinedWorktree(pr, args, providerEnvVars, ocrHome) {
  const quarantineDir = mkdtempSync(path.join(tmpdir(), "verenu-pr-quarantine-"));
  try {
    // Reviewed security exception: OCR needs real files on disk to read via
    // its tool-use, so we materialize the PR head here — detached, hooks
    // disabled, no submodules, no LFS smudge, and nothing in this tree is
    // ever executed or installed from.
    await git(["worktree", "add", "--detach", "--no-track", quarantineDir, pr.head.sha]);
    await git(["config", "core.hooksPath", "/dev/null"], { cwd: quarantineDir });

    if (!(await previewOk(quarantineDir, pr, providerEnvVars, ocrHome))) {
      return { code: 1, stdout: "", stderr: "ocr preview check failed in the quarantined worktree; aborting before the billed review" };
    }
    return await runOcrAt(quarantineDir, args, providerEnvVars, ocrHome);
  } finally {
    try {
      await git(["worktree", "remove", "--force", quarantineDir]);
    } catch {
      rmSync(quarantineDir, { recursive: true, force: true });
    }
  }
}

function parseOcrFindings(stdout) {
  let data;
  try {
    data = JSON.parse(stdout);
  } catch (err) {
    console.error(`failed to parse OCR findings JSON: ${err.message}`);
    console.error(`raw stdout: ${stdout.slice(0, 2000)}`);
    return [];
  }
  const list = Array.isArray(data) ? data : data.findings || data.issues || data.results || [];
  return list
    .map((f) => ({
      file: f.file || f.path || f.filename,
      line: f.line || f.line_number || f.startLine,
      severity: f.severity || f.level || "info",
      message: f.message || f.description || f.body || "",
    }))
    .filter((f) => f.message);
}

async function postFindings(prNumber, pr, findings) {
  if (findings.length === 0) return;

  const positioned = findings.filter((f) => f.file && f.line);
  const unpositioned = findings.filter((f) => !f.file || !f.line);

  if (positioned.length > 0) {
    try {
      await gh(`/repos/${OWNER}/${REPO}/pulls/${prNumber}/reviews`, {
        method: "POST",
        body: JSON.stringify({
          commit_id: pr.head.sha,
          event: "COMMENT",
          comments: positioned.map((f) => ({ path: f.file, line: Number(f.line), body: `**[${f.severity}]** ${f.message}` })),
        }),
      });
    } catch (err) {
      console.log(`inline review post failed, falling back to a summary comment: ${err.message}`);
      unpositioned.push(...positioned);
      positioned.length = 0;
    }
  }

  if (unpositioned.length > 0) {
    const body = [
      "**Verenu AI Review — findings**",
      "",
      ...unpositioned.map((f) => `- \`${f.file || "unknown"}:${f.line || "?"}\` [${f.severity}] ${f.message}`),
    ].join("\n");
    await gh(`/repos/${OWNER}/${REPO}/issues/${prNumber}/comments`, {
      method: "POST",
      body: JSON.stringify({ body }),
    });
  }
}

// --- main -------------------------------------------------------------------

async function main() {
  const ctx = await resolveContext();
  if (!ctx) {
    console.log("no actionable event; exiting");
    return;
  }
  const { prNumber, pr, mode, forceFull } = ctx;

  if (ctx.trusted && pr.draft) {
    console.log("PR is draft; skipping automatic review");
    return;
  }

  const existingComment = await findStateComment(prNumber);
  const existingState = parseState(existingComment);

  if (!forceFull && existingState && existingState.headSha === pr.head.sha && existingState.completed) {
    // Don't touch the existing state comment here — it holds the last real
    // review's findings, and headSha already matches, so there's nothing to
    // update. Rewriting it would destroy that history for a no-op skip.
    console.log(`head ${pr.head.sha} already reviewed (mode=${existingState.mode}); skipping`);
    return;
  }

  const selection = selectProvider(pr, mode);
  if (!selection) {
    await upsertStateComment(
      prNumber,
      existingComment,
      "No AI review provider is configured (missing `DEEPSEEK_API_KEY` and `ZAI_API_KEY`). Skipping automated review.",
      existingState || { prNumber, headSha: pr.head.sha, mode, model: null, timestamp: new Date().toISOString(), completed: false },
    );
    return;
  }

  // Short, run-specific context only. The durable Verenu review policy lives
  // in .github/verenu-review-rules.md via the OCR rule file (--rule), not here.
  const background =
    mode === "security"
      ? "Automated Verenu PR review (mode: security). Prioritize security-relevant defects this run."
      : `Automated Verenu PR review (mode: ${mode}).`;

  await fetchPrCommits(pr);

  const providerEnvVars = providerEnv(selection.provider);
  const args = ocrReviewArgs({ baseSha: pr.base.sha, headSha: pr.head.sha, model: selection.model, background });
  const ocrHome = makeOcrHome();

  console.log(`running ocr review: mode=${mode} provider=${selection.provider} model=${selection.model} changedLines=${selection.changedLines}`);

  try {
    let result = null;
    let usedWorktree = false;

    if (await previewOk(process.cwd(), pr, providerEnvVars, ocrHome)) {
      result = await runOcrAt(process.cwd(), args, providerEnvVars, ocrHome);
    } else {
      console.log("git-object-only preview failed; skipping straight to the quarantined worktree fallback");
    }

    if (!result || result.code !== 0) {
      if (result) {
        console.log("git-object-only OCR run failed; retrying with a quarantined read-only worktree");
        console.log(`ocr exit ${result.code}: ${result.stderr.slice(0, 300)}`);
      }
      usedWorktree = true;
      result = await reviewWithQuarantinedWorktree(pr, args, providerEnvVars, ocrHome);
    }

    if (!result || result.code !== 0) {
      console.error("OCR review failed even with a materialized worktree; stopping rather than weakening the checkout security model");
      console.error((result?.stderr || "").slice(0, 800));
      process.exitCode = 1;
      return;
    }

    const findings = parseOcrFindings(result.stdout);
    await postFindings(prNumber, pr, findings);

    const fallbackNote = selection.fellBack ? " (escalation requested — GLM-5.2 unavailable, fell back to DeepSeek V4 Pro)" : "";
    const summary = [
      `**Verenu AI Review** — ${mode} mode, \`${selection.model}\`${fallbackNote}.`,
      `Reviewed \`${pr.head.sha.slice(0, 7)}\` (${findings.length} finding${findings.length === 1 ? "" : "s"})${usedWorktree ? ", using a quarantined read-only worktree" : ""}.`,
    ].join("\n");

    await upsertStateComment(prNumber, existingComment, summary, {
      prNumber,
      headSha: pr.head.sha,
      mode,
      model: selection.model,
      provider: selection.provider,
      timestamp: new Date().toISOString(),
      completed: true,
    });
  } finally {
    rmSync(ocrHome, { recursive: true, force: true });
  }
}

main().catch((err) => {
  console.error(err.message);
  process.exitCode = 1;
});
