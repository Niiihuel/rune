// Offline synthetic browser fixture. Never execute against a real login page.
const assert = require("node:assert/strict");
const vm = require("node:vm");
const fs = require("node:fs");
const path = require("node:path");
const capability = "a".repeat(64);
const source = fs.readFileSync(path.join(__dirname, "../src/handoff.js"), "utf8")
  .replace("__RUNE_CAPABILITY__", capability);

function fixture(origin = "https://discord.com", child = false, engine = "webkit") {
  const wakes = [];
  let clock = 0;
  class XMLHttpRequest { open() {} setRequestHeader() {} }
  const channel = { postMessage: value => wakes.push(value) };
  const context = {
    XMLHttpRequest, URL, Headers, Request, WeakMap, Reflect,
    location: { origin, href: origin + "/login" },
    Date: { now: () => clock },
    fetch: () => Promise.resolve("untouched"),
    // WebKitGTK's handler on Linux, WebView2's channel on Windows.
    ...(engine === "webkit"
      ? { webkit: { messageHandlers: { runeLoginWake: channel } } }
      : { chrome: { webview: channel } })
  };
  context.window = context;
  context.top = child ? {} : context;
  vm.runInNewContext(source, context);
  return { context, wakes, expire: () => clock = 600001,
    take: () => context["__rune_take_" + capability]?.() };
}

for (const [origin, child] of [["https://evil.invalid", false], ["https://discord.com", true]]) {
  const test = fixture(origin, child);
  test.context.fetch("https://discord.com/api/v10/users/@me", { headers: { authorization: "SYNTHETIC_SESSION_MARKER" } });
  assert.deepEqual(test.wakes, []);
  assert.equal(test.take(), undefined);
}
{
  const test = fixture();
  const xhr = new test.context.XMLHttpRequest();
  for (const url of ["https://evil.invalid/api/v10/users/@me", "/login", "https://discord.com.evil.invalid/api/v10/users/@me"]) {
    xhr.open("GET", url);
    xhr.setRequestHeader("Authorization", "SYNTHETIC_SESSION_MARKER");
  }
  assert.deepEqual(test.wakes, []);
  xhr.open("GET", "/api/v10/users/@me");
  xhr.setRequestHeader("Authorization", "x".repeat(2049));
  xhr.setRequestHeader("Authorization", "invalid whitespace marker");
  assert.deepEqual(test.wakes, []);
  xhr.setRequestHeader("Authorization", "SYNTHETIC_SESSION_MARKER");
  xhr.setRequestHeader("Authorization", "SYNTHETIC_SECOND_MARKER");
  assert.deepEqual(test.wakes, [true]);
  assert.equal(test.take(), capability + ":SYNTHETIC_SESSION_MARKER");
  assert.equal(test.take(), null);
}
{
  // WebView2: the same one wake, through the channel taken at document start.
  const test = fixture("https://discord.com", false, "webview2");
  test.context.chrome.webview = { postMessage: () => { throw new Error("replaced by the page"); } };
  test.context.fetch("/api/v10/users/@me", { headers: { authorization: "SYNTHETIC_WEBVIEW2_MARKER" } });
  assert.deepEqual(test.wakes, [true]);
  assert.equal(test.take(), capability + ":SYNTHETIC_WEBVIEW2_MARKER");
}
for (const invalidate of [test => test.expire(), test => test.context.location.origin = "https://evil.invalid"]) {
  const test = fixture();
  test.context.fetch("/api/v10/users/@me", { headers: { authorization: "SYNTHETIC_FETCH_MARKER" } });
  assert.deepEqual(test.wakes, [true]);
  invalidate(test);
  assert.equal(test.take(), null);
}
console.log("Handoff synthetic fixture passed: origin, frame, bounds, one-shot, expiry and credential-free wake (WebKit and WebView2).");
