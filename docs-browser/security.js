(function installOkfSecurity(global) {
  "use strict";

  function escapeHtml(value) {
    return String(value)
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;")
      .replace(/"/g, "&quot;")
      .replace(/'/g, "&#39;");
  }

  function isDocumentPath(pathname) {
    return pathname.startsWith("/okf-docs/") || pathname.startsWith("/okf-root/");
  }

  function classifyMarkdownUrl(target, baseHref, expectedOrigin) {
    if (typeof target !== "string" || target.length > 8192 || /[\u0000-\u001f\u007f]/u.test(target)) {
      return null;
    }
    const trimmed = target.trim();
    if (!trimmed) return null;

    let url;
    try {
      url = new URL(trimmed, baseHref);
    } catch (_error) {
      return null;
    }
    if (url.username || url.password) return null;

    const protocol = url.protocol.toLowerCase();
    if (url.origin === expectedOrigin) {
      if ((protocol === "http:" || protocol === "https:") && isDocumentPath(url.pathname)) {
        return {
          kind: "document",
          href: url.href,
          documentPath: `${url.pathname}${url.search}${url.hash}`
        };
      }
      return null;
    }

    if (protocol === "http:" || protocol === "https:" || protocol === "mailto:") {
      return { kind: "external", href: url.href };
    }
    return null;
  }

  function normalizePairingCode(value) {
    const digits = String(value).replace(/\D/g, "").slice(0, 12);
    return digits.match(/.{1,4}/g)?.join("-") || "";
  }

  function isValidPairingCode(value) {
    return /^\d{4}-\d{4}-\d{4}$/u.test(String(value));
  }

  const api = {
    classifyMarkdownUrl,
    escapeHtml,
    isDocumentPath,
    isValidPairingCode,
    normalizePairingCode
  };
  global.OkfSecurity = Object.freeze(api);
  if (typeof module !== "undefined" && module.exports) {
    module.exports = api;
  }
})(typeof window !== "undefined" ? window : globalThis);
