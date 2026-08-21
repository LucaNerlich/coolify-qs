// Pure parsing/formatting shared by BarWidget.qml and Panel.qml.
// Kept in plain JS so node can exercise it without a QML engine.

function parseLine(line) {
  var text = String(line || "").trim();
  if (text === "") return null;
  var parsed;
  try {
    parsed = JSON.parse(text);
  } catch (e) {
    return null;
  }
  if (parsed === null || typeof parsed !== "object") return null;

  var state = String(parsed.state || "");
  if (state !== "ok" && state !== "error") return null;

  return {
    state: state,
    error: typeof parsed.error === "string" ? parsed.error : "",
    servers: Array.isArray(parsed.servers) ? parsed.servers : []
  };
}

// Aggregate counts across all servers.
function totals(status) {
  var running = 0, queued = 0, failed = 0, online = 0, offline = 0;
  var servers = status && Array.isArray(status.servers) ? status.servers : [];
  for (var i = 0; i < servers.length; i++) {
    var s = servers[i];
    if (s.online === true) online++; else offline++;
    running += Number(s.running) || 0;
    queued += Number(s.queued) || 0;
    failed += Number(s.failed) || 0;
  }
  return {
    running: running,
    queued: queued,
    failed: failed,
    online: online,
    offline: offline,
    count: servers.length
  };
}

// Distinct names of applications with a deployment in progress, in
// first-seen order.
function runningAppNames(status) {
  var names = [];
  if (!status || status.state !== "ok") return names;
  var servers = Array.isArray(status.servers) ? status.servers : [];
  for (var i = 0; i < servers.length; i++) {
    var apps = servers[i] && servers[i].apps;
    if (!Array.isArray(apps)) continue;
    for (var j = 0; j < apps.length; j++) {
      var deps = apps[j] && apps[j].deployments;
      if (!Array.isArray(deps)) continue;
      for (var k = 0; k < deps.length; k++) {
        if (deps[k] && deps[k].status === "in_progress") {
          var name = String(apps[j].name || apps[j].uuid || "").trim();
          if (name !== "" && names.indexOf(name) < 0) names.push(name);
        }
      }
    }
  }
  return names;
}

function shortName(name, max) {
  var s = String(name || "");
  if (s.length <= max) return s;
  return s.slice(0, max - 1) + "\u2026";
}

// Bar label: the running app name(s) plus the queued count, or a plain
// rocket when idle.
function labelText(status) {
  if (!status) return "\uD83D\uDE80 \u2026";
  if (status.state === "error") return "\uD83D\uDE80 !";
  var t = totals(status);
  var parts = [];
  if (t.running > 0) {
    var names = runningAppNames(status);
    if (names.length === 0) {
      parts.push("\u27F3 " + t.running);
    } else {
      var shown = names.slice(0, 2).map(function(n) {
        return shortName(n, 14);
      });
      if (names.length > 2) shown.push("+" + (names.length - 2));
      parts.push("\u27F3 " + shown.join(" \u00B7 "));
    }
  }
  if (t.queued > 0) parts.push("\u23F3 " + t.queued);
  if (parts.length === 0) return "\uD83D\uDE80";
  return parts.join(" ");
}

function tooltipText(status) {
  if (!status) return "Coolify";
  if (status.state === "error")
    return "Coolify \u2014 " + (status.error || "config error");
  var t = totals(status);
  if (t.running === 0 && t.queued === 0)
    return "Coolify \u2014 idle\nClick to open the panel";
  var lines = ["Coolify"];
  var servers = status.servers || [];
  for (var i = 0; i < servers.length; i++) {
    var s = servers[i];
    if (!s.online) {
      lines.push(s.name + ": offline");
      continue;
    }
    var r = Number(s.running) || 0;
    var q = Number(s.queued) || 0;
    if (r === 0 && q === 0) {
      lines.push(s.name + ": idle");
      continue;
    }
    var parts = [];
    if (r > 0) parts.push("\u27F3 " + r);
    if (q > 0) parts.push("\u23F3 " + q);
    lines.push(s.name + ": " + parts.join(" "));
  }
  return lines.join("\n");
}

// Panel hero meta line, e.g. "2 servers · ⟳ 1 running · ⏳ 2 queued".
function metaLine(status) {
  if (!status) return "\u2026";
  if (status.state === "error") return "Config error";
  var t = totals(status);
  var parts = [];
  if (t.count > 0) parts.push(t.count === 1 ? "1 server" : t.count + " servers");
  if (t.running > 0) parts.push("\u27F3 " + t.running + " running");
  if (t.queued > 0) parts.push("\u23F3 " + t.queued + " queued");
  if (t.failed > 0) parts.push("\u2717 " + t.failed + " failed");
  if (t.offline > 0) parts.push(t.offline + " offline");
  if (parts.length === 0) return "All idle";
  return parts.join(" \u00B7 ");
}

function isIdle(status) {
  if (!status || status.state !== "ok") return false;
  var t = totals(status);
  return t.running === 0 && t.queued === 0;
}

// Compact relative time for an ISO timestamp ("5m ago", "2d ago", …).
function relativeTime(iso) {
  if (!iso) return "";
  var t = Date.parse(String(iso));
  if (!isFinite(t)) return "";
  var diff = Date.now() - t;
  if (diff < 0) diff = 0;
  var s = Math.floor(diff / 1000);
  if (s < 60) return "just now";
  var m = Math.floor(s / 60);
  if (m < 60) return m + "m ago";
  var h = Math.floor(m / 60);
  if (h < 24) return h + "h ago";
  var d = Math.floor(h / 24);
  if (d < 7) return d + "d ago";
  var date = new Date(t);
  var mm = String(date.getMonth() + 1).padStart(2, "0");
  var dd = String(date.getDate()).padStart(2, "0");
  return date.getFullYear() + "-" + mm + "-" + dd;
}

function statusGlyph(status) {
  switch (String(status || "")) {
    case "in_progress": return "\u27F3";
    case "queued": return "\u23F3";
    case "finished": return "\u2713";
    case "failed": return "\u2717";
    case "cancelled": return "\u2298";
    default: return "\u00B7";
  }
}

function isActive(status) {
  var s = String(status || "");
  return s === "in_progress" || s === "queued";
}

function shortSha(commit) {
  var c = String(commit || "").trim();
  return c.length > 7 ? c.slice(0, 7) : c;
}

// "https://coolify.example.com/anything" -> "coolify.example.com".
function hostLabel(url) {
  var u = String(url || "").replace(/^https?:\/\//, "");
  var slash = u.indexOf("/");
  return slash >= 0 ? u.slice(0, slash) : u;
}

// Server subline: host plus per-server activity, e.g.
// "coolify.example.com · ⟳ 1 · ⏳ 2".
function serverLine(server) {
  if (!server) return "";
  var host = hostLabel(server.url);
  if (!server.online) return host;
  var r = Number(server.running) || 0;
  var q = Number(server.queued) || 0;
  if (r === 0 && q === 0) return host;
  var parts = [];
  if (r > 0) parts.push("\u27F3 " + r);
  if (q > 0) parts.push("\u23F3 " + q);
  return host + " \u00B7 " + parts.join(" ");
}

// Collapse runs of whitespace (including newlines in multi-line commit
// messages) into single spaces, so rows wrap cleanly in one line block.
function collapseWhitespace(value) {
  return String(value || "").replace(/\s+/g, " ").trim();
}

// One-line deployment summary: commit message + short sha, falling back to
// the deployment id and finally the status word so rows without git data
// still read as something ("#123", "finished") instead of an empty entry.
function deploymentText(deployment) {
  var d = deployment || {};
  var msg = collapseWhitespace(d.commitMessage);
  var sha = shortSha(d.commit);
  if (msg !== "" && sha !== "") return msg + " \u00B7 " + sha;
  if (msg !== "") return msg;
  if (sha !== "") return sha;
  var id = Number(d.id);
  if (isFinite(id) && id > 0) return "#" + id;
  return statusWord(d.status);
}

// Apps with deployment history or a per-app fetch error. Apps with
// neither would render as empty sections and are collapsed into a single
// muted caption instead.
function appsWithDeployments(apps) {
  var list = Array.isArray(apps) ? apps : [];
  var out = [];
  for (var i = 0; i < list.length; i++) {
    var deps = list[i] && list[i].deployments;
    var errored = !!(list[i] && list[i].error);
    if ((Array.isArray(deps) && deps.length > 0) || errored) out.push(list[i]);
  }
  return out;
}

function statusWord(status) {
  switch (String(status || "")) {
    case "in_progress": return "running";
    case "queued": return "queued";
    case "finished": return "finished";
    case "failed": return "failed";
    case "cancelled": return "cancelled";
    default: return "deployment";
  }
}

// Replace markup-significant characters outright (not entity-escape) for
// sinks that always render plain text: AutoText only parses markup when the
// text contains a raw "<", and entity sequences without one draw verbatim —
// "foo & bar" would show as "foo &amp; bar". Notification bodies go through
// a real StyledText renderer, so those keep entity-escaping in the Rust
// backend.
function stripMarkup(value) {
  return String(value || "").replace(/[&<>"']/g, "");
}

if (typeof module !== "undefined" && module.exports) {
  module.exports = {
    parseLine, totals, labelText, tooltipText, metaLine, isIdle,
    relativeTime, statusGlyph, isActive, shortSha, hostLabel,
    serverLine, deploymentText, statusWord, collapseWhitespace,
    appsWithDeployments, runningAppNames, shortName, stripMarkup
  };
}
