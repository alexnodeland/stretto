// The explainer's stage: easing, a seeded generator, DOM helpers, and the
// pieces every chapter draws with (cards, chips, arrows, code, captions).
//
// A frame is a pure function of t: nothing animates by itself (no CSS
// transitions, no timers), so render.mjs can draw frames in any order, on
// several pages at once, and get the same film. The look follows PACING.md:
// a flat, still frame, one diagram a chapter, labels in a monospace face,
// and the line set at the foot of the frame with each word lit as it is said.
(() => {
  const C = {
    bg: '#0b0f11', surface: '#121719', surface2: '#191f21', border: '#272f32', borderStrong: '#3b4649',
    text: '#eceff0', muted: '#b3bcbf', subtle: '#909a9d', faint: '#606b6f', dim: '#384346',
    petrol: '#5dd2d8', petrolG: '#33c0c7', petrolDeep: '#1d6f74', petrolSoft: 'rgba(51,192,199,0.10)', petrolLine: 'rgba(93,210,216,0.55)',
    amber: '#d9a35b', amberSoft: 'rgba(217,163,91,0.10)', amberLine: 'rgba(217,163,91,0.55)',
  };

  const clamp = (x, a = 0, b = 1) => Math.min(b, Math.max(a, x));
  const lerp = (a, b, u) => a + (b - a) * u;
  const E = {
    lin: u => u,
    out2: u => 1 - (1 - u) * (1 - u),
    io2: u => (u < 0.5 ? 2 * u * u : 1 - 2 * (1 - u) * (1 - u)),
    out3: u => 1 - Math.pow(1 - u, 3),
    io3: u => (u < 0.5 ? 4 * u * u * u : 1 - Math.pow(-2 * u + 2, 3) / 2),
    out4: u => 1 - Math.pow(1 - u, 4),
    out5: u => 1 - Math.pow(1 - u, 5),
    outBack: (u, s = 1.3) => 1 + (s + 1) * Math.pow(u - 1, 3) + s * Math.pow(u - 1, 2),
  };
  /** 0 → 1 as t goes t0 → t1, eased. */
  const ramp = (t, t0, t1, ease = E.io3) => (t1 <= t0 ? (t >= t1 ? 1 : 0) : ease(clamp((t - t0) / (t1 - t0))));
  /** Up over [a, b], down over [c, d]. */
  const fade = (t, a, b, c = Infinity, d = Infinity, ease = E.io2) => ramp(t, a, b, ease) * (1 - ramp(t, c, d, ease));

  /** A seeded generator (mulberry32): every scatter is the same on every render. */
  function rng(seed = 1) {
    let a = seed >>> 0;
    return () => {
      a = (a + 0x6d2b79f5) >>> 0;
      let t = a;
      t = Math.imul(t ^ (t >>> 15), t | 1);
      t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
      return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
    };
  }

  const SVGNS = 'http://www.w3.org/2000/svg';
  const SVG_TAGS = /^(svg|g|path|circle|rect|line|polyline|polygon|text|tspan|defs|linearGradient|stop|clipPath|mask|marker)$/;
  function el(tag, attrs = {}, parent = null, text = null) {
    const n = SVG_TAGS.test(tag) ? document.createElementNS(SVGNS, tag) : document.createElement(tag);
    for (const [k, v] of Object.entries(attrs)) {
      if (v == null) continue;
      if (k === 'style' && typeof v === 'object') Object.assign(n.style, v);
      else if (k === 'html') n.innerHTML = v;
      else n.setAttribute(k, v);
    }
    if (text != null) n.textContent = text;
    if (parent) parent.appendChild(n);
    return n;
  }
  /** An absolutely placed HTML box, in frame pixels; ax, ay anchor it (0.5 centres it). */
  function box(parent, { x = 0, y = 0, w, h, ax = 0, ay = 0, cls = '', html, text, style = {} } = {}) {
    const n = el('div', { class: cls, html }, parent, text ?? null);
    Object.assign(n.style, { position: 'absolute', left: `${x}px`, top: `${y}px`, ...style });
    if (w != null) n.style.width = `${w}px`;
    if (h != null) n.style.height = `${h}px`;
    n._ax = ax; n._ay = ay;
    n.style.translate = ax || ay ? `${-ax * 100}% ${-ay * 100}%` : '';
    return n;
  }
  /** Set a node's entrance: fades in and rises dy pixels over d seconds from `at`; out at `out`. */
  function show(n, t, at, { d = 0.5, dy = 12, dx = 0, out = Infinity, od = 0.35, scale = 0 } = {}) {
    const u = ramp(t, at, at + d, E.out3);
    const o = u * (1 - ramp(t, out, out + od, E.io2));
    n.style.opacity = o.toFixed(4);
    const tf = [];
    if (dx || dy) tf.push(`translate(${((1 - u) * dx).toFixed(2)}px, ${((1 - u) * dy).toFixed(2)}px)`);
    if (scale) tf.push(`scale(${(1 - scale * (1 - u)).toFixed(4)})`);
    n.style.transform = tf.join(' ');
    n.style.visibility = o < 0.002 ? 'hidden' : '';
    return o;
  }
  const esc = s => String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');

  // ------------------------------------------------------------------ pieces
  /** A card: a raised surface with an optional mono header strip. */
  function card(parent, { x, y, w, h, title = null, tag = null, accent = null, ax = 0, ay = 0 }) {
    const n = box(parent, { x, y, w, h, ax, ay, cls: `card${accent ? ' ' + accent : ''}` });
    if (title != null) {
      const hd = el('div', { class: 'card-h' }, n);
      el('span', { class: 'card-t', html: title }, hd);
      if (tag) el('span', { class: 'card-tag', html: tag }, hd);
    }
    const body = el('div', { class: 'card-b' }, n);
    n.body = body;
    return n;
  }
  /** A tool call or a label in a pill: kind is call, read, write, ghost or plain. */
  function chip(parent, text, { x, y, kind = 'call', ax = 0, ay = 0, size = 22, html = false } = {}) {
    const n = box(parent, { x, y, ax, ay, cls: `chip ${kind}` });
    if (html) n.innerHTML = text; else n.textContent = text;
    n.style.fontSize = `${size}px`;
    return n;
  }
  /** A path drawn on as u goes 0 → 1, with an arrowhead once it lands. */
  function arrow(svg, d, { color = C.faint, width = 2, head = true, dash = null } = {}) {
    const g = el('g', {}, svg);
    const p = el('path', { d, fill: 'none', stroke: color, 'stroke-width': width, 'stroke-linecap': 'round', 'stroke-linejoin': 'round' }, g);
    const len = p.getTotalLength();
    let hd = null;
    if (head) {
      const a = p.getPointAtLength(len), b = p.getPointAtLength(Math.max(0, len - 6));
      const ang = (Math.atan2(a.y - b.y, a.x - b.x) * 180) / Math.PI;
      hd = el('path', { d: 'M0 0 L-11 -5.5 L-11 5.5 Z', fill: color, transform: `translate(${a.x} ${a.y}) rotate(${ang})` }, g);
    }
    return {
      g, path: p, len,
      set(u, o = 1) {
        u = clamp(u);
        if (dash) {
          p.setAttribute('stroke-dasharray', dash);
          g.style.opacity = (u > 0 ? o * clamp(u * 3) : 0).toFixed(3);
          p.style.clipPath = '';
        } else {
          p.setAttribute('stroke-dasharray', `${len} ${len}`);
          p.setAttribute('stroke-dashoffset', (len * (1 - u)).toFixed(2));
          g.style.opacity = u > 0 ? o : 0;
        }
        if (hd) hd.setAttribute('opacity', u > 0.97 ? 1 : 0);
      },
      at(u) { return p.getPointAtLength(len * clamp(u)); },
    };
  }
  /** A dot that travels a path: position u along it. */
  function packet(svg, { r = 7, color = C.petrolG } = {}) {
    const g = el('g', {}, svg);
    const halo = el('circle', { r: r * 2.2, fill: color, opacity: 0.16 }, g);
    const c = el('circle', { r, fill: color }, g);
    return {
      set(pt, o = 1) {
        g.setAttribute('transform', `translate(${pt.x.toFixed(2)} ${pt.y.toFixed(2)})`);
        g.style.opacity = o.toFixed(3);
        void halo; void c;
      },
    };
  }
  /** Code: lines of HTML in the mono face. Returns the line nodes. */
  function code(parent, lines, { x, y, size = 24, lh = 1.6, w = null, cls = '' } = {}) {
    const n = box(parent, { x, y, w, cls: `code ${cls}` });
    n.style.fontSize = `${size}px`;
    n.style.lineHeight = String(lh);
    const rows = lines.map(l => el('div', { class: 'row', html: l || '&nbsp;' }, n));
    n.rows = rows;
    return n;
  }
  /** Type `text` into node n as t passes [t0, t0 + text.length / cps], with a caret. */
  function type(n, text, t, t0, cps = 38, caret = true) {
    const k = Math.max(0, Math.min(text.length, Math.floor((t - t0) * cps)));
    const done = k >= text.length;
    const blink = caret && (!done || Math.floor((t - t0) * 2) % 2 === 0) && t >= t0 - 0.4;
    n.innerHTML = esc(text.slice(0, k)) + (blink && (!done || t - (t0 + text.length / cps) < 1.2) ? '<span class="caret"></span>' : '');
    return done;
  }

  // ---------------------------------------------------------------- captions
  /**
   * The line at the foot of the frame: word spans, each lit as it is spoken.
   * `em` phrases are lit in petrol. `times` are the words' start times.
   */
  function caption(parent, text, em = []) {
    const n = box(parent, { x: 960, y: 978, w: 1560, ax: 0.5, ay: 0.5, cls: 'cap' });
    const words = text.split(/\s+/).filter(Boolean);
    const norm = s => s.toLowerCase().replace(/[^a-z0-9.%]/g, '');
    const lit = new Array(words.length).fill(false);
    for (const phrase of em) {
      const ps = phrase.split(/\s+/).map(norm);
      for (let i = 0; i + ps.length <= words.length; i++) {
        if (ps.every((p, j) => norm(words[i + j]) === p)) for (let j = 0; j < ps.length; j++) lit[i + j] = true;
      }
    }
    const spans = words.map((w, i) => {
      const s = el('span', { class: lit[i] ? 'w em' : 'w' }, n, w);
      if (i < words.length - 1) n.appendChild(document.createTextNode(' '));
      return s;
    });
    return { n, spans };
  }
  function lightCaption(cap, t, times, t0, out) {
    // In over [t0 - 0.3, t0], out over the quarter second before `out`.
    cap.n.style.opacity = fade(t, t0 - 0.3, t0 + 0.02, out - 0.25, out).toFixed(4);
    cap.n.style.visibility = t < t0 - 0.3 || t > out ? 'hidden' : '';
    cap.spans.forEach((s, i) => {
      const u = ramp(t, times[i] - 0.06, times[i] + 0.22, E.out3);
      s.style.setProperty('--u', u.toFixed(3));
    });
  }

  window.ST = { C, clamp, lerp, E, ramp, fade, rng, el, box, show, esc, card, chip, arrow, packet, code, type, caption, lightCaption };
})();
