// The explainer's illustrations, drawn in SVG: flat and geometric, one stroke
// weight, one corner family, the brand's dark palette with petrol for what
// stretto does and a warm amber for writes, locks and cost. Each builder
// returns its root group and the parts the scenes animate.
(() => {
  const { s } = E;

  const C = {
    bg: '#0b0f11', surface: '#121719', surface2: '#191f21', border: '#272f32', borderStrong: '#606b6f',
    text: '#eceff0', muted: '#b3bcbf', subtle: '#909a9d', faint: '#606b6f',
    petrol: '#33c0c7', petrolText: '#5dd2d8', soft: '#0b292c', paper: '#f8fbfb',
    warm: '#e8a33d', warmSoft: '#2a1e0d', warmEdge: '#6b4d1f',
    base: '#606b6f', chip: '#172023', chipEdge: '#2b3538', line: '#2d383b', panel: '#141b1d',
  };
  const SANS = 'Inter', MONO = 'JetBrains Mono';
  const MONO_W = 0.6; // JetBrains Mono's advance, in ems

  // A line of text. o: x, y, size, mono, weight, fill, anchor, ls (letter
  // spacing, em), caps, opsz, cls.
  function T(str, o = {}) {
    const size = o.size || 20;
    const el = s('text', {
      x: o.x || 0, y: o.y || 0, fill: o.fill || C.text, 'text-anchor': o.anchor || 'start',
      'font-family': o.mono ? MONO : SANS, 'font-size': size, 'font-weight': o.weight || (o.mono ? 400 : 500),
      class: o.cls,
    });
    const st = [];
    if (o.ls) st.push(`letter-spacing:${o.ls}em`);
    if (!o.mono) st.push(`font-variation-settings:"opsz" ${o.opsz || (size >= 40 ? 32 : 20)}`);
    if (o.tnum) st.push('font-feature-settings:"tnum","kern"');
    if (st.length) el.setAttribute('style', st.join(';'));
    el.textContent = o.caps ? str.toUpperCase() : str;
    return el;
  }
  // Mono text with coloured runs: parts is [[text, fill], ...].
  function TM(parts, o = {}) {
    const el = T('', o);
    for (const [str, fill, weight] of parts) {
      const t = s('tspan', { fill: fill || o.fill || C.text });
      if (weight) t.setAttribute('font-weight', weight);
      t.textContent = str;
      el.appendChild(t);
    }
    return el;
  }
  const monoWidth = (str, size) => str.length * size * MONO_W;
  const label = (str, x, y, o = {}) => T(str, { x, y, size: o.size || 30, weight: 620, fill: o.fill || C.subtle, ls: 0.14, caps: true, anchor: o.anchor || 'middle' });

  // ------------------------------------------------------------------ defs
  function defs(d) {
    const stop = (off, col, op = 1) => s('stop', { offset: off, 'stop-color': col, 'stop-opacity': op });
    const radial = (id, stops, a = {}) => s('radialGradient', { id, ...a }, ...stops.map(x => stop(...x)));
    const linear = (id, stops, a = {}) => s('linearGradient', { id, ...a }, ...stops.map(x => stop(...x)));
    E.add(d,
      radial('gHalo', [[0, C.petrol, 0.34], [0.35, C.petrol, 0.12], [1, C.petrol, 0]]),
      radial('gHaloWarm', [[0, C.warm, 0.34], [0.4, C.warm, 0.1], [1, C.warm, 0]]),
      radial('gHaloWhite', [[0, '#dff6f7', 0.22], [1, '#dff6f7', 0]]),
      radial('gShadow', [[0, '#000', 0.55], [1, '#000', 0]]),
      radial('gOrb', [[0, '#26343a'], [0.7, '#151d20'], [1, '#101618']], { cx: 0.38, cy: 0.32, r: 0.78 }),
      radial('gCore', [[0, '#e9fbfc', 0.95], [0.25, '#7fe0e4', 0.8], [0.6, C.petrol, 0.3], [1, C.petrol, 0]]),
      linear('gCab', [[0, '#1c2528'], [1, '#12181a']], { x1: 0, y1: 0, x2: 0, y2: 1 }),
      linear('gCard', [[0, '#1d2629'], [1, '#161d20']], { x1: 0, y1: 0, x2: 0, y2: 1 }),
      linear('gGlass', [[0, '#0f3a3e', 0.92], [0.5, '#0b292c', 0.9], [1, '#0e3336', 0.95]], { x1: 0, y1: 0, x2: 1, y2: 1 }),
      linear('gSheen', [[0, '#fff', 0], [0.5, '#fff', 0.13], [1, '#fff', 0]], { x1: 0, y1: 0, x2: 1, y2: 0 }),
      linear('gFadeR', [[0, C.petrol, 0], [1, C.petrol, 0.9]], { x1: 0, y1: 0, x2: 1, y2: 0 }),
      linear('gBeam', [[0, C.petrol, 0.0], [1, C.petrol, 0.28]], { x1: 0, y1: 0, x2: 1, y2: 0 }),
      linear('gTrailPaper', [[0, C.paper, 0], [1, C.paper, 0.55]], { x1: 0, y1: 0, x2: 1, y2: 0 }),
      linear('gTrailPetrol', [[0, C.petrol, 0], [1, C.petrol, 0.55]], { x1: 0, y1: 0, x2: 1, y2: 0 }),
      linear('gFog', [[0, C.bg, 1], [1, C.bg, 0]], { x1: 0, y1: 0, x2: 0, y2: 1 }),
      s('pattern', { id: 'pHatch', width: 12, height: 12, patternUnits: 'userSpaceOnUse', patternTransform: 'rotate(45)' },
        s('rect', { width: 5, height: 12, fill: '#0b0f11', 'fill-opacity': 0.42 })),
    );
  }

  // Soft contact shadow under an object.
  const shadow = (cx, cy, rx, ry, op = 0.8) => s('ellipse', { cx, cy, rx, ry, fill: 'url(#gShadow)', opacity: op });

  // ---------------------------------------------------------------- agent
  // An abstract model: an orb with two orbits and a glowing core. It thinks
  // with its core (brighter, bigger), its rings (faster) and ripples.
  function agent() {
    const halo = s('circle', { r: 230, fill: 'url(#gHalo)' });
    const body = s('circle', { r: 92, fill: 'url(#gOrb)', stroke: '#33434a', 'stroke-width': 2 });
    const rim = s('path', { d: 'M -70 -48 A 86 86 0 0 1 40 -80', fill: 'none', stroke: '#4b5c62', 'stroke-width': 2.5, 'stroke-linecap': 'round', opacity: 0.8 });
    const inner = s('circle', { r: 66, fill: 'none', stroke: C.petrol, 'stroke-opacity': 0.13, 'stroke-width': 1.5 });
    const ring = rot => {
      const e = s('ellipse', { rx: 76, ry: 25, fill: 'none', stroke: C.petrol, 'stroke-opacity': 0.42, 'stroke-width': 1.8 });
      const dot = s('circle', { r: 4.5, fill: C.petrolText });
      const g = s('g', { transform: `rotate(${rot})` }, e, dot);
      return { g, e, dot, rot };
    };
    const r1 = ring(-30), r2 = ring(30);
    const ripples = [0, 1, 2].map(() => s('circle', { r: 30, fill: 'none', stroke: C.petrol, 'stroke-width': 2, opacity: 0 }));
    const coreGlow = s('circle', { r: 50, fill: 'url(#gCore)' });
    const core = s('circle', { r: 17, fill: C.petrolText });
    const hot = s('circle', { r: 7, fill: '#effdfd' });
    const lab = label('agent', 0, 146);
    const g = s('g', { class: 'agent' }, halo, shadow(0, 120, 96, 15), body, rim, inner, ...ripples, r1.g, r2.g, coreGlow, core, hot, lab);
    // think: 0..1, how hard it is thinking; phase: accumulated thinking, in
    // seconds, which spins the orbits; t: the clock, for idle motion.
    const set = (t, think, phase, glow = 0) => {
      const k = E.clamp(think);
      E.attr(halo, 'opacity', (0.5 + 0.5 * k + 0.4 * glow).toFixed(3));
      E.attr(halo, 'r', (230 + 40 * k).toFixed(1));
      // In a loop (E.LOOP seconds), every idle motion turns a whole number of times.
      const LP = E.LOOP ? (2 * Math.PI) / E.LOOP : 0;
      const a = E.LOOP ? t * LP * 2 : t * 0.9 + phase * 5.2;
      for (const [r, off] of [[r1, 0], [r2, Math.PI * 0.8]]) {
        const ang = a * (off ? 1.12 : 1) + off;
        E.attr(r.dot, 'cx', (76 * Math.cos(ang)).toFixed(2));
        E.attr(r.dot, 'cy', (25 * Math.sin(ang)).toFixed(2));
        const wob = E.LOOP ? 0 : Math.sin(t * 0.35 + off);
        E.attr(r.g, 'transform', `rotate(${(r.rot + wob * 6 + k * 10 * Math.sin(phase * 3 + off)).toFixed(2)})`);
        E.attr(r.e, 'stroke-opacity', (0.36 + 0.4 * k).toFixed(3));
      }
      const pulse = 1 + (E.LOOP ? 0 : 0.06 * Math.sin(t * 2.1)) + 0.28 * k + 0.12 * glow;
      E.attr(core, 'r', (17 * pulse).toFixed(2));
      E.attr(coreGlow, 'r', (50 * (1 + 0.55 * k + 0.3 * glow)).toFixed(2));
      E.attr(hot, 'r', (7 * (1 + 0.35 * k)).toFixed(2));
      ripples.forEach((c, i) => {
        const f = (phase * 1.25 + i / 3) % 1;
        E.attr(c, 'r', (24 + 72 * f).toFixed(1));
        E.attr(c, 'opacity', (k * 0.55 * (1 - f) * Math.min(1, f * 6)).toFixed(3));
      });
    };
    return { g, set, label: lab };
  }

  // -------------------------------------------------------------- customer
  function customer(lines) {
    const frame = s('circle', { r: 68, fill: '#141b1e', stroke: '#33434a', 'stroke-width': 2 });
    const clipId = 'clipCust';
    const clip = s('clipPath', { id: clipId }, s('circle', { r: 66 }));
    const fig = s('g', { 'clip-path': `url(#${clipId})` },
      s('circle', { cx: 0, cy: -16, r: 24, fill: '#c3cbcd' }),
      s('path', { d: 'M -52 70 C -50 26 -28 14 0 14 C 28 14 50 26 52 70 Z', fill: '#c3cbcd' }));
    const lab = label('customer', 0, 124);
    const avatar = s('g', {}, clip, shadow(0, 96, 80, 13), frame, fig);
    const bubble = speech(lines, C.paper, '#0e1416');
    const g = s('g', { class: 'customer' }, avatar, lab, bubble.g);
    return { g, bubble: bubble.g, avatar, label: lab, bubbleSize: bubble };
  }
  // A speech bubble whose tail points down at (0, 0); lines of text inside.
  function speech(lines, fill, ink, size = 28) {
    const w = Math.max(...lines.map(l => l.length)) * size * 0.49 + 40, lh = size * 1.3, h = lines.length * lh + 26;
    const y0 = -h - 16;
    const g = s('g', {},
      s('rect', { x: -w / 2, y: y0, width: w, height: h, rx: 20, fill }),
      s('path', { d: `M -14 ${-18} L 4 0 L 14 ${-18} Z`, fill }),
      ...lines.map((l, i) => T(l, { x: 0, y: y0 + 13 + lh * (i + 0.78), size, weight: 520, fill: ink, anchor: 'middle' })));
    return { g, w, h };
  }

  // ---------------------------------------------------------------- server
  // A cabinet: three read drawers, and below them the write controls behind
  // a glass door and a lock.
  function server() {
    const W = 272, H = 372, x0 = -W / 2, y0 = -H / 2;
    const body = s('rect', { x: x0, y: y0, width: W, height: H, rx: 22, fill: 'url(#gCab)', stroke: '#33434a', 'stroke-width': 2 });
    const top = s('g', {},
      s('line', { x1: x0 + 2, y1: y0 + 46, x2: x0 + W - 2, y2: y0 + 46, stroke: '#2b3539', 'stroke-width': 2 }),
      T('MCP', { x: x0 + W - 22, y: y0 + 29, size: 14, mono: true, weight: 700, fill: C.subtle, anchor: 'end' }));
    const leds = [0, 1, 2].map(i => s('circle', { cx: x0 + 26 + i * 18, cy: y0 + 23, r: 4.5, fill: '#2c373a' }));
    const names = ['users', 'orders', 'products'];
    const drawers = names.map((name, i) => {
      const y = y0 + 64 + i * 72;
      const glow = s('rect', { x: x0 + 18, y, width: W - 36, height: 60, rx: 12, fill: C.petrol, 'fill-opacity': 0.14, stroke: C.petrol, 'stroke-width': 2.5, opacity: 0 });
      const face = s('rect', { x: x0 + 18, y, width: W - 36, height: 60, rx: 12, fill: '#1b2427', stroke: '#34424a', 'stroke-width': 1.5 });
      const handle = s('rect', { x: -24, y: y + 44, width: 48, height: 6, rx: 3, fill: '#3e4b50' });
      const dot = s('circle', { cx: x0 + W - 40, cy: y + 24, r: 5, fill: '#2c373a' });
      const lab = T(name, { x: x0 + 36, y: y + 34, size: 27, mono: true, fill: C.muted });
      const g = s('g', {}, face, glow, handle, dot, lab);
      return { g, glow, dot, face, label: lab, name, cy: y + 30, node: new E.Node(g) };
    });
    // The write compartment.
    const wy = y0 + 64 + 3 * 72 + 6;
    const wh = H - (wy - y0) - 18;
    const wface = s('rect', { x: x0 + 18, y: wy, width: W - 36, height: wh, rx: 12, fill: '#1d1710', stroke: C.warmEdge, 'stroke-width': 1.5 });
    const wglow = s('rect', { x: x0 + 18, y: wy, width: W - 36, height: wh, rx: 12, fill: C.warm, 'fill-opacity': 0.1, stroke: C.warm, 'stroke-width': 2.5, opacity: 0 });
    const switches = ['cancel', 'exchange', 'refund'].map((name, i) => {
      const cx = x0 + 22 + (W - 96) * (i + 0.5) / 3;
      return s('g', {},
        s('rect', { x: cx - 17, y: wy + 14, width: 34, height: 17, rx: 8.5, fill: '#2a2115', stroke: '#7a5a28', 'stroke-width': 1.3 }),
        s('circle', { cx: cx - 8, cy: wy + 22.5, r: 5.5, fill: '#b9852f' }),
        T(name, { x: cx, y: wy + 49, size: 12.5, mono: true, fill: '#c89a52', anchor: 'middle' }));
    });
    // Glass: two sheen strokes across the compartment.
    const glass = s('g', { opacity: 0.55 },
      s('line', { x1: x0 + 60, y1: wy + wh - 4, x2: x0 + 92, y2: wy + 4, stroke: '#fff', 'stroke-opacity': 0.08, 'stroke-width': 10 }),
      s('line', { x1: x0 + 104, y1: wy + wh - 4, x2: x0 + 118, y2: wy + 4, stroke: '#fff', 'stroke-opacity': 0.06, 'stroke-width': 5 }));
    const lock = padlock();
    lock.node.base.x = x0 + W - 44;
    lock.node.base.y = wy + 30;
    lock.g.setAttribute('transform', `translate(${lock.node.base.x} ${lock.node.base.y})`);
    const write = s('g', {}, wface, wglow, ...switches, glass, lock.g);
    const lab = label('MCP server', 0, H / 2 + 54);
    const g = s('g', { class: 'server' }, shadow(0, H / 2 + 16, 160, 18), body, top, ...leds, ...drawers.map(d => d.g), write, lab);
    return { g, drawers, leds, write, wglow, lock, label: lab, W, H, x0, y0, wy, wh };
  }

  function padlock() {
    const glow = s('circle', { cx: 0, cy: 6, r: 44, fill: 'url(#gHaloWarm)', opacity: 0 });
    const shackle = s('path', { d: 'M -10 0 V -9 A 10 10 0 0 1 10 -9 V 0', fill: 'none', stroke: C.warm, 'stroke-width': 4.5, 'stroke-linecap': 'round' });
    const bodyR = s('rect', { x: -16, y: -2, width: 32, height: 26, rx: 6, fill: C.warm });
    const hole = s('g', {}, s('circle', { cx: 0, cy: 8, r: 3.6, fill: '#3a2708' }), s('rect', { x: -1.6, y: 9, width: 3.2, height: 8, rx: 1.4, fill: '#3a2708' }));
    const g = s('g', {}, glow, shackle, bodyR, hole);
    return { g, glow, shackle, node: new E.Node(g) };
  }

  // ----------------------------------------------------------------- proxy
  // stretto: a glass lens holding the mark's three bars.
  function proxy() {
    const halo = s('circle', { r: 220, fill: 'url(#gHalo)', opacity: 0.7 });
    const clipId = 'clipLens';
    const clip = s('clipPath', { id: clipId }, s('rect', { x: -62, y: -128, width: 124, height: 256, rx: 62 }));
    const capsule = s('rect', { x: -62, y: -128, width: 124, height: 256, rx: 62, fill: 'url(#gGlass)', stroke: C.petrol, 'stroke-opacity': 0.9, 'stroke-width': 2.2 });
    const rimIn = s('rect', { x: -53, y: -119, width: 106, height: 238, rx: 53, fill: 'none', stroke: C.petrolText, 'stroke-opacity': 0.18, 'stroke-width': 1.5 });
    const sheen = s('rect', { x: -120, y: -170, width: 60, height: 340, fill: 'url(#gSheen)', transform: 'rotate(18)' });
    const sheenG = s('g', { 'clip-path': `url(#${clipId})` }, sheen);
    const k = 3.2, bw = 20 * k, bh = 7 * k, rx = 1.5 * k;
    const offs = [[0, 0], [6 * k, 10 * k], [10 * k, 20 * k]];
    const ox = -(10 * k + bw) / 2, oy = -(20 * k + bh) / 2;
    const bars = offs.map(([dx, dy], i) => {
      const glow = s('rect', { x: ox + dx - 6, y: oy + dy - 6, width: bw + 12, height: bh + 12, rx: rx + 6, fill: i ? C.petrol : C.paper, opacity: 0 });
      const r = s('rect', { x: ox + dx, y: oy + dy, width: bw, height: bh, rx, fill: i ? C.petrol : C.paper });
      return { r, glow, g: s('g', {}, glow, r) };
    });
    const lab = T('stretto', { x: 0, y: 176, size: 31, mono: true, weight: 700, fill: C.petrolText, anchor: 'middle' });
    const g = s('g', { class: 'proxy' }, clip, halo, shadow(0, 140, 80, 13), capsule, sheenG, rimIn, ...bars.map(b => b.g), lab);
    return { g, halo, capsule, sheen, bars, label: lab };
  }

  // ----------------------------------------------------------------- cards
  // A call or result card: a direction glyph, a tool name and short JSON.
  // parts: [[text, fill], ...] for the second line.
  function card({ dir = '→', title, sub, kind = 'call', size = 22, minW = 0 }) {
    const subText = typeof sub === 'string' ? sub : sub.map(p => p[0]).join('');
    const w = Math.max(minW, 64 + Math.max(monoWidth(title, size), monoWidth(subText, size - 3)) + 26);
    const h = 94;
    const accent = kind === 'result' ? C.petrol : kind === 'write' ? C.warm : '#dfe5e6';
    const clipId = `clipCard${card.n = (card.n || 0) + 1}`;
    const g = s('g', { class: 'card' },
      s('clipPath', { id: clipId }, s('rect', { x: -w / 2, y: -h / 2, width: w, height: h, rx: 14 })),
      s('rect', { x: -w / 2 + 4, y: -h / 2 + 10, width: w, height: h, rx: 14, fill: '#000', opacity: 0.35 }),
      s('rect', { x: -w / 2, y: -h / 2, width: w, height: h, rx: 14, fill: 'url(#gCard)', stroke: kind === 'write' ? C.warmEdge : '#384850', 'stroke-width': 1.5 }),
      s('rect', { x: -w / 2, y: -h / 2, width: 5, height: h, fill: accent, 'clip-path': `url(#${clipId})` }),
      T(dir, { x: -w / 2 + 22, y: -6, size, mono: true, fill: kind === 'result' ? C.petrolText : C.subtle }),
      T(title, { x: -w / 2 + 56, y: -6, size, mono: true, fill: kind === 'write' ? C.warm : C.text }),
      typeof sub === 'string'
        ? T(sub, { x: -w / 2 + 56, y: 26, size: size - 3, mono: true, fill: C.subtle })
        : TM(sub, { x: -w / 2 + 56, y: 26, size: size - 3, mono: true, fill: C.subtle }));
    return { g, w, h, node: new E.Node(g) };
  }
  // stretto's own read: a petrol pill.
  function lookup(text, size = 23) {
    const w = monoWidth(text, size) + 36, h = 48;
    const g = s('g', { class: 'lookup' },
      s('rect', { x: -w / 2 - 8, y: -h / 2 - 8, width: w + 16, height: h + 16, rx: 16, fill: C.petrol, opacity: 0.16 }),
      s('rect', { x: -w / 2, y: -h / 2, width: w, height: h, rx: 9, fill: C.petrol }),
      T(text, { x: 0, y: 8, size, mono: true, weight: 700, fill: '#08201f', anchor: 'middle' }));
    return { g, w, h, node: new E.Node(g) };
  }
  // A lookup's result, clipped under the card it rides in.
  function rider(parts, w, size = 21) {
    const h = 46;
    const g = s('g', { class: 'rider' },
      s('rect', { x: -w / 2, y: -h / 2, width: w, height: h, rx: 9, fill: '#0c2629', stroke: C.petrol, 'stroke-width': 1.5 }),
      TM(parts, { x: -w / 2 + 16, y: 7, size, mono: true, fill: C.petrolText }));
    return { g, w, h, node: new E.Node(g) };
  }

  // A turn on the timeline.
  function turnChip({ n, tool, arg, kind = 'read', w = 230 }) {
    const h = 88;
    const edge = kind === 'write' ? C.warm : kind === 'reply' ? '#4a565a' : C.chipEdge;
    const rect = s('rect', { x: 0, y: 0, width: w, height: h, rx: 15, fill: kind === 'write' ? '#1f1a12' : C.chip, stroke: edge, 'stroke-width': kind === 'write' ? 2.2 : 1.8, 'stroke-opacity': kind === 'write' ? 0.8 : 1 });
    const ring = s('rect', { x: -1, y: -1, width: w + 2, height: h + 2, rx: 16, fill: 'none', stroke: C.petrol, 'stroke-width': 3.5, opacity: 0 });
    const big = w > 80;
    const ix = big ? 24 : w / 2;
    const icon = kind === 'reply'
      ? s('path', { d: `M ${ix - 13} 27 h 26 a 6 6 0 0 1 6 6 v 11 a 6 6 0 0 1 -6 6 h -14 l -8 7 v -7 h -4 a 6 6 0 0 1 -6 -6 v -11 a 6 6 0 0 1 6 -6 z`, fill: 'none', stroke: C.muted, 'stroke-width': 2.2, transform: 'translate(0 3)' })
      : s('circle', { cx: ix, cy: 44, r: 7, fill: kind === 'write' ? C.warm : C.petrol });
    const parts = [rect, ring, icon];
    if (big) {
      const tx = kind === 'reply' ? 56 : 44;
      parts.push(T(tool, { x: tx, y: arg ? 39 : 55, size: kind === 'reply' ? 32 : 30, weight: 620, fill: kind === 'write' ? C.warm : C.text }));
      if (arg) parts.push(T(arg, { x: tx, y: 72, size: 24, mono: true, fill: C.muted }));
    }
    const num = big
      ? T(`turn ${n}`, { x: 2, y: -14, size: 22, weight: 640, fill: '#8a969a', ls: 0.12, caps: true })
      : T(String(n), { x: w / 2, y: -14, size: 22, weight: 640, fill: '#8a969a', anchor: 'middle', tnum: true });
    parts.push(num);
    const g = s('g', { class: 'turn' }, ...parts);
    return { g, ring, num, w, h, node: new E.Node(g) };
  }
  // A ghost turn: the dashed outline of a turn not taken yet.
  function ghostChip(w = 214, h = 64) {
    return s('rect', { x: 0, y: 0, width: w, height: h, rx: 13, fill: 'none', stroke: '#4a5559', 'stroke-width': 2, 'stroke-dasharray': '8 7' });
  }

  // ------------------------------------------------------------------ mark
  // The mark's three bars at k px per unit of its 32-unit grid, with bar 1's
  // top-left at the origin. Each bar is its own node, for entrances.
  function mark(k, colors = [C.paper, C.petrol, C.petrol]) {
    const bw = 20 * k, bh = 7 * k, rx = 1.5 * k;
    const offs = [[0, 0], [6 * k, 10 * k], [10 * k, 20 * k]];
    const bars = offs.map(([dx, dy], i) => {
      const trail = s('rect', { x: -bw * 2.2, y: 0, width: bw * 2.2, height: bh, rx: bh / 2, fill: i ? 'url(#gTrailPetrol)' : 'url(#gTrailPaper)', opacity: 0 });
      const r = s('rect', { x: 0, y: 0, width: bw, height: bh, rx, fill: colors[i] });
      const g = s('g', { transform: `translate(${dx} ${dy})` }, trail, r);
      return { g, r, trail, x: dx, y: dy, w: bw, h: bh, node: new E.Node(g, { x: dx, y: dy }) };
    });
    return { g: s('g', {}, ...bars.map(b => b.g)), bars, w: 30 * k, h: 27 * k, bw, bh };
  }

  // The wordmark, as its letters: the path from logo/stretto-wordmark-dark.svg,
  // split into subpaths and grouped by letter, so each can enter on its own.
  const WORD_D = document.getElementById('wordmark-src').getAttribute('d');
  function wordmark() {
    const subs = WORD_D.split(/(?=M)/).map(d => d.trim()).filter(Boolean);
    // Letter boundaries in font units (s t r e t t o).
    const cuts = [1000, 1750, 2400, 3440, 4060, 4740];
    const letters = Array.from({ length: 7 }, () => []);
    for (const d of subs) {
      const nums = d.match(/-?\d+(?:\.\d+)?/g).map(Number);
      let lo = Infinity, hi = -Infinity;
      for (let i = 0; i < nums.length; i += 2) { lo = Math.min(lo, nums[i]); hi = Math.max(hi, nums[i]); }
      const cx = (lo + hi) / 2;
      letters[cuts.filter(c => cx > c).length].push(d);
    }
    const els = letters.map(ds => {
      const p = s('path', { d: ds.join(' '), fill: C.paper });
      const g = s('g', {}, p);
      return { g, node: new E.Node(g) };
    });
    // In the lockup's units: glyphs at x 62.38, baseline 42, 1/32 of a font unit.
    const g = s('g', { transform: 'translate(62.38 42) scale(0.03125)' }, ...els.map(l => l.g));
    return { g, letters: els };
  }

  // The lockup (mark and wordmark) at k px per lockup unit (244 x 42.8).
  function lockup(k) {
    const m = mark(k * 31.11 / 20);
    const w = wordmark();
    const g = s('g', {}, s('g', { transform: `scale(${k})` }, w.g), m.g);
    return { g, mark: m, word: w, w: 244.03 * k, h: 42.78 * k };
  }

  // ------------------------------------------------------------------- HUD
  function hud() {
    const W = 740, H = 128;
    const g = s('g', { class: 'hud' },
      s('rect', { x: 0, y: 0, width: W, height: H, rx: 22, fill: '#11171a', 'fill-opacity': 0.88, stroke: '#2a3437', 'stroke-width': 1.6 }));
    const cols = [['LLM turns', 0], ['time', 1], ['tokens', 2]].map(([name, i]) => {
      const x = 32 + i * 236;
      const lab = T(name, { x, y: 45, size: 24, weight: 640, fill: C.subtle, ls: 0.1, caps: true });
      const val = T('0', { x, y: 102, size: 48, weight: 640, fill: C.text, tnum: true, opsz: 32 });
      const delta = T('', { x: x + 212, y: 101, size: 28, weight: 660, fill: C.petrolText, anchor: 'end', tnum: true });
      if (i) g.appendChild(s('line', { x1: x - 20, y1: 24, x2: x - 20, y2: H - 24, stroke: '#2a3437', 'stroke-width': 1.6 }));
      E.add(g, lab, val, delta);
      return { lab, val, delta };
    });
    return { g, turns: cols[0], time: cols[1], tokens: cols[2], W, H, node: new E.Node(g) };
  }

  window.C = C;
  window.A = { T, TM, label, monoWidth, defs, shadow, agent, customer, speech, server, padlock, proxy, card, lookup, rider, turnChip, ghostChip, mark, wordmark, lockup, hud, SANS, MONO };
})();
