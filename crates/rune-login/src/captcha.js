// Only RUNE's own verification page. The person solves the challenge; this
// script only renders it and hands the answer back once.
(() => {
  const page = "https://captcha.rune.invalid/";
  if (window !== window.top || location.href !== page) return;
  const config = __RUNE_CAPTCHA__;
  // The engine's channel to RUNE: WebView2's on Windows, WebKit's on Linux.
  const channel = window.chrome?.webview ?? window.webkit?.messageHandlers?.runeCaptchaWake;
  if (!channel) return;
  const wake = channel.postMessage.bind(channel);
  const started = Date.now();
  let reply = null;
  let finished = false;
  const active = () => !finished && Date.now() - started < config.lifetime &&
    window === window.top && location.href === page;
  const send = (kind, value = "") => {
    if (!active() || reply !== null) return;
    reply = config.capability + ":" + kind + ":" + value;
    // This IPC carries no answer. The native side queries the main frame.
    wake(true);
  };
  Object.defineProperty(window, "__rune_captcha_take_" + config.capability, {
    configurable: false, writable: false,
    value: () => {
      if (!active()) { reply = null; finished = true; return null; }
      if (reply === null) return null;
      const value = reply;
      reply = null;
      finished = true;
      return value;
    }
  });
  // What `Challenge` accepts as an answer: 1 to 8192 visible ASCII characters.
  const answerable = answer => typeof answer === "string" &&
    answer.length > 0 && answer.length <= 8192 &&
    Array.from(answer).every(character => character >= "!" && character <= "~");
  document.addEventListener("DOMContentLoaded", () => {
    const status = document.getElementById("status");
    const start = document.getElementById("start");
    const fail = () => {
      status.textContent = config.texts.failed;
      send("error");
    };
    const solved = answer => {
      if (!answerable(answer)) return fail();
      status.textContent = config.texts.verified;
      send("verified", answer);
    };
    const expire = () => send("expired");
    window.runeCaptchaLoaded = () => {
      try {
        const options = { sitekey: config.sitekey, theme: "dark", callback: solved };
        options.size = config.invisible ? "invisible" : "normal";
        // Both the token and the open challenge can run out; either sends nothing.
        for (const name of ["expired-callback", "chalexpired-callback"]) options[name] = expire;
        options["error-callback"] = fail;
        const widget = window.hcaptcha.render("captcha", options);
        // The service's request data, as the official client sets it.
        if (config.rqdata) window.hcaptcha.setData(widget, { rqdata: config.rqdata });
        status.textContent = config.texts.ready;
        if (config.invisible) {
          start.hidden = false;
          start.addEventListener("click", () => {
            try { window.hcaptcha.execute(widget); } catch { fail(); }
          });
        }
      } catch { fail(); }
    };
    const script = document.createElement("script");
    script.src = "https://js.hcaptcha.com/1/api.js?onload=runeCaptchaLoaded&render=explicit&recaptchacompat=off";
    script.async = true;
    script.onerror = fail;
    document.head.appendChild(script);
  }, { once: true });
})();
