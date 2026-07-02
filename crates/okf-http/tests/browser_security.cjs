"use strict";

const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");
const {
  classifyMarkdownUrl,
  escapeHtml,
  isValidPairingCode,
  normalizePairingCode
} = require("../browser/security.js");

const origin = "http://127.0.0.1:8003";
const base = `${origin}/okf-docs/okf/concepts/current.md`;

test("allows only OKF same-origin document routes", () => {
  assert.equal(classifyMarkdownUrl("../target.md#part", base, origin)?.kind, "document");
  assert.equal(
    classifyMarkdownUrl("../target.md#part", base, origin)?.documentPath,
    "/okf-docs/okf/target.md#part"
  );
  assert.equal(classifyMarkdownUrl("/api/okf/voyage/index", base, origin), null);
  assert.equal(classifyMarkdownUrl("/api/v1/access/pair", base, origin), null);
  assert.equal(classifyMarkdownUrl("/api/v1/access/logout", base, origin), null);
  assert.equal(classifyMarkdownUrl("/docs-browser/app.js", base, origin), null);
});

test("allows safe external schemes and rejects active or local schemes", () => {
  assert.equal(classifyMarkdownUrl("https://example.com/path", base, origin)?.kind, "external");
  assert.equal(classifyMarkdownUrl("http://example.com/path", base, origin)?.kind, "external");
  assert.equal(classifyMarkdownUrl("mailto:reader@example.com", base, origin)?.kind, "external");
  for (const target of [
    "javascript:alert(1)",
    "java\nscript:alert(1)",
    "data:text/html,<script>alert(1)</script>",
    "file:///etc/passwd",
    "https://user:secret@example.com/",
    "https://example.com/\nheader: value",
    "http://["
  ]) {
    assert.equal(classifyMarkdownUrl(target, base, origin), null, target);
  }
});

test("escapes every HTML attribute delimiter", () => {
  assert.equal(
    escapeHtml(`&<>"' onmouseover="alert(1)`),
    "&amp;&lt;&gt;&quot;&#39; onmouseover=&quot;alert(1)"
  );
});

test("normalizes and validates only the human pairing-code format", () => {
  assert.equal(normalizePairingCode("1234 5678-9012 extra"), "1234-5678-9012");
  assert.equal(normalizePairingCode("<script>123456789012"), "1234-5678-9012");
  assert.equal(isValidPairingCode("1234-5678-9012"), true);
  assert.equal(isValidPairingCode("123456789012"), false);
  assert.equal(isValidPairingCode("1234-5678-901x"), false);
});

test("browser pairing keeps authority out of persistent and rendered storage", () => {
  const browserRoot = path.join(__dirname, "..", "browser");
  const app = fs.readFileSync(path.join(browserRoot, "app.js"), "utf8");
  const html = fs.readFileSync(path.join(browserRoot, "index.html"), "utf8");

  assert.match(app, /\/api\/v1\/access\/pair/u);
  assert.match(app, /\/api\/v1\/access\/login/u);
  assert.match(app, /\/api\/v1\/access\/session\/refresh/u);
  assert.match(app, /\/api\/v1\/access\/logout/u);
  assert.match(app, /X-OKF-CSRF-Token/u);
  assert.match(app, /credentials:\s*"same-origin"/u);
  assert.doesNotMatch(app, /X-OKF-Session-Token/u);
  assert.doesNotMatch(app, /localStorage|sessionStorage/u);
  assert.doesNotMatch(app, /textContent\s*=\s*sessionCsrfToken/u);
  assert.doesNotMatch(html, /ai-session-token|X-OKF-Session-Token/u);
  assert.match(html, /<dialog id="pairing-dialog"/u);
  assert.match(html, /<dialog id="login-dialog"/u);
  assert.match(html, /autocomplete="username"/u);
  assert.match(html, /autocomplete="current-password"/u);
  assert.match(html, /aria-labelledby="pairing-title"/u);
  assert.match(app, /Run a Voyage query embedding/u);
  assert.match(app, /Write \$\{accepted\} accepted relation/u);
  assert.match(app, /every suggestion in this review set/u);
  assert.match(html, /id="mode-roots"/u);
  assert.match(html, /id="root-reviewed"/u);
  assert.match(html, /id="root-source-confirm"/u);
  assert.match(app, /\/api\/v1\/roots\/proposals/u);
  assert.match(app, /X-OKF-Config-Write/u);
  assert.match(app, /X-OKF-Source-Write/u);
  assert.match(app, /operation:\s*"source_initialization"|createRootProposal\("source_initialization"\)/u);
  assert.doesNotMatch(html, /type="file"/u);
  assert.doesNotMatch(app, /force[_-]admit|unsafe[_-]override/iu);
  assert.match(html, /id="root-change-review"/u);
  assert.match(html, /Dismissal does not approve files or move the baseline/u);
  assert.match(app, /\/monitoring\/check/u);
  assert.match(app, /\/monitoring\/pending/u);
  assert.match(app, /X-OKF-Change-Review/u);
  assert.match(app, /Filtering never changes the reviewed snapshot/u);
  assert.doesNotMatch(app, /setInterval\([^)]*monitoring\/check/su);
});

test("CSV rendering escapes cells and never evaluates formula markers", () => {
  const browserRoot = path.join(__dirname, "..", "browser");
  const app = fs.readFileSync(path.join(browserRoot, "app.js"), "utf8");
  assert.match(app, /header\.forEach\(\(cell\) => table\.push\(`<th>\$\{escapeHtml\(cell\)\}<\/th>`\)\)/u);
  assert.match(app, /row\.forEach\(\(cell\) => table\.push\(`<td>\$\{escapeHtml\(cell\)\}<\/td>`\)\)/u);
  assert.match(app, /spreadsheet formula marker/u);
  assert.doesNotMatch(app, /eval\s*\(/u);
});
