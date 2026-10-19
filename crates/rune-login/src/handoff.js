// Only this temporary, owner-operated top-level login document is observed.
(() => {
  if (window !== window.top || location.origin !== "https://discord.com") return;
  const key = "__RUNE_CAPABILITY__";
  const started = Date.now();
  let candidate = null;
  let finished = false;
  // The engine's channel to RUNE, taken before the page's own scripts run:
  // WebView2's on Windows, WebKit's handler on Linux.
  const channel = window.chrome?.webview ?? window.webkit?.messageHandlers?.runeLoginWake;
  if (!channel) return;
  const wake = channel.postMessage.bind(channel);
  const active = () => !finished && Date.now() - started < 600000 &&
    window === window.top && location.origin === "https://discord.com";
  const apiRequest = address => {
    try {
      const parsed = new URL(address, location.href);
      return parsed.origin === "https://discord.com" && !parsed.username &&
        !parsed.password && /^\/api\/v[0-9]+\//.test(parsed.pathname);
    } catch { return false; }
  };
  const observe = value => {
    if (!active() || candidate !== null || typeof value !== "string" ||
        value.length < 16 || value.length > 2048 || !/^[\x21-\x7e]+$/.test(value)) return;
    candidate = value;
    // This IPC carries no credential. The native side queries the main frame.
    wake(true);
  };
  Object.defineProperty(window, "__rune_take_" + key, {
    configurable: false, writable: false,
    value: () => {
      if (!active()) { candidate = null; finished = true; return null; }
      if (candidate === null) return null;
      const result = key + ":" + candidate;
      candidate = null;
      finished = true;
      return result;
    }
  });
  const targets = new WeakMap();
  const xhrOpen = XMLHttpRequest.prototype.open;
  const xhrHeader = XMLHttpRequest.prototype.setRequestHeader;
  XMLHttpRequest.prototype.open = function (...args) {
    targets.set(this, apiRequest(args[1]));
    return Reflect.apply(xhrOpen, this, args);
  };
  XMLHttpRequest.prototype.setRequestHeader = function (...args) {
    if (targets.get(this) && String(args[0]).toLowerCase() === "authorization") observe(args[1]);
    return Reflect.apply(xhrHeader, this, args);
  };
  const fetch = window.fetch;
  window.fetch = function (input, options) {
    try {
      const request = input instanceof Request;
      if (apiRequest(request ? input.url : input)) {
        const headers = new Headers(options?.headers ?? (request ? input.headers : undefined));
        observe(headers.get("authorization"));
      }
    } catch { /* Observation must not change the browser request's behavior. */ }
    return Reflect.apply(fetch, this, arguments);
  };
})();
