// Menu overlay injected over the main-menu background page.
// The game sends the menu as JSON (window.__bts_set); clicks and edits go back through
// window.ipc.postMessage. Everything lives in a shadow root so the background page's
// CSS cannot restyle it and ours cannot leak out.
//
// Look: glass panels over the background, a thin-weight spaced title treatment, accent
// rails on the big buttons, switches and slider tracks for settings. Entrance animations
// play only when the page changes; plain re-renders (a toggle flip, a keystroke) swap the
// DOM in place without replaying them, and keep keyboard focus where it was.
(function () {
  if (window.__bts_menu_loaded) return;
  window.__bts_menu_loaded = true;

  const SANS = '"Inter", "Adwaita Sans", "Segoe UI Variable Display", "Segoe UI", "SF Pro Display", ' +
    '-apple-system, "Noto Sans", "Cantarell", "Helvetica Neue", Arial, sans-serif';
  const MONO = '"JetBrains Mono", "Adwaita Mono", "Hack", ui-monospace, "SF Mono", "DejaVu Sans Mono", ' +
    'Menlo, Consolas, monospace';

  const CSS = `
    :host { all: initial; }
    * { box-sizing: border-box; }
    .layer {
      --accent: 115, 199, 255;
      --accent2: 63, 127, 208;
      --warm: 255, 196, 92;
      --ink: #e6eef9;
      --dim: #8d9bb3;
      --line: rgba(150, 190, 255, .16);
      --ease: cubic-bezier(.2, .8, .2, 1);
      position: fixed; inset: 0; pointer-events: none;
      font-family: ${SANS};
      color: var(--ink); font-size: 13px; line-height: 1.4; letter-spacing: .01em;
      -webkit-font-smoothing: antialiased; text-rendering: optimizeLegibility;
      user-select: none; -webkit-user-select: none;
    }
    .layer.hidden { display: none; }

    /* ---------- overlays ---------- */
    .overlay { position: absolute; inset: 0; display: flex; align-items: center; }
    .overlay.center {
      justify-content: center; pointer-events: auto;
      background:
        radial-gradient(ellipse at 50% 45%, rgba(4, 8, 20, .42) 0%, rgba(2, 4, 12, .78) 70%, rgba(1, 2, 8, .9) 100%);
      -webkit-backdrop-filter: blur(5px) saturate(120%); backdrop-filter: blur(5px) saturate(120%);
    }
    .enter .overlay.center { animation: fade .35s ease-out both; }

    /* Main menu: a frosted rail down the left edge that fades into the background. */
    .overlay.left { justify-content: flex-start; }
    .rail {
      position: relative; height: 100%; width: 600px; max-width: 100vw; flex: none;
      display: flex; align-items: center; padding: 0 0 0 clamp(28px, 6.5vw, 112px);
    }
    .rail::before {
      content: ''; position: absolute; inset: 0; pointer-events: none;
      background: linear-gradient(90deg, rgba(2, 4, 12, .86) 0%, rgba(3, 6, 16, .66) 52%, rgba(3, 6, 16, 0) 100%);
      -webkit-backdrop-filter: blur(12px) saturate(130%); backdrop-filter: blur(12px) saturate(130%);
      -webkit-mask-image: linear-gradient(90deg, #000 0%, #000 50%, transparent 100%);
      mask-image: linear-gradient(90deg, #000 0%, #000 50%, transparent 100%);
    }
    .rail::after {
      content: ''; position: absolute; top: 9%; bottom: 9%; left: clamp(14px, 3.2vw, 56px); width: 1px;
      background: linear-gradient(180deg, transparent, rgba(var(--accent), .45) 30%, rgba(var(--accent), .45) 70%, transparent);
      box-shadow: 0 0 10px rgba(var(--accent), .5);
    }
    .enter .rail::before { animation: fade .6s ease-out both; }
    .enter .rail::after { animation: grow .9s var(--ease) .1s both; transform-origin: 50% 50%; }
    .stack {
      position: relative; z-index: 1; width: 400px; max-width: calc(100vw - 48px);
      display: flex; flex-direction: column; gap: 8px; pointer-events: auto;
    }

    /* Centered pages: a glass panel with HUD corner brackets. */
    .panel {
      position: relative; max-width: 94vw; max-height: 92vh; display: flex; flex-direction: column;
      border-radius: 18px;
      background:
        linear-gradient(180deg, rgba(255, 255, 255, .055) 0%, rgba(255, 255, 255, 0) 120px),
        linear-gradient(160deg, rgba(18, 30, 54, .78) 0%, rgba(7, 12, 26, .86) 55%, rgba(5, 9, 20, .9) 100%);
      border: 1px solid rgba(150, 195, 255, .17);
      box-shadow:
        0 30px 80px -10px rgba(0, 0, 0, .75),
        0 0 0 1px rgba(0, 0, 0, .35),
        inset 0 1px 0 rgba(255, 255, 255, .09),
        0 0 60px -20px rgba(var(--accent), .35);
      -webkit-backdrop-filter: blur(22px) saturate(150%); backdrop-filter: blur(22px) saturate(150%);
    }
    .enter .panel { animation: pop .5s var(--ease) both; }
    .panel::before, .panel::after {
      content: ''; position: absolute; width: 22px; height: 22px; pointer-events: none;
      border: 2px solid rgba(var(--accent), .7);
      filter: drop-shadow(0 0 4px rgba(var(--accent), .6));
    }
    .panel::before { top: -1px; left: -1px; border-right: 0; border-bottom: 0; border-top-left-radius: 18px; }
    .panel::after { bottom: -1px; right: -1px; border-left: 0; border-top: 0; border-bottom-right-radius: 18px; }
    .scroll {
      overflow-y: auto; overflow-x: hidden; display: flex; flex-direction: column; gap: 7px;
      padding: 26px 34px 28px; border-radius: inherit;
    }
    .scroll::-webkit-scrollbar { width: 8px; }
    .scroll::-webkit-scrollbar-track { background: transparent; margin: 18px 0; }
    .scroll::-webkit-scrollbar-thumb { background: rgba(var(--accent), .25); border-radius: 4px; border: 2px solid transparent; background-clip: padding-box; }
    .scroll::-webkit-scrollbar-thumb:hover { background-color: rgba(var(--accent), .45); }

    /* Staggered entrance for the items of the page that just opened. */
    .enter .stack > *, .enter .scroll > * { animation: rise .55s var(--ease) both; animation-delay: calc(var(--i, 0) * 38ms + 80ms); }

    /* ---------- layout ---------- */
    .row { display: flex; align-items: center; gap: 10px; }
    .wrap { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
    .col { display: flex; flex-direction: column; gap: 3px; }
    .spread { display: flex; align-items: center; justify-content: space-between; gap: 12px; width: 100%; }
    .columns { display: flex; gap: 16px; }
    .columns > div { flex: 1 1 0; min-width: 0; display: flex; flex-direction: column; gap: 4px; }
    .fixed { flex: none; display: flex; flex-direction: column; gap: 4px; max-width: 100%; }
    .row > .fixed { flex: 1 1 auto; }
    .spacer { flex: 1; }
    .gap { height: 8px; flex: none; }
    .div {
      height: 1px; width: 100%; margin: 8px 0 4px; flex: none;
      background: linear-gradient(90deg, rgba(var(--accent), 0), rgba(var(--accent), .3) 20%, rgba(var(--accent), .12) 80%, rgba(var(--accent), 0));
    }
    .dot { width: 8px; height: 8px; border-radius: 50%; flex: none; box-shadow: 0 0 8px currentColor; }

    /* ---------- text ---------- */
    .t { overflow-wrap: anywhere; }
    .dim { color: var(--dim); font-size: 12.5px; line-height: 1.5; }
    .label { color: #b9c6da; font-size: 11px; font-weight: 700; letter-spacing: .16em; text-transform: uppercase; margin: 6px 0 -1px; }
    .label .hint { margin-left: 8px; color: var(--dim); font-weight: 400; letter-spacing: .02em; text-transform: none; font-size: 12px; }
    .ver { font-family: ${MONO}; font-size: 10.5px; letter-spacing: .12em; color: rgba(141, 155, 179, .65); }

    .hero { display: flex; flex-direction: column; gap: 12px; margin-bottom: 4px; }
    .kicker {
      display: flex; align-items: center; gap: 10px;
      font-family: ${MONO}; font-size: 10.5px; letter-spacing: .32em; text-transform: uppercase;
      color: rgba(var(--accent), .85);
    }
    .kicker::before {
      content: ''; width: 7px; height: 7px; transform: rotate(45deg); flex: none;
      border: 1.5px solid rgba(var(--accent), .9); box-shadow: 0 0 8px rgba(var(--accent), .7);
    }
    .kicker::after { content: ''; height: 1px; flex: 1; max-width: 120px; background: linear-gradient(90deg, rgba(var(--accent), .6), transparent); }
    .wordmark {
      font-size: 46px; line-height: 1.02; font-weight: 200; letter-spacing: .2em; text-transform: uppercase;
      margin-right: -.2em;
      background: linear-gradient(180deg, #ffffff 0%, #dbe9ff 45%, #8fc6ff 100%);
      -webkit-background-clip: text; background-clip: text; color: transparent; -webkit-text-fill-color: transparent;
      filter: drop-shadow(0 0 18px rgba(var(--accent), .35)) drop-shadow(0 2px 2px rgba(0, 0, 0, .6));
    }
    .wordmark b { display: block; font-weight: 700; letter-spacing: .34em; font-size: 1.32em; }
    .tagline { color: #a7b5cb; font-size: 13.5px; line-height: 1.55; max-width: 360px; font-weight: 400; }
    .enter .wordmark { animation: reveal 1.1s var(--ease) both; }

    .title-wrap { display: flex; flex-direction: column; gap: 6px; margin-bottom: 6px; }
    .title {
      font-size: 26px; font-weight: 300; letter-spacing: .2em; text-transform: uppercase; color: #fff;
      text-shadow: 0 0 22px rgba(var(--accent), .35);
    }
    .title::after {
      content: ''; display: block; margin-top: 12px; width: 64px; height: 2px; border-radius: 2px;
      background: linear-gradient(90deg, rgb(var(--accent)), rgba(var(--accent), 0));
      box-shadow: 0 0 10px rgba(var(--accent), .7);
    }
    .head {
      display: flex; align-items: center; gap: 12px; margin: 6px 0 0;
      font-size: 11px; font-weight: 700; letter-spacing: .24em; text-transform: uppercase;
    }
    .head::after { content: ''; flex: 1; height: 1px; background: linear-gradient(90deg, rgba(var(--accent), .35), rgba(var(--accent), 0)); }
    .notice {
      align-self: flex-start; display: flex; align-items: center; gap: 9px; margin-top: 4px;
      padding: 7px 14px 7px 11px; border-radius: 999px; font-size: 12.5px; font-weight: 500;
      background: rgba(var(--warm), .1); border: 1px solid rgba(var(--warm), .32);
      box-shadow: 0 0 20px -6px rgba(var(--warm), .5);
    }
    .notice::before {
      content: ''; width: 7px; height: 7px; border-radius: 50%; flex: none;
      background: rgb(var(--warm)); box-shadow: 0 0 8px rgb(var(--warm));
      animation: pulse 1.8s ease-in-out infinite;
    }
    .enter .notice, .notice.fresh { animation: rise .45s var(--ease) both; }

    .bar { width: 100%; height: 6px; border-radius: 3px; background: rgba(255, 255, 255, .08); overflow: hidden; flex: none;
      box-shadow: inset 0 1px 2px rgba(0, 0, 0, .5); }
    .bar > i { display: block; height: 100%; border-radius: 3px; box-shadow: 0 0 8px currentColor; }

    /* ---------- cards (save slots) ---------- */
    .card {
      position: relative; display: flex; flex-direction: column; gap: 6px; padding: 13px 16px 13px 20px;
      border-radius: 12px; border: 1px solid; overflow: hidden;
      transition: transform .25s var(--ease), box-shadow .25s ease, border-color .2s ease, background-color .2s ease;
    }
    .card::before {
      content: ''; position: absolute; left: 0; top: 0; bottom: 0; width: 3px;
      background: var(--ac); box-shadow: 0 0 12px var(--ac); opacity: .8;
    }
    .card:hover { transform: translateY(-1px); box-shadow: 0 10px 28px -12px rgba(0, 0, 0, .8), 0 0 0 1px var(--ac); }
    .card .cardtitle { font-size: 15.5px; font-weight: 600; letter-spacing: .02em; }
    .meta { display: flex; flex-wrap: wrap; align-items: center; gap: 4px 0; color: var(--dim); font-size: 12px; }
    .meta span + span::before { content: ''; display: inline-block; width: 3px; height: 3px; border-radius: 50%; margin: 0 9px 2px; background: rgba(var(--accent), .55); vertical-align: middle; }

    /* ---------- buttons ---------- */
    button {
      font: inherit; color: inherit; cursor: pointer; outline: none; position: relative;
      -webkit-appearance: none; appearance: none; margin: 0;
    }
    button:focus-visible, .toggle:focus-visible { box-shadow: 0 0 0 2px rgba(3, 6, 14, 1), 0 0 0 4px rgba(var(--accent), .9); }

    .btn {
      display: inline-flex; align-items: center; justify-content: center; gap: 6px; white-space: nowrap;
      padding: 7px 15px; border-radius: 9px; font-size: 12.5px; font-weight: 600; letter-spacing: .03em;
      color: #dfe9f7; border: 1px solid rgba(150, 195, 255, .2);
      background: linear-gradient(180deg, rgba(255, 255, 255, .08), rgba(255, 255, 255, .02)), rgba(20, 34, 60, .7);
      box-shadow: inset 0 1px 0 rgba(255, 255, 255, .07), 0 2px 6px -2px rgba(0, 0, 0, .6);
      transition: transform .15s var(--ease), background-color .15s ease, border-color .15s ease, box-shadow .2s ease, color .15s ease;
    }
    .btn:hover { border-color: rgba(var(--accent), .6); background-color: rgba(30, 54, 92, .85); color: #fff;
      box-shadow: inset 0 1px 0 rgba(255, 255, 255, .1), 0 0 16px -4px rgba(var(--accent), .6); transform: translateY(-1px); }
    .btn:active { transform: translateY(0) scale(.97); transition-duration: .05s; }
    .btn.active, .btn.primary {
      color: #fff; border-color: rgba(var(--accent), .75);
      background: linear-gradient(180deg, rgba(120, 205, 255, .55), rgba(40, 110, 200, .55)), rgba(20, 70, 140, .9);
      box-shadow: inset 0 1px 0 rgba(255, 255, 255, .28), 0 0 18px -6px rgba(var(--accent), .9);
      text-shadow: 0 1px 1px rgba(0, 0, 0, .4);
    }
    .btn.active:hover, .btn.primary:hover { box-shadow: inset 0 1px 0 rgba(255, 255, 255, .3), 0 0 24px -4px rgba(var(--accent), 1); }
    .btn.danger {
      color: #fff; border-color: rgba(255, 110, 95, .75);
      background: linear-gradient(180deg, rgba(255, 110, 90, .5), rgba(170, 30, 30, .55)), rgba(110, 20, 20, .9);
      box-shadow: inset 0 1px 0 rgba(255, 255, 255, .2), 0 0 18px -6px rgba(255, 80, 60, .9);
    }
    .btn.danger:hover { box-shadow: inset 0 1px 0 rgba(255, 255, 255, .25), 0 0 24px -4px rgba(255, 80, 60, 1); border-color: rgba(255, 150, 130, .9); }
    .btn.flat { background: transparent; border-color: transparent; box-shadow: none; color: #b9c8de; }
    .btn.flat:hover { background: rgba(var(--accent), .1); border-color: rgba(var(--accent), .2); box-shadow: none; }
    .btn.disabled, .btn.disabled:hover, .btn.disabled:active {
      cursor: default; color: rgba(160, 172, 192, .4); transform: none;
      background: rgba(12, 18, 30, .5); border-color: rgba(120, 140, 170, .12); box-shadow: none;
    }

    /* Big menu buttons: label left, note right, accent rail and chevron on hover. */
    .big {
      width: 100%; display: flex; align-items: center; gap: 14px; text-align: left;
      min-height: 52px; padding: 12px 18px 12px 22px; border-radius: 12px; overflow: hidden;
      color: #e8f0fb; border: 1px solid rgba(150, 195, 255, .1);
      background: linear-gradient(90deg, rgba(255, 255, 255, .06), rgba(255, 255, 255, .015)), rgba(10, 18, 34, .55);
      box-shadow: inset 0 1px 0 rgba(255, 255, 255, .05);
      transition: transform .25s var(--ease), background-color .2s ease, border-color .2s ease, box-shadow .25s ease;
    }
    .big::before {
      content: ''; position: absolute; left: 0; top: 22%; bottom: 22%; width: 3px; border-radius: 0 3px 3px 0;
      background: rgb(var(--accent)); box-shadow: 0 0 12px rgba(var(--accent), .9);
      transform: scaleY(0); opacity: 0; transition: transform .3s var(--ease), opacity .2s ease;
    }
    .big::after {
      content: ''; position: absolute; inset: 0; pointer-events: none;
      background: linear-gradient(105deg, transparent 30%, rgba(255, 255, 255, .13) 48%, transparent 66%);
      transform: translateX(-110%); transition: none;
    }
    .big .lbl { flex: 1; font-size: 17px; font-weight: 500; letter-spacing: .05em; transition: transform .3s var(--ease), letter-spacing .3s var(--ease); }
    .big .note {
      font-family: ${MONO}; font-size: 10.5px; letter-spacing: .08em; text-transform: uppercase;
      color: rgba(170, 188, 214, .72); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 55%;
      transition: color .2s ease;
    }
    .big .note.key {
      padding: 2px 7px; border-radius: 5px; border: 1px solid rgba(170, 188, 214, .3); background: rgba(255, 255, 255, .04);
      box-shadow: inset 0 -1px 0 rgba(255, 255, 255, .06);
    }
    .big .chev {
      width: 9px; height: 9px; flex: none; margin-left: -4px; border-right: 2px solid rgb(var(--accent)); border-top: 2px solid rgb(var(--accent));
      transform: translateX(-8px) rotate(45deg); opacity: 0; transition: transform .3s var(--ease), opacity .2s ease;
    }
    .big:hover, .big:focus-visible {
      border-color: rgba(var(--accent), .38); background-color: rgba(22, 44, 80, .7);
      transform: translateX(4px);
      box-shadow: inset 0 1px 0 rgba(255, 255, 255, .08), 0 8px 26px -10px rgba(0, 0, 0, .9), 0 0 22px -8px rgba(var(--accent), .7);
    }
    .big:hover::before, .big:focus-visible::before { transform: scaleY(1); opacity: 1; }
    .big:hover::after { transform: translateX(110%); transition: transform .8s var(--ease); }
    .big:hover .lbl, .big:focus-visible .lbl { letter-spacing: .08em; }
    .big:hover .note { color: rgba(205, 222, 245, .95); }
    .big:hover .chev, .big:focus-visible .chev { opacity: 1; transform: translateX(0) rotate(45deg); }
    .big:active { transform: translateX(4px) scale(.985); transition-duration: .06s; }

    .big.primary, .big.active {
      border-color: rgba(var(--accent), .6);
      background: linear-gradient(100deg, rgba(95, 200, 255, .38) 0%, rgba(50, 120, 220, .3) 55%, rgba(40, 90, 190, .16) 100%), rgba(12, 34, 70, .75);
      box-shadow: inset 0 1px 0 rgba(255, 255, 255, .22), 0 0 30px -10px rgba(var(--accent), .95);
    }
    .big.primary .lbl, .big.active .lbl { font-weight: 600; color: #fff; text-shadow: 0 0 14px rgba(var(--accent), .7); }
    .big.primary .note, .big.active .note { color: rgba(215, 236, 255, .85); }
    .big.primary .note.key { border-color: rgba(215, 236, 255, .4); }
    .big.primary::before, .big.active::before { transform: scaleY(1); opacity: 1; }
    .big.primary:hover, .big.active:hover { background-color: rgba(20, 54, 104, .85);
      box-shadow: inset 0 1px 0 rgba(255, 255, 255, .26), 0 10px 30px -10px rgba(0, 0, 0, .9), 0 0 36px -6px rgba(var(--accent), 1); }
    .big.danger { border-color: rgba(255, 110, 95, .45); background-color: rgba(70, 16, 18, .6); }
    .big.danger::before { background: #ff6a55; box-shadow: 0 0 12px #ff6a55; }
    .big.danger .chev { border-color: #ff8a75; }
    .big.danger:hover { border-color: rgba(255, 130, 110, .75); box-shadow: 0 0 24px -8px rgba(255, 80, 60, .9); }
    .big.flat { background: transparent; border-color: transparent; box-shadow: none; }
    .big.disabled, .big.disabled:hover { cursor: default; transform: none; opacity: .45; box-shadow: none; border-color: rgba(150, 195, 255, .08); }
    .big.disabled::before, .big.disabled::after, .big.disabled .chev { display: none; }
    .big.disabled .lbl { letter-spacing: .05em; }

    /* ---------- fields ---------- */
    input.field {
      width: 100%; font: inherit; font-size: 15px; font-weight: 500; letter-spacing: .02em; color: #fff;
      height: 44px; padding: 0 14px; border-radius: 10px; outline: none; caret-color: rgb(var(--accent));
      background: linear-gradient(180deg, rgba(0, 0, 0, .25), rgba(0, 0, 0, .1)), rgba(8, 16, 32, .75);
      border: 1px solid rgba(150, 195, 255, .2);
      box-shadow: inset 0 2px 6px rgba(0, 0, 0, .45);
      transition: border-color .2s ease, box-shadow .25s ease, background-color .2s ease;
      user-select: text; -webkit-user-select: text;
    }
    input.field:hover { border-color: rgba(var(--accent), .4); }
    input.field:focus {
      border-color: rgba(var(--accent), .85); background-color: rgba(12, 28, 54, .85);
      box-shadow: inset 0 2px 6px rgba(0, 0, 0, .4), 0 0 0 3px rgba(var(--accent), .18), 0 0 22px -6px rgba(var(--accent), .8);
    }
    input.field::placeholder { color: rgba(141, 155, 179, .6); font-weight: 400; font-style: italic; }
    input.field::selection { background: rgba(var(--accent), .4); }

    /* ---------- toggles & steppers ---------- */
    .setting {
      min-height: 34px; display: flex; align-items: center; justify-content: space-between; gap: 14px;
      padding: 2px 10px 2px 12px; margin: -2px -10px -2px -12px; width: calc(100% + 22px);
      border-radius: 10px; border: 1px solid transparent; font-size: 14px; text-align: left; color: #dbe5f3;
      background: transparent; transition: background-color .2s ease, border-color .2s ease;
    }
    .setting:hover { background-color: rgba(var(--accent), .06); border-color: rgba(var(--accent), .12); }
    .setting > .name { flex: 1; min-width: 0; }
    .toggle { cursor: pointer; }
    .switch {
      position: relative; width: 42px; height: 24px; flex: none; border-radius: 12px;
      background: rgba(255, 255, 255, .08); border: 1px solid rgba(150, 195, 255, .22);
      box-shadow: inset 0 1px 3px rgba(0, 0, 0, .6);
      transition: background-color .25s ease, border-color .25s ease, box-shadow .25s ease;
    }
    .switch::after {
      content: ''; position: absolute; top: 2px; left: 2px; width: 18px; height: 18px; border-radius: 50%;
      background: linear-gradient(180deg, #c9d3e2, #8f9bb0); box-shadow: 0 1px 3px rgba(0, 0, 0, .6);
      transition: transform .28s var(--ease), background .25s ease;
    }
    .switch.on {
      background: linear-gradient(90deg, rgba(60, 140, 230, .85), rgba(110, 205, 255, .9));
      border-color: rgba(var(--accent), .9); box-shadow: inset 0 1px 2px rgba(0, 0, 0, .3), 0 0 14px -2px rgba(var(--accent), .8);
    }
    .switch.on::after { transform: translateX(18px); background: linear-gradient(180deg, #ffffff, #dcecff); }
    .switch.flip::after { animation: knob-on .3s var(--ease); }
    .switch.flip:not(.on)::after { animation: knob-off .3s var(--ease); }
    .toggle:hover .switch { border-color: rgba(var(--accent), .6); }
    .toggle:active .switch::after { width: 22px; }
    .toggle:active .switch.on::after { transform: translateX(14px); }

    .ctl { display: flex; align-items: center; gap: 8px; flex: none; }
    .step {
      width: 28px; height: 28px; flex: none; border-radius: 50%; padding: 0;
      display: inline-flex; align-items: center; justify-content: center;
      border: 1px solid rgba(150, 195, 255, .25); background: rgba(255, 255, 255, .05); color: #cfe0f5;
      transition: transform .15s var(--ease), background-color .15s ease, border-color .15s ease, box-shadow .2s ease;
    }
    .step::before, .step.plus::after { content: ''; position: absolute; left: 50%; top: 50%; width: 10px; height: 2px; margin: -1px 0 0 -5px; border-radius: 1px; background: currentColor; }
    .step.plus::after { transform: rotate(90deg); }
    .step:hover { border-color: rgba(var(--accent), .8); background: rgba(var(--accent), .18); color: #fff; box-shadow: 0 0 14px -3px rgba(var(--accent), .8); }
    .step:active { transform: scale(.88); transition-duration: .05s; }
    .track { position: relative; width: 120px; height: 6px; border-radius: 3px; background: rgba(255, 255, 255, .08); box-shadow: inset 0 1px 2px rgba(0, 0, 0, .6); }
    .track > i {
      position: absolute; left: 0; top: 0; bottom: 0; border-radius: 3px;
      background: linear-gradient(90deg, rgba(60, 140, 230, .9), rgb(var(--accent)));
      box-shadow: 0 0 10px rgba(var(--accent), .6);
      transition: width .3s var(--ease);
    }
    .track > i::after {
      content: ''; position: absolute; right: -6px; top: 50%; width: 12px; height: 12px; margin-top: -6px; border-radius: 50%;
      background: #fff; box-shadow: 0 0 0 3px rgba(var(--accent), .35), 0 0 10px rgba(var(--accent), .9);
    }
    .val {
      min-width: 62px; text-align: center; padding: 3px 8px; border-radius: 7px;
      font-family: ${MONO}; font-size: 12px; font-weight: 600; letter-spacing: .04em; color: #fff;
      background: rgba(0, 0, 0, .25); border: 1px solid rgba(150, 195, 255, .14);
    }

    /* ---------- keyframes ---------- */
    @keyframes fade { from { opacity: 0; } to { opacity: 1; } }
    @keyframes rise { from { opacity: 0; transform: translateY(14px); } to { opacity: 1; transform: none; } }
    @keyframes pop { from { opacity: 0; transform: translateY(18px) scale(.97); } to { opacity: 1; transform: none; } }
    @keyframes grow { from { transform: scaleY(0); opacity: 0; } to { transform: none; opacity: 1; } }
    @keyframes reveal {
      from { opacity: 0; letter-spacing: .5em; filter: blur(8px) drop-shadow(0 0 18px rgba(115, 199, 255, 0)); }
      to { opacity: 1; }
    }
    @keyframes pulse { 0%, 100% { opacity: 1; } 50% { opacity: .35; } }
    @keyframes knob-on { from { transform: translateX(0); } to { transform: translateX(18px); } }
    @keyframes knob-off { from { transform: translateX(18px); } to { transform: translateX(0); } }
    @media (prefers-reduced-motion: reduce) {
      *, *::before, *::after { animation-duration: .01ms !important; animation-delay: 0s !important; transition-duration: .01ms !important; }
    }
  `;

  // Decorative captions above each page's title (not part of the game's data).
  const KICKERS = {
    main: 'Colony command',
    pause: 'Session suspended',
    new: 'Mission setup',
    load: 'Colony archives',
    settings: 'Configuration',
    multiplayer: 'Shared universe',
    create: 'New shared world',
    host: 'Open a world to friends',
  };

  let version = 0;
  let page = null;
  let layer = null;
  let root = null;
  // Per-render state used to animate changes between re-renders of the same page.
  let prevToggles = new Map();
  let prevFracs = new Map();
  let prevNotices = new Set();
  let nextToggles = new Map();
  let nextFracs = new Map();
  let nextNotices = new Set();

  function send(msg) {
    try { window.ipc.postMessage(JSON.stringify(msg)); } catch (e) { /* not in the game */ }
  }

  function css(c) {
    return c ? `rgba(${c[0]},${c[1]},${c[2]},${c[3]})` : 'inherit';
  }

  // Colour roles, matched loosely so a tweak to the game's palette keeps the styling.
  function isAccent(c) {
    return !!c && c[2] > 225 && c[1] > 150 && c[0] < 170;
  }

  function isWarm(c) {
    return !!c && c[0] > 225 && c[1] > 160 && c[1] < 230 && c[2] < 150;
  }

  function isGrey(c) {
    if (!c) return false;
    const hi = Math.max(c[0], c[1], c[2]);
    const lo = Math.min(c[0], c[1], c[2]);
    return hi - lo < 70 && hi > 110 && hi < 210;
  }

  function clamp01(f) {
    return Math.max(0, Math.min(1, Number(f) || 0));
  }

  function h(tag, cls, kids) {
    const e = document.createElement(tag);
    if (cls) e.className = cls;
    (kids || []).forEach((k) => k && e.appendChild(k));
    return e;
  }

  function span(s, cls) {
    const e = document.createElement('span');
    if (cls) e.className = cls;
    e.textContent = s;
    return e;
  }

  // Plain text with the game's colour and size; recognised roles get their own styling.
  function text(s, color, size) {
    const e = document.createElement('div');
    e.className = 't';
    e.textContent = s;
    const z = Number(size) || 0;
    if (z >= 26) {
      e.className = 't title';
      return h('div', 'title-wrap', [kicker(), e]);
    }
    if (isHead({ k: 'text', c: color, s: z })) {
      e.className = 't head';
      e.style.color = css(color);
      return e;
    }
    if (isWarm(color)) {
      e.className = 't notice';
      e.style.color = css(color);
      if (!prevNotices.has(s)) e.classList.add('fresh');
      nextNotices.add(s);
      return e;
    }
    if (isGrey(color) && /^v\d/.test(s)) {
      e.className = 't ver';
      return e;
    }
    if (isGrey(color) && z <= 12.5) {
      e.className = 't dim';
      return e;
    }
    if (color) e.style.color = css(color);
    if (z) e.style.fontSize = z + 'px';
    return e;
  }

  function kicker() {
    const k = h('div', 'kicker');
    k.textContent = KICKERS[page] || 'Beyond the stars';
    k.setAttribute('aria-hidden', 'true');
    return k;
  }

  function button(label, action, style, extra) {
    const b = document.createElement('button');
    const st = style || 'normal';
    b.className = 'btn ' + st + (extra ? ' ' + extra : '');
    b.textContent = label;
    b.dataset.key = 'btn:' + label;
    wire(b, action, st);
    return b;
  }

  function wire(b, action, style) {
    b.type = 'button';
    if (style === 'disabled' || action === null || action === undefined) {
      b.setAttribute('aria-disabled', 'true');
      b.tabIndex = -1;
      return;
    }
    b.addEventListener('click', () => send({ t: 'act', i: action, v: version }));
  }

  // Children get an index for the staggered entrance.
  function stagger(e) {
    Array.prototype.forEach.call(e.children, (c, i) => c.style.setProperty('--i', String(Math.min(i, 24))));
    return e;
  }

  function isHead(n) {
    return !!n && n.k === 'text' && isAccent(n.c) && (Number(n.s) || 0) >= 13.5 && (Number(n.s) || 0) <= 15;
  }

  function many(list) {
    const l = list || [];
    // A divider right before a section heading would double the heading's own rule.
    return l.map((n, i) => {
      if (n && n.k === 'div' && isHead(l[i + 1])) return h('div', 'gap');
      if (isDim(n) && hasField(l[i + 1])) return fieldLabel(n.t);
      return node(n);
    });
  }

  function isDim(n) {
    return !!n && n.k === 'text' && isGrey(n.c) && (Number(n.s) || 0) <= 12.5;
  }

  function hasField(n) {
    if (!n) return false;
    if (n.k === 'field') return true;
    return ['row', 'fixed', 'col'].indexOf(n.k) >= 0 && (n.c || []).some(hasField);
  }

  // "World seed (a number or any word; empty = random)" -> caption + softer hint.
  function fieldLabel(s) {
    const e = h('div', 'label');
    const m = /^(.*?)\s*(\(.*\))\s*$/.exec(String(s));
    if (m && m[1]) {
      e.appendChild(document.createTextNode(m[1]));
      e.appendChild(span(m[2], 'hint'));
    } else {
      e.textContent = s;
    }
    return e;
  }

  // The main menu's first two texts are the game title and its tagline.
  function hero(nodes) {
    const list = nodes.slice();
    const first = list[0];
    if (!first || first.k !== 'text' || (Number(first.s) || 0) < 30) return many(list);
    list.shift();
    const words = String(first.t).trim().split(/\s+/);
    const mark = h('div', 'wordmark');
    mark.setAttribute('aria-label', first.t);
    if (words.length > 1) {
      mark.appendChild(document.createTextNode(words.slice(0, -1).join(' ')));
      const last = document.createElement('b');
      last.textContent = words[words.length - 1];
      mark.appendChild(last);
    } else {
      mark.textContent = first.t;
    }
    const kids = [kicker(), mark];
    const second = list[0];
    if (second && second.k === 'text' && (Number(second.s) || 0) < 20) {
      list.shift();
      const tag = h('div', 'tagline');
      tag.textContent = second.t;
      kids.push(tag);
    }
    return [h('div', 'hero', kids)].concat(many(list));
  }

  function cardBody(items) {
    return (items || []).map((n) => {
      if (n.k === 'text' && isGrey(n.c) && String(n.t).indexOf(' | ') >= 0) {
        return h('div', 'meta', String(n.t).split(' | ').map((p) => span(p)));
      }
      if (n.k === 'spread' && n.c && n.c[0] && n.c[0].k === 'text') {
        const t = text(n.c[0].t, n.c[0].c);
        t.classList.add('cardtitle');
        return h('div', 'spread', [t].concat(many(n.c.slice(1))));
      }
      return node(n);
    });
  }

  function node(n) {
    if (!n) return null;
    switch (n.k) {
      case 'text': return text(n.t, n.c, n.s);
      case 'btn': return button(n.t, n.a, n.st);
      case 'row': return h('div', 'row', many(n.c));
      case 'wrap': return h('div', 'wrap', many(n.c));
      case 'col': return h('div', 'col', many(n.c));
      case 'spread': return h('div', 'spread', many(n.c));
      case 'columns': return h('div', 'columns', (n.c || []).map((c) => h('div', '', many(c))));
      case 'fixed': { const e = h('div', 'fixed', many(n.c)); e.style.width = n.w + 'px'; return e; }
      case 'gap': return h('div', 'gap');
      case 'div': return h('div', 'div');
      case 'spacer': return h('div', 'spacer');
      case 'dot': {
        const e = h('div', 'dot');
        e.style.background = css(n.c);
        e.style.color = css(n.c);
        if (n.s) { e.style.width = e.style.height = n.s + 'px'; }
        return e;
      }
      case 'bar': {
        const i = h('i');
        i.style.width = clamp01(n.f) * 100 + '%';
        i.style.background = css(n.c);
        i.style.color = css(n.c);
        return h('div', 'bar', [i]);
      }
      case 'card': {
        const e = h('div', 'card', cardBody(n.c));
        const ac = n.ac || [115, 199, 255, 1];
        e.style.setProperty('--ac', css(ac.map((v, i) => (i === 3 ? 0.9 : v))));
        e.style.borderColor = css(ac.map((v, i) => (i === 3 ? 0.22 : v)));
        e.style.background = `linear-gradient(100deg, ${css(ac.map((v, i) => (i === 3 ? 0.1 : v)))}, ${css(ac.map((v, i) => (i === 3 ? 0.03 : v)))}), rgba(8, 14, 28, .55)`;
        return e;
      }
      case 'panel': {
        const e = h('div', 'panel', [stagger(h('div', 'scroll', many(n.c)))]);
        e.style.width = n.w + 'px';
        return e;
      }
      case 'overlay':
        return n.left
          ? h('div', 'overlay left', [h('div', 'rail', [stagger(h('div', 'stack', hero(n.c || [])))])])
          : h('div', 'overlay center', many(n.c));
      case 'big': {
        const b = document.createElement('button');
        const st = n.st || 'normal';
        b.className = 'big ' + st;
        b.dataset.key = 'big:' + n.t;
        b.appendChild(span(n.t, 'lbl'));
        if (n.note) {
          // Short notes like "Esc" or "F5" are key hints and get a keycap.
          const isKey = /^(Esc|F\d{1,2}|Enter|Tab|Space)$/.test(n.note);
          b.appendChild(span(n.note, isKey ? 'note key' : 'note'));
        }
        const chev = h('i', 'chev');
        chev.setAttribute('aria-hidden', 'true');
        b.appendChild(chev);
        wire(b, n.a, st);
        return b;
      }
      case 'field': {
        const i = document.createElement('input');
        i.className = 'field';
        i.type = 'text';
        i.value = n.v;
        i.placeholder = n.hint || '';
        i.dataset.field = n.id;
        i.autocomplete = 'off';
        i.spellcheck = false;
        i.setAttribute('aria-label', n.hint || n.id);
        i.addEventListener('input', () => send({ t: 'field', id: n.id, v: i.value }));
        i.addEventListener('keydown', (ev) => {
          if (ev.key === 'Enter' || ev.key === 'Escape') { ev.stopPropagation(); i.blur(); }
        });
        return i;
      }
      case 'toggle': {
        // The whole row is the switch: a big target, and the label reads with the state.
        const b = document.createElement('button');
        b.className = 'setting toggle';
        b.dataset.key = 'toggle:' + n.label;
        b.setAttribute('role', 'switch');
        b.setAttribute('aria-checked', n.on ? 'true' : 'false');
        const sw = h('span', 'switch' + (n.on ? ' on' : ''));
        const was = prevToggles.get(n.label);
        if (was !== undefined && was !== !!n.on) sw.classList.add('flip');
        nextToggles.set(n.label, !!n.on);
        b.appendChild(span(n.label, 'name'));
        b.appendChild(sw);
        wire(b, n.a, 'normal');
        return b;
      }
      case 'stepper': {
        const dec = document.createElement('button');
        dec.className = 'step minus';
        dec.dataset.key = 'dec:' + n.label;
        dec.setAttribute('aria-label', 'Decrease ' + n.label);
        wire(dec, n.dec, 'normal');
        const inc = document.createElement('button');
        inc.className = 'step plus';
        inc.dataset.key = 'inc:' + n.label;
        inc.setAttribute('aria-label', 'Increase ' + n.label);
        wire(inc, n.inc, 'normal');
        const ctl = [dec];
        if (n.frac !== null && n.frac !== undefined) {
          const f = clamp01(n.frac);
          const i = h('i');
          const was = prevFracs.get(n.label);
          i.style.width = (was !== undefined ? was : f) * 100 + '%';
          if (was !== undefined && was !== f) {
            // Slide from the old value once the new node is in the document.
            requestAnimationFrame(() => requestAnimationFrame(() => { i.style.width = f * 100 + '%'; }));
          }
          nextFracs.set(n.label, f);
          ctl.push(h('div', 'track', [i]));
        }
        const v = span(n.value, 'val');
        ctl.push(v, inc);
        return h('div', 'setting stepper', [span(n.label, 'name'), h('div', 'ctl', ctl)]);
      }
      default: return null;
    }
  }

  // Large windows get a slightly larger menu, small ones a smaller one.
  function fit() {
    if (!layer) return;
    const s = Math.max(0.72, Math.min(1.45, Math.min(window.innerWidth / 1500, window.innerHeight / 930)));
    layer.style.zoom = String(Math.round(s * 100) / 100);
  }

  function focusables() {
    return Array.prototype.filter.call(
      root.querySelectorAll('button, input'),
      (e) => e.getAttribute('aria-disabled') !== 'true' && e.offsetParent !== null,
    );
  }

  function render(state) {
    version = state.v;
    const changed = state.page !== page;
    const previous = page;
    page = state.page;
    // Keep keyboard focus (and the caret in fields) across re-renders.
    const active = root.activeElement;
    const keep = active && active.dataset
      ? { f: active.dataset.field, k: active.dataset.key, v: active.value, s: active.selectionStart, e: active.selectionEnd }
      : null;
    const scroller = root.querySelector('.scroll');
    const scrollTop = scroller ? scroller.scrollTop : 0;
    if (changed) {
      prevToggles = new Map();
      prevFracs = new Map();
      prevNotices = new Set();
    }
    nextToggles = new Map();
    nextFracs = new Map();
    nextNotices = new Set();
    layer.classList.toggle('enter', changed);
    layer.replaceChildren(...many(state.nodes));
    prevToggles = nextToggles;
    prevFracs = nextFracs;
    prevNotices = nextNotices;
    if (!changed) {
      const again = root.querySelector('.scroll');
      if (again) again.scrollTop = scrollTop;
    }
    if (keep && keep.f) {
      const again = root.querySelector(`input[data-field="${keep.f}"]`);
      if (again) { again.value = keep.v; again.focus(); try { again.setSelectionRange(keep.s, keep.e); } catch (e) { /* ignore */ } }
    } else if (keep && keep.k && !changed) {
      const again = focusables().find((e) => e.dataset.key === keep.k);
      if (again) again.focus({ preventScroll: true });
    }
    if (changed) {
      window.dispatchEvent(new CustomEvent('bts-menu-page', { detail: { page, previous } }));
    }
  }

  // Arrow keys walk the buttons, so the menu works without a mouse.
  function arrows(ev) {
    if (ev.key !== 'ArrowDown' && ev.key !== 'ArrowUp') return;
    const active = root.activeElement;
    if (active && active.tagName === 'INPUT') return;
    const list = focusables();
    if (!list.length) return;
    const at = list.indexOf(active);
    const step = ev.key === 'ArrowDown' ? 1 : -1;
    const next = at < 0 ? (step > 0 ? 0 : list.length - 1) : (at + step + list.length) % list.length;
    list[next].focus();
    ev.preventDefault();
  }

  function mount() {
    const host = document.createElement('div');
    host.id = 'bts-menu-host';
    host.style.cssText = 'position:fixed;inset:0;z-index:2147483647;pointer-events:none;';
    root = host.attachShadow({ mode: 'open' });
    try {
      const sheet = new CSSStyleSheet();
      sheet.replaceSync(CSS);
      root.adoptedStyleSheets = [sheet];
    } catch (e) {
      const s = document.createElement('style');
      s.textContent = CSS;
      root.appendChild(s);
    }
    layer = h('div', 'layer');
    root.appendChild(layer);
    (document.body || document.documentElement).appendChild(host);
    fit();
    window.addEventListener('resize', fit);
    window.__bts_set = render;
    window.addEventListener('keydown', (ev) => {
      if (ev.key === 'Escape') send({ t: 'back' });
      else arrows(ev);
    });
    send({ t: 'ready' });
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', mount);
  } else {
    mount();
  }
})();
