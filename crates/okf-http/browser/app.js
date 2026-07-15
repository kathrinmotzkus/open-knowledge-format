let DOCS = [];
const {
  classifyMarkdownUrl,
  escapeHtml,
  isValidPairingCode,
  normalizePairingCode
} = window.OkfSecurity;

const searchEl = document.getElementById("search");
const treeEl = document.getElementById("tree");
const viewerEl = document.getElementById("viewer");
const titleEl = document.getElementById("doc-title");
const pathEl = document.getElementById("doc-path");
const kindEl = document.getElementById("doc-kind");
const frontmatterEl = document.getElementById("frontmatter");
const rawLinkEl = document.getElementById("raw-link");
const documentsPanelEl = document.getElementById("documents-panel");
const graphPanelEl = document.getElementById("graph-panel");
const aiPanelEl = document.getElementById("ai-panel");
const graphStatusEl = document.getElementById("graph-status");
const graphViewEl = document.getElementById("graph-view");
const modeDocumentsEl = document.getElementById("mode-documents");
const modeGraphEl = document.getElementById("mode-graph");
const modeAiEl = document.getElementById("mode-ai");
const modeRootsEl = document.getElementById("mode-roots");
const rootsPanelEl = document.getElementById("roots-panel");
const rootAccessRequiredEl = document.getElementById("root-access-required");
const rootAccessGuideEl = document.getElementById("root-access-guide");
const rootProtectedContentEl = document.getElementById("root-protected-content");
const graphShowTopicsEl = document.getElementById("graph-show-topics");
const graphTopicFilterEl = document.getElementById("graph-topic-filter");
const graphKindFilterEl = document.getElementById("graph-kind-filter");
const graphResetLayoutEl = document.getElementById("graph-reset-layout");
const graphFitViewEl = document.getElementById("graph-fit-view");
const graphClearFiltersEl = document.getElementById("graph-clear-filters");
const graphClearFocusEl = document.getElementById("graph-clear-focus");
const okfAiOpenEl = document.getElementById("okf-ai-open");
const aiRunAnalysisEl = document.getElementById("ai-run-analysis");
const aiApplyRelationsEl = document.getElementById("ai-apply-relations");
const aiSaveStateEl = document.getElementById("ai-save-state");
const aiExportStateEl = document.getElementById("ai-export-state");
const aiAcceptAllEl = document.getElementById("ai-accept-all");
const aiDenyAllEl = document.getElementById("ai-deny-all");
const aiSemanticSearchEl = document.getElementById("ai-semantic-search");
const aiSemanticSearchRunEl = document.getElementById("ai-semantic-search-run");
const aiAccessStatusEl = document.getElementById("ai-access-status");
const aiPairOpenEl = document.getElementById("ai-pair-open");
const aiSearchResultsEl = document.getElementById("ai-search-results");
const aiSuggestionsEl = document.getElementById("ai-suggestions");
const aiStatusEl = document.getElementById("ai-status");
const aiExportPreviewEl = document.getElementById("ai-export-preview");
const aiStatDocumentsEl = document.getElementById("ai-stat-documents");
const aiStatEdgesEl = document.getElementById("ai-stat-edges");
const aiStatSuggestionsEl = document.getElementById("ai-stat-suggestions");
const aiStatAcceptedEl = document.getElementById("ai-stat-accepted");
const serverModeEl = document.getElementById("server-mode");
const accessStatusEl = document.getElementById("access-status");
const accessPairOpenEl = document.getElementById("access-pair-open");
const accessLoginOpenEl = document.getElementById("access-login-open");
const accessLogoutEl = document.getElementById("access-logout");
const pairingDialogEl = document.getElementById("pairing-dialog");
const pairingFormEl = document.getElementById("pairing-form");
const pairingCodeEl = document.getElementById("pairing-code");
const pairingErrorEl = document.getElementById("pairing-error");
const pairingCancelEl = document.getElementById("pairing-cancel");
const pairingSubmitEl = document.getElementById("pairing-submit");
const loginDialogEl = document.getElementById("login-dialog");
const loginFormEl = document.getElementById("login-form");
const loginUsernameEl = document.getElementById("login-username");
const loginPasswordEl = document.getElementById("login-password");
const loginErrorEl = document.getElementById("login-error");
const loginCancelEl = document.getElementById("login-cancel");
const loginSubmitEl = document.getElementById("login-submit");
const rootProposalFormEl = document.getElementById("root-proposal-form");
const rootPathEl = document.getElementById("root-path");
const rootMountEl = document.getElementById("root-mount");
const rootPriorityEl = document.getElementById("root-priority");
const rootWatchEl = document.getElementById("root-watch");
const rootPreviewEl = document.getElementById("root-preview");
const rootCancelEl = document.getElementById("root-cancel");
const rootStatusEl = document.getElementById("root-status");
const rootOverridesEl = document.getElementById("root-overrides");
const rootReviewEl = document.getElementById("root-review");
const rootReviewPathEl = document.getElementById("root-review-path");
const rootExpiryEl = document.getElementById("root-expiry");
const rootSummaryEl = document.getElementById("root-summary");
const rootConflictsEl = document.getElementById("root-conflicts");
const rootTreeFilterEl = document.getElementById("root-tree-filter");
const rootTreeBodyEl = document.getElementById("root-tree-body");
const rootTreeCountEl = document.getElementById("root-tree-count");
const rootReviewedEl = document.getElementById("root-reviewed");
const rootRegisterEl = document.getElementById("root-register");
const rootRegisterInitializeEl = document.getElementById("root-register-initialize");
const rootReviewCancelEl = document.getElementById("root-review-cancel");
const configuredRootsEl = document.getElementById("configured-roots");
const rootChangeReviewEl = document.getElementById("root-change-review");
const rootChangeDigestEl = document.getElementById("root-change-digest");
const rootChangeListEl = document.getElementById("root-change-list");
const rootChangeFilterEl = document.getElementById("root-change-filter");
const rootChangeInventoryEl = document.getElementById("root-change-inventory");
const rootChangeCountEl = document.getElementById("root-change-count");
const rootChangeReviewedEl = document.getElementById("root-change-reviewed");
const rootChangeAcceptEl = document.getElementById("root-change-accept");
const rootChangeDismissEl = document.getElementById("root-change-dismiss");
const rootChangeCloseEl = document.getElementById("root-change-close");
const rootChangeStatusEl = document.getElementById("root-change-status");
const rootInitializationDialogEl = document.getElementById("root-initialization-dialog");
const rootInitializationFormEl = document.getElementById("root-initialization-form");
const rootInitializationGitEl = document.getElementById("root-initialization-git");
const rootInitializationChangesEl = document.getElementById("root-initialization-changes");
const rootSourceConfirmEl = document.getElementById("root-source-confirm");
const rootInitializationErrorEl = document.getElementById("root-initialization-error");
const rootInitializationCancelEl = document.getElementById("root-initialization-cancel");
const rootInitializationSubmitEl = document.getElementById("root-initialization-submit");
let graphFallbackRootPath = "";
const GRAPH_INITIAL_ZOOM = 1;
const GRAPH_MIN_ZOOM = 0.35;
const GRAPH_MAX_ZOOM = 2.5;
const GRAPH_WHEEL_ZOOM_STEP = 0.14;
const FETCH_TIMEOUT_MS = 8000;
const AI_ACTION_TIMEOUT_MS = 10 * 60 * 1000;

let currentPath = "";
let currentMode = "documents";
const documentCache = new Map();
let graphBuilt = false;
let graphInstance = null;
let graphData = { nodes: [], edges: [] };
let pinnedNodeId = null;
let aiSuggestions = [];
let localPrefilterSuggestions = [];
let acceptedAiEdges = [];
let currentReviewSet = null;
let serverMode = "unknown";
let pairingAvailable = false;
let sessionAuthenticated = false;
let sessionCsrfToken = "";
let sessionScope = "";
let sessionUsername = "";
let sessionCapabilities = new Set();
let sessionExpiresAt = 0;
let sessionClock = null;
let rootConfiguration = null;
let rootProposal = null;
let rootInitializationProposal = null;
let rootInitializationPlan = null;
let rootResourceTypes = {};
let rootMonitoring = new Map();
let currentPendingChanges = null;

function protectedControls() {
  return [
    aiRunAnalysisEl,
    aiApplyRelationsEl,
    aiSaveStateEl,
    aiAcceptAllEl,
    aiDenyAllEl,
    aiSemanticSearchEl,
    aiSemanticSearchRunEl,
    rootPreviewEl
  ].filter(Boolean);
}

function editorModeAvailable() {
  return serverMode === "local-editor" || serverMode === "authenticated-tls";
}

function hasCapability(capability) {
  return sessionAuthenticated && sessionCapabilities.has(capability);
}

function hasCapabilities(...capabilities) {
  return capabilities.every(hasCapability);
}

function formatRemainingSession() {
  const remaining = Math.max(0, Math.ceil((sessionExpiresAt - Date.now()) / 1000));
  const minutes = Math.floor(remaining / 60);
  const seconds = remaining % 60;
  return `${minutes}:${String(seconds).padStart(2, "0")}`;
}

function updateAccessDisplay(message = "") {
  const paired = editorModeAvailable() && sessionAuthenticated && Boolean(sessionCsrfToken);
  for (const control of protectedControls()) {
    control.disabled = !paired;
  }
  if (aiRunAnalysisEl) aiRunAnalysisEl.disabled = !hasCapabilities("voyage.spend", "derived.rebuild");
  if (aiSemanticSearchRunEl) aiSemanticSearchRunEl.disabled = !hasCapability("voyage.spend");
  if (aiApplyRelationsEl) aiApplyRelationsEl.disabled = !hasCapability("content.write");
  if (aiAcceptAllEl) aiAcceptAllEl.disabled = !hasCapability("review.decide");
  if (aiDenyAllEl) aiDenyAllEl.disabled = !hasCapability("review.decide");
  if (rootPreviewEl) rootPreviewEl.disabled = !hasCapability("roots.propose");
  updateRootActionState();

  const canPair = editorModeAvailable() && !paired && pairingAvailable;
  accessPairOpenEl?.classList.toggle("hidden", !canPair);
  aiPairOpenEl?.classList.toggle("hidden", !canPair);
  accessLoginOpenEl?.classList.toggle("hidden", serverMode !== "authenticated-tls" || paired);
  accessLogoutEl?.classList.toggle("hidden", !paired);
  rootAccessRequiredEl?.classList.toggle("hidden", sessionAuthenticated);
  rootProtectedContentEl?.classList.toggle("hidden", !sessionAuthenticated);

  let status = message;
  if (!status && serverMode === "read-only") {
    status = "Reading anonymously · protected actions are disabled";
  } else if (!status && paired) {
    const identity = sessionUsername ? ` · ${sessionUsername}` : "";
    status = `${sessionScope || "Local editor"}${identity} session · expires in ${formatRemainingSession()}`;
  } else if (!status && serverMode === "authenticated-tls") {
    status = "Reading anonymously · HTTPS account sign-in is available";
  } else if (!status && editorModeAvailable() && pairingAvailable) {
    status = "Reading anonymously · local editor pairing is available";
  } else if (!status && editorModeAvailable()) {
    status = "Reading anonymously · restart okf-http to create a new pairing code";
  } else if (!status) {
    status = "Reading anonymously";
  }

  if (accessStatusEl) accessStatusEl.textContent = status;
  if (aiAccessStatusEl) aiAccessStatusEl.textContent = status;
  renderAiSuggestions();
}

function openRootAccessGuide() {
  const guide = allKnownItems().find((item) =>
    item.logicalPath === "okf/security/document-root-access.md" ||
    item.logicalPath?.endsWith("/security/document-root-access.md")
  );
  if (guide) {
    void setMode("documents");
    openDocument(guide);
    return;
  }
  if (accessStatusEl) {
    accessStatusEl.textContent = "The document-root access guide is not part of the configured document roots.";
  }
}

function clearLocalSession(message = "") {
  sessionAuthenticated = false;
  sessionCsrfToken = "";
  sessionScope = "";
  sessionUsername = "";
  sessionCapabilities = new Set();
  sessionExpiresAt = 0;
  if (sessionClock !== null) {
    window.clearInterval(sessionClock);
    sessionClock = null;
  }
  updateAccessDisplay(message);
}

function startSessionClock() {
  if (sessionClock !== null) window.clearInterval(sessionClock);
  sessionClock = window.setInterval(() => {
    if (!sessionAuthenticated) return;
    if (Date.now() >= sessionExpiresAt) {
      clearLocalSession("Local editor session expired · restart okf-http to pair again");
      return;
    }
    updateAccessDisplay();
  }, 1000);
}

function applySessionStatus(data) {
  pairingAvailable = Boolean(data.pairing_available);
  sessionAuthenticated = Boolean(data.authenticated && data.csrf_token);
  sessionCsrfToken = sessionAuthenticated ? data.csrf_token : "";
  sessionScope = sessionAuthenticated ? (data.scope || "local-editor") : "";
  sessionUsername = sessionAuthenticated ? (data.username || "") : "";
  sessionCapabilities = new Set(sessionAuthenticated ? (data.capabilities || []) : []);
  sessionExpiresAt = sessionAuthenticated
    ? Date.now() + Math.max(0, Number(data.expires_in_seconds || 0)) * 1000
    : 0;
  if (sessionAuthenticated) {
    startSessionClock();
  } else {
    clearLocalSession();
  }
  updateAccessDisplay();
}

function applyServerMode(mode) {
  serverMode = mode;
  if (serverModeEl) serverModeEl.textContent = mode;
  const editorAvailable = mode === "local-editor" || mode === "authenticated-tls";
  if (okfAiOpenEl) {
    okfAiOpenEl.disabled = !editorAvailable;
    okfAiOpenEl.classList.toggle("hidden", !editorAvailable);
  }
  if (modeAiEl) {
    modeAiEl.disabled = !editorAvailable;
    modeAiEl.classList.toggle("hidden", !editorAvailable);
  }
  if (!editorAvailable && aiStatusEl) {
    aiStatusEl.textContent = mode === "read-only"
      ? "Read-only mode: restart okf-http with --local-editor to enable protected actions."
      : "Protected actions are unavailable because the server mode could not be verified.";
  }
  updateAccessDisplay();
}

async function loadServerMode() {
  try {
    const response = await fetchWithTimeout("/api/v1/health");
    const data = await apiData(response);
    applyServerMode(data.mode || "unknown");
  } catch (_error) {
    applyServerMode("unknown");
    clearLocalSession("Reading anonymously · server access status is unavailable");
    return;
  }
  try {
    await refreshSessionStatus();
  } catch (_error) {
    clearLocalSession("Reading anonymously · editor session status is unavailable");
  }
}

async function refreshSessionStatus() {
  const sessionResponse = await fetchWithTimeout("/api/v1/access/session", {
    credentials: "same-origin"
  });
  const status = await apiData(sessionResponse);
  if (status.authenticated && editorModeAvailable()) {
    const refreshResponse = await fetchWithTimeout("/api/v1/access/session/refresh", {
      method: "POST",
      credentials: "same-origin"
    });
    const recovered = await apiData(refreshResponse);
    applySessionStatus({ ...status, ...recovered });
    return;
  }
  applySessionStatus(status);
}

function graphLayoutOptions() {
  return {
    name: "cose",
    animate: false,
    fit: false,
    padding: 40
  };
}

function clamp(value, min, max) {
  return Math.min(Math.max(value, min), max);
}

function pathToUrl(path) {
  const url = new URL(window.location.href);
  url.searchParams.set("doc", path);
  url.searchParams.set("mode", currentMode);
  url.hash = "";
  return `${url.pathname}${url.search}`;
}

function displayDocumentPath(path) {
  return path
    .replace(/^\/okf-docs\//, "")
    .replace(/^\/okf-root\/(\d+)\//, "root $1/");
}

function titleFromPath(path) {
  const filename = path.split("/").pop() || path;
  return filename.replace(/\.md$/i, "").replace(/[-_]/g, " ");
}

async function setMode(mode) {
  if (mode === "ai" && !aiPanelEl) {
    mode = "documents";
  }
  if (mode === "ai" && !editorModeAvailable()) {
    mode = "documents";
  }
  currentMode = mode;
  const documents = mode === "documents";
  const graph = mode === "graph";
  const ai = mode === "ai";
  const roots = mode === "roots";
  documentsPanelEl.classList.toggle("hidden", !documents);
  frontmatterEl.classList.toggle("hidden", !documents);
  graphPanelEl.classList.toggle("hidden", !graph);
  aiPanelEl?.classList.toggle("hidden", !ai);
  rootsPanelEl?.classList.toggle("hidden", !roots);
  modeDocumentsEl.classList.toggle("active", documents);
  modeGraphEl.classList.toggle("active", graph);
  modeAiEl?.classList.toggle("active", ai);
  modeRootsEl?.classList.toggle("active", roots);
  modeDocumentsEl.setAttribute("aria-selected", String(documents));
  modeGraphEl.setAttribute("aria-selected", String(graph));
  modeAiEl?.setAttribute("aria-selected", String(ai));
  modeRootsEl?.setAttribute("aria-selected", String(roots));

  if (graph || ai) {
    await ensureGraph();
  }

  if (graph) {
    if (graphBuilt) {
      refreshGraphForCurrentPath();
    }
  }

  if (ai) {
    refreshAiPanel();
  }

  if (roots && sessionAuthenticated) {
    void loadRootConfiguration();
  }

  const state = window.history.state || { docPath: currentPath };
  window.history.replaceState(
    { ...state, docPath: currentPath, mode: currentMode },
    "",
    pathToUrl(currentPath)
  );
}

function normalizeDocumentPath(path) {
  const hashIndex = path.indexOf("#");
  return hashIndex >= 0 ? path.slice(0, hashIndex) : path;
}

function allKnownItems() {
  return DOCS.flatMap((group) => group.items);
}

function rootLabel(root, index) {
  if (root.mount) return root.mount;
  const normalized = String(root.source_path || "").replace(/\/+$/, "");
  return normalized.split("/").pop() || `Root ${index + 1}`;
}

function documentGroupLabel(document, roots) {
  const root = roots[document.root_index] || {};
  return rootLabel(root, document.root_index);
}

function documentKind(document) {
  if (document.is_plan) return "plan";
  if (/\bindex\.md$/i.test(document.source_relative_path || "")) return "index";
  return document.kind || document.type || "document";
}

function buildDocumentCatalog(payload) {
  const groups = new Map();
  for (const document of payload.documents || []) {
    const group = documentGroupLabel(document, payload.roots || []);
    const item = {
      title: document.title || titleFromPath(document.source_relative_path),
      path: document.browser_path,
      kind: documentKind(document),
      logicalPath: document.logical_path,
      okfUri: document.okf_uri || "",
      directoryParts: String(document.directory_path || "").split("/").filter(Boolean),
      navigationClass: document.navigation_class || "root-document",
      topic: document.topic || ""
    };
    if (!groups.has(group)) groups.set(group, []);
    groups.get(group).push(item);
  }
  return [...groups.entries()]
    .sort(([left], [right]) => left.localeCompare(right))
    .map(([group, items]) => ({
      group,
      items: items.sort((left, right) => left.title.localeCompare(right.title))
    }));
}

async function loadDocumentCatalog() {
  const response = await fetchWithTimeout("/api/v1/documents");
  if (!response.ok) {
    throw new Error(`${response.status} ${response.statusText}`);
  }
  const envelope = await response.json();
  if (envelope.api_version !== "v1" || !envelope.data) {
    throw new Error("Unsupported OKF API response");
  }
  const payload = envelope.data;
  DOCS = buildDocumentCatalog(payload);
  graphFallbackRootPath = allKnownItems().find((item) => /(?:^|\/)index\.md$/i.test(item.path))?.path
    || allKnownItems()[0]?.path
    || "";
}

function findItemByPath(path) {
  return allKnownItems().find((item) => item.path === path) || null;
}

async function fetchWithTimeout(resource, options = {}, timeoutMs = FETCH_TIMEOUT_MS) {
  const controller = new AbortController();
  const timeout = window.setTimeout(() => controller.abort(), timeoutMs);
  try {
    return await fetch(resource, { ...options, signal: controller.signal });
  } finally {
    window.clearTimeout(timeout);
  }
}

async function authorizedFetch(resource, options = {}, timeoutMs = FETCH_TIMEOUT_MS) {
  if (!editorModeAvailable()) {
    throw new Error("Protected actions are disabled in this server mode.");
  }
  if (!sessionAuthenticated || !sessionCsrfToken) {
    throw new Error("Pair or sign in to this browser first.");
  }
  const headers = new Headers(options.headers || {});
  headers.set("X-OKF-CSRF-Token", sessionCsrfToken);
  if (options.body && !headers.has("Content-Type")) headers.set("Content-Type", "application/json");
  const response = await fetchWithTimeout(
    resource,
    { ...options, credentials: "same-origin", headers },
    timeoutMs
  );
  if (response.status === 401) {
    clearLocalSession("Local editor session expired or was revoked");
  }
  return response;
}

function openLoginDialog() {
  if (serverMode !== "authenticated-tls" || sessionAuthenticated) return;
  if (loginErrorEl) loginErrorEl.textContent = "";
  if (loginPasswordEl) loginPasswordEl.value = "";
  loginDialogEl?.showModal();
  window.setTimeout(() => loginUsernameEl?.focus(), 0);
}

function closeLoginDialog() {
  if (loginPasswordEl) loginPasswordEl.value = "";
  if (loginErrorEl) loginErrorEl.textContent = "";
  loginDialogEl?.close();
}

async function submitLogin(event) {
  event.preventDefault();
  if (!loginUsernameEl || !loginPasswordEl || !loginSubmitEl) return;
  loginSubmitEl.disabled = true;
  if (loginErrorEl) loginErrorEl.textContent = "Signing in…";
  try {
    const response = await fetchWithTimeout("/api/v1/access/login", {
      method: "POST",
      credentials: "same-origin",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        username: loginUsernameEl.value,
        password: loginPasswordEl.value
      })
    });
    const data = await apiData(response);
    loginPasswordEl.value = "";
    applySessionStatus({ ...data, mode: serverMode, pairing_available: false, scope: "persistent-user" });
    loginDialogEl?.close();
    window.location.reload();
  } catch (error) {
    loginPasswordEl.value = "";
    if (loginErrorEl) loginErrorEl.textContent = String(error.message || error);
    loginPasswordEl.focus();
  } finally {
    loginSubmitEl.disabled = false;
  }
}

function openPairingDialog() {
  if (!editorModeAvailable() || !pairingAvailable || sessionAuthenticated) return;
  if (pairingErrorEl) pairingErrorEl.textContent = "";
  if (pairingCodeEl) pairingCodeEl.value = "";
  pairingDialogEl?.showModal();
  window.setTimeout(() => pairingCodeEl?.focus(), 0);
}

function closePairingDialog() {
  if (pairingCodeEl) pairingCodeEl.value = "";
  if (pairingErrorEl) pairingErrorEl.textContent = "";
  pairingDialogEl?.close();
}

async function submitPairing(event) {
  event.preventDefault();
  if (!pairingCodeEl || !pairingSubmitEl) return;
  const code = pairingCodeEl.value;
  if (!isValidPairingCode(code)) {
    if (pairingErrorEl) pairingErrorEl.textContent = "Enter the 12-digit code in the shown format.";
    pairingCodeEl.focus();
    return;
  }

  pairingSubmitEl.disabled = true;
  if (pairingErrorEl) pairingErrorEl.textContent = "Pairing this browser…";
  try {
    const response = await fetchWithTimeout("/api/v1/access/pair", {
      method: "POST",
      credentials: "same-origin",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ code })
    });
    const data = await apiData(response);
    pairingCodeEl.value = "";
    applySessionStatus({
      ...data,
      mode: serverMode,
      pairing_available: false,
      scope: "local-editor"
    });
    pairingDialogEl?.close();
    if (aiStatusEl) {
      aiStatusEl.textContent = "Local editor session established. Protected actions are available.";
    }
  } catch (error) {
    if (pairingErrorEl) pairingErrorEl.textContent = String(error.message || error);
    pairingCodeEl.select();
  } finally {
    pairingSubmitEl.disabled = false;
  }
}

async function logoutLocalSession() {
  if (!sessionAuthenticated) return;
  try {
    await apiData(await authorizedFetch("/api/v1/access/logout", { method: "POST" }));
    clearLocalSession("Logged out · reading anonymously");
    if (aiStatusEl) aiStatusEl.textContent = "Local editor session ended.";
  } catch (error) {
    if (aiStatusEl) aiStatusEl.textContent = String(error.message || error);
  }
}

function uniqueSorted(values) {
  return [...new Set(values.filter(Boolean))].sort((a, b) => a.localeCompare(b));
}

function slugifyHeading(text) {
  return text
    .toLowerCase()
    .replace(/[^\p{L}\p{N}\s-]/gu, "")
    .trim()
    .replace(/\s+/g, "-")
    .replace(/^-+|-+$/g, "");
}

function formatInlineText(text) {
  let html = escapeHtml(text);
  html = html.replace(/`([^`]+)`/g, "<code>$1</code>");
  html = html.replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>");
  html = html.replace(/\*([^*]+)\*/g, "<em>$1</em>");
  return html;
}

function applyInlineMarkdown(text, basePath = currentPath) {
  const pattern = /\[([^\]]+)\]\(([^)]+)\)/g;
  const output = [];
  let offset = 0;
  let match;
  while ((match = pattern.exec(text))) {
    output.push(formatInlineText(text.slice(offset, match.index)));
    const [, label, target] = match;
    const baseHref = new URL(basePath, window.location.href).href;
    const classified = classifyMarkdownUrl(target, baseHref, window.location.origin);
    if (!classified) {
      output.push(formatInlineText(label));
    } else if (classified.kind === "document") {
      output.push(
        `<a href="${escapeHtml(classified.href)}" data-doc-path="${escapeHtml(classified.documentPath)}">${formatInlineText(label)}</a>`
      );
    } else {
      output.push(
        `<a href="${escapeHtml(classified.href)}" target="_blank" rel="noopener noreferrer">${formatInlineText(label)}</a>`
      );
    }
    offset = pattern.lastIndex;
  }
  output.push(formatInlineText(text.slice(offset)));
  return output.join("");
}

function parseFrontmatter(source) {
  if (!source.startsWith("---\n")) {
    return { meta: {}, body: source };
  }

  const end = source.indexOf("\n---\n", 4);
  if (end === -1) {
    return { meta: {}, body: source };
  }

  const rawMeta = source.slice(4, end).trim();
  const body = source.slice(end + 5);
  const meta = {};
  let currentListKey = null;

  for (const line of rawMeta.split("\n")) {
    if (/^\s*-\s+/.test(line) && currentListKey) {
      meta[currentListKey] ||= [];
      meta[currentListKey].push(line.replace(/^\s*-\s+/, "").trim());
      continue;
    }

    const match = line.match(/^([A-Za-z0-9_.-]+):\s*(.*)$/);
    if (!match) {
      currentListKey = null;
      continue;
    }

    const [, key, rawValue] = match;
    const value = rawValue.trim();
    if (value === "") {
      meta[key] = [];
      currentListKey = key;
    } else {
      meta[key] = value;
      currentListKey = null;
    }
  }

  return { meta, body };
}

async function fetchDocumentSource(path) {
  const normalized = normalizeDocumentPath(path);
  if (documentCache.has(normalized)) {
    return documentCache.get(normalized);
  }

  const response = await fetchWithTimeout(normalized);
  if (!response.ok) {
    throw new Error(`${response.status} ${response.statusText}`);
  }
  const text = await response.text();
  const parsed = parseFrontmatter(text);
  const cached = { path: normalized, source: text, ...parsed };
  documentCache.set(normalized, cached);
  return cached;
}

function renderMarkdown(source, basePath = currentPath) {
  const { meta, body } = parseFrontmatter(source);
  renderFrontmatter(meta);

  const lines = body.replace(/\r/g, "").split("\n");
  const out = [];
  const usedHeadingIds = new Map();
  let inCode = false;
  let codeLines = [];
  let listType = null;
  let tableBuffer = [];

  function flushList() {
    if (!listType) return;
    out.push(`</${listType}>`);
    listType = null;
  }

  function flushTable() {
    if (tableBuffer.length < 2) {
      tableBuffer = [];
      return;
    }
    const [header, divider, ...rows] = tableBuffer;
    if (!divider.includes("---")) {
      tableBuffer = [];
      return;
    }
    const splitRow = (line) =>
      line
        .trim()
        .replace(/^\|/, "")
        .replace(/\|$/, "")
        .split("|")
        .map((cell) => applyInlineMarkdown(cell.trim(), basePath));
    const headerCells = splitRow(header);
    out.push("<table><thead><tr>");
    headerCells.forEach((cell) => out.push(`<th>${cell}</th>`));
    out.push("</tr></thead><tbody>");
    rows.forEach((row) => {
      const cells = splitRow(row);
      out.push("<tr>");
      cells.forEach((cell) => out.push(`<td>${cell}</td>`));
      out.push("</tr>");
    });
    out.push("</tbody></table>");
    tableBuffer = [];
  }

  for (const line of lines) {
    if (line.startsWith("```")) {
      flushList();
      flushTable();
      if (!inCode) {
        inCode = true;
        codeLines = [];
      } else {
        out.push(`<pre><code>${escapeHtml(codeLines.join("\n"))}</code></pre>`);
        inCode = false;
      }
      continue;
    }

    if (inCode) {
      codeLines.push(line);
      continue;
    }

    if (line.includes("|")) {
      tableBuffer.push(line);
      continue;
    }
    flushTable();

    if (!line.trim()) {
      flushList();
      continue;
    }

    const heading = line.match(/^(#{1,6})\s+(.*)$/);
    if (heading) {
      flushList();
      const level = heading[1].length;
      const rawHeading = heading[2].trim();
      let headingId = slugifyHeading(rawHeading) || `section-${usedHeadingIds.size + 1}`;
      const currentCount = usedHeadingIds.get(headingId) || 0;
      usedHeadingIds.set(headingId, currentCount + 1);
      if (currentCount > 0) {
        headingId = `${headingId}-${currentCount + 1}`;
      }
      out.push(
        `<h${level} id="${escapeHtml(headingId)}">${applyInlineMarkdown(rawHeading, basePath)}</h${level}>`
      );
      continue;
    }

    const bullet = line.match(/^\s*-\s+(.*)$/);
    if (bullet) {
      if (listType !== "ul") {
        flushList();
        out.push("<ul>");
        listType = "ul";
      }
      out.push(`<li>${applyInlineMarkdown(bullet[1], basePath)}</li>`);
      continue;
    }

    const ordered = line.match(/^\s*\d+\.\s+(.*)$/);
    if (ordered) {
      if (listType !== "ol") {
        flushList();
        out.push("<ol>");
        listType = "ol";
      }
      out.push(`<li>${applyInlineMarkdown(ordered[1], basePath)}</li>`);
      continue;
    }

    const quote = line.match(/^>\s?(.*)$/);
    if (quote) {
      flushList();
      out.push(`<blockquote>${applyInlineMarkdown(quote[1], basePath)}</blockquote>`);
      continue;
    }

    if (line.trim() === "---") {
      flushList();
      out.push("<hr />");
      continue;
    }

    flushList();
    out.push(`<p>${applyInlineMarkdown(line, basePath)}</p>`);
  }

  flushList();
  flushTable();
  return out.join("");
}

function renderFrontmatter(meta) {
  const entries = Object.entries(meta);
  if (!entries.length) {
    frontmatterEl.innerHTML = "";
    return;
  }

  frontmatterEl.innerHTML = entries
    .map(([key, value]) => {
      const printable = Array.isArray(value) ? value.join(", ") : String(value);
      return `<span class="meta-chip"><strong>${escapeHtml(key)}</strong>: ${escapeHtml(printable)}</span>`;
    })
    .join("");
}

function renderCsv(source) {
  renderFrontmatter({});
  const firstLine = source.split(/\r?\n/u).find((line) => line.trim() !== "") || "";
  const delimiters = [",", ";", "\t"];
  const delimiter = delimiters
    .map((value) => ({ value, count: firstLine.split(value).length - 1 }))
    .sort((left, right) => right.count - left.count)[0];
  const separator = delimiter?.count > 0 ? delimiter.value : ",";
  const rows = [];
  let row = [];
  let field = "";
  let quoted = false;
  for (let index = 0; index < source.length; index += 1) {
    const character = source[index];
    if (quoted) {
      if (character === '"' && source[index + 1] === '"') {
        field += '"';
        index += 1;
      } else if (character === '"') {
        quoted = false;
      } else {
        field += character;
      }
    } else if (character === '"' && field === "") {
      quoted = true;
    } else if (character === separator) {
      row.push(field);
      field = "";
    } else if (character === "\n") {
      row.push(field.replace(/\r$/u, ""));
      rows.push(row);
      row = [];
      field = "";
    } else {
      field += character;
    }
  }
  if (field !== "" || row.length > 0) {
    row.push(field.replace(/\r$/u, ""));
    rows.push(row);
  }
  if (!rows.length) {
    return "<p class=\"muted\">Empty CSV file.</p>";
  }

  const [header, ...body] = rows;
  const table = [];
  const formulaCells = rows.reduce(
    (count, cells) => count + cells.filter((cell) => /^[\s]*[=+\-@]/u.test(cell)).length,
    0
  );
  if (formulaCells > 0) {
    table.push(
      `<p class="muted">Safety warning: ${formulaCells} cell(s) begin with a spreadsheet formula marker. Values are displayed as escaped text and are not evaluated.</p>`
    );
  }
  table.push("<table><thead><tr>");
  header.forEach((cell) => table.push(`<th>${escapeHtml(cell)}</th>`));
  table.push("</tr></thead><tbody>");
  body.forEach((row) => {
    table.push("<tr>");
    row.forEach((cell) => table.push(`<td>${escapeHtml(cell)}</td>`));
    table.push("</tr>");
  });
  table.push("</tbody></table>");
  return table.join("");
}

async function openDocument(item) {
  return openDocumentWithOptions(item, { pushHistory: true });
}

async function openDocumentWithOptions(item, options = {}) {
  const { pushHistory = true } = options;
  const hashIndex = item.path.indexOf("#");
  const fetchPath = hashIndex >= 0 ? item.path.slice(0, hashIndex) : item.path;
  const targetHash = hashIndex >= 0 ? decodeURIComponent(item.path.slice(hashIndex + 1)) : "";
  currentPath = item.path;
  if (currentMode === "graph") {
    refreshGraphForCurrentPath();
  }
  titleEl.textContent = item.title;
  pathEl.textContent = displayDocumentPath(item.path);
  kindEl.textContent = item.kind;
  rawLinkEl.href = fetchPath;

  if (pushHistory) {
    window.history.pushState({ docPath: item.path, mode: currentMode }, "", pathToUrl(item.path));
  }

  try {
    if (item.kind === "csv") {
      const response = await fetchWithTimeout(fetchPath);
      if (!response.ok) {
        throw new Error(`${response.status} ${response.statusText}`);
      }
      const text = await response.text();
      viewerEl.innerHTML = renderCsv(text);
      renderFrontmatter({});
    } else {
      const { source } = await fetchDocumentSource(fetchPath);
      viewerEl.innerHTML = renderMarkdown(source, fetchPath);
    }
    bindViewerLinks();
    if (targetHash) {
      const target = document.getElementById(targetHash);
      if (target) {
        target.scrollIntoView({ behavior: "smooth", block: "start" });
      }
    } else {
      viewerEl.scrollIntoView({ behavior: "auto", block: "start" });
    }
  } catch (error) {
    renderFrontmatter({});
    viewerEl.innerHTML = `
      <p>Could not load this document in the browser.</p>
      <p class="muted">If you opened this file directly with <code>file://</code>, start a local HTTP server instead. Error: ${escapeHtml(String(error.message || error))}</p>
    `;
  }

  [...treeEl.querySelectorAll("button")].forEach((button) => {
    button.classList.toggle("active", button.dataset.path === item.path);
  });
}

function bindViewerLinks() {
  viewerEl.querySelectorAll("a[data-doc-path]").forEach((link) => {
    link.addEventListener("click", (event) => {
      event.preventDefault();
      const path = link.dataset.docPath;
      let item = DOCS.flatMap((group) => group.items).find((candidate) => candidate.path === path);
      if (!item && path) {
        const normalized = normalizeDocumentPath(path);
        const known = findItemByPath(normalized);
        if (known) {
          item = { ...known, path };
        }
      }
      if (!item && path) {
        item = {
          title: titleFromPath(path),
          path,
          kind: /\.csv(?:#.*)?$/i.test(path) ? "csv" : "document"
        };
      }
      if (item) {
        openDocument(item);
      }
    });
  });
}

function extractMarkdownLinks(body, basePath) {
  const found = [];
  const regex = /\[([^\]]+)\]\(([^)]+)\)/g;
  let match;
  while ((match = regex.exec(body))) {
    const target = match[2].trim();
    const classified = classifyMarkdownUrl(
      target,
      new URL(basePath, window.location.href).href,
      window.location.origin
    );
    if (!classified || classified.kind !== "document") continue;
    const normalized = normalizeDocumentPath(classified.documentPath);
    if (!/\.md$/i.test(normalized)) continue;
    found.push(normalized);
  }
  return found;
}

async function buildGraphData() {
  const nodes = [];
  const edges = [];
  const knownMarkdownItems = allKnownItems().filter((item) => /\.md$/i.test(normalizeDocumentPath(item.path)));
  const nodeIds = new Set();
  const edgeIds = new Set();
  const topicIds = new Set();

  for (const item of knownMarkdownItems) {
    const normalizedPath = normalizeDocumentPath(item.path);
    nodeIds.add(normalizedPath);
    nodes.push({
      data: {
        id: normalizedPath,
        type: "document",
        title: item.title,
        path: normalizedPath,
        kind: item.kind,
        topic: null,
        group: DOCS.find((group) => group.items.includes(item))?.group || "Documents"
      }
    });
  }

  for (const item of knownMarkdownItems) {
    const normalizedPath = normalizeDocumentPath(item.path);
    let body;
    let meta;
    try {
      ({ body, meta } = await fetchDocumentSource(normalizedPath));
    } catch (error) {
      const node = nodes.find((candidate) => candidate.data.id === normalizedPath);
      if (node) {
        node.data.status = "unavailable";
        node.data.loadError = String(error.message || error);
      }
      continue;
    }
    const topic = Array.isArray(meta.topic) ? meta.topic[0] : meta.topic || "";
    const node = nodes.find((candidate) => candidate.data.id === normalizedPath);
    if (node) {
      node.data.topic = topic || "";
      node.data.status = Array.isArray(meta.status) ? meta.status[0] : meta.status || "";
    }

    if (topic) {
      const topicId = `topic:${topic}`;
      if (!topicIds.has(topicId)) {
        topicIds.add(topicId);
        nodes.push({
          data: {
            id: topicId,
            type: "topic",
            label: topic
          }
        });
      }
      const topicEdgeId = `${normalizedPath}->${topicId}`;
      if (!edgeIds.has(topicEdgeId)) {
        edgeIds.add(topicEdgeId);
        edges.push({
          data: {
            id: topicEdgeId,
            type: "has_topic",
            source: normalizedPath,
            target: topicId
          }
        });
      }
    }

    const targets = extractMarkdownLinks(body, normalizedPath);
    for (const target of targets) {
      if (!nodeIds.has(target)) continue;
      const edgeId = `${normalizedPath}->${target}`;
      if (edgeIds.has(edgeId)) continue;
      edgeIds.add(edgeId);
      edges.push({
        data: {
          id: edgeId,
          type: "links_to",
          source: normalizedPath,
          target
        }
      });
    }
  }

  try {
    const graph = await apiData(await fetchWithTimeout("/api/v1/graph"));
    const logicalToBrowser = new Map(
      allKnownItems().map((item) => [item.logicalPath, normalizeDocumentPath(item.path)])
    );
    for (const edge of graph.edges || []) {
      if (edge.type !== "canonical_relation") continue;
      const source = logicalToBrowser.get(edge.source);
      const target = logicalToBrowser.get(edge.target);
      if (!source || !target || !nodeIds.has(source) || !nodeIds.has(target)) continue;
      const edgeId = `api:${edge.id}`;
      if (edgeIds.has(edgeId)) continue;
      edgeIds.add(edgeId);
      edges.push({
        data: {
          id: edgeId,
          type: "canonical_relation",
          source,
          target,
          label: edge.provenance?.ai_generated ? "Canonical AI relation" : "Canonical relation",
          provenance: edge.provenance || null
        }
      });
    }
  } catch (error) {
    console.warn("Canonical graph relations are unavailable", error);
  }

  for (const edge of acceptedAiEdges) {
    if (!nodeIds.has(edge.source) || !nodeIds.has(edge.target)) continue;
    const edgeId = `accepted:${edge.source}->${edge.target}`;
    if (edgeIds.has(edgeId)) continue;
    edgeIds.add(edgeId);
    edges.push({
      data: {
        id: edgeId,
        type: "ai_accepted",
        source: edge.source,
        target: edge.target,
        label: "AI accepted"
      }
    });
  }

  return { nodes, edges };
}

function documentNodes() {
  return graphData.nodes.filter((node) => node.data.type === "document");
}

function existingGraphEdgePairs() {
  return new Set(
    graphData.edges
      .filter((edge) => edge.data.type === "links_to")
      .map((edge) => `${edge.data.source}->${edge.data.target}`)
  );
}

function aiCandidateReason(left, right) {
  if (left.data.topic && left.data.topic === right.data.topic) {
    return `shared topic: ${left.data.topic}`;
  }
  if (left.data.kind && left.data.kind === right.data.kind) {
    return `shared kind: ${left.data.kind}`;
  }
  if (left.data.group && left.data.group === right.data.group) {
    return `same documentation group: ${left.data.group}`;
  }
  return "";
}

function aiCandidateScore(left, right) {
  let score = 0;
  if (left.data.topic && left.data.topic === right.data.topic) score += 0.46;
  if (left.data.kind && left.data.kind === right.data.kind) score += 0.24;
  if (left.data.group && left.data.group === right.data.group) score += 0.2;
  return Math.min(score, 0.95);
}

function createLocalPrefilterSuggestions() {
  const nodes = documentNodes();
  const existing = existingGraphEdgePairs();
  const suggestions = [];
  for (let leftIndex = 0; leftIndex < nodes.length; leftIndex += 1) {
    for (let rightIndex = leftIndex + 1; rightIndex < nodes.length; rightIndex += 1) {
      const left = nodes[leftIndex];
      const right = nodes[rightIndex];
      if (existing.has(`${left.data.id}->${right.data.id}`) || existing.has(`${right.data.id}->${left.data.id}`)) {
        continue;
      }
      const reason = aiCandidateReason(left, right);
      const score = aiCandidateScore(left, right);
      if (!reason || score < 0.4) continue;
      const id = `ai:${left.data.id}->${right.data.id}`;
      suggestions.push({
        id,
        source: left.data.id,
        target: right.data.id,
        sourceTitle: left.data.title,
        targetTitle: right.data.title,
        score,
        reason,
        provider: "okf-browser-derived",
        model: "local-metadata-v1",
        generationMethod: "browser_graph_prefilter",
        aiGenerated: false,
        status: "local-prefilter",
        createdAt: new Date().toISOString()
      });
    }
  }
  return suggestions.sort((left, right) => right.score - left.score).slice(0, 24);
}

function syncAcceptedAiEdges() {
  acceptedAiEdges = aiSuggestions
    .filter((suggestion) => suggestion.status === "accepted")
    .map((suggestion) => ({ ...suggestion, source: suggestion.sourceDocument, target: suggestion.targetDocument }));
}

function exportPayload() {
  return {
    type: "okf-ai-edge-review",
    review_set_id: currentReviewSet?.id || "",
    accepted_edges: acceptedAiEdges.map((edge) => ({
      id: edge.id,
      provider: edge.provider,
      model: edge.model,
      generation_method: edge.generationMethod,
      ai_generated: edge.aiGenerated,
      source_chunk: edge.sourceChunk,
      target_chunk: edge.targetChunk,
      score: edge.score,
      created_at: edge.createdAt
    }))
  };
}

function exportMarkdown() {
  if (!acceptedAiEdges.length) {
    return "No accepted OKF AI changes have been saved yet.";
  }
  const lines = [
    "# Accepted OKF AI Suggestions",
    "",
    "These entries are AI-derived or AI-assisted suggestions exported from the OKF browser.",
    "A host tool should review and apply them to canonical OKF Markdown files.",
    ""
  ];
  for (const edge of acceptedAiEdges) {
    lines.push(`- [ ] \`${edge.source}\` -> \`${edge.target}\``);
    lines.push(`  - source: ${edge.sourceTitle}`);
    lines.push(`  - target: ${edge.targetTitle}`);
    lines.push(`  - reason: ${edge.reason}`);
    lines.push(`  - score: ${edge.score.toFixed(3)}`);
    lines.push(`  - provider: ${edge.provider}`);
    lines.push(`  - model: ${edge.model}`);
    lines.push(`  - generation_method: ${edge.generationMethod}`);
    lines.push("  - ai_generated: true");
    lines.push("");
  }
  return lines.join("\n");
}

function updateAiStats() {
  if (!aiStatDocumentsEl || !aiStatEdgesEl || !aiStatSuggestionsEl || !aiStatAcceptedEl) return;
  aiStatDocumentsEl.textContent = String(documentNodes().length);
  aiStatEdgesEl.textContent = String(graphData.edges.length);
  aiStatSuggestionsEl.textContent = String(aiSuggestions.length);
  aiStatAcceptedEl.textContent = String(acceptedAiEdges.length);
}

function renderAiSuggestions() {
  if (!aiSuggestionsEl) return;
  const disabled = hasCapability("review.decide") ? "" : " disabled";
  if (!aiSuggestions.length) {
    aiSuggestionsEl.innerHTML = '<p class="muted">No suggestions yet. Run analysis to prepare candidate edges.</p>';
    return;
  }
  aiSuggestionsEl.innerHTML = aiSuggestions
    .map(
      (suggestion) => `
        <article class="ai-suggestion ${suggestion.status === "accepted" ? "accepted" : ""} ${suggestion.status === "denied" ? "denied" : ""}">
          <div>
            <div class="ai-suggestion-title">${escapeHtml(suggestion.sourceTitle)} → ${escapeHtml(suggestion.targetTitle)}</div>
            <div class="muted">Voyage similarity · score ${suggestion.score.toFixed(3)} · AI-derived · ${escapeHtml(suggestion.provider)} / ${escapeHtml(suggestion.model)} · ${escapeHtml(suggestion.generationMethod)}</div>
          </div>
          <div class="ai-suggestion-actions">
            <button type="button" data-ai-action="accept" data-ai-id="${escapeHtml(suggestion.id)}"${disabled}>Accept</button>
            <button type="button" data-ai-action="deny" data-ai-id="${escapeHtml(suggestion.id)}"${disabled}>Deny</button>
          </div>
        </article>
      `
    )
    .join("") + (localPrefilterSuggestions.length ? `
      <section class="ai-local-prefilter">
        <h5>Local metadata prefilter — not Voyage AI</h5>
        ${localPrefilterSuggestions.map((suggestion) => `
          <article class="ai-suggestion local-prefilter">
            <div>
              <div class="ai-suggestion-title">${escapeHtml(suggestion.sourceTitle)} → ${escapeHtml(suggestion.targetTitle)}</div>
              <div class="muted">${escapeHtml(suggestion.reason)} · local score ${suggestion.score.toFixed(3)} · ${escapeHtml(suggestion.generationMethod)} · not reviewable</div>
            </div>
          </article>`).join("")}
      </section>` : "");
}

function renderAiSearchResults() {
  if (aiSearchResultsEl && !aiSearchResultsEl.dataset.loaded) {
    aiSearchResultsEl.textContent = "Enter a prompt and explicitly run Voyage semantic search.";
  }
}

function refreshAiPanel() {
  if (!aiPanelEl) return;
  updateAiStats();
  renderAiSuggestions();
  renderAiSearchResults();
  if (aiExportPreviewEl) {
    aiExportPreviewEl.textContent = exportMarkdown();
  }
}

async function runAiAnalysis() {
  if (aiStatusEl) {
    aiStatusEl.textContent = "Preparing OKF AI analysis request from loaded documents…";
  }
  try {
    await ensureGraph();
    localPrefilterSuggestions = createLocalPrefilterSuggestions();
    const planResponse = await fetchWithTimeout("/api/v1/voyage/plan", { method: "POST" });
    const plan = await apiData(planResponse);
    const approved = window.confirm(
      `Run protected Voyage indexing and replace the current derived review set? ` +
      `Estimated provider usage: ${plan.plan.estimated_tokens} tokens in ` +
      `${plan.plan.estimated_requests} request(s).`
    );
    if (!approved) throw new Error("Voyage analysis was cancelled before protected actions.");
    await apiData(await authorizedFetch("/api/v1/voyage/index", { method: "POST" }, AI_ACTION_TIMEOUT_MS));
    const generated = await apiData(await authorizedFetch(
      "/api/v1/suggestions/generate",
      { method: "POST", body: JSON.stringify({ threshold: 0.82 }) },
      AI_ACTION_TIMEOUT_MS
    ));
    applySuggestionPayload(generated);
    if (aiStatusEl) aiStatusEl.textContent = `Durable Voyage review set ${currentReviewSet.id} is ready in SQLite.`;
  } catch (error) {
    if (aiStatusEl) aiStatusEl.textContent = String(error.message || error);
  }
  refreshAiPanel();
}

async function setSuggestionStatus(id, status) {
  const suggestion = aiSuggestions.find((candidate) => candidate.id === id);
  if (!suggestion) return;
  try {
    const action = status === "accepted" ? "accept" : "deny";
    await apiData(await authorizedFetch(`/api/v1/suggestions/${encodeURIComponent(id)}/${action}`, { method: "POST" }));
    suggestion.status = status;
    syncAcceptedAiEdges();
    refreshAcceptedAiGraphEdges();
  } catch (error) {
    if (aiStatusEl) aiStatusEl.textContent = String(error.message || error);
  }
  refreshAiPanel();
}

async function setAllSuggestions(status) {
  if (!currentReviewSet) return;
  const actionLabel = status === "accepted" ? "accept" : "deny";
  if (!window.confirm(`${actionLabel[0].toUpperCase()}${actionLabel.slice(1)} every suggestion in this review set?`)) {
    return;
  }
  try {
    const action = status === "accepted" ? "accept-all" : "deny-all";
    await apiData(await authorizedFetch(`/api/v1/review-sets/${encodeURIComponent(currentReviewSet.id)}/${action}`, { method: "POST" }));
    aiSuggestions = aiSuggestions.map((suggestion) => ({ ...suggestion, status }));
    syncAcceptedAiEdges();
    refreshAcceptedAiGraphEdges();
  } catch (error) {
    if (aiStatusEl) aiStatusEl.textContent = String(error.message || error);
  }
  refreshAiPanel();
}

const acceptAllSuggestions = () => setAllSuggestions("accepted");
const denyAllSuggestions = () => setAllSuggestions("denied");

function downloadAcceptedChanges() {
  const payload = JSON.stringify(exportPayload(), null, 2);
  const blob = new Blob([payload], { type: "application/json" });
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = "okf-ai-accepted-edges.json";
  link.click();
  URL.revokeObjectURL(url);
  refreshAiPanel();
}

async function apiData(response) {
  const envelope = await response.json().catch(() => ({}));
  if (!response.ok || envelope.api_version !== "v1" || !envelope.data) {
    throw new Error(envelope.error?.message || `${response.status} ${response.statusText}`);
  }
  return envelope.data;
}

function mapApiSuggestion(suggestion) {
  const sourceItem = allKnownItems().find((item) => item.logicalPath === suggestion.source_document);
  const targetItem = allKnownItems().find((item) => item.logicalPath === suggestion.target_document);
  return {
    id: suggestion.id,
    reviewSetId: suggestion.review_set_id,
    source: suggestion.source_document,
    target: suggestion.target_document,
    sourceDocument: suggestion.source_document,
    targetDocument: suggestion.target_document,
    sourceChunk: suggestion.source_chunk,
    targetChunk: suggestion.target_chunk,
    sourceTitle: sourceItem?.title || titleFromPath(suggestion.source_document),
    targetTitle: targetItem?.title || titleFromPath(suggestion.target_document),
    reason: "Voyage embedding similarity",
    score: suggestion.score,
    provider: suggestion.provider,
    model: suggestion.model,
    generationMethod: suggestion.generation_method,
    aiGenerated: suggestion.ai_generated,
    status: suggestion.status,
    createdAt: suggestion.created_at
  };
}

function applySuggestionPayload(payload) {
  currentReviewSet = payload.review_set;
  aiSuggestions = (payload.suggestions || []).map(mapApiSuggestion);
  syncAcceptedAiEdges();
}

async function loadPersistedSuggestions() {
  const data = await apiData(await authorizedFetch("/api/v1/suggestions"));
  applySuggestionPayload(data);
  refreshAcceptedAiGraphEdges();
  refreshAiPanel();
}

async function applyAcceptedRelations() {
  const accepted = aiSuggestions.filter((suggestion) => suggestion.status === "accepted").length;
  if (!accepted) {
    if (aiStatusEl) aiStatusEl.textContent = "No accepted suggestions are ready to apply.";
    return;
  }
  if (!window.confirm(`Write ${accepted} accepted relation(s) to canonical OKF frontmatter?`)) return;
  try {
    const result = await apiData(await authorizedFetch(
      "/api/v1/edges/apply",
      { method: "POST", body: JSON.stringify({ dry_run: false }) },
      AI_ACTION_TIMEOUT_MS
    ));
    if (graphInstance) graphInstance.destroy();
    graphInstance = null;
    graphBuilt = false;
    graphData = { nodes: [], edges: [] };
    await loadPersistedSuggestions();
    await ensureGraph();
    if (aiStatusEl) {
      const recovered = (result.files || []).reduce((sum, file) => sum + (file.recovered || 0), 0);
      aiStatusEl.textContent = `${result.message} Applied ${result.applied}; recovered ${recovered} audit record(s).`;
    }
  } catch (error) {
    if (aiStatusEl) aiStatusEl.textContent = String(error.message || error);
  }
  refreshAiPanel();
}

async function runSemanticSearch() {
  const query = aiSemanticSearchEl?.value.trim();
  if (!query || !aiSearchResultsEl) return;
  if (!window.confirm("Run a Voyage query embedding? This operation may consume provider tokens.")) {
    return;
  }
  aiSearchResultsEl.textContent = "Running explicit Voyage semantic search…";
  try {
    const data = await apiData(await authorizedFetch(
      "/api/v1/voyage/search",
      { method: "POST", body: JSON.stringify({ query, limit: 8 }) },
      AI_ACTION_TIMEOUT_MS
    ));
    aiSearchResultsEl.dataset.loaded = "true";
    aiSearchResultsEl.innerHTML = data.results.length
      ? data.results.map((result) => {
          const item = allKnownItems().find((candidate) => candidate.logicalPath === result.document_path);
          const path = item?.path || result.document_path;
          return `<button type="button" class="ai-search-result" data-doc-path="${escapeHtml(path)}">${escapeHtml(item?.title || titleFromPath(result.document_path))}<span>cosine ${result.score.toFixed(3)} · ${escapeHtml(result.model)}</span></button>`;
        }).join("")
      : '<span class="muted">No semantic matches.</span>';
  } catch (error) {
    aiSearchResultsEl.textContent = String(error.message || error);
  }
}

function populateGraphFilters() {
  const documentNodes = graphData.nodes.filter((node) => node.data.type === "document");
  const topics = uniqueSorted(documentNodes.map((node) => node.data.topic));
  const kinds = uniqueSorted(documentNodes.map((node) => node.data.kind));

  graphTopicFilterEl.innerHTML =
    '<option value="">All topics</option>' +
    topics.map((topic) => `<option value="${escapeHtml(topic)}">${escapeHtml(topic)}</option>`).join("");

  graphKindFilterEl.innerHTML =
    '<option value="">All kinds</option>' +
    kinds.map((kind) => `<option value="${escapeHtml(kind)}">${escapeHtml(kind)}</option>`).join("");
}

function applyGraphFilters() {
  if (!graphInstance) return;

  const selectedTopic = graphTopicFilterEl.value;
  const selectedKind = graphKindFilterEl.value;
  const showTopics = graphShowTopicsEl.checked;

  graphInstance.batch(() => {
    graphInstance.elements().addClass("hidden-by-filter");

    const visibleDocuments = graphInstance.nodes('[type = "document"]').filter((node) => {
      const topicOk = !selectedTopic || node.data("topic") === selectedTopic;
      const kindOk = !selectedKind || node.data("kind") === selectedKind;
      return topicOk && kindOk;
    });

    visibleDocuments.removeClass("hidden-by-filter");

    const visibleTopicIds = new Set();
    if (showTopics) {
      visibleDocuments.forEach((node) => {
        const topic = node.data("topic");
        if (topic) {
          visibleTopicIds.add(`topic:${topic}`);
        }
      });
      graphInstance.nodes('[type = "topic"]').forEach((node) => {
        if (visibleTopicIds.has(node.id())) {
          node.removeClass("hidden-by-filter");
        }
      });
    }

    graphInstance.edges().forEach((edge) => {
      const type = edge.data("type");
      if (type === "links_to") {
        const sourceVisible = !edge.source().hasClass("hidden-by-filter");
        const targetVisible = !edge.target().hasClass("hidden-by-filter");
        if (sourceVisible && targetVisible) {
          edge.removeClass("hidden-by-filter");
        }
      }
      if (type === "ai_accepted" || type === "canonical_relation") {
        const sourceVisible = !edge.source().hasClass("hidden-by-filter");
        const targetVisible = !edge.target().hasClass("hidden-by-filter");
        if (sourceVisible && targetVisible) {
          edge.removeClass("hidden-by-filter");
        }
      }
      if (type === "has_topic" && showTopics) {
        const sourceVisible = !edge.source().hasClass("hidden-by-filter");
        const targetVisible = !edge.target().hasClass("hidden-by-filter");
        if (sourceVisible && targetVisible) {
          edge.removeClass("hidden-by-filter");
        }
      }
    });
  });

  updateGraphRootHighlight();
  const visibleDocs = graphInstance.nodes('[type = "document"]').filter((node) => !node.hasClass("hidden-by-filter")).length;
  const visibleTopics = graphInstance.nodes('[type = "topic"]').filter((node) => !node.hasClass("hidden-by-filter")).length;
  const visibleEdges = graphInstance.edges().filter((edge) => !edge.hasClass("hidden-by-filter")).length;
  const rootLabel = currentGraphRootLabel();
  graphStatusEl.textContent = `Graph ready: Root: ${rootLabel} | ${visibleDocs} documents, ${visibleTopics} topics, ${visibleEdges} visible edges.`;
  refreshPinnedFocus();
}

function clearGraphFilters() {
  graphShowTopicsEl.checked = true;
  graphTopicFilterEl.value = "";
  graphKindFilterEl.value = "";
  applyGraphFilters();
}

function fitGraphView() {
  if (!graphInstance) return;
  const visible = graphInstance.elements().filter((element) => !element.hasClass("hidden-by-filter"));
  if (visible.length) {
    graphInstance.fit(visible, 40);
  }
}

function currentGraphRootPath() {
  const preferred = normalizeDocumentPath(currentPath);
  if (/\.md$/i.test(preferred)) {
    return preferred;
  }
  return graphFallbackRootPath;
}

function graphRootNode() {
  if (!graphInstance) return null;
  const preferred = graphInstance.getElementById(currentGraphRootPath());
  if (preferred && preferred.length && !preferred.hasClass("hidden-by-filter")) {
    return preferred;
  }

  const fallback = graphInstance.getElementById(graphFallbackRootPath);
  if (fallback && fallback.length && !fallback.hasClass("hidden-by-filter")) {
    return fallback;
  }

  const visibleDocuments = graphInstance
    .nodes('[type = "document"]')
    .filter((node) => !node.hasClass("hidden-by-filter"));
  return visibleDocuments.length ? visibleDocuments[0] : null;
}

function currentGraphRootLabel() {
  const root = graphRootNode();
  if (root) {
    return root.data("title") || root.data("label") || titleFromPath(root.id());
  }
  return titleFromPath(currentGraphRootPath());
}

function updateGraphRootHighlight() {
  if (!graphInstance) return;
  graphInstance.nodes().removeClass("root-node");
  const root = graphRootNode();
  if (root) {
    root.addClass("root-node");
  }
}

function refreshGraphForCurrentPath() {
  if (!graphBuilt || !graphInstance) return;
  window.requestAnimationFrame(() => {
    if (!graphInstance) return;
    graphInstance.resize();
    applyGraphFilters();
    focusGraphRootView();
  });
}

function focusGraphRootView() {
  if (!graphInstance) return;
  const root = graphRootNode();
  if (!root) return;
  graphInstance.zoom(clamp(GRAPH_INITIAL_ZOOM, GRAPH_MIN_ZOOM, GRAPH_MAX_ZOOM));
  graphInstance.center(root);
}

function runGraphLayout(options = {}) {
  if (!graphInstance) return;
  const { focusRoot = false } = options;
  const layout = graphInstance.layout(graphLayoutOptions());
  if (focusRoot) {
    layout.on("layoutstop", () => {
      focusGraphRootView();
    });
  }
  layout.run();
}

function resetGraphLayout() {
  runGraphLayout({ focusRoot: true });
}

function clearNeighborhoodHighlight() {
  if (!graphInstance) return;
  graphInstance.elements().removeClass("dimmed-by-focus");
  graphInstance.elements().removeClass("focus-neighborhood");
}

function applyNeighborhoodHighlight(node) {
  if (!graphInstance) return;
  clearNeighborhoodHighlight();

  const visibleElements = graphInstance.elements().filter((element) => !element.hasClass("hidden-by-filter"));
  visibleElements.addClass("dimmed-by-focus");

  const neighborhood = node.closedNeighborhood().filter((element) => !element.hasClass("hidden-by-filter"));
  neighborhood.removeClass("dimmed-by-focus");
  neighborhood.addClass("focus-neighborhood");
  node.removeClass("dimmed-by-focus");
  node.addClass("focus-neighborhood");
}

function pinNodeFocus(node) {
  pinnedNodeId = node ? node.id() : null;
  if (node) {
    applyNeighborhoodHighlight(node);
  } else {
    clearNeighborhoodHighlight();
  }
}

function clearGraphFocus() {
  pinnedNodeId = null;
  clearNeighborhoodHighlight();
}

function refreshAcceptedAiGraphEdges() {
  if (!graphBuilt || !graphInstance) return;
  graphInstance.batch(() => {
    graphInstance.edges('[type = "ai_accepted"]').remove();
    for (const edge of acceptedAiEdges) {
      const source = graphInstance.getElementById(edge.source);
      const target = graphInstance.getElementById(edge.target);
      if (!source.length || !target.length) continue;
      graphInstance.add({
        group: "edges",
        data: {
          id: `accepted:${edge.source}->${edge.target}`,
          type: "ai_accepted",
          source: edge.source,
          target: edge.target,
          label: "AI accepted"
        }
      });
    }
  });
  applyGraphFilters();
}

function zoomGraphAroundPoint(nextZoom, renderedPosition) {
  if (!graphInstance) return;
  graphInstance.zoom({
    level: clamp(nextZoom, GRAPH_MIN_ZOOM, GRAPH_MAX_ZOOM),
    renderedPosition
  });
}

function installGraphWheelZoom() {
  if (!graphInstance) return;
  const container = graphInstance.container();
  if (!container || container.dataset.okfWheelZoomInstalled === "true") {
    return;
  }

  container.addEventListener(
    "wheel",
    (event) => {
      event.preventDefault();

      const rect = container.getBoundingClientRect();
      const renderedPosition = {
        x: event.clientX - rect.left,
        y: event.clientY - rect.top
      };
      const currentZoom = graphInstance.zoom();
      const zoomFactor = event.deltaY < 0 ? 1 + GRAPH_WHEEL_ZOOM_STEP : 1 / (1 + GRAPH_WHEEL_ZOOM_STEP);
      zoomGraphAroundPoint(currentZoom * zoomFactor, renderedPosition);
    },
    { passive: false }
  );

  container.dataset.okfWheelZoomInstalled = "true";
}

function refreshPinnedFocus() {
  if (!graphInstance || !pinnedNodeId) return;
  const node = graphInstance.getElementById(pinnedNodeId);
  if (node && node.length && !node.hasClass("hidden-by-filter")) {
    applyNeighborhoodHighlight(node);
  } else {
    clearGraphFocus();
  }
}

async function ensureGraph() {
  if (graphBuilt) return;
  if (typeof window.cytoscape !== "function") {
    graphStatusEl.textContent = "Cytoscape could not be loaded from docs-browser/vendor/cytoscape.min.js.";
    return;
  }

  graphStatusEl.textContent = "Building document graph…";

  try {
    graphData = await buildGraphData();
    populateGraphFilters();
    graphInstance = window.cytoscape({
      container: graphViewEl,
      elements: [...graphData.nodes, ...graphData.edges],
      minZoom: GRAPH_MIN_ZOOM,
      maxZoom: GRAPH_MAX_ZOOM,
      userZoomingEnabled: false,
      style: [
        {
          selector: "node",
          style: {
            "background-color": "#4ea1ff",
            color: "#edf2f7",
            label: "data(title)",
            "font-size": 11,
            "text-wrap": "wrap",
            "text-max-width": 150,
            "text-valign": "center",
            "text-halign": "center",
            shape: "round-rectangle",
            width: "label",
            height: "label",
            padding: "12px",
            "border-width": 2,
            "border-color": "#d8e8ff"
          }
        },
        {
          selector: 'node[kind = "index"]',
          style: {
            "background-color": "#26a269",
            shape: "rectangle"
          }
        },
        {
          selector: 'node[kind = "plan"]',
          style: {
            "background-color": "#e5a50a",
            shape: "cut-rectangle"
          }
        },
        {
          selector: 'node[type = "topic"]',
          style: {
            "background-color": "#8f5cff",
            label: "data(label)",
            shape: "round-rectangle",
            width: "label",
            height: "label",
            padding: "10px",
            "text-max-width": 120
          }
        },
        {
          selector: "edge",
          style: {
            width: 2,
            "line-color": "#4b5568",
            "target-arrow-color": "#4b5568",
            "target-arrow-shape": "triangle",
            "curve-style": "bezier",
            opacity: 0.85
          }
        },
        {
          selector: 'edge[type = "has_topic"]',
          style: {
            "line-style": "dashed",
            "target-arrow-shape": "none",
            "line-color": "#8f5cff",
            opacity: 0.7
          }
        },
        {
          selector: 'edge[type = "ai_accepted"]',
          style: {
            width: 3,
            "line-style": "dotted",
            "line-color": "#ff6bcb",
            "target-arrow-color": "#ff6bcb",
            "target-arrow-shape": "triangle",
            opacity: 0.9
          }
        },
        {
          selector: 'edge[type = "canonical_relation"]',
          style: {
            width: 4,
            "line-style": "solid",
            "line-color": "#26a269",
            "target-arrow-color": "#26a269",
            "target-arrow-shape": "triangle",
            opacity: 0.95
          }
        },
        {
          selector: ".hidden-by-filter",
          style: {
            display: "none"
          }
        },
        {
          selector: ".dimmed-by-focus",
          style: {
            opacity: 0.14
          }
        },
        {
          selector: ".focus-neighborhood",
          style: {
            opacity: 1
          }
        },
        {
          selector: ".root-node",
          style: {
            "border-width": 5,
            "border-color": "#ffd166",
            padding: "16px",
            "font-weight": "700",
            "text-outline-width": 1,
            "text-outline-color": "#11151d"
          }
        },
        {
          selector: ":selected",
          style: {
            "border-color": "#ffffff",
            "border-width": 3,
            "line-color": "#ffffff",
            "target-arrow-color": "#ffffff"
          }
        }
      ]
    });

    graphInstance.on("tap", "node", (event) => {
      const node = event.target;
      const type = node.data("type");
      if (type === "topic") {
        pinNodeFocus(node);
        graphTopicFilterEl.value = node.data("label") || "";
        applyGraphFilters();
        fitGraphView();
        return;
      }

      const path = node.data("path");
      const known = findItemByPath(path);
      const item =
        known ||
        {
          title: titleFromPath(path),
          path,
          kind: "document"
        };
      void setMode("documents");
      openDocument(item);
    });

    graphInstance.on("mouseover", "node", (event) => {
      if (pinnedNodeId) return;
      applyNeighborhoodHighlight(event.target);
    });

    graphInstance.on("mouseout", "node", () => {
      if (pinnedNodeId) return;
      clearNeighborhoodHighlight();
    });

    graphInstance.on("tap", (event) => {
      if (event.target === graphInstance) {
        clearGraphFocus();
      }
    });

    graphInstance.on("cxttap", "node", (event) => {
      const node = event.target;
      if (pinnedNodeId === node.id()) {
        clearGraphFocus();
      } else {
        pinNodeFocus(node);
      }
    });

    graphBuilt = true;
    applyGraphFilters();
    installGraphWheelZoom();
    runGraphLayout({ focusRoot: true });
  } catch (error) {
    graphStatusEl.textContent = `Could not build the graph: ${error.message || error}`;
  }
}

function buildTree(query = "") {
  const q = query.trim().toLowerCase();
  treeEl.innerHTML = "";

  DOCS.forEach((group) => {
    const items = group.items.filter((item) => {
      if (!q) return true;
      return (
        item.title.toLowerCase().includes(q) ||
        item.path.toLowerCase().includes(q) ||
        item.kind.toLowerCase().includes(q) ||
        item.directoryParts.some((part) => part.toLowerCase().includes(q))
      );
    });

    if (!items.length) return;

    const wrapper = document.createElement("section");
    wrapper.className = "tree-group tree-root";

    const title = document.createElement("div");
    title.className = "tree-group-title";
    title.textContent = group.group;
    wrapper.appendChild(title);

    const list = document.createElement("div");
    list.className = "tree-list";
    renderNavigationNode(list, buildNavigationNode(items), Boolean(q));

    wrapper.appendChild(list);
    treeEl.appendChild(wrapper);
  });
}

function buildNavigationNode(items) {
  const root = { documents: [], directories: new Map() };
  for (const item of items) {
    let node = root;
    for (const directory of item.directoryParts || []) {
      if (!node.directories.has(directory)) {
        node.directories.set(directory, { documents: [], directories: new Map() });
      }
      node = node.directories.get(directory);
    }
    node.documents.push(item);
  }
  return root;
}

function navigationNodeContainsPath(node, path) {
  if (node.documents.some((item) => item.path === path)) return true;
  return [...node.directories.values()].some((child) => navigationNodeContainsPath(child, path));
}

function renderNavigationNode(container, node, revealMatches = false) {
  for (const item of node.documents.sort((left, right) => left.title.localeCompare(right.title))) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = `tree-document ${item.navigationClass}`;
    button.dataset.path = item.path;
    const identity = item.okfUri || displayDocumentPath(item.path);
    button.innerHTML = `${escapeHtml(item.title)}<span class="small">${escapeHtml(identity)}</span>`;
    button.addEventListener("click", () => openDocument(item));
    if (item.path === currentPath) button.classList.add("active");
    container.appendChild(button);
  }

  const directories = [...node.directories.entries()]
    .sort(([left], [right]) => left.localeCompare(right));
  for (const [name, child] of directories) {
    const directory = document.createElement("details");
    directory.className = "tree-directory";
    directory.open = revealMatches || navigationNodeContainsPath(child, currentPath);
    const label = document.createElement("summary");
    label.textContent = name;
    directory.appendChild(label);
    const contents = document.createElement("div");
    contents.className = "tree-directory-contents";
    renderNavigationNode(contents, child, revealMatches);
    directory.appendChild(contents);
    container.appendChild(directory);
  }
}

function clearNode(node) {
  while (node?.firstChild) node.removeChild(node.firstChild);
}

function appendDefinition(list, label, value) {
  const wrapper = document.createElement("div");
  const term = document.createElement("dt");
  const detail = document.createElement("dd");
  term.textContent = label;
  detail.textContent = String(value);
  wrapper.append(term, detail);
  list.appendChild(wrapper);
}

function updateRootActionState() {
  const ready = Boolean(
    rootProposal?.confirmable &&
    rootReviewedEl?.checked &&
    hasCapability("roots.configure")
  );
  if (rootRegisterEl) rootRegisterEl.disabled = !ready;
  if (rootRegisterInitializeEl) {
    rootRegisterInitializeEl.disabled = !ready || !hasCapability("content.initialize");
  }
}

function renderRootOverrides(summary = rootProposal?.summary) {
  if (!rootOverridesEl) return;
  const active = [];
  if (rootConfiguration?.process_override_active || summary?.process_override_active) active.push("process environment");
  if (rootConfiguration?.dotenv_override_active || summary?.dotenv_override_active) active.push(".env");
  if (summary?.cli_override_active) active.push("CLI");
  if (rootConfiguration?.effective_differs_from_browser_configuration) active.push("effective runtime configuration");
  rootOverridesEl.classList.toggle("hidden", active.length === 0);
  rootOverridesEl.textContent = active.length
    ? `Higher-precedence ${active.join(", ")} overrides are active. The browser saves only ~/.config/okf/config.toml; restart okf-http and resolve overrides before expecting this root to become effective.`
    : "";
}

function renderConfiguredRoots() {
  if (!configuredRootsEl || !rootConfiguration) return;
  clearNode(configuredRootsEl);
  if (!rootConfiguration.roots?.length) {
    const empty = document.createElement("p");
    empty.className = "muted";
    empty.textContent = "No browser-managed document roots are configured.";
    configuredRootsEl.appendChild(empty);
  }
  for (const root of rootConfiguration.roots || []) {
    const card = document.createElement("article");
    card.className = "configured-root";
    const heading = document.createElement("strong");
    heading.textContent = root.mount || root.root_id || "Document root";
    const path = document.createElement("code");
    path.textContent = root.path || "Physical path hidden";
    const meta = document.createElement("span");
    meta.className = "muted";
    meta.textContent = `priority ${root.priority ?? "default"} · ${root.enabled === false ? "disabled" : "enabled"} · change checks ${root.check_for_changes ? "on" : "off"}`;
    card.append(heading, path, meta);
    if (root.check_for_changes) {
      const monitoring = rootMonitoring.get(root.root_id);
      const indicator = document.createElement("strong");
      indicator.className = monitoring?.pending ? "root-change-indicator" : "muted";
      indicator.textContent = monitoring?.pending
        ? `${monitoring.change_count} pending change${monitoring.change_count === 1 ? "" : "s"}`
        : monitoring?.initialized ? "No pending changes" : "Baseline is being initialized";
      const actions = document.createElement("div");
      actions.className = "root-actions";
      const check = document.createElement("button");
      check.type = "button";
      check.dataset.rootAction = "check";
      check.dataset.rootId = root.root_id;
      check.textContent = "Check now";
      actions.appendChild(check);
      if (monitoring?.pending) {
        const details = document.createElement("button");
        details.type = "button";
        details.dataset.rootAction = "details";
        details.dataset.rootId = root.root_id;
        details.textContent = "Details";
        actions.appendChild(details);
      }
      card.append(indicator, actions);
    }
    configuredRootsEl.appendChild(card);
  }
  if (rootConfiguration.restart_required) {
    const warning = document.createElement("p");
    warning.className = "root-warning";
    warning.textContent = "Restart okf-http before relying on the changed root configuration.";
    configuredRootsEl.prepend(warning);
  }
  renderRootOverrides();
}

async function loadRootConfiguration() {
  if (!hasCapability("roots.propose")) return;
  rootConfiguration = await apiData(await authorizedFetch("/api/v1/roots/configuration"));
  try {
    const monitoring = await apiData(await authorizedFetch("/api/v1/roots/monitoring"));
    rootMonitoring = new Map((monitoring.roots || []).map((root) => [root.root_id, root]));
  } catch (_error) {
    rootMonitoring = new Map();
  }
  renderConfiguredRoots();
}

async function checkRootChanges(rootId) {
  if (rootStatusEl) rootStatusEl.textContent = "Checking the selected root once without using Voyage AI…";
  try {
    const pending = await apiData(await authorizedFetch(
      `/api/v1/roots/${encodeURIComponent(rootId)}/monitoring/check`,
      { method: "POST" },
      AI_ACTION_TIMEOUT_MS
    ));
    await loadRootConfiguration();
    if (pending.changes?.length) {
      renderPendingChanges(pending);
      if (rootStatusEl) rootStatusEl.textContent = "Changes detected. Review the cached snapshot details.";
    } else if (rootStatusEl) {
      rootStatusEl.textContent = "Check complete. No changes are pending.";
    }
  } catch (error) {
    if (rootStatusEl) rootStatusEl.textContent = String(error.message || error);
  }
}

async function loadPendingChanges(rootId) {
  try {
    const pending = await apiData(await authorizedFetch(
      `/api/v1/roots/${encodeURIComponent(rootId)}/monitoring/pending`
    ));
    renderPendingChanges(pending);
  } catch (error) {
    if (rootStatusEl) rootStatusEl.textContent = String(error.message || error);
  }
}

function renderPendingChanges(pending) {
  currentPendingChanges = pending;
  rootChangeReviewEl?.classList.remove("hidden");
  if (rootChangeDigestEl) rootChangeDigestEl.textContent = `Root ${pending.root_id} · snapshot ${pending.snapshot_digest} · ${pending.changes.length} changes`;
  clearNode(rootChangeListEl);
  for (const change of pending.changes || []) {
    const item = document.createElement("article");
    const title = document.createElement("strong");
    const detail = document.createElement("span");
    title.textContent = `${change.kind}: ${change.path}`;
    detail.className = "muted";
    detail.textContent = `${change.previous_path ? `Previous path: ${change.previous_path}. ` : ""}${change.detail}`;
    item.append(title, detail);
    rootChangeListEl?.appendChild(item);
  }
  clearNode(rootChangeInventoryEl);
  for (const entry of pending.inventory || []) {
    const row = document.createElement("tr");
    row.dataset.search = `${entry.path} ${entry.state} ${entry.detail || ""}`.toLowerCase();
    for (const value of [entry.path, entry.state, entry.detail || "—"]) {
      const cell = document.createElement("td");
      cell.textContent = value;
      row.appendChild(cell);
    }
    rootChangeInventoryEl?.appendChild(row);
  }
  if (rootChangeReviewedEl) rootChangeReviewedEl.checked = false;
  if (rootChangeFilterEl) rootChangeFilterEl.value = "";
  if (rootChangeStatusEl) rootChangeStatusEl.textContent = "";
  updatePendingReviewActions();
  filterPendingInventory();
  rootChangeReviewEl?.scrollIntoView({ behavior: "smooth", block: "start" });
}

function filterPendingInventory() {
  const query = rootChangeFilterEl?.value.trim().toLowerCase() || "";
  let shown = 0;
  for (const row of rootChangeInventoryEl?.rows || []) {
    const visible = !query || row.dataset.search.includes(query);
    row.hidden = !visible;
    if (visible) shown += 1;
  }
  if (rootChangeCountEl && currentPendingChanges) {
    rootChangeCountEl.textContent = `Showing ${shown} of ${currentPendingChanges.inventory.length} current entries. Filtering never changes the reviewed snapshot.`;
  }
}

function updatePendingReviewActions() {
  const enabled = Boolean(currentPendingChanges && rootChangeReviewedEl?.checked && hasCapability("roots.configure"));
  if (rootChangeAcceptEl) rootChangeAcceptEl.disabled = !enabled;
  if (rootChangeDismissEl) rootChangeDismissEl.disabled = !enabled;
}

async function decidePendingChanges(action) {
  if (!currentPendingChanges || !rootChangeReviewedEl?.checked) return;
  const pending = currentPendingChanges;
  rootChangeAcceptEl.disabled = true;
  rootChangeDismissEl.disabled = true;
  if (rootChangeStatusEl) rootChangeStatusEl.textContent = action === "accept"
    ? "Revalidating the exact snapshot before moving the baseline…"
    : "Dismissing this notice without moving the baseline…";
  try {
    await apiData(await authorizedFetch(
      `/api/v1/roots/${encodeURIComponent(pending.root_id)}/monitoring/${action}`,
      {
        method: "POST",
        headers: { "X-OKF-Change-Review": action },
        body: JSON.stringify({ snapshot_digest: pending.snapshot_digest })
      },
      AI_ACTION_TIMEOUT_MS
    ));
    currentPendingChanges = null;
    rootChangeReviewEl?.classList.add("hidden");
    await loadRootConfiguration();
    if (rootStatusEl) rootStatusEl.textContent = action === "accept"
      ? "The unchanged reviewed snapshot is now the accepted monitoring baseline."
      : "Notice dismissed. The accepted baseline was not changed.";
  } catch (error) {
    if (rootChangeStatusEl) rootChangeStatusEl.textContent = String(error.message || error);
    updatePendingReviewActions();
  }
}

function proposalInput(operation) {
  const priorityText = rootPriorityEl?.value.trim() || "";
  const payload = {
    path: rootPathEl?.value.trim() || "",
    operation,
    check_for_changes: Boolean(rootWatchEl?.checked)
  };
  const mount = rootMountEl?.value.trim() || "";
  if (mount) payload.mount = mount;
  if (priorityText) payload.priority = Number(priorityText);
  return payload;
}

async function createRootProposal(operation) {
  return apiData(await authorizedFetch("/api/v1/roots/proposals", {
    method: "POST",
    body: JSON.stringify(proposalInput(operation))
  }, AI_ACTION_TIMEOUT_MS));
}

function renderProposalTree() {
  if (!rootTreeBodyEl || !rootProposal) return;
  clearNode(rootTreeBodyEl);
  rootResourceTypes = {};
  for (const entry of rootProposal.tree || []) {
    const row = document.createElement("tr");
    row.dataset.search = `${entry.path} ${entry.state} ${entry.detail || ""}`.toLowerCase();
    const path = document.createElement("td");
    const state = document.createElement("td");
    const detail = document.createElement("td");
    const resource = document.createElement("td");
    path.textContent = entry.path;
    state.textContent = entry.state;
    state.className = entry.state.startsWith("rejected") ? "root-rejected" : "";
    detail.textContent = entry.detail || "—";
    if (entry.state === "undeclared_resource") {
      const input = document.createElement("input");
      input.type = "text";
      input.placeholder = "Explicit type";
      input.setAttribute("aria-label", `Resource type for ${entry.path}`);
      input.addEventListener("input", () => {
        const value = input.value.trim();
        if (value) rootResourceTypes[entry.path] = value;
        else delete rootResourceTypes[entry.path];
      });
      resource.appendChild(input);
    } else {
      resource.textContent = "—";
    }
    row.append(path, state, detail, resource);
    rootTreeBodyEl.appendChild(row);
  }
  filterProposalTree();
}

function filterProposalTree() {
  const query = rootTreeFilterEl?.value.trim().toLowerCase() || "";
  let shown = 0;
  for (const row of rootTreeBodyEl?.rows || []) {
    const visible = !query || row.dataset.search.includes(query);
    row.hidden = !visible;
    if (visible) shown += 1;
  }
  if (rootTreeCountEl && rootProposal) {
    rootTreeCountEl.textContent = `Showing ${shown} of ${rootProposal.tree?.length || 0} entries. Filtering does not change what will be registered.`;
  }
}

function renderRootProposal() {
  if (!rootProposal) return;
  rootReviewEl?.classList.remove("hidden");
  if (rootReviewPathEl) rootReviewPathEl.textContent = rootProposal.canonical_root;
  if (rootExpiryEl) rootExpiryEl.textContent = `Expires in ${rootProposal.expires_in_seconds}s`;
  clearNode(rootSummaryEl);
  appendDefinition(rootSummaryEl, "Accepted files", rootProposal.summary.accepted_files);
  appendDefinition(rootSummaryEl, "Rejected entries", rootProposal.summary.rejected_entries);
  appendDefinition(rootSummaryEl, "Inspected entries", rootProposal.summary.inspected_entries);
  appendDefinition(rootSummaryEl, "Candidate bytes", rootProposal.summary.total_candidate_bytes);
  appendDefinition(rootSummaryEl, "Writable", rootProposal.summary.writable ? "yes" : "no");
  clearNode(rootConflictsEl);
  for (const conflict of rootProposal.conflicts || []) {
    const item = document.createElement("p");
    item.className = conflict.blocking ? "root-warning" : "muted";
    item.textContent = `${conflict.blocking ? "Blocking" : "Notice"}: ${conflict.code}${conflict.path ? ` · ${conflict.path}` : ""} · ${conflict.detail}`;
    rootConflictsEl.appendChild(item);
  }
  if (!(rootProposal.conflicts || []).length) {
    const clear = document.createElement("p");
    clear.className = "muted";
    clear.textContent = "No identity, mount, or path conflicts were found.";
    rootConflictsEl.appendChild(clear);
  }
  if (rootReviewedEl) rootReviewedEl.checked = false;
  renderProposalTree();
  renderRootOverrides(rootProposal.summary);
  updateRootActionState();
  rootReviewEl?.focus?.();
}

function resetRootProposal({ clearForm = false } = {}) {
  rootProposal = null;
  rootInitializationProposal = null;
  rootInitializationPlan = null;
  rootResourceTypes = {};
  rootReviewEl?.classList.add("hidden");
  if (rootReviewedEl) rootReviewedEl.checked = false;
  if (rootTreeFilterEl) rootTreeFilterEl.value = "";
  if (clearForm) rootProposalFormEl?.reset();
  updateRootActionState();
}

async function previewRoot(event) {
  event.preventDefault();
  resetRootProposal();
  if (rootStatusEl) rootStatusEl.textContent = "Scanning the selected root once and building a bounded proposal…";
  try {
    await loadRootConfiguration();
    rootProposal = await createRootProposal("registration");
    renderRootProposal();
    if (rootStatusEl) rootStatusEl.textContent = rootProposal.confirmable
      ? "Preview ready. Review the complete tree before choosing an action."
      : "Preview ready, but blocking conflicts prevent registration.";
  } catch (error) {
    if (rootStatusEl) rootStatusEl.textContent = String(error.message || error);
  }
}

async function registerReviewedRoot(initialize) {
  if (!rootProposal || !rootReviewedEl?.checked) return;
  if (rootStatusEl) rootStatusEl.textContent = "Registering the reviewed proposal…";
  try {
    const registration = await apiData(await authorizedFetch(
      `/api/v1/roots/proposals/${encodeURIComponent(rootProposal.id)}/registration`,
      {
        method: "POST",
        headers: { "X-OKF-Config-Write": "confirm" },
        body: JSON.stringify({
          proposal_digest: rootProposal.proposal_digest,
          expected_revision: rootConfiguration?.revision || ""
        })
      }
    ));
    await loadRootConfiguration();
    if (!initialize) {
      resetRootProposal();
      if (rootStatusEl) rootStatusEl.textContent = registration.restart_required
        ? "Root registered read-only. Restart okf-http before relying on the new configuration."
        : "Root registered read-only.";
      return;
    }
    if (rootStatusEl) rootStatusEl.textContent = "Registration complete. Preparing a separate source-initialization preview…";
    rootInitializationProposal = await createRootProposal("source_initialization");
    rootInitializationPlan = await apiData(await authorizedFetch(
      `/api/v1/roots/proposals/${encodeURIComponent(rootInitializationProposal.id)}/initialization/plan`,
      { method: "POST", body: JSON.stringify({ proposal_digest: rootInitializationProposal.proposal_digest, resource_types: rootResourceTypes }) },
      AI_ACTION_TIMEOUT_MS
    ));
    renderInitializationPlan();
  } catch (error) {
    if (rootStatusEl) rootStatusEl.textContent = String(error.message || error);
  }
}

function renderInitializationPlan() {
  clearNode(rootInitializationChangesEl);
  if (rootInitializationGitEl) {
    const git = rootInitializationPlan?.git;
    rootInitializationGitEl.classList.toggle("hidden", !git?.dirty);
    rootInitializationGitEl.textContent = git?.dirty
      ? `The Git worktree is dirty (${git.entries.length} reported entries). Review the diff especially carefully.`
      : "";
  }
  for (const change of rootInitializationPlan?.changes || []) {
    const article = document.createElement("article");
    const heading = document.createElement("strong");
    const diff = document.createElement("pre");
    heading.textContent = change.path;
    diff.textContent = change.diff;
    article.append(heading, diff);
    rootInitializationChangesEl?.appendChild(article);
  }
  if (!(rootInitializationPlan?.changes || []).length) {
    const empty = document.createElement("p");
    empty.className = "muted";
    empty.textContent = "No source changes are required.";
    rootInitializationChangesEl?.appendChild(empty);
  }
  if (rootSourceConfirmEl) rootSourceConfirmEl.checked = false;
  if (rootInitializationSubmitEl) rootInitializationSubmitEl.disabled = true;
  if (rootInitializationErrorEl) rootInitializationErrorEl.textContent = "";
  rootInitializationDialogEl?.showModal();
}

async function confirmRootInitialization(event) {
  event.preventDefault();
  if (!rootSourceConfirmEl?.checked || !rootInitializationProposal || !rootInitializationPlan) return;
  rootInitializationSubmitEl.disabled = true;
  if (rootInitializationErrorEl) rootInitializationErrorEl.textContent = "Writing the reviewed source changes…";
  try {
    await apiData(await authorizedFetch(
      `/api/v1/roots/proposals/${encodeURIComponent(rootInitializationProposal.id)}/initialization`,
      {
        method: "POST",
        headers: { "X-OKF-Source-Write": "confirm" },
        body: JSON.stringify({
          proposal_digest: rootInitializationProposal.proposal_digest,
          plan_digest: rootInitializationPlan.plan_digest,
          resource_types: rootResourceTypes
        })
      },
      AI_ACTION_TIMEOUT_MS
    ));
    rootInitializationDialogEl?.close();
    resetRootProposal({ clearForm: true });
    if (rootStatusEl) rootStatusEl.textContent = "Root registered and OKF source initialization completed. Restart okf-http before relying on the new root configuration.";
  } catch (error) {
    if (rootInitializationErrorEl) rootInitializationErrorEl.textContent = String(error.message || error);
    rootInitializationSubmitEl.disabled = false;
  }
}

window.addEventListener("popstate", (event) => {
  const docPath = event.state?.docPath;
  const mode = event.state?.mode || "documents";
  currentMode = mode;
  if (!docPath) {
    return;
  }
  const known = DOCS.flatMap((group) => group.items).find((item) => item.path === docPath);
  const item =
    known ||
    {
      title: titleFromPath(docPath),
      path: docPath,
      kind: /\.csv(?:#.*)?$/i.test(docPath) ? "csv" : "document"
    };
  openDocumentWithOptions(item, { pushHistory: false });
  void setMode(mode);
});

modeDocumentsEl.addEventListener("click", () => void setMode("documents"));
modeGraphEl.addEventListener("click", () => void setMode("graph"));
modeAiEl?.addEventListener("click", () => void setMode("ai"));
modeRootsEl?.addEventListener("click", () => void setMode("roots"));
rootAccessGuideEl?.addEventListener("click", openRootAccessGuide);
graphShowTopicsEl.addEventListener("change", applyGraphFilters);
graphTopicFilterEl.addEventListener("change", applyGraphFilters);
graphKindFilterEl.addEventListener("change", applyGraphFilters);
graphResetLayoutEl.addEventListener("click", resetGraphLayout);
graphFitViewEl.addEventListener("click", fitGraphView);
graphClearFiltersEl.addEventListener("click", clearGraphFilters);
graphClearFocusEl.addEventListener("click", clearGraphFocus);
okfAiOpenEl?.addEventListener("click", () => void setMode("ai"));
aiRunAnalysisEl?.addEventListener("click", runAiAnalysis);
aiApplyRelationsEl?.addEventListener("click", applyAcceptedRelations);
aiAcceptAllEl?.addEventListener("click", acceptAllSuggestions);
aiDenyAllEl?.addEventListener("click", denyAllSuggestions);
aiSaveStateEl?.addEventListener("click", () => void loadPersistedSuggestions().catch((error) => {
  if (aiStatusEl) aiStatusEl.textContent = String(error.message || error);
}));
aiExportStateEl?.addEventListener("click", downloadAcceptedChanges);
aiSemanticSearchRunEl?.addEventListener("click", runSemanticSearch);
accessPairOpenEl?.addEventListener("click", openPairingDialog);
accessLoginOpenEl?.addEventListener("click", openLoginDialog);
aiPairOpenEl?.addEventListener("click", openPairingDialog);
accessLogoutEl?.addEventListener("click", logoutLocalSession);
pairingFormEl?.addEventListener("submit", submitPairing);
pairingCancelEl?.addEventListener("click", closePairingDialog);
pairingDialogEl?.addEventListener("cancel", () => {
  if (pairingCodeEl) pairingCodeEl.value = "";
  if (pairingErrorEl) pairingErrorEl.textContent = "";
});
pairingCodeEl?.addEventListener("input", () => {
  pairingCodeEl.value = normalizePairingCode(pairingCodeEl.value);
});
loginFormEl?.addEventListener("submit", submitLogin);
loginCancelEl?.addEventListener("click", closeLoginDialog);
loginDialogEl?.addEventListener("cancel", () => {
  if (loginPasswordEl) loginPasswordEl.value = "";
  if (loginErrorEl) loginErrorEl.textContent = "";
});
rootProposalFormEl?.addEventListener("submit", previewRoot);
rootCancelEl?.addEventListener("click", () => {
  resetRootProposal({ clearForm: true });
  if (rootStatusEl) rootStatusEl.textContent = "Root onboarding cancelled.";
});
rootReviewCancelEl?.addEventListener("click", () => resetRootProposal());
rootReviewedEl?.addEventListener("change", updateRootActionState);
rootTreeFilterEl?.addEventListener("input", filterProposalTree);
rootRegisterEl?.addEventListener("click", () => void registerReviewedRoot(false));
rootRegisterInitializeEl?.addEventListener("click", () => void registerReviewedRoot(true));
rootInitializationFormEl?.addEventListener("submit", confirmRootInitialization);
rootInitializationCancelEl?.addEventListener("click", () => {
  rootInitializationDialogEl?.close();
  resetRootProposal({ clearForm: true });
  if (rootStatusEl) rootStatusEl.textContent = "Root remains registered read-only. No source files were changed.";
});
rootSourceConfirmEl?.addEventListener("change", () => {
  if (rootInitializationSubmitEl) rootInitializationSubmitEl.disabled = !rootSourceConfirmEl.checked;
});
configuredRootsEl?.addEventListener("click", (event) => {
  const button = event.target.closest("button[data-root-action]");
  if (!button) return;
  if (button.dataset.rootAction === "check") void checkRootChanges(button.dataset.rootId);
  if (button.dataset.rootAction === "details") void loadPendingChanges(button.dataset.rootId);
});
rootChangeFilterEl?.addEventListener("input", filterPendingInventory);
rootChangeReviewedEl?.addEventListener("change", updatePendingReviewActions);
rootChangeAcceptEl?.addEventListener("click", () => void decidePendingChanges("accept"));
rootChangeDismissEl?.addEventListener("click", () => void decidePendingChanges("dismiss"));
rootChangeCloseEl?.addEventListener("click", () => {
  currentPendingChanges = null;
  rootChangeReviewEl?.classList.add("hidden");
});
aiSuggestionsEl?.addEventListener("click", (event) => {
  const button = event.target.closest("button[data-ai-action]");
  if (!button) return;
  void setSuggestionStatus(button.dataset.aiId, button.dataset.aiAction === "accept" ? "accepted" : "denied");
});
aiSearchResultsEl?.addEventListener("click", (event) => {
  const button = event.target.closest("button[data-doc-path]");
  if (!button) return;
  const path = button.dataset.docPath;
  const item = findItemByPath(path) || { title: titleFromPath(path), path, kind: "document" };
  void setMode("documents");
  openDocument(item);
});

searchEl.addEventListener("input", () => buildTree(searchEl.value));

async function initializeBrowser() {
  await loadServerMode();
  window.setInterval(() => {
    void refreshSessionStatus().catch(() => {
      if (sessionAuthenticated) {
        updateAccessDisplay("Local editor session status could not be refreshed");
      }
    });
  }, 30_000);
  try {
    await loadDocumentCatalog();
  } catch (error) {
    treeEl.innerHTML = '<p class="muted">The OKF document inventory could not be loaded.</p>';
    viewerEl.innerHTML = `<p>Could not load the OKF repository.</p><p class="muted">${escapeHtml(String(error.message || error))}</p>`;
    return;
  }

  buildTree();
  const items = allKnownItems();
  if (!items.length) {
    titleEl.textContent = "No OKF documents";
    pathEl.textContent = "Configure at least one readable OKF document root.";
    kindEl.textContent = "empty repository";
    viewerEl.innerHTML = '<p>No OKF-compatible documents were found in the configured roots.</p>';
    const requestedMode = new URL(window.location.href).searchParams.get("mode") || "documents";
    await setMode(requestedMode === "roots" ? "roots" : "documents");
    return;
  }

  const url = new URL(window.location.href);
  const initialDoc = url.searchParams.get("doc");
  const initialMode = url.searchParams.get("mode") || "documents";
  const known = initialDoc ? findItemByPath(initialDoc) : null;
  const item = known || items[0];
  currentMode = initialMode;
  currentPath = item.path;
  window.history.replaceState({ docPath: item.path, mode: currentMode }, "", pathToUrl(item.path));
  await openDocumentWithOptions(item, { pushHistory: false });
  await setMode(initialMode);
}

void initializeBrowser();
