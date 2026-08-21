import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import assert from "node:assert/strict";
import vm from "node:vm";

const __dirname = dirname(fileURLToPath(import.meta.url));
const source = readFileSync(join(__dirname, "Model.js"), "utf8");
const sandbox = { console };
vm.createContext(sandbox);
vm.runInContext(source, sandbox);
const Model = sandbox;

function test(name, fn) {
  try {
    fn();
    console.log(`ok - ${name}`);
  } catch (err) {
    console.error(`not ok - ${name}`);
    throw err;
  }
}

function sampleStatus(overrides) {
  return Object.assign({
    state: "ok",
    servers: [
      {
        name: "home",
        url: "https://coolify.example.com",
        online: true,
        running: 1,
        queued: 2,
        failed: 0,
        apps: [
          {
            uuid: "u1",
            name: "website",
            fqdn: "example.com",
            deployments: [
              {
                status: "in_progress",
                commit: "abcdef1234567890",
                commitMessage: "ship it",
                createdAt: new Date(Date.now() - 5 * 60 * 1000).toISOString(),
                deploymentUrl: "https://coolify.example.com/deployment/1"
              },
              {
                status: "failed",
                commit: "deadbeef",
                commitMessage: null,
                createdAt: new Date(Date.now() - 3 * 3600 * 1000).toISOString()
              }
            ]
          }
        ]
      },
      { name: "prod", url: "https://prod.example.com", online: false, error: "HTTP 401", running: 0, queued: 0, failed: 0, apps: [] }
    ]
  }, overrides || {});
}

test("parseLine rejects garbage", () => {
  assert.equal(Model.parseLine(""), null);
  assert.equal(Model.parseLine("not-json"), null);
  assert.equal(Model.parseLine('{"state":"nope"}'), null);
});

test("parseLine ok snapshot", () => {
  const status = Model.parseLine(JSON.stringify(sampleStatus()));
  assert.equal(status.state, "ok");
  assert.equal(status.servers.length, 2);
  assert.equal(status.servers[0].name, "home");
  assert.equal(status.servers[0].apps[0].deployments[0].status, "in_progress");
});

test("parseLine error snapshot", () => {
  const status = Model.parseLine(
    JSON.stringify({ state: "error", error: "config file not found" }),
  );
  assert.equal(status.state, "error");
  assert.equal(status.error, "config file not found");
});

test("totals sums servers and counts offline", () => {
  const t = Model.totals(Model.parseLine(JSON.stringify(sampleStatus())));
  assert.equal(t.running, 1);
  assert.equal(t.queued, 2);
  assert.equal(t.online, 1);
  assert.equal(t.offline, 1);
  assert.equal(t.count, 2);
});

test("labelText names the running app", () => {
  const status = Model.parseLine(JSON.stringify(sampleStatus()));
  assert.equal(Model.labelText(status), "\u27F3 website \u23F3 2");
});

test("labelText shows only running count without names", () => {
  const status = Model.parseLine(JSON.stringify(sampleStatus({
    servers: [{ name: "home", url: "u", online: true, running: 3, queued: 0, failed: 0, apps: [] }],
  })));
  assert.equal(Model.labelText(status), "\u27F3 3");
});

test("labelText caps app names and joins", () => {
  const mk = (name) => ({
    name: "home", url: "u", online: true, running: 1, queued: 0, failed: 0,
    apps: [{ uuid: "u", name, deployments: [{ status: "in_progress" }] }],
  });
  const two = Model.parseLine(JSON.stringify(sampleStatus({
    servers: [mk("alpha"), mk("beta")],
  })));
  assert.equal(Model.labelText(two), "\u27F3 alpha \u00B7 beta");
  const many = Model.parseLine(JSON.stringify(sampleStatus({
    servers: [mk("alpha"), mk("beta"), mk("gamma")],
  })));
  assert.equal(Model.labelText(many), "\u27F3 alpha \u00B7 beta \u00B7 +1");
  const long = Model.parseLine(JSON.stringify(sampleStatus({
    servers: [mk("a-very-long-application-name-indeed")],
  })));
  assert.equal(Model.labelText(long), "\u27F3 a-very-long-a\u2026");
});

test("labelText idle and error states", () => {
  const idle = Model.parseLine(JSON.stringify(sampleStatus({
    servers: [{ name: "home", url: "u", online: true, running: 0, queued: 0, failed: 0, apps: [] }],
  })));
  assert.equal(Model.labelText(idle), "\uD83D\uDE80");
  assert.equal(Model.labelText(null), "\uD83D\uDE80 \u2026");
  const err = Model.parseLine(JSON.stringify({ state: "error", error: "x" }));
  assert.equal(Model.labelText(err), "\uD83D\uDE80 !");
});

test("isIdle only when ok and no activity", () => {
  const idle = Model.parseLine(JSON.stringify(sampleStatus({
    servers: [{ name: "home", url: "u", online: true, running: 0, queued: 0, failed: 0, apps: [] }],
  })));
  assert.equal(Model.isIdle(idle), true);
  assert.equal(Model.isIdle(Model.parseLine(JSON.stringify(sampleStatus()))), false);
  assert.equal(Model.isIdle(null), false);
  assert.equal(Model.isIdle(Model.parseLine(JSON.stringify({ state: "error", error: "x" }))), false);
});

test("metaLine combines counts", () => {
  const status = Model.parseLine(JSON.stringify(sampleStatus()));
  assert.equal(Model.metaLine(status), "2 servers \u00B7 \u27F3 1 running \u00B7 \u23F3 2 queued \u00B7 1 offline");
  const idle = Model.parseLine(JSON.stringify(sampleStatus({
    servers: [{ name: "home", url: "u", online: true, running: 0, queued: 0, failed: 0, apps: [] }],
  })));
  assert.equal(Model.metaLine(idle), "1 server");
});

test("relativeTime buckets", () => {
  const now = new Date().toISOString();
  assert.equal(Model.relativeTime(now), "just now");
  const minutes = new Date(Date.now() - 5 * 60 * 1000).toISOString();
  assert.equal(Model.relativeTime(minutes), "5m ago");
  const hours = new Date(Date.now() - 3 * 3600 * 1000).toISOString();
  assert.equal(Model.relativeTime(hours), "3h ago");
  const days = new Date(Date.now() - 2 * 86400 * 1000).toISOString();
  assert.equal(Model.relativeTime(days), "2d ago");
  const old = new Date(Date.now() - 30 * 86400 * 1000).toISOString();
  assert.match(Model.relativeTime(old), /^\d{4}-\d{2}-\d{2}$/);
  assert.equal(Model.relativeTime(""), "");
  assert.equal(Model.relativeTime("not a date"), "");
});

test("statusGlyph covers all states", () => {
  assert.equal(Model.statusGlyph("in_progress"), "\u27F3");
  assert.equal(Model.statusGlyph("queued"), "\u23F3");
  assert.equal(Model.statusGlyph("finished"), "\u2713");
  assert.equal(Model.statusGlyph("failed"), "\u2717");
  assert.equal(Model.statusGlyph("cancelled"), "\u2298");
  assert.equal(Model.statusGlyph("mystery"), "\u00B7");
});

test("isActive flags running states", () => {
  assert.equal(Model.isActive("in_progress"), true);
  assert.equal(Model.isActive("queued"), true);
  assert.equal(Model.isActive("finished"), false);
  assert.equal(Model.isActive(""), false);
});

test("shortSha truncates", () => {
  assert.equal(Model.shortSha("abcdef1234567890"), "abcdef1");
  assert.equal(Model.shortSha("abc"), "abc");
  assert.equal(Model.shortSha(""), "");
  assert.equal(Model.shortSha(null), "");
});

test("hostLabel strips scheme and path", () => {
  assert.equal(Model.hostLabel("https://coolify.example.com/deployment/1"), "coolify.example.com");
  assert.equal(Model.hostLabel("http://10.0.0.5:8000"), "10.0.0.5:8000");
  assert.equal(Model.hostLabel(""), "");
});

test("tooltipText lists per-server activity", () => {
  const status = Model.parseLine(JSON.stringify(sampleStatus()));
  const tip = Model.tooltipText(status);
  assert.match(tip, /home: \u27F3 1 \u23F3 2/);
  assert.match(tip, /prod: offline/);
});

test("serverLine shows activity or bare host", () => {
  const status = Model.parseLine(JSON.stringify(sampleStatus()));
  assert.equal(
    Model.serverLine(status.servers[0]),
    "coolify.example.com \u00B7 \u27F3 1 \u23F3 2",
  );
  assert.equal(Model.serverLine(status.servers[1]), "prod.example.com");
  assert.equal(Model.serverLine(null), "");
});

test("deploymentText combines message and sha", () => {
  assert.equal(
    Model.deploymentText({ commitMessage: "ship it", commit: "abcdef1234567890" }),
    "ship it \u00B7 abcdef1",
  );
  assert.equal(Model.deploymentText({ commitMessage: "only message" }), "only message");
  assert.equal(Model.deploymentText({ commit: "deadbeef" }), "deadbee");
  assert.equal(Model.deploymentText({}), "deployment");
  assert.equal(Model.deploymentText(null), "deployment");
});

test("deploymentText falls back to id and status", () => {
  assert.equal(Model.deploymentText({ id: 123 }), "#123");
  assert.equal(Model.deploymentText({ id: 0, status: "failed" }), "failed");
  assert.equal(Model.deploymentText({ commitMessage: "  ", commit: "", status: "queued" }), "queued");
});

test("deploymentText collapses multi-line messages", () => {
  assert.equal(
    Model.deploymentText({
      commitMessage: "fix: use system font stack\n\nnext/font fails\n\n  two spaces",
      commit: "ae32b688",
    }),
    "fix: use system font stack next/font fails two spaces \u00B7 ae32b68",
  );
});

test("appsWithDeployments filters empty apps but keeps errored ones", () => {
  const apps = [
    { name: "a", deployments: [{ status: "finished" }] },
    { name: "b", deployments: [] },
    { name: "c", deployments: null },
    { name: "d", deployments: [], error: "HTTP 500" },
  ];
  const shown = Model.appsWithDeployments(apps);
  assert.equal(shown.length, 2);
  assert.equal(shown[0].name, "a");
  assert.equal(shown[1].name, "d");
  assert.equal(Model.appsWithDeployments(null).length, 0);
  assert.equal(Model.appsWithDeployments("nope").length, 0);
});

test("runningAppNames collects and dedupes", () => {
  const status = Model.parseLine(JSON.stringify(sampleStatus({
    servers: [
      {
        name: "home", url: "u", online: true, running: 2, queued: 0, failed: 0,
        apps: [
          { name: "website", deployments: [{ status: "in_progress" }, { status: "in_progress" }] },
          { name: "api", deployments: [{ status: "queued" }] },
        ],
      },
    ],
  })));
  const names = Model.runningAppNames(status);
  assert.equal(names.length, 1);
  assert.equal(names[0], "website");
  assert.equal(Model.runningAppNames(null).length, 0);
});

test("shortName truncates with ellipsis", () => {
  assert.equal(Model.shortName("short", 14), "short");
  assert.equal(Model.shortName("a-very-long-name", 14), "a-very-long-n\u2026");
});

test("stripMarkup removes markup-significant characters", () => {
  assert.equal(Model.stripMarkup("plain"), "plain");
  assert.equal(Model.stripMarkup("<b>x</b>"), "bx/b");
  assert.equal(Model.stripMarkup("a & b \"c\" 'd'"), "a  b c d");
  assert.equal(Model.stripMarkup(""), "");
  assert.equal(Model.stripMarkup(null), "");
  // Glyphs used by the widget must pass through untouched.
  assert.equal(Model.stripMarkup("\u27F3 \u23F3 \uD83D\uDE80"), "\u27F3 \u23F3 \uD83D\uDE80");
});

test("statusWord humanizes states", () => {
  assert.equal(Model.statusWord("in_progress"), "running");
  assert.equal(Model.statusWord("queued"), "queued");
  assert.equal(Model.statusWord("finished"), "finished");
  assert.equal(Model.statusWord("failed"), "failed");
  assert.equal(Model.statusWord("cancelled"), "cancelled");
  assert.equal(Model.statusWord("mystery"), "deployment");
});
