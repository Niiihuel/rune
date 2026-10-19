// Offline synthetic page fixture. Never loads hCaptcha nor contacts a network.
const assert = require("node:assert/strict");
const vm = require("node:vm");
const fs = require("node:fs");
const path = require("node:path");
const capability = "a".repeat(64);
const page = "https://captcha.rune.invalid/";
const config = { capability, sitekey: "synthetic-site-key", rqdata: "SYNTHETIC_RQDATA", invisible: false, lifetime: 300000,
  texts: { failed: "failed", verified: "verified", ready: "ready" } };
const source = fs.readFileSync(path.join(__dirname, "../src/captcha.js"), "utf8")
  .replace("__RUNE_CAPTCHA__", JSON.stringify(config));

function fixture(href = page, child = false, engine = "webkit") {
  const wakes = [];
  const loaded = [];
  let ready = null;
  let clock = 0;
  const element = () => ({ hidden: true, textContent: "", addEventListener() {} });
  const nodes = { status: element(), start: element() };
  const context = {
    location: { href },
    Date: { now: () => clock },
    // WebKitGTK's handler on Linux, WebView2's channel on Windows.
    ...(engine === "webkit"
      ? { webkit: { messageHandlers: { runeCaptchaWake: { postMessage: value => wakes.push(value) } } } }
      : { chrome: { webview: { postMessage: value => wakes.push(value) } } }),
    document: {
      addEventListener: (_, listener) => { ready = listener; },
      getElementById: id => nodes[id],
      createElement: () => ({}),
      head: { appendChild: script => loaded.push(script) }
    }
  };
  context.window = context;
  context.top = child ? {} : context;
  vm.runInNewContext(source, context);
  return { context, wakes, loaded, nodes, expire: () => clock = 300001,
    ready: () => ready?.(),
    take: () => context["__rune_captcha_take_" + capability]?.() };
}

function widget(test) {
  const calls = {};
  test.context.hcaptcha = {
    render: (_, options) => { calls.options = options; return 7; },
    setData: (id, data) => { calls.data = [id, data]; },
    execute: () => {}
  };
  test.ready();
  assert.equal(test.loaded.length, 1);
  assert.match(test.loaded[0].src, /^https:\/\/js\.hcaptcha\.com\/1\/api\.js\?onload=runeCaptchaLoaded&render=explicit/);
  test.context.runeCaptchaLoaded();
  return calls;
}

for (const [href, child] of [["https://discord.com/", false], [page, true], [page + "other", false]]) {
  const test = fixture(href, child);
  assert.equal(test.take(), undefined, "no slot outside RUNE's own top-level page");
}
{
  const test = fixture();
  const calls = widget(test);
  assert.equal(calls.options.sitekey, "synthetic-site-key");
  assert.equal(JSON.stringify(calls.data), JSON.stringify([7, { rqdata: "SYNTHETIC_RQDATA" }]));
  assert.equal(test.take(), null, "nothing before the person answers");
  calls.options.callback("two words");
  assert.equal(test.take(), capability + ":error:", "a malformed answer is an error, not a reply");
  assert.equal(test.take(), null);
}
{
  const test = fixture();
  const calls = widget(test);
  calls.options.callback("P1_SYNTHETIC_ANSWER");
  calls.options["expired-callback"]();
  assert.deepEqual(test.wakes, [true], "one wake, carrying no answer");
  assert.equal(test.take(), capability + ":verified:P1_SYNTHETIC_ANSWER");
  assert.equal(test.take(), null, "handed back once");
}
{
  const test = fixture(page, false, "webview2");
  const calls = widget(test);
  calls.options.callback("P1_SYNTHETIC_WEBVIEW2");
  assert.deepEqual(test.wakes, [true], "WebView2's channel carries the same answer-free wake");
  assert.equal(test.take(), capability + ":verified:P1_SYNTHETIC_WEBVIEW2");
}
{
  const test = fixture();
  const calls = widget(test);
  calls.options["chalexpired-callback"]();
  assert.equal(test.take(), capability + ":expired:");
}
for (const invalidate of [test => test.expire(), test => test.context.location.href = "https://evil.invalid/"]) {
  const test = fixture();
  const calls = widget(test);
  calls.options.callback("P1_SYNTHETIC_ANSWER");
  invalidate(test);
  assert.equal(test.take(), null);
}
console.log("Verification synthetic fixture passed: own page, frame, one-shot answer, expiry and answer-free wake.");
