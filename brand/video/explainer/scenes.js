// The scenes, part one: shared helpers, then the open, the turns, the turns
// the tools decided, the idea, and what stretto cannot touch. Each scene
// anchors its beats to the words of its line (ctx.word), so the pictures land
// as the voice names them. scenes2.js has the rest.
(() => {
  const { s } = E;
  const { T, TM, label } = A;
  const N = (el, base) => W.node(el, base);
  const G = (parent, attrs = {}) => { const g = s('g', attrs); parent.appendChild(g); return g; };
  const L = E.lerp;

  // ----------------------------------------------------------- the helpers
  const H = {};
  H.N = N; H.G = G;
  // Hold a track at its current value up to t, so keys added from t on do
  // not bend what came before.
  H.hold = (n, prop, t) => {
    const k = n.tr[prop];
    if (k && k.length) n.key(prop, [t - 1e-4, E.trackAt(k, t - 1e-4), 'lin']);
  };
  H.fadeIn = (n, t, d = 0.5, dy = 14, ease = 'out3') => {
    H.hold(n, 'o', t); n.key('o', [t, 0], [t + d, 1, ease]);
    if (dy) { H.hold(n, 'dy', t); n.key('dy', [t, dy], [t + d * 1.3, 0, ease]); }
    return n;
  };
  H.fadeOut = (n, t, d = 0.4, ease = 'in2') => { H.hold(n, 'o', t); n.key('o', [t + 1e-4, E.trackAt(n.tr.o || [[0, n.base.o]], t)], [t + d, 0, ease]); return n; };
  H.popIn = (n, t, d = 0.75, from = 0.5) => {
    H.hold(n, 'o', t); n.key('o', [t, 0], [t + 0.18, 1, 'lin']);
    H.hold(n, 's', t); n.key('s', [t, from], [t + d, 1, 'spring']);
    return n;
  };
  // Keyframed motion along arcs: keys [{t, x, y, s, o, r, ease, arc}].
  H.flight = (n, keys) => {
    const at = (p, k) => { p.x = k.x; p.y = k.y; p.s = k.s ?? 1; p.o = k.o ?? 1; p.r = k.r ?? 0; };
    n.on((p, t) => {
      if (t <= keys[0].t) return at(p, keys[0]);
      for (let i = 1; i < keys.length; i++) {
        const b = keys[i];
        if (t < b.t) {
          const a = keys[i - 1], q = (t - a.t) / (b.t - a.t), e = E.easeOf(b.ease || 'io3')(q);
          p.x = L(a.x, b.x, e);
          p.y = L(a.y, b.y, e) - (b.arc || 0) * 4 * e * (1 - e);
          p.s = L(a.s ?? 1, b.s ?? 1, e);
          p.o = L(a.o ?? 1, b.o ?? 1, E.clamp(b.oe ? E.easeOf(b.oe)(q) : q));
          p.r = L(a.r ?? 0, b.r ?? 0, e);
          return;
        }
      }
      at(p, keys[keys.length - 1]);
    });
    return n;
  };
  // Stroke a path on, from t0 over d.
  H.drawOn = (ctx, el, t0, d, ease = 'io3') => {
    let len = null;
    ctx.fn(t => {
      if (len === null) { len = el.getTotalLength(); el.setAttribute('stroke-dasharray', `${len.toFixed(1)} ${(len + 10).toFixed(1)}`); }
      const p = E.P(t, t0, d, ease);
      E.attr(el, 'stroke-dashoffset', (len * (1 - p)).toFixed(1));
      E.attr(el, 'display', p > 0 ? 'inline' : 'none');
    });
  };
  // Type text into el from t0 at cps characters a second, with a caret.
  H.type = (ctx, el, full, t0, cps = 40, caret = true, until = Infinity) => {
    ctx.fn(t => {
      const n = Math.max(0, Math.min(full.length, Math.floor((t - t0) * cps)));
      const on = caret && t >= t0 - 0.4 && t < until && (n < full.length || Math.floor(t * 2.2) % 2 === 0);
      E.text(el, full.slice(0, n) + (on ? '▍' : ''));
    });
  };
  // A number counting from a to b over [t0, t0 + d].
  H.count = (ctx, el, t0, d, a, b, fmt, ease = 'out3') => ctx.fn(t => E.text(el, fmt(L(a, b, E.P(t, t0, d, ease)))));
  // Paint a group only in [a, b].
  H.during = (ctx, el, a, b) => ctx.fn(t => E.attr(el, 'display', t >= a && t <= b ? 'inline' : 'none'));
  // A pill tag with a check mark.
  H.tag = (text, color = C.petrolText, fill = C.soft, check = true) => {
    const w = text.length * 14.4 + (check ? 82 : 50);
    return s('g', {},
      s('rect', { x: -w / 2, y: -28, width: w, height: 56, rx: 28, fill, stroke: color, 'stroke-opacity': 0.75, 'stroke-width': 1.8 }),
      check ? s('path', { d: `M ${-w / 2 + 24} 0 l 8 8 l 14 -15`, fill: 'none', stroke: color, 'stroke-width': 3.2, 'stroke-linecap': 'round', 'stroke-linejoin': 'round' }) : null,
      T(text, { x: check ? -w / 2 + 56 : 0, y: 9, size: 26, weight: 620, fill: color, anchor: check ? 'start' : 'middle' }));
  };
  // A small card in flight during the montage.
  H.pellet = (kind, text = '') => {
    const w = kind === 'reply' ? 72 : text.length * 9.6 + 34, h = 42;
    const fill = kind === 'result' ? '#0d2a2d' : kind === 'write' ? '#241b0f' : kind === 'reply' ? '#1c2427' : '#1d2629';
    const edge = kind === 'result' ? C.petrol : kind === 'write' ? C.warm : '#3f4d52';
    const kids = [s('rect', { x: -w / 2, y: -h / 2, width: w, height: h, rx: 10, fill, stroke: edge, 'stroke-width': 1.5 })];
    if (kind === 'reply') kids.push(s('path', { d: 'M -14 -6 h 28 M -14 3 h 18', stroke: C.muted, 'stroke-width': 3, 'stroke-linecap': 'round' }));
    else kids.push(T(text, { x: 0, y: 5.5, size: 16, mono: true, fill: kind === 'result' ? C.petrolText : kind === 'write' ? C.warm : C.text, anchor: 'middle' }));
    return s('g', {}, ...kids);
  };
  // The result stretto holds, with its two reads clipped under it, one width.
  H.bundle = res => {
    const parts = [
      [['+ get_order_details #W7a  ', C.petrolText], ['{"status":"pending",…}', '#8fd9dd']],
      [['+ get_order_details #W7b  ', C.petrolText], ['{"status":"delivered",…}', '#8fd9dd']],
    ];
    const wide = Math.max(...parts.map(p => p.reduce((a, q) => a + q[0].length, 0) * 21 * 0.6 + 36));
    const resCard = A.card({ ...res, minW: wide });
    const g = s('g', {});
    const riders = parts.map(p => A.rider(p, resCard.w));
    const riderNs = riders.map(rd => { g.appendChild(rd.g); return N(rd.node, { y: resCard.h / 2 + 31, o: 0 }); });
    g.appendChild(resCard.g);
    return { g, resCard, riderNs };
  };
  H.fmtTime = x => { const v = Math.round(x); return `${Math.floor(v / 60)}:${String(v % 60).padStart(2, '0')}`; };
  H.fmtTok = x => (x < 9950 ? `${(x / 1000).toFixed(1)}k` : `${Math.round(x / 1000)}k`);

  // -------------------------------------------------- the shared machinery
  // What the tableau's characters do, as lists the scenes add to: the
  // agent's thinking, the drawers' reads, the proxy's flashes, the HUD's
  // counts. One function per character draws them each frame.
  function shared(ctx) {
    const st = { thinks: [], glows: [], reads: [], access: [], writes: [], proxyHits: [], hud: [], locks: [] };
    const ov = W.ovRoot;
    st.layers = {};
    for (const k of ['hud', 'labels', 'fog', 'panel', 'iris', 'wipe', 'dip', 'title']) st.layers[k] = G(ov, { id: `ov-${k}` });

    // The agent.
    const thinkAt = t => { let k = 0.05; for (const [a, b, lv] of st.thinks) k = Math.max(k, lv * E.pulse(t, a, b, 0.22, 0.38)); return k; };
    const phaseAt = t => { let ph = 0; for (const [a, b, lv] of st.thinks) ph += lv * E.clamp(t - a, 0, b - a); return ph; };
    const glowAt = t => { let g = 0; for (const [a, d] of st.glows) g = Math.max(g, E.pulse(t, a, a + d, 0.2, d * 0.6)); return g; };
    ctx.fn(t => W.agent.set(t, thinkAt(t), phaseAt(t), glowAt(t)));

    // The agent reads the whole conversation: a beam, and a band down it.
    ctx.fn(t => {
      let on = 0, pos = 0;
      for (const [a, b] of st.reads) {
        const k = E.pulse(t, a, b, 0.2, 0.3);
        if (k > on) { on = k; pos = E.P(t, a + 0.1, Math.max(0.3, b - a - 0.35), 'io2'); }
      }
      const top = -W.scroll.h / 2 + 12, bot = W.scroll.h / 2 - 52;
      const y = top + (bot - top) * pos;
      const sx = W.P.scroll[0] + W.scroll.w / 2 - 2, sy = W.P.scroll[1];
      const ax = W.P.agent[0] - 30, ay = W.P.agent[1];
      E.attr(W.beam, 'd', `M ${ax} ${ay - 10} L ${sx} ${(sy + y).toFixed(1)} L ${sx} ${(sy + y + 40).toFixed(1)} L ${ax} ${ay + 10} Z`);
      E.attr(W.beam, 'opacity', on.toFixed(3));
      E.attr(W.scroll.read, 'y', y.toFixed(1));
      E.attr(W.scroll.read, 'opacity', on.toFixed(3));
    });

    // The conversation's entries stack, and scroll once they overflow.
    ctx.fn(t => {
      let y = -W.scroll.h / 2 + 22;
      const placed = [];
      for (const e of W.scroll.entries) {
        const k = E.P(t, e.tIn, 0.4, 'out3') * (e.tOut < Infinity ? 1 - E.P(t, e.tOut - 0.35, 0.35, 'in2') : 1);
        placed.push([e, y, k]);
        y += (e.h + 10) * k;
      }
      const over = Math.max(0, y - (W.scroll.h / 2 - 12));
      for (const [e, yy, k] of placed) {
        e.n.p.x = 0;
        e.n.p.y = yy - over;
        e.n.p.o = k;
        e.n.p.dx = (1 - k) * 26;
      }
    });

    // The server: drawers slide out and glow when read; the write door when written.
    ctx.fn(t => {
      W.server.drawers.forEach((dr, i) => {
        let k = 0;
        for (const a of st.access) if (a.i === i) k = Math.max(k, E.pulse(t, a.t, a.t + a.d, 0.16, 0.5));
        dr.node.p.dx = 16 * k;
        E.attr(dr.glow, 'opacity', k.toFixed(3));
        E.attr(dr.dot, 'fill', k > 0.4 ? C.petrol : '#2c373a');
      });
      let w = 0;
      for (const a of st.writes) w = Math.max(w, E.pulse(t, a.t, a.t + a.d, 0.16, 0.5));
      E.attr(W.server.wglow, 'opacity', w.toFixed(3));
      // (In the teaser's loop they hold still: every change costs the GIF bytes.)
      W.server.leds.forEach((l, i) => E.attr(l, 'fill', (E.LOOP ? i === 0 : Math.floor(t * 2.6 + i * 1.7) % 5 === 0) ? C.petrol : i === 0 ? '#3a6b6e' : '#2c373a'));
      // The lock glows and shakes when stretto's reach comes to it.
      let lk = 0, shake = 0;
      for (const a of st.locks) {
        lk = Math.max(lk, E.pulse(t, a, a + 1.6, 0.15, 0.8));
        if (t > a && t < a + 1.2) shake += 12 * Math.exp(-(t - a) * 4.5) * Math.sin((t - a) * 34);
      }
      E.attr(W.server.lock.glow, 'opacity', lk.toFixed(3));
      W.server.lock.node.p.r = shake;
    });

    // The proxy's bars flash in stretto order when a call passes through.
    ctx.fn(t => {
      W.proxy.bars.forEach((b, i) => {
        let k = 0;
        for (const h of st.proxyHits) k = Math.max(k, E.pulse(t, h + i * 0.09, h + i * 0.09 + 0.55, 0.08, 0.4));
        E.attr(b.glow, 'opacity', (0.55 * k).toFixed(3));
      });
      let k = 0;
      for (const h of st.proxyHits) k = Math.max(k, E.pulse(t, h, h + 0.8, 0.1, 0.6));
      E.attr(W.proxy.capsule, 'stroke-width', (2.2 + 2 * k).toFixed(2));
      E.attr(W.proxy.halo, 'opacity', (0.7 + 0.5 * k).toFixed(3));
      E.attr(W.proxy.sheen, 'x', (E.LOOP ? -40 : -120 + ((t * 40) % 260)).toFixed(1));
    });

    // The HUD: LLM turns, time and tokens, as steps [{t, turns, time, tokens, note}].
    const hud = A.hud();
    st.layers.hud.appendChild(hud.g);
    st.hudN = N(hud.node, { x: 1920 - hud.W - 46, y: 42, o: 0 });
    ctx.fn(t => {
      let prev = { turns: 0, time: 0, tokens: 0 }, cur = prev, t0 = -1;
      for (const h of st.hud) { if (h.t > t) break; prev = cur; cur = h; t0 = h.t; }
      const p = t0 < 0 ? 1 : E.P(t, t0, 0.45, 'out3');
      E.text(hud.turns.val, String(Math.round(L(prev.turns, cur.turns, p))));
      E.text(hud.time.val, H.fmtTime(L(prev.time, cur.time, p)));
      E.text(hud.tokens.val, H.fmtTok(L(prev.tokens, cur.tokens, p)));
      const note = key => {
        const d = cur[key] - prev[key];
        if (!(t0 >= 0 && t - t0 < 1.8 && Math.abs(d) > 1e-6 && cur.note)) return '';
        const v = Math.abs(d);
        return (d > 0 ? '+' : '−') + (key === 'tokens' ? H.fmtTok(v) : key === 'time' ? `${Math.round(v)}s` : v);
      };
      E.text(hud.turns.delta, note('turns'));
      E.text(hud.time.delta, note('time'));
      E.text(hud.tokens.delta, note('tokens'));
      const warm = cur.note === 'warm';
      for (const col of [hud.turns, hud.time, hud.tokens]) E.attr(col.delta, 'fill', warm ? C.warm : C.petrolText);
      E.attr(hud.tokens.val, 'fill', warm && t - t0 < 1.8 ? C.warm : C.text);
    });
    st.hudAdd = (t, turns, time, tokens, note = '') => { st.hud.push({ t, turns, time, tokens, note }); st.hud.sort((a, b) => a.t - b.t); };
    return st;
  }

  // Where cards leave and arrive.
  const agentOut = [-104, W.laneY.call], agentIn = [-96, W.laneY.result];
  const serverIn = [488, W.laneY.call], serverOut = [488, W.laneY.result];
  const drawerAt = i => [W.P.server[0] - 30, W.P.server[1] + W.server.drawers[i].cy];
  const writeAt = () => [W.P.server[0] - 20, W.P.server[1] + W.server.wy + W.server.wh / 2];
  const scrollIn = () => [W.P.scroll[0] + 10, W.P.scroll[1] - 40];
  const flyer = el => { W.flyers.appendChild(el); return N(el, { o: 0 }); };
  Object.assign(H, { agentOut, agentIn, serverIn, serverOut, drawerAt, writeAt, scrollIn, flyer });

  // A call to the server, and its result back into the conversation.
  function callAndResult(ctx, st, { t, dur = 1.0, drawer = 0, call, result, kind = 'read', big = false }) {
    const d = dur;
    const cn = flyer(big ? A.card(call).g : H.pellet(kind === 'write' ? 'write' : 'call', call));
    const end = kind === 'write' ? writeAt() : drawerAt(drawer);
    H.flight(cn, [
      { t, x: agentOut[0] - 20, y: agentOut[1], s: 0.55, o: 0 },
      { t: t + 0.18 * d, x: agentOut[0] + 20, y: agentOut[1], s: 1, o: 1, ease: 'out3' },
      { t: t + 0.8 * d, x: serverIn[0] - (big ? 150 : 90), y: serverIn[1], s: 1, o: 1, ease: 'io3', arc: big ? 26 : 16 },
      { t: t + d, x: end[0], y: end[1], s: 0.3, o: 0, ease: 'in2' },
    ]);
    if (kind === 'write') st.writes.push({ t: t + 0.85 * d, d: 1.0 });
    else st.access.push({ i: drawer, t: t + 0.85 * d, d: 0.9 + 0.3 * d });
    if (!result) return { cn, back: t + d };
    const r0 = t + d + 0.12 * d;
    const rn = flyer(big ? A.card(result).g : H.pellet('result', result));
    const si = scrollIn();
    H.flight(rn, [
      { t: r0, x: end[0], y: end[1], s: 0.3, o: 0 },
      { t: r0 + 0.2 * d, x: serverOut[0] - (big ? 150 : 90), y: serverOut[1], s: 1, o: 1, ease: 'out3' },
      { t: r0 + 0.95 * d, x: agentIn[0] + (big ? 130 : 60), y: agentIn[1], s: 1, o: 1, ease: 'io3', arc: -(big ? 22 : 14) },
      { t: r0 + 1.3 * d, x: si[0], y: si[1], s: 0.22, o: 0, ease: 'in3' },
    ]);
    W.scroll.push('result', r0 + 1.22 * d);
    return { cn, rn, back: r0 + 1.3 * d };
  }

  // ------------------------------------------------------------- the open
  // Black. The mark's three bars enter along a staff, each before the last
  // has finished; the wordmark writes on; the tagline. Then the bars fold
  // into a point of light, and an iris opens on the world from it.
  function open(ctx, st) {
    const { S, word, cam, ev } = ctx;
    const T1 = S.turns.t0;
    const k = 3.2;
    const lk = A.lockup(k);
    const LX = (1920 - lk.w) / 2, LY = 396;
    st.lockup = { LX, LY, k };
    const openG = G(st.layers.title);
    const openN = N(openG);
    // The staff: five faint lines through the bars' heights.
    const mk = lk.mark;
    const step = mk.bars[1].y - mk.bars[0].y;
    const staffG = G(openG);
    const staffN = N(staffG);
    [-1, 0, 1, 2, 3].forEach((i, j) => {
      const y = LY + mk.bars[0].y + mk.bh / 2 + i * step;
      const l = s('line', { x1: -40, y1: y, x2: 1960, y2: y, stroke: '#1f2a2d', 'stroke-width': 2 });
      staffG.appendChild(l);
      H.drawOn(ctx, l, 0.05 + j * 0.07, 1.1, 'io3');
    });
    H.fadeOut(staffN, 1.9, 0.9, 'io2');
    const lockG = G(openG);
    lockG.appendChild(lk.g);
    N(lockG, { x: LX, y: LY });
    // The bars: each enters from the left, fast, and settles.
    const starts = [0.42, 0.74, 1.02];
    ctx.music.open = starts.map(x => +x.toFixed(3));
    mk.bars.forEach((b, i) => {
      const a = starts[i];
      const n = N(b.node);
      n.key('o', [a - 0.001, 0], [a, 1, 'lin']);
      n.key('dx', [a, -1500 - LX], [a + 1.25, 0, 'out5']);
      ctx.fn(t => E.attr(b.trail, 'opacity', (0.95 * (1 - E.P(t, a + 0.05, 0.85, 'out2')) * (t >= a ? 1 : 0)).toFixed(3)));
      ev(a, 'swoosh-in', 0.2);
    });
    // The wordmark writes on, letter by letter.
    lk.word.letters.forEach((l, j) => {
      const n = N(l.node);
      const a = 1.48 + j * 0.055;
      n.key('o', [a, 0], [a + 0.35, 1, 'out2']);
      n.key('dy', [a, 520], [a + 0.7, 0, 'out4']);
    });
    ev(1.5, 'shimmer', 0.3);
    const tagEl = T('Read ahead of your agent.', { x: 960, y: 668, size: 58, weight: 620, anchor: 'middle', ls: -0.028 });
    openG.appendChild(tagEl);
    const tagN = N(tagEl);
    H.fadeIn(tagN, Math.max(2.3, word('open', 'reads ahead') - 0.1), 0.8, 18);
    // A slow push in.
    ctx.fn(t => { const sc = 1 + 0.045 * E.P(t, 0, T1, 'io2'); openN.p.s = sc; openN.p.x = 960 * (1 - sc); openN.p.y = 540 * (1 - sc); });
    // Out: the words go; the bars fold into a point.
    const out = T1 - 0.95;
    H.fadeOut(tagN, out, 0.35);
    lk.word.letters.forEach((l, j) => H.fadeOut(l.node, out + j * 0.02, 0.3));
    const seed = s('g', {}, s('circle', { r: 70, fill: 'url(#gHalo)' }), s('circle', { r: 15, fill: C.petrolText }), s('circle', { r: 6, fill: '#effdfd' }));
    st.layers.title.appendChild(seed);
    const seedN = N(seed, { x: 960, y: 540, o: 0 });
    seedN.key('o', [T1 - 0.2, 0], [T1 - 0.02, 1, 'out2'], [T1 + 0.35, 1], [T1 + 0.75, 0, 'in2']);
    seedN.key('s', [T1 - 0.2, 0.3], [T1 + 0.1, 1.3, 'out3'], [T1 + 0.6, 2.2, 'in2']);
    mk.bars.forEach((b, i) => {
      const a = T1 - 0.62 + i * 0.1;
      b.node.on((p, t) => {
        const q = E.P(t, a, 0.5, 'in3');
        if (q <= 0) return;
        // The centre of the screen, in the bar's own coordinates.
        const sc = openN.p.s, ox = openN.p.x, oy = openN.p.y;
        const cx = (960 - ox) / sc - LX - b.w / 2, cy = (540 - oy) / sc - LY - b.h / 2;
        const bx = b.x + p.dx, by = b.y;
        const ss = L(1, 0.08, q);
        p.dx = 0;
        p.x = L(bx, cx, q) + (b.w / 2) * (1 - ss);
        p.y = L(by, cy, q) + (b.h / 2) * (1 - ss);
        p.s = ss;
        p.o = 1 - E.P(t, a + 0.35, 0.15, 'lin');
      });
    });
    ev(T1 - 0.62, 'whoosh-soft', 0.32);
    H.during(ctx, openG, 0, T1 + 0.2);

    // The iris opens on the world, from the point where the bars folded.
    const iris = s('path', { d: '', fill: C.bg, 'fill-rule': 'evenodd' });
    const ring = s('circle', { cx: 960, cy: 540, r: 0, fill: 'none', stroke: C.petrol, 'stroke-width': 2.5 });
    E.add(st.layers.iris, iris, ring);
    const i0 = T1 - 0.05;
    ctx.fn(t => {
      const r = t < i0 ? 0 : 12 + 1250 * E.P(t, i0, 1.35, 'io3');
      if (r >= 1240) { E.attr(iris, 'display', 'none'); E.attr(ring, 'display', 'none'); return; }
      E.attr(iris, 'display', 'inline');
      E.attr(iris, 'd', `M -10 -10 H 1930 V 1090 H -10 Z M ${(960 - r).toFixed(1)} 540 a ${r.toFixed(1)} ${r.toFixed(1)} 0 1 0 ${(2 * r).toFixed(1)} 0 a ${r.toFixed(1)} ${r.toFixed(1)} 0 1 0 ${(-2 * r).toFixed(1)} 0 Z`);
      E.attr(ring, 'display', r > 1 ? 'inline' : 'none');
      E.attr(ring, 'r', r.toFixed(1));
      E.attr(ring, 'opacity', (0.8 * (1 - E.P(t, i0 + 0.3, 1.0, 'lin'))).toFixed(3));
    });
    ev(i0, 'swoosh-in', 0.28);
    // The world, seen first from close on the agent's core.
    cam.start = { x: W.P.agent[0], y: W.P.agent[1], z: 3.4, r: -5, tilt: 0 };
    cam.go(T1 + 0.05, 2.7, { x: -70, y: 96, z: 1.06, r: 0 }, 'smooth', 'io3');
  }

  // ------------------------------------------------------------ the turns
  function turns(ctx, st) {
    const { S, word, cam, ev } = ctx;
    const T1 = S.turns.t0;
    const w = p => word('turns', p);
    const wAgent = w('AI agent'), wTurns = w('works in turns'), wReads = w('reads the whole'), wPicks = w('picks one');
    const wAnswer = w('answer the user'), wTool = w('call a tool'), wSeconds = w('seconds'), wTokens = w('thousands of tokens');

    // The customer asks.
    const bub = W.customer.bubbleN;
    H.popIn(bub, T1 + 0.9, 0.8, 0.4);
    ev(T1 + 0.9, 'pop', 0.28);
    const msg = flyer(s('rect', { x: -40, y: -7, width: 80, height: 14, rx: 7, fill: C.paper }));
    const [cx, cy] = W.P.customer, [sx, sy] = W.P.scroll;
    H.flight(msg, [
      { t: T1 + 1.7, x: cx + 30, y: cy - 150, s: 0.6, o: 0 },
      { t: T1 + 1.85, x: cx + 60, y: cy - 150, s: 1, o: 1, ease: 'out2' },
      { t: T1 + 2.35, x: sx - 20, y: sy - 140, s: 0.8, o: 0.2, ease: 'io3', arc: 40 },
    ]);
    W.scroll.push('user', T1 + 2.3);

    st.glows.push([wAgent - 0.1, 1.1]);
    H.fadeIn(W.timeline.label, Math.max(wTurns, T1 + 2.2), 0.6, 0);
    // A turn: the model reads the whole conversation, then picks one step.
    st.thinks.push([wReads - 0.25, wPicks + 0.35, 1]);
    st.reads.push([wReads, Math.max(wReads + 1.4, wPicks - 0.1)]);
    ev(wReads, 'shimmer', 0.16);
    // The choice: answer the user, or call a tool.
    for (const [arm, at, hl] of [[W.fork.user, wPicks, wAnswer], [W.fork.tool, wPicks + 0.12, wTool]]) {
      H.drawOn(ctx, arm.path, at, 0.55, 'out3');
      H.fadeIn(arm.n, at, 0.35, 0);
      const hot = arm === W.fork.tool ? C.petrolText : C.text;
      ctx.fn(t => {
        const k = E.pulse(t, hl - 0.05, arm === W.fork.tool ? hl + 2.5 : hl + 0.9, 0.2, 0.35);
        const col = E.mixColor(C.muted, hot, k);
        E.attr(arm.path, 'stroke', col); E.attr(arm.head, 'stroke', col); E.attr(arm.txt, 'fill', col);
      });
      ctx.fn(t => {
        // The arrowhead rides the end of the drawn path.
        const len = arm.path.__len || (arm.path.__len = arm.path.getTotalLength());
        const q = E.P(t, at, 0.55, 'out3');
        const p1 = arm.path.getPointAtLength(Math.max(0.01, len * q)), p0 = arm.path.getPointAtLength(Math.max(0, len * q - 2));
        const ang = (Math.atan2(p1.y - p0.y, p1.x - p0.x) * 180) / Math.PI;
        E.attr(arm.head, 'transform', `translate(${p1.x.toFixed(1)} ${p1.y.toFixed(1)}) rotate(${ang.toFixed(1)})`);
        E.attr(arm.head, 'display', q > 0.02 ? 'inline' : 'none');
      });
    }
    ev(wPicks, 'tick', 0.2);
    const c1 = Math.max(wTool + 0.3, wPicks + 1.0);
    H.fadeOut(W.fork.user.n, c1 + 0.2, 0.4);
    H.fadeOut(W.fork.tool.n, c1 + 0.9, 0.5);

    // Turn 1, followed by the camera: get_user_details, and its result.
    const specs = [
      { tool: 'get user', arg: 'user_7' }, { tool: 'get order', arg: '#W7a' }, { tool: 'get order', arg: '#W7b' },
      { tool: 'reply', kind: 'reply' }, { tool: 'cancel order', arg: '#W7a', kind: 'write' },
      { kind: 'reply' }, { kind: 'read' }, { kind: 'read' }, { kind: 'reply' }, { kind: 'read' }, { kind: 'write' }, { kind: 'reply' }, { kind: 'read' }, { kind: 'reply' },
    ];
    st.chips = specs.map((sp, i) => W.timeline.add({ tool: sp.tool || '', arg: sp.arg, kind: sp.kind || 'read' }, i));
    const chipIn = (i, t) => { const c = st.chips[i].n; c.key('o', [t, 0], [t + 0.3, 1, 'out2']); c.key('dy', [t, 22], [t + 0.7, 0, 'spring']); };
    st.res1 = { dir: '←', title: 'get_user_details', kind: 'result', sub: [['{"orders":["', null], ['#W7a', C.petrolText], ['","', null], ['#W7b', C.petrolText], ['"],…}', null]] };
    const one = callAndResult(ctx, st, { t: c1, dur: 1.45, drawer: 0, big: true, call: { dir: '→', title: 'get_user_details', sub: '{"user_id":"user_7"}' }, result: st.res1 });
    W.scroll.push('call', c1 + 0.3);
    chipIn(0, c1);
    ev(c1, 'swoosh-in', 0.32);
    ev(c1 + 1.3, 'click', 0.28);
    st.hudN.key('o', [c1 + 0.2, 0], [c1 + 0.8, 1, 'out2']);
    st.hudAdd(c1 + 0.2, 1, 0, 0);
    st.hudAdd(Math.max(wSeconds, c1 + 0.9), 1, 6, 0);
    st.hudAdd(Math.max(wTokens, c1 + 1.5), 1, 6, 4200);
    ev(Math.max(wSeconds, c1 + 0.9), 'tick', 0.22);
    ev(Math.max(wTokens, c1 + 1.5), 'tick', 0.22);
    cam.go(c1 - 0.25, 1.5, { x: 180, y: 20, z: 1.3 }, 'smooth', 'io3');
    cam.go(c1 + 1.95, 1.5, { x: -110, y: 40, z: 1.12 }, 'smooth', 'io3');

    // Then the turns repeat, faster: one task, dozens of turns.
    const m0 = Math.max(one.back + 0.15, wTokens + 0.35);
    const m1 = Math.min(S.turns.t1 - 0.7, Math.max(S.turns.vend + 0.35, m0 + 5.6));
    cam.go(m0 - 0.3, 2.9, { x: -15, y: 140, z: 1.0 }, 'smooth', 'io3');
    const plan = [
      ['read', 1, 'get_order_details', '{…}'], ['read', 1, 'get_order_details', '{…}'], ['reply'], ['write', 0, 'cancel_pending_order'],
      ['reply'], ['read', 2, 'get_product_details', '{…}'], ['read', 1, 'get_order_details', '{…}'], ['reply'],
      ['read', 2, 'get_product_details', '{…}'], ['write', 0, 'modify_pending_order'], ['reply'], ['read', 0, 'get_user_details', '{…}'], ['reply'],
    ];
    let time = 6, tokens = 4200;
    const rnd = E.rng(7);
    const tAt = j => L(m0, m1, Math.pow(j / plan.length, 0.72));
    plan.forEach(([kind, drawer, call, result], j) => {
      const tj = tAt(j), dj = Math.min(1.5, (tAt(j + 1) - tj) * 1.6);
      st.thinks.push([tj - 0.05, tj + 0.3 * dj, 0.8]);
      const launch = tj + 0.28 * dj;
      if (kind === 'reply') {
        const bn = flyer(H.pellet('reply'));
        const [ax, ay] = W.P.agent;
        H.flight(bn, [
          { t: launch, x: ax - 60, y: ay - 70, s: 0.5, o: 0 },
          { t: launch + 0.15 * dj, x: ax - 110, y: ay - 100, s: 1, o: 1, ease: 'out2' },
          { t: launch + 0.75 * dj, x: cx + 40, y: cy - 120, s: 0.7, o: 0, ease: 'io3', arc: 50 },
        ]);
        W.scroll.push('reply', launch + 0.5 * dj);
      } else {
        callAndResult(ctx, st, { t: launch, dur: 0.72 * dj, drawer, call, result: kind === 'write' ? null : result, kind });
        W.scroll.push(kind === 'write' ? 'write' : 'call', launch + 0.2 * dj);
      }
      chipIn(j + 1, launch);
      time += 5 + Math.round(rnd() * 4);
      tokens += 3600 + 520 * (j + 1) + Math.round(rnd() * 900);
      st.hudAdd(launch + 0.1, j + 2, time, tokens);
      ev(launch, 'tick', Math.max(0.1, 0.24 - j * 0.011));
    });
    st.turnsEnd = { time, tokens };
    // The customer answers the agent's question (turn 4).
    const yesT = tAt(3) + 0.3;
    const yes = A.speech(['Yes, #W7a.'], C.paper, '#0e1416');
    W.customer.g.appendChild(yes.g);
    const yesN = N(yes.g, { y: -84, o: 0 });
    H.popIn(yesN, yesT, 0.6, 0.6);
    H.fadeOut(bub, yesT - 0.2, 0.2);
    W.scroll.push('user', yesT + 0.3);
    st.yesN = yesN;
  }

  // -------------------------------------------- the turns the tools decided
  // Freeze; the first result lifts out of the conversation; the camera dives
  // into it until it is the frame.
  function decided(ctx, st) {
    const { S, word, cam, ev } = ctx;
    const D0 = S.decided.t0, I0 = S.idea.t0;
    const w = p => word('decided', p);
    const C0 = [-360, -318];
    const CW = 288, CH = 162, SC = CW / 1920;
    const cardG = G(W.tableau);
    const cardN = N(cardG, { x: C0[0], y: C0[1], o: 0 });
    const clip = s('clipPath', { id: 'clipDetail' }, s('rect', { x: -CW / 2, y: -CH / 2, width: CW, height: CH, rx: 10 }));
    const face = s('rect', { x: -CW / 2, y: -CH / 2, width: CW, height: CH, rx: 10, fill: '#121a1c', stroke: C.petrol, 'stroke-opacity': 0.8, 'stroke-width': 1.4 });
    const halo = s('rect', { x: -CW / 2 - 18, y: -CH / 2 - 18, width: CW + 36, height: CH + 36, rx: 26, fill: C.petrol, opacity: 0.1 });
    // The card as the tableau sees it: a result with two order ids.
    const summary = s('g', {},
      T('← get_user_details', { x: -CW / 2 + 18, y: -CH / 2 + 34, size: 17, mono: true, fill: C.petrolText }),
      T('{"user_id": "user_7",', { x: -CW / 2 + 18, y: -CH / 2 + 70, size: 15, mono: true, fill: C.muted }),
      TM([[' "orders": ["', C.muted], ['#W7a', C.petrolText], ['", "', C.muted], ['#W7b', C.petrolText], ['"],', C.muted]], { x: -CW / 2 + 18, y: -CH / 2 + 96, size: 15, mono: true }),
      T(' …}', { x: -CW / 2 + 18, y: -CH / 2 + 122, size: 15, mono: true, fill: C.muted }));
    // The card from inside: the record, laid out at 1920 x 1080, scaled into the card.
    const detail = s('g', {});
    const inner = s('g', { 'clip-path': 'url(#clipDetail)' }, detail);
    E.add(cardG, clip, halo, face, inner, summary);
    const sumN = N(summary), detN = N(detail, { s: SC });
    // Semantic zoom: the summary gives way to the detail as the camera nears.
    ctx.fn((t, v) => {
      const z = v.z;
      sumN.p.o = 1 - E.smooth(2.2, 2.9, z);
      detN.p.o = E.smooth(3.0, 3.9, z);
      E.attr(face, 'stroke-opacity', (0.8 * (1 - E.smooth(3, 5, z))).toFixed(3));
      E.attr(halo, 'opacity', (0.1 * (1 - E.smooth(2, 3.5, z))).toFixed(3));
    });

    // Inside: the user record, each order id on its own line.
    const X0 = -770, Y0 = -232, LH = 76, FS = 44, CWm = FS * 0.6;
    const lines = [
      [['{', C.muted]],
      [['  "user_id": ', C.subtle], ['"user_7"', C.text], [',', C.subtle]],
      [['  "name": ', C.subtle], ['"Mei Tan"', C.text], [',', C.subtle]],
      [['  "email": ', C.subtle], ['"c7@example.com"', C.text], [',', C.subtle]],
      [['  "orders": [', C.subtle]],
      [['    ', C.subtle], ['"#W7a"', C.text], [',', C.subtle]],
      [['    ', C.subtle], ['"#W7b"', C.text]],
      [['  ]', C.subtle]],
      [['}', C.muted]],
    ];
    E.add(detail,
      label('result of turn 1', X0 + 2, Y0 - 128, { anchor: 'start', size: 40, fill: C.subtle }),
      T('← get_user_details', { x: X0, y: Y0 - 66, size: 42, mono: true, fill: C.petrolText }));
    const idLine = [5, 6];
    const hls = idLine.map(li => {
      const r = s('rect', { x: X0 + 4 * CWm - 8, y: Y0 + li * LH - FS * 0.84, width: 6 * CWm + 13, height: FS * 1.18, rx: 11, fill: C.soft, stroke: C.petrol, 'stroke-width': 3 });
      detail.appendChild(r);
      return r;
    });
    const idTexts = [];
    lines.forEach((parts, i) => {
      const el = TM(parts, { x: X0, y: Y0 + i * LH, size: FS, mono: true });
      detail.appendChild(el);
      if (idLine.includes(i)) idTexts.push(el.childNodes[1]);
    });
    const wTwo = w('two orders');
    hls.forEach((r, i) => {
      const n = N(r, { o: 0 });
      const a = wTwo + i * 0.28;
      n.key('o', [a, 0], [a + 0.3, 1, 'out2']);
      ctx.fn(t => E.attr(idTexts[i], 'fill', E.mixColor(C.text, C.petrolText, E.P(t, a, 0.3))));
      ev(a, 'pop', 0.28);
    });
    // The next two turns, ghosted, level with the ids that decide them.
    const wNext = w('next two turns');
    const GX = 150, GW = 680, GH = 64;
    const ghosts = idLine.map((li, i) => {
      const cy = Y0 + li * LH - 14, y = cy - GH / 2;
      const g = s('g', {},
        s('rect', { x: GX, y, width: GW, height: GH, rx: 18, fill: '#0f1517', stroke: '#56646a', 'stroke-width': 3, 'stroke-dasharray': '14 12' }),
        T(`turn ${i + 2}`, { x: GX + 26, y: cy + 13, size: 36, weight: 640, fill: C.subtle, ls: 0.12, caps: true }));
      // The call the model types takes the label's place.
      const txt = T('', { x: GX + 26, y: cy + 14, size: 40, mono: true, fill: C.muted });
      g.appendChild(txt);
      detail.appendChild(g);
      const n = N(g, { o: 0 });
      H.fadeIn(n, wNext + i * 0.22, 0.5, 0);
      n.key('dx', [wNext + i * 0.22, 40], [wNext + i * 0.22 + 0.7, 0, 'out3']);
      return { g, txt, lab: g.childNodes[1], n, cy };
    });
    ghosts.forEach((gh, i) => {
      const x0 = X0 + 11 * CWm + 14, x1 = GX - 16, y = gh.cy;
      const p = s('path', { d: `M ${x0} ${y} C ${x0 + 160} ${y}, ${x1 - 160} ${y}, ${x1} ${y}`, fill: 'none', stroke: C.petrol, 'stroke-width': 4, 'stroke-linecap': 'round' });
      const dot = s('circle', { cx: x0, cy: y, r: 7, fill: C.petrol });
      const head = s('path', { d: `M ${x1 - 16} ${y - 12} L ${x1} ${y} L ${x1 - 16} ${y + 12}`, fill: 'none', stroke: C.petrol, 'stroke-width': 4, 'stroke-linecap': 'round', 'stroke-linejoin': 'round' });
      E.add(detail, p, dot, head);
      const a = wNext + 0.3 + i * 0.25;
      H.drawOn(ctx, p, a, 0.6, 'io3');
      const hn = N(head, { o: 0 }), dn = N(dot, { o: 0 });
      hn.key('o', [a + 0.5, 0], [a + 0.62, 1]);
      dn.key('o', [a, 0], [a + 0.1, 1]);
    });
    ev(wNext + 0.3, 'swoosh-in', 0.2);
    // The label: the tools made that choice.
    const wTools = w('The tools made');
    const lab = T('decided by the tools', { x: GX, y: Y0 + 8 * LH - 2, size: 50, weight: 640, fill: C.petrolText, ls: -0.015 });
    detail.appendChild(lab);
    H.fadeIn(N(lab, { o: 0 }), wTools, 0.6, 20);
    // The customer, who did not decide them.
    const cust = s('g', {},
      s('circle', { r: 46, fill: '#141b1e', stroke: '#33434a', 'stroke-width': 3 }),
      s('circle', { cy: -12, r: 17, fill: '#c3cbcd' }),
      s('path', { d: 'M -34 40 C -32 12 -18 8 0 8 C 18 8 32 12 34 40 Z', fill: '#c3cbcd' }),
      s('rect', { x: 76, y: -38, width: 440, height: 76, rx: 22, fill: C.paper }),
      T('Please cancel my pending order.', { x: 296, y: 9, size: 26, weight: 520, fill: '#0e1416', anchor: 'middle' }),
      T('the customer asked for this', { x: 80, y: 86, size: 27, weight: 520, fill: C.subtle }));
    detail.appendChild(cust);
    const custN = N(cust, { x: GX + 50, y: -176, o: 0 });
    H.fadeIn(custN, Math.min(wNext - 0.6, wTwo + 0.9), 0.5, 0);
    const wNot = w('not the customer');
    custN.key('o', [wNot + 0.1, 1], [wNot + 0.7, 0.22, 'io2']);
    // The model spends a turn typing each one out.
    const wType = w('spends a turn');
    ghosts.forEach((g, i) => {
      const full = `get_order_details("#W7${i ? 'b' : 'a'}")`;
      const a = wType + 0.15 + i * 0.9;
      H.type(ctx, g.txt, full, a, 30, true, i ? Infinity : a + 1.1);
      ctx.fn(t => E.attr(g.txt, 'fill', E.mixColor(C.muted, C.text, E.P(t, a, 0.4))));
      ctx.fn(t => E.attr(g.lab, 'opacity', (1 - E.P(t, a - 0.7, 0.28)).toFixed(3)));
      ev(a, 'click', 0.12);
    });

    // The card lifts out of the conversation, and the camera dives in.
    H.flight(cardN, [
      { t: D0 + 0.05, x: W.P.scroll[0], y: W.P.scroll[1] - 40, s: 0.22, o: 0 },
      { t: D0 + 0.95, x: C0[0], y: C0[1], s: 1, o: 1, ease: 'springSoft' },
      { t: I0 + 1.6, x: C0[0], y: C0[1], s: 1, o: 1, ease: 'lin' },
      { t: I0 + 2.15, x: W.P.scroll[0], y: W.P.scroll[1] - 30, s: 0.2, o: 0, ease: 'in3' },
    ]);
    ev(D0 + 0.1, 'pop', 0.24);
    H.fadeOut(st.hudN, D0 + 0.2, 0.4);
    const dive0 = D0 + 0.7;
    cam.go(dive0, 2.35, { x: C0[0], y: C0[1], z: 6.95, r: 0 }, 'dive', 'io4', { ez: 'io3' });
    cam.go(dive0 + 2.4, Math.max(1, I0 - dive0 - 2.5), { x: C0[0] + 3, y: C0[1] + 2, z: 7.1, r: 0 }, 'smooth', 'io2');
    ev(dive0, 'whoosh-soft', 0.38);
    ev(dive0 + 2.2, 'thump', 0.2);
    H.during(ctx, cardG, D0 - 0.2, I0 + 2.3);
    // The world is quieter in there.
    ctx.setDrift(t => 1 - 0.75 * E.pulse(t, dive0 + 1.5, I0 + 0.5, 0.8, 0.8));
  }

  // --------------------------------------------------------------- the idea
  function idea(ctx, st) {
    const { S, word, cam, ev } = ctx;
    const I0 = S.idea.t0, F0 = S.safe.t0;
    const w = p => word('idea', p);
    // Out of the card, back to the world.
    cam.go(I0 - 0.35, 2.3, { x: -40, y: 70, z: 1.04, r: 0 }, 'zoom', 'io3', { rho: 1.0 });
    ev(I0 - 0.3, 'whoosh-soft', 0.32);
    // The episode rewinds to the customer's request, for a replay with stretto.
    const rew = I0 + 1.0;
    for (const e of W.scroll.entries) if (e.tIn > S.turns.t0 + 2.4 && e.tOut === Infinity) e.tOut = rew + 0.3;
    H.popIn(W.customer.bubbleN, rew, 0.7, 0.8);
    H.fadeOut(st.yesN, rew - 0.25, 0.25);

    // stretto drops in between the agent and the server.
    const wSits = Math.max(w('stretto sits'), I0 + 1.5);
    const pn = W.proxy.n;
    pn.key('o', [wSits, 0], [wSits + 0.12, 1, 'lin']);
    pn.key('dy', [wSits, -760], [wSits + 1.0, 0, 'springHard']);
    ev(wSits + 0.3, 'thump', 0.42);
    ev(wSits + 0.4, 'shimmer', 0.28);
    st.proxyHits.push(wSits + 0.35);
    const mcpTag = label('MCP proxy', 0, 220, { size: 26, fill: C.subtle });
    W.proxy.g.appendChild(mcpTag);
    H.fadeIn(N(mcpTag, { o: 0 }), Math.max(w('MCP proxy'), wSits + 0.8), 0.5, 8);

    // The replay of turn 1, through stretto.
    const c = Math.max(w('When a result'), wSits + 1.3);
    st.thinks.push([c - 0.7, c + 0.05, 0.9]);
    const call = flyer(A.card({ dir: '→', title: 'get_user_details', sub: '{"user_id":"user_7"}' }).g);
    const [px] = W.P.proxy;
    H.flight(call, [
      { t: c, x: agentOut[0] - 10, y: agentOut[1], s: 0.55, o: 0 },
      { t: c + 0.2, x: agentOut[0] + 40, y: agentOut[1], s: 1, o: 1, ease: 'out3' },
      { t: c + 0.75, x: px, y: W.laneY.call, s: 0.9, o: 1, ease: 'io3' },
      { t: c + 1.2, x: serverIn[0] - 150, y: W.laneY.call, s: 1, o: 1, ease: 'io3' },
      { t: c + 1.45, x: drawerAt(0)[0], y: drawerAt(0)[1], s: 0.3, o: 0, ease: 'in2' },
    ]);
    st.proxyHits.push(c + 0.62);
    st.access.push({ i: 0, t: c + 1.3, d: 1.3 });
    ev(c, 'swoosh-in', 0.28);
    W.scroll.push('call', c + 0.3);
    // The result comes back to stretto, which reads the ids in it.
    const r = c + 1.65;
    const { g: bundleG, riderNs } = H.bundle(st.res1);
    const bn = flyer(bundleG);
    // stretto holds the result just above itself while it reads.
    const hold = [px - 70, -292];
    const wReads = Math.max(w('makes those reads'), r + 1.0);
    const wInside = Math.max(w('inside the result'), wReads + 2.0);
    const si = scrollIn();
    H.flight(bn, [
      { t: r, x: drawerAt(0)[0], y: drawerAt(0)[1], s: 0.3, o: 0 },
      { t: r + 0.25, x: serverOut[0] - 150, y: W.laneY.result, s: 1, o: 1, ease: 'out3' },
      { t: r + 0.95, x: hold[0], y: hold[1], s: 1, o: 1, ease: 'io3', arc: 40 },
      { t: wInside, x: hold[0], y: hold[1], s: 1, o: 1, ease: 'lin' },
      { t: wInside + 1.05, x: agentIn[0] + 150, y: W.laneY.result + 6, s: 1, o: 1, ease: 'io3', arc: 30 },
      { t: wInside + 1.45, x: si[0], y: si[1] + 10, s: 0.2, o: 0, ease: 'in3' },
    ]);
    st.proxyHits.push(r + 0.85);
    ev(r + 0.9, 'click', 0.26);
    // stretto makes the two reads at once: lookups to the orders drawer.
    const [odx, ody] = drawerAt(1);
    ['get_order_details #W7a', 'get_order_details #W7b'].forEach((text, i) => {
      const n = flyer(A.lookup(text).g);
      const a = wReads + i * 0.2;
      H.flight(n, [
        { t: a, x: px + 20, y: -20 + i * 10, s: 0.4, o: 0 },
        { t: a + 0.2, x: px + 130, y: -70 + i * 50, s: 1, o: 1, ease: 'out3' },
        { t: a + 0.7, x: odx - 70, y: ody - 14 + i * 16, s: 0.9, o: 1, ease: 'io3' },
        { t: a + 0.9, x: odx + 10, y: ody, s: 0.3, o: 0, ease: 'in2' },
      ]);
      st.access.push({ i: 1, t: a + 0.75, d: 1.1 });
      ev(a, 'pop', 0.32);
      // Their results come back and clip under the result card.
      const rn = riderNs[i];
      const b0 = a + 1.0;
      rn.key('o', [b0, 0], [b0 + 0.25, 1, 'out2']);
      rn.key('dx', [b0, (odx - hold[0]) * 0.9], [b0 + 0.7, 0, 'io3']);
      rn.key('dy', [b0, ody - hold[1] - 60], [b0 + 0.7, i * 54, 'io3']);
      ev(b0 + 0.65, 'click', 0.28);
    });
    st.proxyHits.push(wReads - 0.05);
    ctx.poster = wInside - 0.3;
    ev(wInside, 'swoosh-in', 0.32);
    W.scroll.push('bundle', wInside + 1.35);
    W.scroll.push('rider', wInside + 1.42);
    W.scroll.push('rider', wInside + 1.49);
    st.thinks.push([wInside + 1.4, wInside + 2.2, 0.7]);

    // The turns close up: 2 and 3 are not taken, the rest move left.
    const wSkips = Math.max(w('skips those turns'), wInside + 1.2);
    cam.go(wSkips - 1.25, 1.35, { x: -360, y: 190, z: 1.1 }, 'smooth', 'io3');
    // Back out once the turns have closed up, so the tags below land in the
    // wide shot, clear of the HUD.
    const wNoNew = w('no new tools');
    const out0 = Math.min(F0 - 2.2, Math.max(wSkips + 1.5, wNoNew - 1.8));
    cam.go(out0, 1.6, { x: -60, y: 90, z: 1.02 }, 'smooth', 'io3');
    // The HUD comes back for the count: 14 turns become 12.
    H.hold(st.hudN, 'o', wSkips - 1.0);
    st.hudN.key('o', [wSkips - 1.0, 0], [wSkips - 0.5, 1, 'out2']);
    const chips = st.chips;
    [1, 2].forEach((i, k) => {
      const c = chips[i];
      ctx.fn(t => E.attr(c.ring, 'opacity', E.pulse(t, wSkips - 0.55 + k * 0.1, wSkips + 0.35, 0.2, 0.25).toFixed(3)));
      H.fadeOut(c.n, wSkips + 0.05 + k * 0.08, 0.45);
      c.n.key('s', [wSkips + 0.05 + k * 0.08, 1], [wSkips + 0.5 + k * 0.08, 0.86, 'in2']);
      ev(wSkips + 0.05 + k * 0.08, 'tick', 0.24);
    });
    let x = W.TL.x0 + (W.TL.w + W.TL.gap);
    chips.slice(3).forEach((c, k) => {
      const a = wSkips + 0.38 + k * 0.05;
      c.n.key('x', [a, c.n.base.x], [a + 0.75, x, 'io3']);
      const was = c.w > 80 ? `TURN ${k + 4}` : String(k + 4), now = c.w > 80 ? `TURN ${k + 2}` : String(k + 2);
      ctx.fn(t => E.text(c.num, t >= a + 0.38 ? now : was));
      x += c.w + (c.w > 80 ? W.TL.gap : 12);
    });
    ev(wSkips + 0.38, 'swoosh-in', 0.28);
    const tEnd = st.turnsEnd;
    st.hudAdd(wSkips - 1.0, 14, tEnd.time, tEnd.tokens);
    st.hudAdd(wSkips + 0.7, 12, tEnd.time - 13, tEnd.tokens - 14000, 'petrol');
    const saved = T('2 turns the agent no longer takes', { x: W.TL.x0 + 2, y: W.TL.y + 150, size: 28, weight: 600, fill: C.petrolText });
    W.timeline.g.el.appendChild(saved);
    const savedN = N(saved, { o: 0 });
    H.fadeIn(savedN, wSkips + 1.0, 0.5, 8);
    H.fadeOut(savedN, F0 - 0.6, 0.4);

    // No new tools; the same prompt.
    const tags = [['no new tools', W.P.agent[0] + 30, -236, Math.max(wNoNew, out0 + 1.4)], ['same prompt', W.P.scroll[0] - 10, -236, Math.max(w("prompt doesn't"), out0 + 1.7)]];
    st.tagNs = tags.map(([text, tx, ty, at]) => {
      const g = H.tag(text);
      W.tableau.appendChild(g);
      const n = N(g, { x: tx, y: ty, o: 0 });
      H.popIn(n, at, 0.7, 0.6);
      H.fadeOut(n, F0 + 0.1, 0.35);
      ev(at, 'pop', 0.24);
      return n;
    });
  }

  // ------------------------------------------------ what it cannot touch
  function safe(ctx, st) {
    const { S, word, cam, ev } = ctx;
    const F0 = S.safe.t0, L0 = S.learn.t0;
    const w = p => word('safe', p);
    cam.go(F0 - 0.35, 1.7, { x: 505, y: -12, z: 1.72, r: 0 }, 'smooth', 'io3');
    ev(F0 - 0.3, 'whoosh-soft', 0.24);
    // stretto's reach: a tendril from the lens to each drawer, and short of the lock.
    const reach = s('path', { d: '', fill: 'none', stroke: C.petrol, 'stroke-width': 4, 'stroke-linecap': 'round' });
    const tip = s('g', {}, s('circle', { r: 16, fill: 'url(#gHalo)' }), s('circle', { r: 6.5, fill: C.petrolText }));
    const rg = G(W.tableau);
    E.add(rg, reach, tip);
    const tipN = N(tip, { o: 0 });
    const wOnly = Math.max(w('only ever reads'), F0 + 0.9);
    const wNever = Math.max(w('never writes'), wOnly + 1.6);
    const [sx0, sy0] = [W.P.proxy[0] + 60, W.P.proxy[1] - 10];
    const ys = W.server.drawers.map(d => W.P.server[1] + d.cy);
    const writeY = W.P.server[1] + W.server.wy + W.server.wh / 2 - 6;
    const x1 = W.P.server[0] - W.server.W / 2 + 6;
    const keys = [[wOnly, ys[0]], [wOnly + 0.45, ys[1]], [wOnly + 0.9, ys[2]], [wNever, writeY]];
    keys.slice(0, 3).forEach(([t], i) => { st.access.push({ i, t: t + 0.05, d: 0.9 }); ev(t + 0.05, 'tick', 0.24); });
    st.locks.push(wNever + 0.28);
    st.writes.push({ t: wNever + 0.15, d: 1.5 });
    ev(wNever + 0.28, 'click', 0.48);
    ctx.fn(t => {
      const grow = E.P(t, wOnly - 0.35, 0.4, 'out3') * (1 - E.P(t, wNever + 0.5, 0.55, 'in3'));
      if (grow <= 0.001) { E.attr(reach, 'display', 'none'); tipN.p.o = 0; return; }
      let y = keys[0][1];
      for (let i = 1; i < keys.length; i++) y = L(y, keys[i][1], E.P(t, keys[i][0] - 0.3, 0.3, 'io3'));
      // Short of the door it stops: the lock does not let it in.
      const stop = E.P(t, wNever - 0.3, 0.35, 'io3');
      const ex = L(sx0, x1 - 6 - 26 * stop, grow), ey = L(sy0, y, grow);
      E.attr(reach, 'd', `M ${sx0} ${sy0} C ${sx0 + 90} ${sy0}, ${(ex - 110).toFixed(1)} ${ey.toFixed(1)}, ${ex.toFixed(1)} ${ey.toFixed(1)}`);
      E.attr(reach, 'display', 'inline');
      tipN.p.x = ex; tipN.p.y = ey; tipN.p.o = 1;
    });
    // Reads and writes, beside the server.
    const readTag = label('reads', x1 - 70, ys[0] - 56, { size: 24, fill: C.petrolText });
    const writeTag = label('writes', x1 - 76, writeY + 76, { size: 24, fill: C.warm });
    E.add(W.tableau, readTag, writeTag);
    const rtN = N(readTag, { o: 0 }), wtN = N(writeTag, { o: 0 });
    H.fadeIn(rtN, wOnly, 0.5, 6); H.fadeIn(wtN, wNever, 0.5, 6);
    H.fadeOut(rtN, L0, 0.4); H.fadeOut(wtN, L0, 0.4);

    // A wrong guess: a lookup the agent does not use. A few tokens; nothing else.
    const wWrong = Math.max(w('guesses wrong'), wNever + 1.4);
    const wToks = Math.max(w('few extra tokens'), wWrong + 1.6);
    cam.go(wWrong - 0.2, 1.4, { x: -300, y: 6, z: 1.5 }, 'smooth', 'io3');
    ev(wWrong - 0.15, 'whoosh-soft', 0.2);
    const dn = flyer(A.lookup('get_product_details #P11').g);
    const si = scrollIn();
    H.flight(dn, [
      { t: wWrong + 0.1, x: W.P.proxy[0], y: W.laneY.result, s: 0.5, o: 0 },
      { t: wWrong + 0.3, x: W.P.proxy[0] - 60, y: W.laneY.result, s: 1, o: 1, ease: 'out3' },
      { t: wWrong + 1.25, x: -40, y: W.laneY.result, s: 1, o: 1, ease: 'io3', arc: -12 },
      { t: wWrong + 1.6, x: si[0], y: si[1] + 60, s: 0.25, o: 0, ease: 'in3' },
    ]);
    W.scroll.push('rider', wWrong + 1.55, wToks + 0.1);
    W.scroll.push('tokens', wToks);
    ev(wToks, 'pop', 0.18);
    // The rider breaks into a few tokens.
    const bits = G(W.tableau);
    const rnd = E.rng(99);
    for (let i = 0; i < 9; i++) {
      const b = s('rect', { x: -4, y: -4, width: 8, height: 8, rx: 2, fill: i % 3 ? C.petrolText : C.warm });
      bits.appendChild(b);
      const n = N(b, { o: 0 });
      const ox = si[0] + rnd.range(-50, 60), oy = si[1] + 60 + rnd.range(-10, 10);
      H.flight(n, [
        { t: wToks - 0.05, x: si[0] + 10, y: si[1] + 52, s: 0.4, o: 0 },
        { t: wToks + 0.25, x: ox, y: oy - rnd.range(30, 70), s: 1, o: 1, ease: 'out3' },
        { t: wToks + 1.1, x: ox + rnd.range(-10, 10), y: oy + 30, s: 0.6, o: 0, ease: 'in2' },
      ]);
    }
    const tokTag = H.tag('+ a few tokens', C.warm, C.warmSoft, false);
    W.tableau.appendChild(tokTag);
    const ttN = N(tokTag, { x: W.P.scroll[0] + 150, y: -206, o: 0 });
    H.popIn(ttN, wToks + 0.1, 0.7, 0.6);
    H.fadeOut(ttN, L0 - 0.2, 0.4);
    const tEnd = st.turnsEnd;
    st.hudAdd(wToks + 0.1, 12, tEnd.time - 13, tEnd.tokens - 14000 + 400, 'warm');
    H.fadeOut(st.hudN, L0 + 0.2, 0.5);
  }

  const Scenes = window.Scenes || (window.Scenes = {});
  Object.assign(Scenes, { H, shared, open, turns, decided, idea, safe, callAndResult });
})();
