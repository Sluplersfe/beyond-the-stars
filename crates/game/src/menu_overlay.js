// Menu overlay injected over the main-menu background page.
// The game sends the menu as JSON (window.__bts_set); clicks and edits go back through
// window.ipc.postMessage. Everything lives in a shadow root so the background page's
// CSS cannot restyle it and ours cannot leak out.
(function () {
  if (window.__bts_menu_loaded) return;
  window.__bts_menu_loaded = true;

  const CSS = `
    :host { all: initial; }
    * { box-sizing: border-box; }
    .layer {
      position: fixed; inset: 0; pointer-events: none;
      font-family: ui-monospace, "JetBrains Mono", "Fira Mono", "DejaVu Sans Mono", Menlo, Consolas, monospace;
      color: #dbe5f5; font-size: 13px;
    }
    .layer.hidden { display: none; }
    .overlay { position: absolute; inset: 0; display: flex; align-items: center; }
    .overlay.center { justify-content: center; background: rgba(3, 5, 13, 0.64); pointer-events: auto;
      animation: fade .22s ease-out; }
    .overlay.left { justify-content: flex-start; padding-left: 7%; }
    .overlay.left > .stack {
      width: 430px; max-width: 92vw; display: flex; flex-direction: column; gap: 9px; padding: 30px;
      border-radius: 14px; background: rgba(3, 8, 18, 0.6); pointer-events: auto;
      backdrop-filter: blur(6px); animation: rise .35s ease-out;
    }
    .panel {
      max-width: 94vw; max-height: 94vh; overflow-y: auto; display: flex; flex-direction: column; gap: 7px;
      padding: 24px; border-radius: 14px; border: 1px solid rgba(64, 128, 217, .55);
      background: rgba(5, 11, 23, .97); animation: rise .25s ease-out;
    }
    .row { display: flex; align-items: center; gap: 6px; }
    .wrap { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; }
    .col { display: flex; flex-direction: column; gap: 1px; }
    .spread { display: flex; align-items: center; justify-content: space-between; gap: 6px; width: 100%; }
    .columns { display: flex; gap: 10px; }
    .columns > div { flex: 1 1 0; display: flex; flex-direction: column; gap: 2px; }
    .fixed { flex: none; display: flex; flex-direction: column; gap: 2px; }
    .spacer { flex: 1; }
    .gap { height: 6px; flex: none; }
    .div { height: 1px; width: 100%; margin: 3px 0; background: rgba(102, 153, 230, .25); }
    .dot { width: 8px; height: 8px; border-radius: 50%; flex: none; }
    .card { display: flex; flex-direction: column; gap: 4px; padding: 10px; border-radius: 6px; border: 1px solid; }
    .bar { width: 100%; height: 6px; border-radius: 3px; background: rgba(255,255,255,.1); overflow: hidden; }
    .bar > i { display: block; height: 100%; border-radius: 3px; }
    button {
      font: inherit; color: inherit; cursor: pointer; border-radius: 5px; padding: 3px 9px;
      border: 1px solid rgba(64, 115, 191, .6); background: rgba(18, 36, 64, .95);
    }
    button:hover { background: rgba(38, 66, 106, .98); border-color: rgba(115, 199, 255, .85); }
    button.active { background: rgba(26, 84, 158, .95); color: #fff; border-color: #73c7ff; }
    button.primary { background: rgba(15, 51, 102, .95); color: #fff; border-color: #66b3ff; }
    button.danger { background: rgba(90, 20, 20, .95); color: #fff; border-color: #d94d4d; }
    button.flat { background: transparent; border-color: transparent; }
    button.disabled { cursor: default; color: #667; background: rgba(13, 18, 26, .8); border-color: rgba(51, 56, 66, .6); }
    button.big {
      width: 100%; display: flex; align-items: center; justify-content: space-between; gap: 12px;
      padding: 12px 18px; border-radius: 8px; font-size: 20px; text-align: left;
      transition: transform .08s ease, background .12s ease;
    }
    button.big:hover { transform: translateX(3px); }
    button.big .note { font-size: 12.5px; color: #8c9db8; }
    input.field {
      width: 100%; font: inherit; font-size: 15px; color: #fff; padding: 8px 10px; border-radius: 6px;
      background: rgba(8, 18, 33, 1); border: 1px solid rgba(64, 115, 191, .6); outline: none;
    }
    input.field:focus { border-color: #73c7ff; background: rgba(13, 31, 56, 1); }
    input.field::placeholder { color: #7587a0; }
    .toggle { display: flex; align-items: center; justify-content: space-between; width: 100%; gap: 8px; font-size: 14px; }
    .toggle button { min-width: 44px; font-size: 12px; }
    .stepper { display: flex; align-items: center; justify-content: space-between; width: 100%; gap: 8px; font-size: 14px; }
    .stepper .ctl { display: flex; align-items: center; gap: 6px; }
    .stepper .val { width: 64px; color: #fff; }
    .stepper .bar { width: 110px; }
    @keyframes fade { from { opacity: 0; } to { opacity: 1; } }
    @keyframes rise { from { opacity: 0; transform: translateY(8px); } to { opacity: 1; transform: none; } }
  `;

  let version = 0;
  let page = null;
  let layer = null;
  let root = null;

  function send(msg) {
    try { window.ipc.postMessage(JSON.stringify(msg)); } catch (e) { /* not in the game */ }
  }

  function css(c) {
    return c ? `rgba(${c[0]},${c[1]},${c[2]},${c[3]})` : 'inherit';
  }

  function h(tag, cls, kids) {
    const e = document.createElement(tag);
    if (cls) e.className = cls;
    (kids || []).forEach((k) => k && e.appendChild(k));
    return e;
  }

  function text(s, color, size) {
    const e = document.createElement('div');
    e.textContent = s;
    if (color) e.style.color = css(color);
    if (size) e.style.fontSize = size + 'px';
    return e;
  }

  function button(label, action, style, extra) {
    const b = document.createElement('button');
    b.className = (style || 'normal') + (extra ? ' ' + extra : '');
    b.textContent = label;
    wire(b, action, style);
    return b;
  }

  function wire(b, action, style) {
    if (style === 'disabled' || action === null || action === undefined) return;
    b.addEventListener('click', () => send({ t: 'act', i: action, v: version }));
  }

  function many(list) {
    return (list || []).map(node);
  }

  function node(n) {
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
      case 'dot': { const e = h('div', 'dot'); e.style.background = css(n.c); if (n.s) { e.style.width = e.style.height = n.s + 'px'; } return e; }
      case 'bar': { const i = h('i'); i.style.width = Math.max(0, Math.min(1, n.f)) * 100 + '%'; i.style.background = css(n.c); return h('div', 'bar', [i]); }
      case 'card': {
        const e = h('div', 'card', many(n.c));
        e.style.borderColor = css(n.ac.map((v, i) => (i === 3 ? 0.6 : v)));
        e.style.background = css(n.ac.map((v, i) => (i === 3 ? 0.08 : v)));
        return e;
      }
      case 'panel': { const e = h('div', 'panel', many(n.c)); e.style.width = n.w + 'px'; return e; }
      case 'overlay':
        return n.left
          ? h('div', 'overlay left', [h('div', 'stack', many(n.c))])
          : h('div', 'overlay center', many(n.c));
      case 'big': {
        const b = document.createElement('button');
        b.className = 'big ' + (n.st || 'normal');
        b.appendChild(text(n.t));
        if (n.note) { const s = text(n.note); s.className = 'note'; b.appendChild(s); }
        wire(b, n.a, n.st);
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
        i.addEventListener('input', () => send({ t: 'field', id: n.id, v: i.value }));
        i.addEventListener('keydown', (ev) => {
          if (ev.key === 'Enter' || ev.key === 'Escape') { ev.stopPropagation(); i.blur(); }
        });
        return i;
      }
      case 'toggle': {
        const b = button(n.on ? 'ON' : 'OFF', n.a, n.on ? 'active' : 'normal');
        return h('div', 'toggle', [text(n.label), b]);
      }
      case 'stepper': {
        const ctl = [button('-', n.dec, 'normal')];
        if (n.frac !== null && n.frac !== undefined) {
          const i = h('i'); i.style.width = Math.max(0, Math.min(1, n.frac)) * 100 + '%'; i.style.background = 'rgb(115,199,255)';
          ctl.push(h('div', 'bar', [i]));
        }
        const v = text(n.value); v.className = 'val';
        ctl.push(v, button('+', n.inc, 'normal'));
        return h('div', 'stepper', [text(n.label), h('div', 'ctl', ctl)]);
      }
      default: return null;
    }
  }

  function render(state) {
    version = state.v;
    // Keep typing focus across re-renders.
    const active = root.activeElement;
    const keep = active && active.dataset ? { f: active.dataset.field, v: active.value, s: active.selectionStart, e: active.selectionEnd } : null;
    layer.replaceChildren(...many(state.nodes));
    if (keep && keep.f) {
      const again = root.querySelector(`input[data-field="${keep.f}"]`);
      if (again) { again.value = keep.v; again.focus(); try { again.setSelectionRange(keep.s, keep.e); } catch (e) { /* ignore */ } }
    }
    if (state.page !== page) {
      const previous = page;
      page = state.page;
      window.dispatchEvent(new CustomEvent('bts-menu-page', { detail: { page, previous } }));
    }
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
    window.__bts_set = render;
    window.addEventListener('keydown', (ev) => {
      if (ev.key === 'Escape') send({ t: 'back' });
    });
    send({ t: 'ready' });
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', mount);
  } else {
    mount();
  }
})();
