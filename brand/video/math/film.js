// The math film's chapters. Each formula is the paper's (paper/stretto.md:
// the event §2.3, Proposition 1, Propositions 3 and 4, the habit §2.4) or the
// code's (alpha's posterior in crates/stretto-model/src/alpha.rs, the
// binding's chance in crates/stretto-report/src/flow.rs, the audit's program
// in crates/stretto-report/src/audit.rs), typeset with KaTeX, each symbol in
// its own color and its terms named where they sit.
//
// What the charts draw is data, not drawing: window.MATH (data.js, written by
// data/results.py) holds the reach round's published rows (the paper's
// Figures 6, 7 and 9 and Table 5) and fugue's own run of alpha's posterior on
// the quickstart's six sessions (data/, stretto-model's code). The numbers
// typed into the frame are the paper's and the claims ledger's, each with its
// scope. Times are words of the voice-over, never hand-typed seconds.
(() => {
  const { C, E, ramp, clamp, lerp, el, box, show, card, chip, arrow, code, markSvg, wordmarkSvg } = window.ST;
  const M = window.MATH;

  const TITLES = {
    event: 'The event', safety: 'Reads only', rule: 'The rule', habit: 'The counts', fugue: 'Alpha, with fugue',
    bindings: 'Arguments', calibration: 'Calibration', sweep: 'The threshold', live: 'Live', audit: 'The audit',
  };
  // The symbols' colors (index.html's --t-*): probability, value, cost, prior.
  const T = { p: '#5dd2d8', b: '#9fb3f7', d: '#d9a35b', a: '#c3a8f0', n: C.text, base: '#7d888b' };

  // ------------------------------------------------------------ helpers
  const scene = (ctx, key, build, opt = {}) => {
    const b = ctx.B[key];
    ctx.scene({ key, t0: b.t0, t1: b.t1, build: layer => build(layer, b), ...opt });
  };
  const svgOf = layer => el('svg', { width: 1920, height: 1080, viewBox: '0 0 1920 1080' }, layer);
  const note = (parent, text, { x, y, w, size = 18, color = C.faint, ax = 0, ay = 0 } = {}) =>
    box(parent, { x, y, w, ax, ay, cls: 'note', html: text, style: { fontSize: `${size}px`, color, lineHeight: '1.45' } });
  const label = (parent, text, { x, y, ax = 0, ay = 0, color = C.subtle, size = 17 } = {}) =>
    box(parent, { x, y, ax, ay, cls: 'label', html: text, style: { color, fontSize: `${size}px` } });
  const P = s => `<span style="color:${T.p}">${s}</span>`;
  const D = s => `<span style="color:${C.faint}">${s}</span>`;

  /** A formula, typeset by KaTeX. `\htmlClass{tp k-q}{q}` colors a term and names it for annotate(). */
  function tex(parent, src, { x, y, size = 56, ax = 0, ay = 0 } = {}) {
    const html = window.katex.renderToString(src, { trust: true, strict: 'ignore', throwOnError: true, output: 'html' });
    return box(parent, { x, y, ax, ay, html, style: { fontSize: `${size}px`, whiteSpace: 'nowrap', lineHeight: '1.2' } });
  }
  const stageEl = () => document.getElementById('stage');
  /** A node's box in frame pixels. */
  function rect(n) {
    const r = n.getBoundingClientRect(), s = stageEl().getBoundingClientRect();
    return { x: r.left - s.left, y: r.top - s.top, w: r.width, h: r.height };
  }
  /**
   * Name a term of a formula: a bracket under (or over) it, a leader, and a
   * label. `key` is the term's k-class. Measured once, when the formula is
   * set (before it moves), so call it right after tex().
   */
  function annotate(layer, svg, eq, key, text, { pos = 'below', dx = 0, dist = 64, color = C.muted, w = null, align = 0.5 } = {}) {
    const t = eq.querySelector(`.k-${key}`);
    if (!t) throw new Error(`math: no term k-${key}`);
    const b = rect(t);
    const below = pos === 'below';
    const ey = below ? b.y + b.h + 8 : b.y - 8;
    const tick = below ? -7 : 7;
    const br = arrow(svg, `M ${b.x + 2} ${ey + tick} L ${b.x + 2} ${ey} L ${b.x + b.w - 2} ${ey} L ${b.x + b.w - 2} ${ey + tick}`, { color, width: 1.6, head: false });
    const mx = b.x + b.w / 2, lx = mx + dx, ly = below ? ey + dist : ey - dist;
    const lead = arrow(svg, `M ${mx} ${ey} C ${mx} ${(ey + ly) / 2}, ${lx} ${(ey + ly) / 2}, ${lx} ${below ? ly - 6 : ly + 6}`, { color, width: 1.3, head: false });
    const lab = box(layer, { x: lx, y: ly, ax: align, ay: below ? 0 : 1, w, cls: `anno${w ? ' wrap' : ''}`, html: text, style: { color } });
    return {
      set(tt, at, out = Infinity) {
        const o = 1 - ramp(tt, out, out + 0.35);
        br.set(ramp(tt, at, at + 0.35), o);
        lead.set(ramp(tt, at + 0.2, at + 0.6), o);
        show(lab, tt, at + 0.4, { d: 0.4, dy: below ? 6 : -6, out });
      },
    };
  }
  /** Light a term of a formula (a soft box behind it) while on. */
  function glow(eq, key, on, color = T.p) {
    const t = eq.querySelector(`.k-${key}`);
    if (!t) return;
    t.style.borderRadius = '6px';
    t.style.background = on ? `${color}22` : 'transparent';
    t.style.boxShadow = on ? `0 0 0 6px ${color}22` : 'none';
  }
  /**
   * One formula becoming the next, as a derivation is worked on a board: the
   * terms the two share (class m-<key> in both) travel from where they sit
   * in `a` to where they sit in `b`, while the rest of `a` fades out and the
   * rest of `b` fades in. Measured once, when both are set. Before t0 it
   * leaves `a` to its caller; from t0 it owns both. `keep` leaves `a` up (a
   * term copied out of it rather than moved).
   */
  function morph(layer, a, b, keys, { keep = false } = {}) {
    const parts = keys.map(k => {
      const ta = a.querySelector(`.m-${k}`), tb = b.querySelector(`.m-${k}`);
      if (!ta || !tb) throw new Error(`math: morph has no term m-${k}`);
      const ra = rect(ta), rb = rect(tb);
      const w = el('div', { class: 'katex' }, layer);
      Object.assign(w.style, { position: 'absolute', left: `${rb.x}px`, top: `${rb.y}px`, fontSize: b.style.fontSize, whiteSpace: 'nowrap', transformOrigin: '0 0', lineHeight: '1.2' });
      w.appendChild(tb.cloneNode(true));
      const rc = rect(w.firstChild);
      w.style.left = `${rb.x + (rb.x - rc.x)}px`;
      w.style.top = `${rb.y + (rb.y - rc.y)}px`;
      w.style.display = 'none';
      return { w, ta, tb, dx: ra.x - rb.x, dy: ra.y - rb.y, s0: ra.h / Math.max(1, rb.h) };
    });
    return {
      set(t, t0, d = 1.1) {
        const u = ramp(t, t0, t0 + d, E.io3);
        if (t < t0) {
          b.style.opacity = '0'; b.style.visibility = 'hidden';
          for (const p of parts) { p.w.style.display = 'none'; p.ta.style.visibility = ''; }
          return;
        }
        const moving = u < 1;
        if (!keep) { a.style.opacity = (1 - ramp(u, 0, 0.3)).toFixed(3); a.style.visibility = u >= 0.3 ? 'hidden' : ''; }
        b.style.visibility = ''; b.style.opacity = ramp(u, 0.6, 1).toFixed(3); b.style.transform = '';
        for (const p of parts) {
          if (!keep) p.ta.style.visibility = moving ? 'hidden' : '';
          p.tb.style.visibility = moving ? 'hidden' : '';
          p.w.style.display = moving ? '' : 'none';
          const sc = lerp(p.s0, 1, u);
          p.w.style.transform = `translate(${(p.dx * (1 - u)).toFixed(2)}px, ${(p.dy * (1 - u)).toFixed(2)}px) scale(${sc.toFixed(4)})`;
        }
      },
    };
  }
  /** A formula re-typeset each frame, for numbers that follow a moving value. */
  const live = (n, src) => { n.innerHTML = window.katex.renderToString(src, { trust: true, strict: 'ignore', output: 'html' }); };
  /** A chart's frame: scales, a baseline, ticks and axis labels, in an SVG. */
  function plot(svg, layer, { x, y, w, h, xd, yd, xt = [], yt = [], xl = '', yl = '', xf = v => v, yf = v => v, grid = true }) {
    const sx = v => x + (w * (v - xd[0])) / (xd[1] - xd[0]);
    const sy = v => y + h - (h * (v - yd[0])) / (yd[1] - yd[0]);
    const g = el('g', {}, svg);
    if (grid) for (const v of yt) el('line', { x1: x, x2: x + w, y1: sy(v), y2: sy(v), stroke: C.border, 'stroke-width': 1 }, g);
    el('line', { x1: x, x2: x + w, y1: y + h, y2: y + h, stroke: C.borderStrong, 'stroke-width': 1.5 }, g);
    el('line', { x1: x, x2: x, y1: y, y2: y + h, stroke: C.borderStrong, 'stroke-width': 1.5 }, g);
    for (const v of xt) el('text', { x: sx(v), y: y + h + 28, class: 'axis', 'text-anchor': 'middle' }, g, xf(v));
    for (const v of yt) el('text', { x: x - 14, y: sy(v) + 5, class: 'axis', 'text-anchor': 'end' }, g, yf(v));
    const xlab = label(layer, xl, { x: x + w, y: y + h + 48, ax: 1 });
    const ylab = label(layer, yl, { x, y: y - 34 });
    return {
      sx, sy, g, xlab, ylab,
      set(t, at, out = Infinity) {
        g.style.opacity = (ramp(t, at, at + 0.5) * (1 - ramp(t, out, out + 0.35))).toFixed(3);
        show(xlab, t, at + 0.2, { d: 0.4, dy: 0, out });
        show(ylab, t, at + 0.2, { d: 0.4, dy: 0, out });
      },
    };
  }
  const line = (pts, sx, sy) => pts.map(([a, b], i) => `${i ? 'L' : 'M'} ${sx(a).toFixed(1)} ${sy(b).toFixed(1)}`).join(' ');
  /** Dots that pop in one after another. */
  function dots(svg, pts, { r = 7, color = C.text, stroke = C.bg } = {}) {
    const g = el('g', {}, svg);
    const cs = pts.map(([x, y, rr]) => el('circle', { cx: x, cy: y, r: rr ?? r, fill: color, stroke, 'stroke-width': 2 }, g));
    return {
      g, cs,
      set(t, at, step = 0.06, out = Infinity) {
        cs.forEach((c, i) => {
          const u = ramp(t, at + i * step, at + i * step + 0.3, E.outBack) * (1 - ramp(t, out, out + 0.35));
          c.setAttribute('opacity', clamp(u).toFixed(3));
          c.setAttribute('transform', `translate(${c.getAttribute('cx')} ${c.getAttribute('cy')}) scale(${Math.max(0, u).toFixed(3)}) translate(${-c.getAttribute('cx')} ${-c.getAttribute('cy')})`);
        });
      },
    };
  }
  const fmt = (v, d = 2) => v.toFixed(d);

  // ================================================================ title
  function sceneTitle(ctx) {
    scene(ctx, 'title', (layer, b) => {
      const T0 = b.t0 + 0.3;
      const g = box(layer, { x: 960, y: 400, ax: 0.5, ay: 0.5, html: `<div style="display:flex;align-items:center;gap:26px">${markSvg(84)}${wordmarkSvg(54)}</div>` });
      const h = box(layer, { x: 960, y: 540, ax: 0.5, ay: 0.5, text: 'The math, and the probabilistic programs', style: { font: '600 56px/1.1 Inter, sans-serif', letterSpacing: '-0.03em', color: C.text, whiteSpace: 'nowrap' } });
      const bars = g.querySelectorAll('rect');
      ctx.music.open = [0, 0.35, 0.65].map(e => +(T0 + e).toFixed(3));
      return t => {
        bars.forEach((r, i) => {
          const u = ramp(t, T0 + [0, 0.35, 0.65][i], T0 + [0, 0.35, 0.65][i] + 0.7, E.out5);
          r.setAttribute('transform', `translate(${r.getAttribute('x')} 0) scale(${u.toFixed(4)} 1) translate(${-r.getAttribute('x')} 0)`);
        });
        show(g, t, T0, { d: 0.3, dy: 0 });
        show(h, t, T0 + 1.1, { d: 0.8, dy: 12 });
      };
    }, { fin: 0 });
  }

  // ================================================================ event
  // An episode's steps; the lookup made after the user's details; the reads
  // before the next write (U(x)); then q, typeset and named term by term;
  // then the paper's two measured gaps between "next" and "before the write".
  function sceneEvent(ctx) {
    scene(ctx, 'event', (layer, b) => {
      const at = ctx.at;
      const svg = svgOf(layer);
      const steps = [['get_user_details', 'call'], ['reply', 'ghost'], ['get_order_details', 'call'], ['cancel_pending_order', 'write']];
      let x = 250;
      const chips = steps.map(([s, k]) => {
        const c = chip(layer, s, { x, y: 210, ay: 0.5, kind: k, size: 24 });
        const w = c.getBoundingClientRect().width;
        const o = { c, w, x0: x };
        x += w + 86;
        return o;
      });
      const links = chips.slice(1).map((ch, i) => arrow(svg, `M ${chips[i].x0 + chips[i].w + 12} 210 L ${ch.x0 - 12} 210`, { color: C.faint }));
      const decide = chips[0].x0 + chips[0].w + 43;
      const tick = box(layer, { x: decide - 1, y: 150, w: 2, h: 112, style: { background: T.p } });
      const now = label(layer, 'a lookup is decided here', { x: decide, y: 128, ax: 0.5, color: T.p });
      const bx0 = chips[1].x0 - 16, bx1 = chips[2].x0 + chips[2].w + 16;
      const bracket = arrow(svg, `M ${bx0} 252 L ${bx0} 266 L ${bx1} 266 L ${bx1} 252`, { color: T.p, width: 2, head: false });
      const uLab = box(layer, { x: (bx0 + bx1) / 2, y: 280, ax: 0.5, cls: 'anno', html: `${P('U(x)')}: what the agent does before its next write`, style: { color: C.muted } });
      const wLab = label(layer, 'the next write', { x: chips[3].x0 + chips[3].w / 2, y: 262, ax: 0.5, color: T.d });

      // q, term by term.
      const q = tex(layer, String.raw`\htmlClass{tp k-q m-q}{q_c} \;=\; \Pr\big(\, \htmlClass{tn k-c}{c} \in \htmlClass{tp k-U}{U(x)} \;\big|\; \htmlClass{tm k-x}{x} \,\big)`, { x: 960, y: 470, ax: 0.5, ay: 0.5, size: 72 });
      const aq = annotate(layer, svg, q, 'q', 'the probability of use', { pos: 'above', dist: 48, color: T.p, dx: -40 });
      const ac = annotate(layer, svg, q, 'c', 'a candidate read', { pos: 'below', dist: 56, dx: -60 });
      const aU = annotate(layer, svg, q, 'U', 'the reads before the next write', { pos: 'below', dist: 56, color: T.p, dx: 40 });
      const ax_ = annotate(layer, svg, q, 'x', 'what the tools have returned', { pos: 'above', dist: 48, dx: 60 });
      const ineq = tex(layer, String.raw`\Pr\big(\, c \text{ next} \,\big|\, x \,\big) \;\le\; \htmlClass{tp k-q2 m-q}{q_c}`, { x: 960, y: 700, ax: 0.5, ay: 0.5, size: 54 });
      const qCopy = morph(layer, q, ineq, ['q'], { keep: true });
      const inN = note(layer, 'Paper §2.3, after Proposition 4: a read the agent makes after a reply is one the speculator must make before it.', { x: 960, y: 760, ax: 0.5, size: 19, color: C.subtle });

      // The paper's two gaps (§4.2): next step against use before the write.
      const PX = 330, PW = 1100;
      const pairs = [
        { head: 'after a user\'s details: <span style="color:#eceff0">an order</span>', next: 0.64, use: 0.94, y: 300 },
        { head: 'after an order: <span style="color:#eceff0">a product</span>', next: 0.08, use: 0.38, y: 560 },
      ].map(pr => {
        const hd = label(layer, pr.head, { x: PX, y: pr.y, color: C.subtle, size: 19 });
        const row = (v, y, col, name) => {
          const n = box(layer, { x: PX, y, cls: 'note', html: name, style: { fontSize: '21px', color: C.muted } });
          const tr = box(layer, { x: PX + 330, y: y + 2, w: PW - 330, h: 26, style: { background: C.surface2, border: `1px solid ${C.border}`, borderRadius: '6px' } });
          const f = box(tr, { x: 0, y: 0, h: 24, w: 0, style: { background: col, borderRadius: '5px' } });
          const num = box(layer, { x: PX + PW + 26, y: y + 15, ay: 0.5, cls: 'big num', text: '', style: { fontSize: '38px', color: col === T.p ? T.p : C.muted } });
          return { n, tr, f, num, v };
        };
        const a = row(pr.next, pr.y + 44, T.base, 'comes next');
        const u = row(pr.use, pr.y + 100, T.p, 'used before the write');
        // The gap: what a next-step score misses.
        const gap = box(layer, { x: PX + 330 + (PW - 330) * pr.next, y: pr.y + 44, w: (PW - 330) * (pr.use - pr.next), h: 84, style: { border: `1.5px dashed ${T.p}88`, borderRadius: '8px', background: 'rgba(93,210,216,0.05)' } });
        const gl = label(layer, `${fmt(pr.use - pr.next)} that “next” misses`, { x: PX + 330 + (PW - 330) * (pr.next + pr.use) / 2, y: pr.y + 160, ax: 0.5, color: T.p, size: 15 });
        return { hd, a, u, gap, gl };
      });
      const scope = note(layer, 'GLM-5 and Claude Sonnet 4.5, replayed on τ²-bench retail, under the habit learned from four other agents (paper §4.2). “Comes next” is the habit’s probability; “used before the write” is how often the agents did.', { x: PX, y: 810, w: 1260, size: 17 });

      const tPays = at('event1', 'pays'), tWrite = at('event1', 'next write'), tQ = at('event2', 'quantity'), tChance = at('event2', 'chance'), tAmong = at('event2', 'among');
      const tNever = at('event3', 'never'), tReply = at('event3', 'reply'), tE4 = at('event4', 'after'), tHabit = at('event4', 'the habit'), tAgents = at('event4', 'the agents'), tE5 = at('event5', 'after'), tProd = at('event5', 'a product'), tBefore = at('event5', 'but before');
      const OUT = tE4 - 0.5;
      return t => {
        chips.forEach((ch, i) => show(ch.c, t, b.t0 + 0.2 + i * 0.22, { d: 0.45, dy: 8, out: OUT }));
        links.forEach((a, i) => a.set(ramp(t, b.t0 + 0.4 + i * 0.22, b.t0 + 0.8 + i * 0.22), 1 - ramp(t, OUT, OUT + 0.35)));
        chips[2].c.className = `chip ${t > tPays + 0.2 ? 'read' : 'call'}`;
        chips[2].c.style.fontSize = '24px';
        show(tick, t, tPays, { d: 0.4, dy: 0, out: OUT });
        show(now, t, tPays, { d: 0.4, dy: 6, out: OUT });
        bracket.set(ramp(t, tWrite, tWrite + 0.6), 1 - ramp(t, OUT, OUT + 0.35));
        show(wLab, t, tWrite, { d: 0.4, dy: 0, out: OUT });
        show(uLab, t, tWrite + 0.4, { d: 0.5, dy: 6, out: OUT });
        show(q, t, tQ, { d: 0.7, dy: 12, out: OUT });
        aq.set(t, tChance, OUT); ac.set(t, tChance + 0.5, OUT); aU.set(t, tAmong, OUT); ax_.set(t, tAmong + 0.6, OUT);
        qCopy.set(t, tNever, 1.2);
        if (t > OUT) ineq.style.opacity = (Math.min(1, +ineq.style.opacity || 0) * (1 - ramp(t, OUT, OUT + 0.35))).toFixed(3);
        show(inN, t, tReply + 0.2, { d: 0.5, dy: 0, out: OUT });
        // the reply chip lights while the voice says it comes between
        chips[1].c.style.boxShadow = t > tReply && t < OUT ? `0 0 0 3px ${C.borderStrong}` : 'none';
        pairs.forEach((pr, i) => {
          const s = i ? tE5 : tE4;
          show(pr.hd, t, s, { d: 0.4, dy: 6 });
          [[pr.a, i ? tProd : tHabit], [pr.u, i ? tBefore : tAgents]].forEach(([r, ts]) => {
            show(r.n, t, s + 0.1, { d: 0.4, dy: 6 }); show(r.tr, t, s + 0.1, { d: 0.4, dy: 6 });
            const u = ramp(t, ts, ts + 0.9, E.out3);
            r.f.style.width = `${((PW - 332) * r.v * u).toFixed(1)}px`;
            r.num.textContent = fmt(r.v * u);
            show(r.num, t, ts, { d: 0.3, dy: 0 });
          });
          const gs = (i ? tBefore : tAgents) + 1.0;
          show(pr.gap, t, gs, { d: 0.5, dy: 0 });
          show(pr.gl, t, gs + 0.2, { d: 0.4, dy: 4 });
        });
        show(scope, t, tAgents + 1.2, { d: 0.5, dy: 0 });
      };
    });
  }

  // =============================================================== safety
  function sceneSafety(ctx) {
    scene(ctx, 'safety', (layer, b) => {
      const at = ctx.at;
      const runs = [
        { label: 'without stretto', y: 280, steps: ['r', 'r', 'r', 'w', 'r', 'w'] },
        { label: 'with stretto', y: 450, steps: ['r', 'R', 'R', 'w', 'r', 'w'] },
      ];
      const X0 = 420, DX = 190;
      const state = ['S₀', 'S₀', 'S₀', 'S₀', 'S₁', 'S₁', 'S₂'];
      const els = runs.map(run => {
        const l = label(layer, run.label, { x: 120, y: run.y, ay: 0.5 });
        const cells = run.steps.map((k, i) => {
          const c = box(layer, { x: X0 + i * DX, y: run.y, ax: 0.5, ay: 0.5, w: 130, h: 54, cls: 'chip', html: k === 'w' ? 'write' : k === 'R' ? 'read ahead' : 'read', style: { textAlign: 'center', display: 'grid', placeItems: 'center', padding: '0', fontSize: '18px' } });
          c.className = `chip ${k === 'w' ? 'write' : k === 'R' ? 'read' : ''}`;
          return c;
        });
        const sts = state.map((s, i) => box(layer, { x: X0 - DX / 2 + i * DX, y: run.y + 62, ax: 0.5, cls: 'code', text: s, style: { fontSize: '20px', color: C.subtle } }));
        return { l, cells, sts };
      });
      const eq = [3, 5].map(i => label(layer, '↕ same state', { x: X0 + (i + 0.5) * DX, y: 398, ax: 0.5, ay: 0.5, color: T.p }));
      const det = box(layer, { x: X0 + 2 * DX, y: 450, ax: 0.5, ay: 0.5, w: 150, h: 70, style: { border: `2px dashed ${C.petrolLine}`, borderRadius: '12px' } });
      const detL = label(layer, 'a detour: tokens only', { x: X0 + 2 * DX, y: 556, ax: 0.5, color: T.p });
      const prop = tex(layer, String.raw`\htmlClass{tp k-s}{\sigma(x)} \subseteq \htmlClass{tn k-R}{\mathcal{R}}\ \ \forall x \;\;\Longrightarrow\;\; \text{the state after every write is unchanged}`, { x: 120, y: 650, size: 40 });
      const svg = svgOf(layer);
      const as = annotate(layer, svg, prop, 's', 'the lookups after state x', { pos: 'below', dist: 40, color: T.p, align: 0, dx: -30 });
      const aR = annotate(layer, svg, prop, 'R', 'the read tools', { pos: 'above', dist: 40, align: 0, dx: 20 });
      const src = note(layer, 'Proposition 1 (safety), if the agent’s calls are those it would make without σ, less the ones answered. paper/stretto.md §2.2; the proof is in Appendix A.', { x: 120, y: 800, w: 1500, size: 18 });
      const tState = at('safety1', 'state'), tWrite = at('safety2', 'same calls'), tSame = at('safety2', 'same state'), tWrong = at('safety3', 'wrong'), tDet = at('safety3', 'detour');
      return t => {
        els.forEach((e, ri) => {
          show(e.l, t, b.t0 + 0.2 + ri * 0.2, { d: 0.4, dy: 0 });
          e.cells.forEach((c, i) => show(c, t, b.t0 + 0.3 + ri * 0.2 + i * 0.1, { d: 0.4, dy: 6 }));
          e.sts.forEach((s, i) => show(s, t, tState + i * 0.08, { d: 0.35, dy: 0 }));
        });
        show(prop, t, tWrite, { d: 0.6, dy: 8 });
        as.set(t, tWrite + 0.6); aR.set(t, tWrite + 0.9);
        show(src, t, tSame + 0.4, { d: 0.5, dy: 0 });
        eq.forEach((e, i) => show(e, t, tSame + i * 0.2, { d: 0.4, dy: 0 }));
        show(det, t, tWrong, { d: 0.4, dy: 0, scale: 0.1 });
        show(detL, t, tDet, { d: 0.4, dy: 0 });
      };
    });
  }

  // ================================================================= rule
  // The expected value, named term by term; the saving and the detour as q
  // sweeps, crossing at θ*, with the formula's numbers following q; the
  // threshold derived on the board, term by term; the live costs put in.
  function sceneRule(ctx) {
    scene(ctx, 'rule', (layer, b) => {
      const at = ctx.at;
      const svg = svgOf(layer);
      const used = tex(layer, String.raw`\text{used: } \htmlClass{tb k-b}{+\beta} \qquad \text{unused: } \htmlClass{td k-d}{-\delta}`, { x: 120, y: 180, size: 44 });
      const ab = annotate(layer, svg, used, 'b', 'the turn it saves, in input tokens', { pos: 'below', dist: 34, color: T.b, align: 0, dx: -30 });
      const ad = annotate(layer, svg, used, 'd', 'its result, carried as a detour', { pos: 'below', dist: 34, color: T.d, align: 0, dx: -30 });
      const ev = tex(layer, String.raw`\mathbb{E}\,u \;=\; \htmlClass{k-gain}{\htmlClass{tp}{q}\,\htmlClass{tb}{\beta}} \;-\; \htmlClass{k-loss}{(1-\htmlClass{tp}{q})\,\htmlClass{td}{\delta}}`, { x: 120, y: 360, size: 58 });
      const ag = annotate(layer, svg, ev, 'gain', 'the expected saving', { pos: 'below', dist: 40, color: T.b });
      const al = annotate(layer, svg, ev, 'loss', 'the expected detour', { pos: 'below', dist: 40, color: T.d });
      // The derivation, worked in one place.
      const Q = k => String.raw`\htmlClass{tp m-${k}}{q}`, B_ = k => String.raw`\htmlClass{tb m-${k}}{\beta}`, D_ = k => String.raw`\htmlClass{td m-${k}}{\delta}`;
      const steps = [
        String.raw`${Q('q')}\,${B_('b')} \;-\; (1-\htmlClass{tp}{q})\,${D_('d')} \;\htmlClass{m-ge}{\ge}\; 0`,
        String.raw`${Q('q')}\,(${B_('b')}+${D_('d')}) \;\htmlClass{m-ge}{\ge}\; ${D_('d2')}`,
        String.raw`${Q('q')} \;\htmlClass{m-ge}{\ge}\; \htmlClass{m-th}{\theta^\star} = \dfrac{${D_('d2')}}{${B_('b')}+${D_('d')}}`,
        String.raw`\htmlClass{m-th}{\theta^\star} \approx \dfrac{\htmlClass{td m-d2}{2{,}530}}{\htmlClass{tb m-b}{6{,}000}+\htmlClass{td m-d}{2{,}530}} \approx \htmlClass{tp}{0.30}`,
      ].map(src => tex(layer, src, { x: 120, y: 520, size: 46 }));
      const m01 = morph(layer, steps[0], steps[1], ['q', 'b', 'd', 'ge']);
      const m12 = morph(layer, steps[1], steps[2], ['q', 'b', 'd', 'd2', 'ge']);
      const m23 = morph(layer, steps[2], steps[3], ['th', 'b', 'd', 'd2']);
      // The live costs, worked: §3 Costs.
      const work = [
        String.raw`\htmlClass{tb}{\beta} \approx \tfrac{1{,}560{,}000 \text{ tokens}}{260 \text{ turns saved}} \approx \htmlClass{tb}{6{,}000}`,
        String.raw`\htmlClass{td}{\delta} \approx \tfrac{43{,}000 \text{ tokens}}{17 \text{ detours}} \approx \htmlClass{td}{2{,}530}`,
      ].map((s, i) => tex(layer, s, { x: 120, y: 660 + i * 70, size: 34 }));
      const liveN = note(layer, 'Live, the paired run: GLM-5.3 on τ²-bench retail and airline, an earlier flow (paper §3).', { x: 120, y: 820, w: 760, size: 17 });

      // The balance: saving qβ and detour (1−q)δ against q, at the live costs.
      const beta = 6.0, delta = 2.53, th = delta / (beta + delta);
      const pl = plot(svg, layer, { x: 1060, y: 250, w: 700, h: 400, xd: [0, 1], yd: [0, 6.5], xt: [0, 0.25, 0.5, 0.75, 1], yt: [0, 2, 4, 6], xl: 'q, the probability of use', yl: 'thousand input tokens' });
      const sav = arrow(svg, line([[0, 0], [1, beta]], pl.sx, pl.sy), { color: T.b, width: 3.5, head: false });
      const cost = arrow(svg, line([[0, delta], [1, 0]], pl.sx, pl.sy), { color: T.d, width: 3.5, head: false });
      const savL = box(layer, { x: pl.sx(0.86), y: pl.sy(beta * 0.86) - 18, ax: 1, ay: 1, cls: 'anno', html: 'saving&nbsp; q β', style: { color: T.b } });
      const costL = box(layer, { x: pl.sx(0.04), y: pl.sy(delta) - 14, ay: 1, cls: 'anno', html: 'detour&nbsp; (1 − q) δ', style: { color: T.d } });
      const pays = box(layer, { x: pl.sx(th), y: 250, w: pl.sx(1) - pl.sx(th), h: 400, style: { background: 'rgba(93,210,216,0.06)', borderLeft: `2px dashed ${T.p}` } });
      const paysL = label(layer, 'a lookup pays', { x: pl.sx(0.97), y: pl.sy(3.2), ax: 1, color: T.p });
      const thL = box(layer, { x: pl.sx(th) + 12, y: 616, cls: 'code', html: `θ* = ${fmt(th)}`, style: { fontSize: '20px', color: T.p } });
      // A probe sweeping q; the expected value, its numbers following it.
      const probe = el('line', { x1: 0, x2: 0, y1: 250, y2: 650, stroke: C.subtle, 'stroke-width': 1.5, 'stroke-dasharray': '3 4' }, svg);
      const pdS = el('circle', { r: 7, fill: T.b }, svg), pdC = el('circle', { r: 7, fill: T.d }, svg);
      const evLive = box(layer, { x: 1060, y: 150, style: { fontSize: '34px', whiteSpace: 'nowrap' } });
      // The line per domain, on one axis (paper, Figure 2 and §3).
      const NL = { x: 1060, y: 778, w: 700 };
      const nl = el('line', { x1: NL.x, x2: NL.x + NL.w, y1: NL.y, y2: NL.y, stroke: C.borderStrong, 'stroke-width': 2 }, svg);
      const nlT = [0, 0.1, 0.2, 0.3, 0.4, 0.5].map(v => el('text', { x: NL.x + NL.w * v / 0.5, y: NL.y + 30, class: 'axis', 'text-anchor': 'middle' }, svg, v.toFixed(1)));
      const doms = [['telecom', 0.12, T.base, -1], ['airline', 0.13, C.subtle, 1], ['live, retail and airline', 0.30, T.p, -1]].map(([n, v, col, side]) => {
        const xx = NL.x + NL.w * v / 0.5;
        const d = el('circle', { cx: xx, cy: NL.y, r: 8, fill: col }, svg);
        const l = box(layer, { x: xx, y: NL.y + (side < 0 ? -18 : 48), ax: 0.5, ay: side < 0 ? 1 : 0, cls: 'code', html: `${n} ${v.toFixed(2)}`, style: { fontSize: '17px', color: col } });
        return { d, l };
      });
      const nlL = label(layer, 'θ*, by domain', { x: NL.x - 20, y: NL.y, ax: 1, ay: 0.5 });
      const nlN = note(layer, 'Airline and telecom: counted in recorded τ²-bench episodes at each domain’s own costs (paper §3, Appendix B).', { x: NL.x, y: NL.y + 86, w: 720, size: 15 });

      const tSaves = at('rule1', 'saves'), tUnused = at('rule1', 'unused'), tEV = at('rule2', 'expected value'), tSaving = at('rule2', 'saving'), tLess = at('rule2', 'less'), tRises = at('rule3', 'rises'), tBal = at('rule3', 'balance'), tDelta = at('rule3', 'delta over');
      const tLive = at('rule4', 'live'), tTurns = at('rule4', 'saved turns'), tDet = at('rule4', 'detours'), tThat = at('rule5', 'that is'), tLine = at('rule5', 'the line'), tCount = at('rule6', 'counted');
      return t => {
        show(used, t, tSaves - 0.3, { d: 0.5, dy: 8 });
        ab.set(t, tSaves); ad.set(t, tUnused + 0.3);
        show(ev, t, tEV, { d: 0.6, dy: 10 });
        ag.set(t, tSaving); al.set(t, tLess);
        // the board: the inequality appears, then becomes the threshold, then takes the live costs
        show(steps[0], t, tBal - 0.3, { d: 0.5, dy: 8 });
        m01.set(t, tBal + 0.5);
        m12.set(t, tDelta + 0.1);
        m23.set(t, tLine - 0.2);
        work.forEach((d, i) => show(d, t, [tTurns + 0.4, tDet + 0.4][i], { d: 0.5, dy: 8 }));
        show(liveN, t, tThat, { d: 0.5, dy: 0 });
        pl.set(t, tSaving - 0.4);
        sav.set(ramp(t, tSaving, tSaving + 0.8));
        cost.set(ramp(t, tLess, tLess + 0.8));
        show(savL, t, tSaving + 0.5, { d: 0.4, dy: 0 });
        show(costL, t, tLess + 0.5, { d: 0.4, dy: 0 });
        // q sweeps up as the voice says it rises, back down, and settles on θ*
        const up = ramp(t, tRises, tBal, E.io2);
        const settle = ramp(t, tBal, tBal + 1.4, E.io3);
        const qv = lerp(lerp(0.04, 0.96, up), th, settle);
        const on = ramp(t, tRises - 0.3, tRises);
        const px = pl.sx(qv);
        probe.setAttribute('x1', px); probe.setAttribute('x2', px); probe.setAttribute('opacity', on.toFixed(3));
        pdS.setAttribute('cx', px); pdS.setAttribute('cy', pl.sy(qv * beta)); pdS.setAttribute('opacity', on.toFixed(3));
        pdC.setAttribute('cx', px); pdC.setAttribute('cy', pl.sy((1 - qv) * delta)); pdC.setAttribute('opacity', on.toFixed(3));
        const evv = qv * beta - (1 - qv) * delta;
        if (t > tRises - 0.4 && t < b.t1 + 0.5) live(evLive, String.raw`\mathbb{E}\,u = \htmlClass{tp}{${fmt(qv)}}\cdot\htmlClass{tb}{6.0} - (1-\htmlClass{tp}{${fmt(qv)}})\cdot\htmlClass{td}{2.53} = ${evv >= 0 ? '+' : '-'}${fmt(Math.abs(evv))}`);
        show(evLive, t, tRises - 0.3, { d: 0.3, dy: 0 });
        show(pays, t, tBal + 1.2, { d: 0.5, dy: 0 });
        show(paysL, t, tBal + 1.5, { d: 0.4, dy: 0 });
        show(thL, t, tBal + 1.4, { d: 0.4, dy: 0 });
        nl.setAttribute('opacity', ramp(t, tCount - 0.3, tCount + 0.2).toFixed(3));
        nlT.forEach(n => n.setAttribute('opacity', ramp(t, tCount - 0.3, tCount + 0.2).toFixed(3)));
        show(nlL, t, tCount - 0.3, { d: 0.4, dy: 0 });
        show(nlN, t, tCount + 1.2, { d: 0.5, dy: 0 });
        doms.forEach((d, i) => {
          const s = i === 2 ? tCount : tCount + 0.6 + i * 0.3;
          d.d.setAttribute('opacity', ramp(t, s, s + 0.3).toFixed(3));
          show(d.l, t, s + 0.1, { d: 0.4, dy: 0 });
        });
        void tLive;
      };
    });
  }

  // ================================================================ habit
  // A step, abstracted; the back-off formula named term by term; the
  // quickstart's own counts, level by level (data/: fugue.json's back-off),
  // recomputed live as α is turned up and down; then the formula becomes the
  // probability of use's, the same counts for a different event.
  function sceneHabit(ctx) {
    scene(ctx, 'habit', (layer, b) => {
      const at = ctx.at;
      const svg = svgOf(layer);
      const step = card(layer, { x: 960, y: 190, w: 980, h: 170, ax: 0.5, title: 'a step, abstracted', tag: 'the habit' });
      code(step.body, [`${P('tool')}      get_order_details`, `${P('outcome')}   returned`, `${P('feature')}   a feature of its result`], { x: 22, y: 6, size: 21, lh: 1.55 });
      const pj = tex(layer, String.raw`\htmlClass{tp k-p m-p}{p_j(t \mid h_j)} \;\htmlClass{m-eq}{=}\; \dfrac{\htmlClass{tn k-n m-n}{n(h_j,\, t)} \;+\; \htmlClass{ta k-a m-a}{\alpha}\; \htmlClass{tp k-prev m-prev}{p_{j-1}(t \mid h_{j-1})}}{\htmlClass{tn k-N m-N}{n(h_j)} \;+\; \htmlClass{ta m-a2}{\alpha}}`, { x: 960, y: 250, ax: 0.5, ay: 0.5, size: 50 });
      const aP = annotate(layer, svg, pj, 'p', 'the chance t comes next, after the last j steps', { pos: 'above', dist: 34, color: T.p, dx: -40 });
      const aN = annotate(layer, svg, pj, 'n', 'times t followed this context', { pos: 'above', dist: 34, dx: 20 });
      const aA = annotate(layer, svg, pj, 'a', 'how much the prior weighs', { pos: 'above', dist: 34, color: T.a, dx: 120 });
      const aPr = annotate(layer, svg, pj, 'prev', 'the shorter context’s prediction: the prior mean', { pos: 'above', dist: 34, color: T.p, dx: 330 });
      const aNN = annotate(layer, svg, pj, 'N', 'times the context was seen', { pos: 'below', dist: 30, dx: 0 });
      const rj = tex(layer, String.raw`\htmlClass{tp k-r m-p}{r_j(t \mid h_j)} \;\htmlClass{m-eq}{=}\; \dfrac{\htmlClass{tp k-m m-n}{m(h_j,\, t)} \;+\; \htmlClass{ta m-a}{\alpha}\; \htmlClass{tp m-prev}{r_{j-1}(t \mid h_{j-1})}}{\htmlClass{tn m-N}{n(h_j)} \;+\; \htmlClass{ta m-a2}{\alpha}}`, { x: 960, y: 250, ax: 0.5, ay: 0.5, size: 50 });
      const toR = morph(layer, pj, rj, ['p', 'eq', 'n', 'a', 'prev', 'N', 'a2']);
      const aM = annotate(layer, svg, rj, 'm', 'times t was called before the next write', { pos: 'above', dist: 34, color: T.p, dx: 0 });
      const aR = annotate(layer, svg, rj, 'r', 'the probability of use', { pos: 'above', dist: 34, color: T.p, dx: -40 });

      // The ladder: p_j(· | h) at each level for one real history of the
      // quickstart's sessions, a bar per action. The counts behind each level
      // are recovered from fugue.json's predictions at α's median, so any α
      // can be put back through the same back-off.
      const ex = M.fugue.backoff.examples.find(e => e.history.join() === 'find_user_id_by_email,get_user_details');
      const V = M.fugue.vocab, nV = V.length, a0 = M.fugue.backoff.alpha;
      const counts = [];
      for (let j = 1; j < ex.next.length; j++) {
        const N = ex.next[j].evidence, pj_ = ex.next[j].p, pp = ex.next[j - 1].p;
        counts.push({ N, n: pj_.map((v, i) => v * (N + a0) - a0 * pp[i]) });
      }
      const levelsAt = a => {
        let prev = new Array(nV).fill(1 / nV);
        const out = [prev];
        for (const c of counts) { prev = c.n.map((n, i) => (n + a * prev[i]) / (c.N + a)); out.push(prev); }
        return out;
      };
      const acts = ['find_user_id_by_email', 'get_user_details', 'get_order_details', 'reply', 'cancel_pending_order'];
      const short = { find_user_id_by_email: 'find_user', get_user_details: 'get_user', get_order_details: 'get_order', reply: 'reply', cancel_pending_order: 'cancel' };
      const target = 'get_order_details', ti = V.indexOf(target);
      const heads = [['uniform', 'j = −1'], ['no context', 'j = 0'], ['after get_user_details', 'j = 1'], ['after find_user, get_user', 'j = 2']];
      const PW = 330, PH = 210, GX = 90, X0 = 960 - (4 * PW + 3 * GX) / 2, Y0 = 560;
      const ladder = ex.next.map((lv, i) => {
        const x0 = X0 + i * (PW + GX);
        const hd = label(layer, heads[i][0], { x: x0, y: Y0 - 62, size: 16, color: C.muted });
        const jj = box(layer, { x: x0, y: Y0 - 36, cls: 'code', text: heads[i][1] + (lv.evidence != null ? `  ·  n(h) = ${lv.evidence}` : ''), style: { fontSize: '15px', color: C.faint } });
        const base = el('line', { x1: x0, x2: x0 + PW, y1: Y0 + PH, y2: Y0 + PH, stroke: C.borderStrong }, svg);
        const bw = PW / acts.length;
        const bars = acts.map((a, k) => {
          const isT = a === target;
          const r = el('rect', { x: x0 + k * bw + 6, width: bw - 12, y: Y0 + PH, height: 0, rx: 3, fill: isT ? T.p : '#3b4649' }, svg);
          const tl = el('text', { x: x0 + k * bw + bw / 2, y: Y0 + PH + 22, class: 'axis', 'text-anchor': 'middle', style: 'font-size:12px' }, svg, short[a]);
          return { r, vi: V.indexOf(a), tl };
        });
        const val = box(layer, { x: x0 + PW / 2, y: Y0 + PH + 38, ax: 0.5, cls: 'big num', text: '', style: { fontSize: '34px', color: T.p } });
        return { hd, jj, base, bars, val };
      });
      const chain = ladder.slice(1).map((_, i) => {
        const xa = X0 + i * (PW + GX) + PW + 10, xb = X0 + (i + 1) * (PW + GX) - 10;
        return arrow(svg, `M ${xa} ${Y0 + PH / 2} L ${xb} ${Y0 + PH / 2}`, { color: T.a, width: 1.8 });
      });
      const chainL = label(layer, 'prior mean →', { x: X0 + PW + GX / 2, y: Y0 + PH / 2 - 28, ax: 0.5, color: T.a, size: 13 });
      // α, turned: a dial on a log scale, and the top level's sum with its numbers.
      const AX = { x: 1240, y: 400, w: 520 };
      const sa = la => AX.x + AX.w * (la + 3.5) / 7.5;
      const track = el('line', { x1: AX.x, x2: AX.x + AX.w, y1: AX.y, y2: AX.y, stroke: C.borderStrong, 'stroke-width': 3, 'stroke-linecap': 'round' }, svg);
      const aTicks = [0.05, 0.5, 5, 50].map(v => el('text', { x: sa(Math.log(v)), y: AX.y + 28, class: 'axis', 'text-anchor': 'middle' }, svg, String(v)));
      const knob = el('circle', { cy: AX.y, r: 11, fill: T.a, stroke: C.bg, 'stroke-width': 3 }, svg);
      const aVal = box(layer, { x: AX.x, y: AX.y - 52, cls: 'code', html: '', style: { fontSize: '24px', color: T.a } });
      const sumLive = box(layer, { x: 160, y: 400, ay: 0.5, style: { fontSize: '32px', whiteSpace: 'nowrap' } });
      const lnote = note(layer, `The quickstart’s six sessions (the console’s test fixtures), counted by stretto-model’s BackoffModel; α = ${fmt(a0)} unless turned.`, { x: 960, y: 872, ax: 0.5, size: 16 });

      // Use before the write: after find_user_id_by_email, at j = 2, the
      // next step sums to one and the probability of use does not.
      const ex1 = M.fugue.backoff.examples.find(e => e.history.join() === 'find_user_id_by_email');
      const nx = ex1.next[3].p, rc = ex1.reach[3].p;
      const SX = 520, SW = 880;
      const sums = [['next step  p₂', nx, T.base, 560], ['used before the write  r₂', rc, T.p, 700]].map(([name, ps, col, y]) => {
        const n = label(layer, name, { x: SX - 30, y: y + 16, ax: 1, ay: 0.5, color: col === T.p ? T.p : C.subtle });
        let acc = 0;
        const segs = acts.map(a => {
          const v = ps[V.indexOf(a)];
          const s = { a, v, x: acc };
          acc += v;
          return s;
        }).filter(s => s.v >= 0.02);
        const unit = SW / 3.2;
        const rs = segs.map(s => {
          const r = box(layer, { x: SX + s.x * unit, y, w: Math.max(2, s.v * unit - 4), h: 34, style: { background: col === T.p ? 'rgba(93,210,216,0.22)' : '#2a3336', border: `1px solid ${col === T.p ? T.p : C.borderStrong}`, borderRadius: '6px' } });
          const tl = box(r, { x: 10, y: 17, ay: 0.5, cls: 'code', text: `${short[s.a]} ${fmt(s.v)}`, style: { fontSize: '14px', color: col === T.p ? T.p : C.muted, whiteSpace: 'nowrap' } });
          return { r, tl };
        });
        const tot = box(layer, { x: SX + acc * unit + 18, y: y + 17, ay: 0.5, cls: 'code', html: `Σ = ${fmt(acc)}`, style: { fontSize: '24px', color: col === T.p ? T.p : C.muted } });
        return { n, rs, tot };
      });
      const one = el('line', { x1: SX + SW / 3.2, x2: SX + SW / 3.2, y1: 540, y2: 760, stroke: C.subtle, 'stroke-dasharray': '4 4' }, svg);
      const oneL = label(layer, '1', { x: SX + SW / 3.2, y: 528, ax: 0.5, ay: 1 });
      const sumN = note(layer, 'After an email lookup, the agent reads the user, reads an order and replies before it writes: three chances near one. The same sessions, the same contexts, a different event.', { x: 960, y: 800, ax: 0.5, w: 1300, size: 17 });

      const tEach = at('habit1', 'each step'), tPred = at('habit2', 'predicts'), tCounts = at('habit3', 'its counts'), tAlpha = at('habit3', 'alpha'), tShort = at('habit3', 'shorter'), tQS = at('habit4', 'quickstart'), tSharp = at('habit4', 'sharpens');
      const tUp = at('habitA', 'turn alpha up'), tPrior = at('habitA', 'the prior'), tDown = at('habitA', 'turn it down'), tCountsDo = at('habitA', 'the counts do'), tAEnd = ctx.L.habitA.t1;
      const tUse = at('habit5', 'probability of use'), tEvent = at('habit5', 'different event'), tCoin = at('habit6', 'own coin'), tSum = at('habit6', 'sum');
      const OUT1 = tPred - 0.5, OUT2 = tUse - 0.4;
      const la0 = Math.log(a0);
      return t => {
        show(step, t, tEach - 0.2, { d: 0.6, dy: 10, out: OUT1 });
        show(pj, t, tPred, { d: 0.6, dy: 10 });
        aP.set(t, tPred + 0.5, OUT2); aN.set(t, tCounts, OUT2); aA.set(t, tAlpha, OUT2); aPr.set(t, tShort, OUT2); aNN.set(t, tCounts + 0.5, OUT2);
        glow(pj, 'prev', t > tShort && t < tShort + 2.5, T.p);
        glow(pj, 'a', t > tUp - 0.2 && t < tAEnd + 0.3, T.a);
        toR.set(t, tUse);
        // α: its median, then up, down, and back, on a log scale
        const la = la0 + (Math.log(40) - la0) * ramp(t, tUp + 0.3, tPrior + 0.6, E.io3) + (Math.log(0.04) - Math.log(40)) * ramp(t, tDown + 0.1, tCountsDo + 0.5, E.io3) + (la0 - Math.log(0.04)) * ramp(t, tCountsDo + 0.9, tAEnd + 1.1, E.io3);
        const alpha = Math.exp(la);
        const L = levelsAt(alpha);
        ladder.forEach((l, i) => {
          const s = tQS + i * 0.7;
          show(l.hd, t, s, { d: 0.4, dy: 6, out: OUT2 }); show(l.jj, t, s, { d: 0.4, dy: 0, out: OUT2 });
          const o = ramp(t, s, s + 0.3) * (1 - ramp(t, OUT2, OUT2 + 0.35));
          l.base.setAttribute('opacity', o.toFixed(3));
          l.bars.forEach(bb => {
            const u = ramp(t, s + 0.1, s + 0.8, E.out3);
            const hgt = PH * L[i][bb.vi] * u;
            bb.r.setAttribute('y', (Y0 + PH - hgt).toFixed(1)); bb.r.setAttribute('height', hgt.toFixed(1)); bb.r.setAttribute('opacity', o.toFixed(3));
            bb.tl.setAttribute('opacity', o.toFixed(3));
          });
          l.val.textContent = fmt(L[i][ti]);
          show(l.val, t, s + 0.6, { d: 0.4, dy: 0, out: OUT2 });
        });
        chain.forEach((c, i) => c.set(ramp(t, tQS + (i + 1) * 0.7 - 0.2, tQS + (i + 1) * 0.7 + 0.2), 1 - ramp(t, OUT2, OUT2 + 0.35)));
        show(chainL, t, tQS + 0.9, { d: 0.4, dy: 0, out: OUT2 });
        show(lnote, t, tSharp, { d: 0.5, dy: 0, out: OUT2 });
        // the dial and the top level's numbers, while α is turned
        const dOn = ramp(t, tUp - 0.4, tUp) * (1 - ramp(t, OUT2, OUT2 + 0.35));
        track.setAttribute('opacity', dOn.toFixed(3)); aTicks.forEach(k => k.setAttribute('opacity', dOn.toFixed(3)));
        knob.setAttribute('cx', sa(la).toFixed(1)); knob.setAttribute('opacity', dOn.toFixed(3));
        aVal.innerHTML = `α = ${alpha < 1 ? alpha.toFixed(2) : alpha.toFixed(1)}`;
        show(aVal, t, tUp - 0.4, { d: 0.3, dy: 0, out: OUT2 });
        if (dOn > 0) {
          const c2 = counts[2], p1 = L[2][ti];
          live(sumLive, String.raw`p_2(\text{get\_order}) = \dfrac{\htmlClass{tn}{${fmt(c2.n[ti], 1)}} + \htmlClass{ta}{${alpha < 1 ? alpha.toFixed(2) : alpha.toFixed(1)}} \cdot \htmlClass{tp}{${fmt(p1)}}}{\htmlClass{tn}{${c2.N}} + \htmlClass{ta}{${alpha < 1 ? alpha.toFixed(2) : alpha.toFixed(1)}}} = \htmlClass{tp}{${fmt(L[3][ti])}}`);
        }
        show(sumLive, t, tUp - 0.4, { d: 0.3, dy: 0, out: OUT2 });
        aR.set(t, tUse + 1.2); aM.set(t, tEvent);
        sums.forEach((s, i) => {
          const st = i ? tCoin + 0.6 : tEvent + 0.6;
          show(s.n, t, st, { d: 0.4, dy: 0 });
          s.rs.forEach((r, k) => { show(r.r, t, st + 0.15 * k, { d: 0.4, dy: 0, dx: -10 }); show(r.tl, t, st + 0.15 * k + 0.2, { d: 0.3, dy: 0 }); });
          show(s.tot, t, st + 0.15 * s.rs.length + 0.2, { d: 0.4, dy: 0 });
        });
        one.setAttribute('opacity', ramp(t, tEvent + 0.6, tEvent + 1).toFixed(3));
        show(oneL, t, tEvent + 0.6, { d: 0.4, dy: 0 });
        show(sumN, t, tSum + 0.4, { d: 0.5, dy: 0 });
      };
    });
  }

  // ================================================================ fugue
  // The program; its prior and its likelihood; their product; the chain
  // walking it, its draws piling into the shape; the median the flow keeps.
  function sceneFugue(ctx) {
    scene(ctx, 'fugue', (layer, b) => {
      const at = ctx.at;
      const svg = svgOf(layer);
      const A = M.fugue.alpha;
      const src = card(layer, { x: 120, y: 160, w: 860, h: 560, title: 'crates/stretto-model/src/alpha.rs', tag: 'fugue' });
      const c = code(src.body, [
        `<span class="k">let</span> draws = ${P('adaptive_mcmc_chain')}(`,
        `    &amp;mut rng,`,
        `    <span class="k">move</span> || {`,
        `        <span class="k">let</span> episodes = episodes.clone();`,
        `        ${P('sample')}(addr!(<span class="s">"log_alpha"</span>),`,
        `               Normal::new(0.0, 2.0).unwrap())`,
        `            .bind(<span class="k">move</span> |log_alpha| {`,
        `                <span class="k">let</span> alpha = log_alpha.exp();`,
        `                ${P('factor')}(prequential_loglik(`,
        `                    &amp;episodes, order, vocab, alpha))`,
        `                    .map(<span class="k">move</span> |_| alpha)`,
        `            })`,
        `    },`,
        `    n_samples,      ${D('// 600 draws kept')}`,
        `    n_samples / 2,  ${D('// after 300 of warm-up')}`,
        `);`,
      ], { x: 22, y: 8, size: 17, lh: 1.62 });
      const HL = { prior: [4, 5], lik: [8, 9], chain: [0, 13, 14] };
      const bayes = tex(layer, String.raw`\htmlClass{tp k-post}{p(\log\alpha \mid \text{sessions})} \;\propto\; \htmlClass{ta k-prior}{\mathcal{N}(\log\alpha;\,0,\,2)} \;\cdot\; \htmlClass{tn k-lik}{\textstyle\prod_t p(s_t \mid s_{<t},\,\alpha)}`, { x: 1440, y: 190, ax: 0.5, ay: 0.5, size: 32 });
      const aPost = annotate(layer, svg, bayes, 'post', 'the posterior', { pos: 'below', dist: 26, color: T.p });
      const aPri = annotate(layer, svg, bayes, 'prior', 'the prior', { pos: 'below', dist: 26, color: T.a });
      const aLik = annotate(layer, svg, bayes, 'lik', 'each step, from the steps before', { pos: 'below', dist: 26 });

      // The density chart, on log α.
      const G = A.grid_log_alpha, dx = G[1] - G[0];
      const norm = arr => { const s = arr.reduce((a, v) => a + v, 0) * dx; return arr.map(v => v / s); };
      const post = norm(A.density);
      const prior = norm(G.map(la => Math.exp(-0.5 * (la / 2) ** 2)));
      const llMax = Math.max(...A.loglik);
      const lik = norm(A.loglik.map(l => Math.exp(l - llMax)));
      const BW = 0.25, bins = [];
      for (let v = -5; v < 5 - 1e-9; v += BW) bins.push(v);
      const logs = A.draws.map(Math.log);
      const yMax = Math.max(...post, ...lik) * 1.1;
      const XT = [Math.log(0.01), Math.log(0.1), Math.log(1), Math.log(10), Math.log(100)];
      const pl = plot(svg, layer, { x: 1060, y: 290, w: 760, h: 280, xd: [-5, 5], yd: [0, yMax], xt: XT, yt: [], xl: 'α, on a log scale', yl: '', xf: v => String(+Math.exp(v).toPrecision(1)), grid: false });
      const toPts = arr => G.map((g, i) => [g, arr[i]]);
      const cPrior = arrow(svg, line(toPts(prior), pl.sx, pl.sy), { color: T.a, width: 2.5, head: false, dash: '7 6' });
      const cLik = arrow(svg, line(toPts(lik), pl.sx, pl.sy), { color: C.muted, width: 2, head: false });
      const area = el('path', { d: `${line(toPts(post), pl.sx, pl.sy)} L ${pl.sx(5)} ${pl.sy(0)} L ${pl.sx(-5)} ${pl.sy(0)} Z`, fill: 'rgba(93,210,216,0.10)' }, svg);
      const cPost = arrow(svg, line(toPts(post), pl.sx, pl.sy), { color: T.p, width: 3.5, head: false });
      const lPri = box(layer, { x: pl.sx(-3.6), y: pl.sy(prior[G.findIndex(g => g >= -3.6)]) - 12, ax: 0.5, ay: 1, cls: 'anno', text: 'prior', style: { color: T.a } });
      const iL = A.loglik.indexOf(llMax);
      const lLik = box(layer, { x: pl.sx(G[iL]) + 60, y: pl.sy(lik[iL]) - 8, ay: 1, cls: 'anno', text: 'likelihood', style: { color: C.muted } });
      // The histogram of the draws so far, as a density.
      const hg = el('g', {}, svg);
      const hb = bins.map(v => el('rect', { x: pl.sx(v) + 1, width: pl.sx(v + BW) - pl.sx(v) - 2, y: pl.sy(0), height: 0, fill: 'rgba(93,210,216,0.38)' }, hg));
      // The trace: each draw's log α in order.
      const TR = { x: 1060, y: 680, w: 760, h: 100 };
      const trBox = el('rect', { x: TR.x, y: TR.y, width: TR.w, height: TR.h, fill: 'none', stroke: C.border }, svg);
      const trY = la => TR.y + TR.h - TR.h * (la + 5) / 10;
      const trPath = el('path', { d: '', fill: 'none', stroke: T.p, 'stroke-width': 1.3, opacity: 0.9 }, svg);
      const trL = label(layer, 'the chain, draw by draw', { x: TR.x, y: TR.y - 26, size: 15 });
      const count = box(layer, { x: TR.x + TR.w, y: TR.y - 26, ax: 1, cls: 'code', text: '', style: { fontSize: '16px', color: C.subtle } });
      // The median and the 5th–95th percentiles.
      const med = Math.log(A.median), lo = Math.log(A.lo), hi = Math.log(A.hi);
      const band = el('rect', { x: pl.sx(lo), width: pl.sx(hi) - pl.sx(lo), y: 290, height: 280, fill: 'rgba(93,210,216,0.06)' }, svg);
      const mLine = el('line', { x1: pl.sx(med), x2: pl.sx(med), y1: 280, y2: 570, stroke: T.p, 'stroke-width': 2.5 }, svg);
      const mLab = box(layer, { x: pl.sx(med) + 120, y: 300, cls: 'code', html: `median α = ${P(fmt(A.median))}<br>${D(`5th–95th: ${fmt(A.lo)} – ${fmt(A.hi)}`)}`, style: { fontSize: '19px', color: C.muted, lineHeight: '1.5' } });
      const flow = box(layer, { x: 120, y: 760, cls: 'code', html: `shop.flow.json  ${D('·')}  "alpha": ${P('0.6973960072663901')}`, style: { fontSize: '19px', color: C.muted } });
      const scope = note(layer, 'fugue’s own run, on the quickstart’s six sessions (the console’s test fixtures): the chain stretto learn ran for the shop flow, seed 7, and the same posterior on a grid, prior times likelihood (data/).', { x: 120, y: 810, w: 1700, size: 16 });

      const tTuned = at('fugue1', 'by hand'), tFugue = at('fugue2', 'fugue'), tPrior = at('fugue3', 'normal prior'), tScore = at('fugue3', 'scores'), tPost = at('fugue4', 'prior'), tIs = at('fugue4', 'posterior');
      const tWalk = at('fugue5', 'walks'), tPile = at('fugue5', 'pile'), tSix = at('fugue6', 'six sessions'), tMed = at('fugue6', 'median'), tLearned = at('fugue6', 'learned with');
      const N = A.draws.length;
      return t => {
        show(src, t, b.t0 + 0.15, { d: 0.6, dy: 10 });
        c.rows.forEach((r, i) => show(r, t, b.t0 + 0.3 + i * 0.04, { d: 0.3, dy: 0 }));
        const k = t >= tWalk ? 'chain' : t >= tScore ? 'lik' : t >= tPrior ? 'prior' : null;
        c.rows.forEach((r, i) => {
          const on = k && HL[k].includes(i);
          r.style.background = on ? 'rgba(51,192,199,0.10)' : 'transparent';
          r.style.boxShadow = on ? `inset 3px 0 0 ${T.p}` : 'none';
        });
        pl.set(t, tFugue);
        cPrior.set(ramp(t, tPrior, tPrior + 0.8)); show(lPri, t, tPrior + 0.5, { d: 0.4, dy: 0 });
        cLik.set(ramp(t, tScore, tScore + 0.8)); show(lLik, t, tScore + 0.6, { d: 0.4, dy: 0 });
        show(bayes, t, tPost - 0.2, { d: 0.6, dy: 8 });
        aPri.set(t, tPost); aLik.set(t, tPost + 0.4); aPost.set(t, tIs);
        cPost.set(ramp(t, tIs, tIs + 0.9));
        area.setAttribute('opacity', ramp(t, tIs + 0.5, tIs + 1.0).toFixed(3));
        // The chain: draws revealed over [tWalk, tSix].
        const kk = Math.round(N * ramp(t, tWalk, tSix - 0.2, E.lin));
        const on = ramp(t, tWalk - 0.2, tWalk + 0.2);
        trBox.setAttribute('opacity', on.toFixed(3)); show(trL, t, tWalk - 0.2, { d: 0.4, dy: 0 });
        count.textContent = `draw ${kk} of ${N}`; show(count, t, tWalk - 0.2, { d: 0.3, dy: 0 });
        trPath.setAttribute('d', logs.slice(0, Math.max(1, kk)).map((v, i) => `${i ? 'L' : 'M'} ${(TR.x + TR.w * i / (N - 1)).toFixed(1)} ${trY(v).toFixed(1)}`).join(' '));
        trPath.setAttribute('opacity', (0.9 * on).toFixed(3));
        const cnt = new Array(bins.length).fill(0);
        for (let i = 0; i < kk; i++) { const j = Math.floor((logs[i] + 5) / BW); if (j >= 0 && j < cnt.length) cnt[j]++; }
        hb.forEach((r, j) => {
          const dens = kk ? cnt[j] / (kk * BW) : 0;
          const y0 = pl.sy(0), y1 = pl.sy(Math.min(yMax, dens));
          r.setAttribute('y', y1.toFixed(1)); r.setAttribute('height', (y0 - y1).toFixed(1));
        });
        hg.setAttribute('opacity', on.toFixed(3));
        const mo = ramp(t, tMed, tMed + 0.5);
        band.setAttribute('opacity', mo.toFixed(3)); mLine.setAttribute('opacity', mo.toFixed(3));
        show(mLab, t, tMed + 0.2, { d: 0.4, dy: 0 });
        show(flow, t, tLearned, { d: 0.5, dy: 6 });
        show(scope, t, tSix, { d: 0.5, dy: 0 });
        void tTuned; void tPile;
      };
    });
  }

  // ============================================================= bindings
  function sceneBindings(ctx) {
    scene(ctx, 'bindings', (layer, b) => {
      const at = ctx.at;
      const svg = svgOf(layer);
      const tbl = card(layer, { x: 120, y: 160, w: 1680, h: 150, title: '$ stretto flow-show notes.flow.json', tag: 'the walkthrough’s flow' });
      code(tbl.body, [
        `${D('Lookup')}            ${D('Argument')}   ${D('Bound from')}                                       ${D('Chance it is the agent\'s')}`,
        `read_text_file    path       search_files at ${P('$')} (3 of 14); at ${P('$[*]')} (11 of 14)    ${P('0.81')} (12/14)`,
      ], { x: 22, y: 10, size: 20, lh: 1.8 });
      const rho = tex(layer, String.raw`\htmlClass{tp k-rho}{\rho} \;=\; \dfrac{\htmlClass{tn k-k}{k} \htmlClass{ta k-one}{\;+\;1}}{\htmlClass{tn k-nn}{n} \htmlClass{ta k-two}{\;+\;2}}`, { x: 440, y: 530, ax: 0.5, ay: 0.5, size: 76 });
      const aRho = annotate(layer, svg, rho, 'rho', 'the chance the bound value is the agent’s', { pos: 'above', dist: 40, color: T.p, dx: -20, w: 230, align: 0 });
      const aK = annotate(layer, svg, rho, 'k', 'times it was right', { pos: 'above', dist: 40, dx: 170 });
      const aNn = annotate(layer, svg, rho, 'nn', 'times it was tried', { pos: 'below', dist: 40, dx: -90 });
      const aPr = annotate(layer, svg, rho, 'two', 'a uniform prior, Beta(1, 1)', { pos: 'below', dist: 40, color: T.a, dx: 120 });
      // The posterior over the binding's rate: Beta(1, 1) to Beta(13, 3).
      const pl = plot(svg, layer, { x: 820, y: 410, w: 860, h: 300, xd: [0, 1], yd: [0, 4.6], xt: [0, 0.25, 0.5, 0.75, 1], yt: [], xl: 'the rate the binding is right', yl: 'posterior density', grid: false });
      const lgam = z => { // Lanczos
        const g = 7, c = [0.99999999999980993, 676.5203681218851, -1259.1392167224028, 771.32342877765313, -176.61502916214059, 12.507343278686905, -0.13857109526572012, 9.9843695780195716e-6, 1.5056327351493116e-7];
        if (z < 0.5) return Math.log(Math.PI / Math.sin(Math.PI * z)) - lgam(1 - z);
        z -= 1; let x = c[0]; for (let i = 1; i < g + 2; i++) x += c[i] / (z + i);
        const tt = z + g + 0.5; return 0.5 * Math.log(2 * Math.PI) + (z + 0.5) * Math.log(tt) - tt + Math.log(x);
      };
      const beta = (a, bb) => x => Math.exp((a - 1) * Math.log(Math.max(x, 1e-9)) + (bb - 1) * Math.log(Math.max(1 - x, 1e-9)) - (lgam(a) + lgam(bb) - lgam(a + bb)));
      const XS = Array.from({ length: 161 }, (_, i) => i / 160);
      const curve = el('path', { d: '', fill: 'rgba(93,210,216,0.10)', stroke: T.p, 'stroke-width': 3 }, svg);
      const mean = el('line', { y1: 410, y2: 710, stroke: T.p, 'stroke-width': 2, 'stroke-dasharray': '5 5' }, svg);
      const mLab = box(layer, { x: 0, y: 380, ax: 0.5, cls: 'code', text: '', style: { fontSize: '20px', color: T.p } });
      const frac = box(layer, { x: 850, y: 430, style: { fontSize: '36px', whiteSpace: 'nowrap' } });
      const nLab = box(layer, { x: 852, y: 560, cls: 'code', text: '', style: { fontSize: '19px', color: C.muted } });
      // The decision.
      const dec = tex(layer, String.raw`\htmlClass{tp k-q}{q} \;\approx\; \htmlClass{tp k-r}{r(t \mid h)} \cdot \htmlClass{tp k-rho2}{\rho} \;=\; 1.00 \times 0.81 \;=\; \htmlClass{tp}{0.81} \;\ge\; \htmlClass{tp}{\theta}=0.3`, { x: 120, y: 800, size: 42 });
      const aR2 = annotate(layer, svg, dec, 'r', 'the read’s chance', { pos: 'above', dist: 22, color: T.p });
      const aRho2 = annotate(layer, svg, dec, 'rho2', 'its arguments’', { pos: 'above', dist: 22, color: T.p, dx: 30 });
      const decN = note(layer, 'flow-show’s own line for the walkthrough’s flow, on the official MCP filesystem server: after search_files, it looks up read_text_file (docs/walkthrough.md). The binding’s chance is crates/stretto-report/src/flow.rs.', { x: 120, y: 880, w: 1680, size: 16 });
      const tWhere = at('bind1', 'where'), tRight = at('bind2', 'right over tried'), tOne = at('bind2', 'one added'), tUni = at('bind2', 'uniform'), tEach = at('bind3', 'each binding'), tTwelve = at('bind3', 'twelve'), tGives = at('bind3', 'gives'), tMult = at('bind4', 'multiplies'), tArgs = at('bind4', 'arguments');
      return t => {
        show(tbl, t, tWhere - 0.3, { d: 0.6, dy: 10 });
        show(rho, t, tRight - 0.2, { d: 0.6, dy: 10 });
        aRho.set(t, tRight + 0.2); aK.set(t, tRight + 0.6); aNn.set(t, tRight + 1.0); aPr.set(t, tUni);
        glow(rho, 'one', t > tOne && t < tOne + 2.4, T.a); glow(rho, 'two', t > tOne && t < tOne + 2.4, T.a);
        pl.set(t, tUni - 0.2);
        // n and k count up to the walkthrough's 14 and 12 as the voice says so,
        // at its rate, and the posterior and the fraction follow them
        const u = ramp(t, tEach, tGives, E.io2);
        const nn = Math.round(14 * u), kk = Math.round(12 * u);
        const a = 1 + kk, bb = 1 + nn - kk;
        if (t > tUni - 0.4 && t < b.t1 + 0.5) live(frac, String.raw`\htmlClass{tp}{\rho} = \dfrac{\htmlClass{tn}{${kk}} \htmlClass{ta}{+\,1}}{\htmlClass{tn}{${nn}} \htmlClass{ta}{+\,2}} = \htmlClass{tp}{${fmt((kk + 1) / (nn + 2))}}`);
        show(frac, t, tUni, { d: 0.4, dy: 0 });
        curve.setAttribute('d', `${line(XS.map(x => [x, Math.min(4.6, beta(a, bb)(x))]), pl.sx, pl.sy)} L ${pl.sx(1)} ${pl.sy(0)} L ${pl.sx(0)} ${pl.sy(0)} Z`);
        const o = ramp(t, tUni - 0.2, tUni + 0.3);
        curve.setAttribute('opacity', o.toFixed(3));
        const m = a / (a + bb);
        mean.setAttribute('x1', pl.sx(m)); mean.setAttribute('x2', pl.sx(m)); mean.setAttribute('opacity', o.toFixed(3));
        mLab.style.left = `${pl.sx(m)}px`; mLab.textContent = `ρ = ${fmt(m)}`; show(mLab, t, tUni, { d: 0.4, dy: 0 });
        nLab.innerHTML = nn === 0 ? 'nothing tried: Beta(1, 1)' : nn === 14 ? `12 right of 14: Beta(${a}, ${bb})` : `${kk} right of ${nn}: Beta(${a}, ${bb})<br><span style="color:${C.faint}">counted up at the walkthrough’s rate</span>`;
        show(nLab, t, tUni + 0.2, { d: 0.4, dy: 0 });
        show(dec, t, tMult, { d: 0.6, dy: 8 });
        aR2.set(t, tMult + 0.5); aRho2.set(t, tArgs);
        show(decN, t, tArgs + 0.6, { d: 0.5, dy: 0 });
        void tTwelve;
      };
    });
  }

  // ========================================================== calibration
  // The reliability diagram of retail's lookups (paper, Figure 6, from the
  // round's rows), then the error per domain (Table 2).
  function sceneCalibration(ctx) {
    scene(ctx, 'calibration', (layer, b) => {
      const at = ctx.at;
      const svg = svgOf(layer);
      const R = M.calibration.retail;
      const pl = plot(svg, layer, { x: 200, y: 190, w: 560, h: 560, xd: [0, 1], yd: [0, 1], xt: [0, 0.25, 0.5, 0.75, 1], yt: [0, 0.25, 0.5, 0.75, 1], xl: 'the lookup’s score', yl: 'share used before the next write' });
      const diag = arrow(svg, `M ${pl.sx(0)} ${pl.sy(0)} L ${pl.sx(1)} ${pl.sy(1)}`, { color: C.subtle, width: 1.5, head: false, dash: '6 6' });
      const diagL = box(layer, { x: pl.sx(0.78), y: pl.sy(0.70), ax: 0, cls: 'anno', text: 'calibrated', style: { color: C.subtle, rotate: '-45deg' } });
      const ex = el('circle', { cx: pl.sx(0.7), cy: pl.sy(0.7), r: 9, fill: 'none', stroke: C.text, 'stroke-width': 2 }, svg);
      const exL = box(layer, { x: pl.sx(0.7) - 18, y: pl.sy(0.7), ax: 1, ay: 0.5, cls: 'anno', text: 'scored 0.7, used 7 in 10', style: { color: C.text } });
      const rad = n => 5 + 2.2 * Math.sqrt(n / 200);
      const mk = (bins, col) => {
        const pts = bins.map(([s, u, n]) => [pl.sx(s), pl.sy(u), rad(n)]);
        const ln = arrow(svg, line(bins.map(([s, u]) => [s, u]), pl.sx, pl.sy), { color: col, width: 2.2, head: false });
        const ds = dots(svg, pts, { color: col });
        const gaps = bins.map(([s, u]) => el('line', { x1: pl.sx(s), x2: pl.sx(s), y1: pl.sy(s), y2: pl.sy(u), stroke: col, 'stroke-width': 1.5, 'stroke-dasharray': '2 3', opacity: 0 }, svg));
        return { ln, ds, gaps };
      };
      const hab = mk(R.habit.bins, T.base), rea = mk(R.reach.bins, T.p);
      const hL = box(layer, { x: pl.sx(0.47), y: pl.sy(0.93) - 16, ax: 0.5, ay: 1, cls: 'anno', html: 'scored on the next step', style: { color: C.muted } });
      const rL = box(layer, { x: pl.sx(0.62), y: pl.sy(0.42), cls: 'anno', html: 'scored on use<br>before the next write', style: { color: T.p } });
      // One bin, read out: the next-step score's lookups rated about one half.
      const hb = R.habit.bins.reduce((a, c) => (Math.abs(c[0] - 0.52) < Math.abs(a[0] - 0.52) ? c : a));
      const ring = el('circle', { cx: pl.sx(hb[0]), cy: pl.sy(hb[1]), r: 20, fill: 'none', stroke: C.text, 'stroke-width': 2, opacity: 0 }, svg);
      const ringLead = arrow(svg, `M ${pl.sx(hb[0]) + 20} ${pl.sy(hb[1])} L ${pl.sx(1) + 34} ${pl.sy(hb[1])}`, { color: C.text, width: 1.3, head: false });
      const ringL = box(layer, { x: pl.sx(1) + 44, y: pl.sy(hb[1]), ay: 0.5, cls: 'anno', html: `one bin of the next-step score:<br>${hb[2].toLocaleString('en-US')} lookups scored about ${hb[0].toFixed(2)},<br>${(hb[1] * 100).toFixed(1)}% of them used`, style: { color: C.text } });
      const sz = note(layer, 'τ²-bench retail, nine agents in replay: a dot per score bin of at least 100 lookups, its area by how many.', { x: 200, y: 830, w: 600, size: 16 });

      // ECE per domain: next step → use before the next write.
      const doms = [['retail', 'Retail'], ['airline', 'Airline'], ['telecom', 'Telecom'], ['solo', 'Telecom, solo']];
      const DX0 = 1080, DW = 620, maxE = 0.18;
      const sxE = v => DX0 + DW * v / maxE;
      const head = label(layer, 'expected calibration error, 10 bins', { x: DX0, y: 210 });
      const better = box(layer, { x: DX0, y: 250, cls: 'anno', html: '← lower is better', style: { color: T.p } });
      const axis = el('line', { x1: DX0, x2: DX0 + DW, y1: 740, y2: 740, stroke: C.borderStrong }, svg);
      const ticks = [0, 0.05, 0.1, 0.15].map(v => el('text', { x: sxE(v), y: 768, class: 'axis', 'text-anchor': 'middle' }, svg, v.toFixed(2)));
      const rows = doms.map(([k, name], i) => {
        const y = 330 + i * 105;
        const a = M.calibration[k].habit.ece, r = M.calibration[k].reach.ece;
        const n = box(layer, { x: DX0 - 24, y, ax: 1, ay: 0.5, cls: 'note', text: name, style: { fontSize: '22px', color: C.text } });
        const track = el('line', { x1: sxE(r), x2: sxE(a), y1: y, y2: y, stroke: C.borderStrong, 'stroke-width': 4 }, svg);
        const da = el('circle', { cx: sxE(a), cy: y, r: 10, fill: T.base }, svg);
        const dr = el('circle', { cx: sxE(a), cy: y, r: 11, fill: T.p }, svg);
        const va = box(layer, { x: sxE(a) + 18, y, ay: 0.5, cls: 'code', text: a.toFixed(3), style: { fontSize: '18px', color: C.subtle } });
        const vr = box(layer, { x: sxE(r), y: y + 22, ax: 0.5, cls: 'code', text: r.toFixed(3), style: { fontSize: '19px', color: T.p } });
        return { n, track, da, dr, va, vr, a, r, y };
      });
      const legend = box(layer, { x: DX0, y: 790, cls: 'note', html: `<span style="color:${T.base}">●</span> next step&nbsp;&nbsp;&nbsp;<span style="color:${T.p}">●</span> use before the next write`, style: { fontSize: '18px', color: C.muted } });
      const scope = note(layer, 'Replay, nine agents (solo telecom: two). Each difference’s 95% interval excludes zero (paper, Table 2; scripts/calibration.py).', { x: DX0, y: 830, w: 720, size: 16 });

      const tCal = at('cal1', 'calibrated'), tSeven = at('cal1', 'seven times'), tNext = at('cal2', 'next step'), tAbove = at('cal2', 'above'), tUnder = at('cal2', 'understates'), tUse = at('cal3', 'use before'), tClose = at('cal3', 'close');
      const tFell = at('cal4', 'fell'), tRetail = at('cal4', 'in retail'), tLower = at('cal4', 'lower is better');
      return t => {
        pl.set(t, b.t0 + 0.2);
        diag.set(ramp(t, tCal - 0.2, tCal + 0.6));
        show(diagL, t, tCal + 0.3, { d: 0.4, dy: 0 });
        const exO = ramp(t, tSeven, tSeven + 0.3) * (1 - ramp(t, tNext, tNext + 0.3));
        ex.setAttribute('opacity', exO.toFixed(3));
        show(exL, t, tSeven, { d: 0.3, dy: 0, out: tNext });
        // the next-step series dims once the right event is shown
        const dim = 1 - 0.55 * ramp(t, tUse, tUse + 0.5);
        hab.ln.set(ramp(t, tNext, tNext + 1.0), dim); hab.ds.set(t, tNext, 0.08);
        hab.ds.g.setAttribute('opacity', dim.toFixed(3));
        hab.gaps.forEach((g, i) => g.setAttribute('opacity', (0.8 * ramp(t, tAbove + i * 0.05, tAbove + 0.3 + i * 0.05) * (1 - ramp(t, tUse, tUse + 0.4))).toFixed(3)));
        show(hL, t, tAbove, { d: 0.4, dy: 0 });
        ring.setAttribute('opacity', (ramp(t, tAbove + 0.5, tAbove + 0.8) * (1 - ramp(t, tUse, tUse + 0.3))).toFixed(3));
        ringLead.set(ramp(t, tAbove + 0.6, tAbove + 1.0), 1 - ramp(t, tUse, tUse + 0.3));
        show(ringL, t, tAbove + 0.8, { d: 0.4, dy: 0, out: tUse });
        rea.ln.set(ramp(t, tUse, tUse + 1.0)); rea.ds.set(t, tUse, 0.08);
        rea.gaps.forEach((g, i) => g.setAttribute('opacity', (0.8 * ramp(t, tClose + i * 0.05, tClose + 0.3 + i * 0.05)).toFixed(3)));
        show(rL, t, tUse + 0.5, { d: 0.4, dy: 0 });
        show(sz, t, tUnder, { d: 0.5, dy: 0 });
        show(head, t, tFell - 0.3, { d: 0.4, dy: 0 });
        axis.setAttribute('opacity', ramp(t, tFell - 0.3, tFell).toFixed(3));
        ticks.forEach(k => k.setAttribute('opacity', ramp(t, tFell - 0.3, tFell).toFixed(3)));
        rows.forEach((r, i) => {
          const s = tFell + i * 0.25;
          show(r.n, t, s, { d: 0.4, dy: 0 });
          const o = ramp(t, s, s + 0.3);
          r.da.setAttribute('opacity', o.toFixed(3));
          show(r.va, t, s + 0.1, { d: 0.3, dy: 0 });
          // the error moves left, to its value under the right event
          const mv = ramp(t, (i ? tRetail + 0.6 : tRetail) + i * 0.25, (i ? tRetail + 1.4 : tRetail + 0.8) + i * 0.25, E.io3);
          const xr = lerp(sxE(r.a), sxE(r.r), mv);
          r.dr.setAttribute('cx', xr.toFixed(1)); r.dr.setAttribute('opacity', (o * ramp(t, tRetail - 0.1, tRetail + 0.1)).toFixed(3));
          r.track.setAttribute('x1', xr.toFixed(1)); r.track.setAttribute('opacity', (o * mv).toFixed(3));
          show(r.vr, t, (i ? tRetail + 1.2 : tRetail + 0.6) + i * 0.25, { d: 0.3, dy: 0 });
        });
        show(better, t, tLower, { d: 0.4, dy: 0, dx: 12 });
        show(legend, t, tFell + 0.4, { d: 0.4, dy: 0 });
        show(scope, t, tLower + 0.4, { d: 0.5, dy: 0 });
      };
    });
  }

  // ================================================================ sweep
  // Net saving against the threshold, GLM-5 in retail (paper, Figure 7, from
  // the round's rows), then the share of the ceiling (Table 3).
  function sceneSweep(ctx) {
    scene(ctx, 'sweep', (layer, b) => {
      const at = ctx.at;
      const svg = svgOf(layer);
      const S = M.sweep['glm-5_enabled_retail_gpt-5.2_4trials'];
      const [cb, cd] = M.costs.retail;
      const th = cd / (cb + cd);
      const pl = plot(svg, layer, { x: 200, y: 200, w: 820, h: 520, xd: [0.1, 0.8], yd: [0, 2.2], xt: [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8], yt: [0, 0.5, 1, 1.5, 2], xl: 'threshold θ', yl: 'net saving, million input tokens', yf: v => v.toFixed(1), xf: v => v.toFixed(1) });
      const thLine = el('line', { x1: pl.sx(th), x2: pl.sx(th), y1: 200, y2: 720, stroke: C.subtle, 'stroke-dasharray': '5 5' }, svg);
      const thL = box(layer, { x: pl.sx(th) + 10, y: 690, cls: 'code', html: `θ* ${fmt(th)}`, style: { fontSize: '18px', color: C.subtle } });
      const hab = arrow(svg, line(S.habit, pl.sx, pl.sy), { color: T.base, width: 3, head: false });
      const rea = arrow(svg, line(S.reach, pl.sx, pl.sy), { color: T.p, width: 3.5, head: false });
      const hd = dots(svg, S.habit.map(([x, y]) => [pl.sx(x), pl.sy(y)]), { color: T.base, r: 6 });
      const rd = dots(svg, S.reach.map(([x, y]) => [pl.sx(x), pl.sy(y)]), { color: T.p, r: 7 });
      const peak = S.reach.reduce((a, c) => (c[1] > a[1] ? c : a));
      const pk = el('circle', { cx: pl.sx(peak[0]), cy: pl.sy(peak[1]), r: 16, fill: 'none', stroke: T.p, 'stroke-width': 2 }, svg);
      const pkL = box(layer, { x: pl.sx(peak[0]) + 22, y: pl.sy(peak[1]) - 22, ay: 1, cls: 'anno', html: `peak at θ = ${peak[0]}: ${fmt(peak[1])}M`, style: { color: T.p } });
      const rL = box(layer, { x: pl.sx(0.62), y: pl.sy(1.12) - 18, ay: 1, cls: 'anno', text: 'use before the write', style: { color: T.p } });
      const hL = box(layer, { x: pl.sx(0.75), y: pl.sy(0.14) + 14, ax: 0.5, cls: 'anno', text: 'next step', style: { color: C.muted } });
      const fall = box(layer, { x: pl.sx(0.38), y: pl.sy(0.62), cls: 'anno', html: 'next-step scores seldom<br>exceed one half', style: { color: C.subtle } });
      // A cursor on θ, reading both curves where it stands.
      const interp = (pts, x) => { for (let i = 1; i < pts.length; i++) if (x <= pts[i][0]) { const [x0, y0] = pts[i - 1], [x1, y1] = pts[i]; return y0 + (y1 - y0) * (x - x0) / (x1 - x0); } return pts[pts.length - 1][1]; };
      const cur = el('line', { y1: 200, y2: 720, stroke: C.text, 'stroke-width': 1.5, opacity: 0 }, svg);
      const cR = el('circle', { r: 8, fill: T.p, stroke: C.bg, 'stroke-width': 2, opacity: 0 }, svg), cH = el('circle', { r: 7, fill: T.base, stroke: C.bg, 'stroke-width': 2, opacity: 0 }, svg);
      const cRead = box(layer, { x: 200, y: 136, cls: 'code', html: '', style: { fontSize: '19px', color: C.muted } });
      const scope = note(layer, 'Replay: GLM-5’s test episodes in τ²-bench retail, learned from four other agents’ sessions; net saving at retail’s counted costs, β·turns saved − δ·detours (paper, Figure 7).', { x: 200, y: 800, w: 820, size: 16 });

      // The share of the ceiling, nine agents (Table 3).
      const BX = 1180, BW = 560;
      const head = label(layer, 'share of retail’s read-only ceiling', { x: BX, y: 250 });
      const up = box(layer, { x: BX, y: 288, cls: 'anno', text: 'higher is better →', style: { color: T.p } });
      const bars = [['next step', 76.2, [71.7, 81.6], T.base], ['use before the write', 86.4, [80.2, 93.1], T.p]].map(([name, v, ci, col], i) => {
        const y = 360 + i * 150;
        const n = box(layer, { x: BX, y, cls: 'note', text: name, style: { fontSize: '22px', color: col === T.p ? T.p : C.muted } });
        const tr = box(layer, { x: BX, y: y + 44, w: BW, h: 30, style: { background: C.surface2, border: `1px solid ${C.border}`, borderRadius: '6px' } });
        const f = box(tr, { x: 0, y: 0, h: 28, w: 0, style: { background: col, borderRadius: '5px' } });
        const ciL = el('line', { x1: BX + BW * ci[0] / 100, x2: BX + BW * ci[1] / 100, y1: y + 59, y2: y + 59, stroke: C.text, 'stroke-width': 2 }, svg);
        const ciT = [ci[0], ci[1]].map(c => el('line', { x1: BX + BW * c / 100, x2: BX + BW * c / 100, y1: y + 51, y2: y + 67, stroke: C.text, 'stroke-width': 2 }, svg));
        const num = box(layer, { x: BX, y: y + 96, cls: 'big num', text: '', style: { fontSize: '36px', color: col === T.p ? T.p : C.muted } });
        const cit = box(layer, { x: BX + 150, y: y + 112, cls: 'code', text: `95% CI ${ci[0]}–${ci[1]}`, style: { fontSize: '16px', color: C.faint } });
        return { n, tr, f, ciL, ciT, num, cit, v };
      });
      const diff = box(layer, { x: BX, y: 680, cls: 'code', html: `difference ${P('+10.2 points')}  ${D('95% CI 6.7–14.2')}`, style: { fontSize: '21px', color: C.muted } });
      const scope2 = note(layer, 'Replay at θ = 0.3, nine agents it never saw, τ²-bench retail (paper, Table 3).', { x: BX, y: 730, w: 560, size: 16 });

      const tBetter = at('sweep1', 'net saving'), tGLM = at('sweep1', 'retail'), tPeaks = at('sweep2', 'peaks'), tSlow = at('sweep2', 'falls slowly'), tNextR = at('sweep3', 'next-step'), tAway = at('sweep3', 'falls away'), tHalf = at('sweep3', 'one half');
      const tNine = at('sweep4', 'nine agents'), tTakes = at('sweep4', 'takes'), tMore = at('sweep4', 'points more');
      return t => {
        pl.set(t, b.t0 + 0.2);
        thLine.setAttribute('opacity', ramp(t, tGLM, tGLM + 0.4).toFixed(3)); show(thL, t, tGLM, { d: 0.4, dy: 0 });
        rea.set(ramp(t, tBetter, tBetter + 1.4, E.io2)); rd.set(t, tBetter, 0.15);
        show(rL, t, tBetter + 1.2, { d: 0.4, dy: 0 });
        pk.setAttribute('opacity', ramp(t, tPeaks, tPeaks + 0.3).toFixed(3)); show(pkL, t, tPeaks + 0.2, { d: 0.4, dy: 0 });
        hab.set(ramp(t, tNextR, tNextR + 1.4, E.io2)); hd.set(t, tNextR, 0.15);
        show(hL, t, tAway, { d: 0.4, dy: 0 });
        show(fall, t, tHalf, { d: 0.4, dy: 0 });
        show(scope, t, tSlow, { d: 0.5, dy: 0 });
        show(head, t, tNine - 0.3, { d: 0.4, dy: 0 }); show(up, t, tNine, { d: 0.4, dy: 0 });
        bars.forEach((r, i) => {
          const s = i ? tTakes : tNine + 0.2;
          show(r.n, t, s, { d: 0.4, dy: 6 }); show(r.tr, t, s, { d: 0.4, dy: 6 });
          const u = ramp(t, s + 0.1, s + 1.0, E.out3);
          r.f.style.width = `${((BW - 2) * r.v / 100 * u).toFixed(1)}px`;
          r.num.textContent = `${(r.v * u).toFixed(1)}%`;
          show(r.num, t, s + 0.1, { d: 0.3, dy: 0 });
          const co = ramp(t, s + 1.0, s + 1.3);
          r.ciL.setAttribute('opacity', co.toFixed(3)); r.ciT.forEach(c => c.setAttribute('opacity', co.toFixed(3)));
          show(r.cit, t, s + 1.0, { d: 0.3, dy: 0 });
        });
        // the cursor walks out past one half as the next-step rule falls away, and comes back to θ*
        const cOn = ramp(t, tAway - 0.3, tAway) * (1 - ramp(t, tNine - 0.2, tNine + 0.2));
        const xv = lerp(lerp(th, 0.78, ramp(t, tAway, tHalf + 0.8, E.io2)), th, ramp(t, tHalf + 1.0, tNine - 0.3, E.io3));
        const yr = interp(S.reach, xv), yh = interp(S.habit, xv);
        cur.setAttribute('x1', pl.sx(xv)); cur.setAttribute('x2', pl.sx(xv)); cur.setAttribute('opacity', (0.6 * cOn).toFixed(3));
        cR.setAttribute('cx', pl.sx(xv)); cR.setAttribute('cy', pl.sy(yr)); cR.setAttribute('opacity', cOn.toFixed(3));
        cH.setAttribute('cx', pl.sx(xv)); cH.setAttribute('cy', pl.sy(yh)); cH.setAttribute('opacity', cOn.toFixed(3));
        cRead.innerHTML = `θ = ${xv.toFixed(2)}   ${P(`use before the write ${yr.toFixed(2)}M`)}   <span style="color:${C.subtle}">next step ${yh.toFixed(2)}M</span>`;
        show(cRead, t, tAway - 0.3, { d: 0.3, dy: 0, out: tNine - 0.2 });
        show(diff, t, tMore, { d: 0.5, dy: 6 });
        show(scope2, t, tMore + 0.5, { d: 0.5, dy: 0 });
      };
    });
  }

  // ================================================================= live
  // Each live task, before and after (paper, Table 5, from the round's
  // rows); then turns saved against the sessions learned from (Figure 9).
  function sceneLive(ctx) {
    scene(ctx, 'live', (layer, b) => {
      const at = ctx.at;
      const svg = svgOf(layer);
      const L = M.live;
      const maxT = Math.ceil(Math.max(...L.flatMap(x => [x.baseline, x.reach])) / 5) * 5;
      const X1 = 330, X2 = 790, Y0 = 210, H = 520;
      const sy = v => Y0 + H - (H * v) / maxT;
      const axis = el('g', {}, svg);
      for (let v = 0; v <= maxT; v += 5) {
        el('line', { x1: X1 - 40, x2: X2 + 40, y1: sy(v), y2: sy(v), stroke: C.border }, axis);
        el('text', { x: X1 - 60, y: sy(v) + 5, class: 'axis', 'text-anchor': 'end' }, axis, String(v));
      }
      const colL = [label(layer, 'without', { x: X1, y: Y0 + H + 22, ax: 0.5 }), label(layer, 'with stretto', { x: X2, y: Y0 + H + 22, ax: 0.5, color: T.p })];
      const yl = label(layer, 'LLM turns per task', { x: X1 - 60, y: Y0 - 40 });
      // Ties and near-ties are spread a little so every task's line shows.
      const jit = window.ST.rng(28);
      const ls = L.map(x => {
        const fewer = x.reach < x.baseline;
        const j1 = (jit() - 0.5) * 18, j2 = (jit() - 0.5) * 18;
        const p = arrow(svg, `M ${X1 + j1} ${sy(x.baseline)} L ${X2 + j2} ${sy(x.reach)}`, { color: fewer ? T.p : '#5a6669', width: fewer ? 2 : 2.2, head: false });
        const a = el('circle', { cx: X1 + j1, cy: sy(x.baseline), r: 4.5, fill: C.muted }, svg);
        const c = el('circle', { cx: X2 + j2, cy: sy(x.reach), r: 4.5, fill: fewer ? T.p : C.muted }, svg);
        return { p, a, c };
      });
      const big = box(layer, { x: 880, y: 300, cls: 'big num', text: '', style: { fontSize: '88px', color: T.p } });
      const bigL = box(layer, { x: 884, y: 400, cls: 'note', html: `fewer LLM turns<br>${D('95% CI 19.1–35.9%')}<br>${D('310.5 → 224 turns')}`, style: { fontSize: '22px', color: C.muted, lineHeight: '1.5' } });
      const cnt = box(layer, { x: 884, y: 560, cls: 'note', html: `${P('23 of 28')} tasks took fewer turns`, style: { fontSize: '22px', color: C.muted } });
      const scope = note(layer, 'Live: GLM-5.3 as agent and user on 20 retail and 8 airline τ²-bench test tasks, the speculator at θ = 0.3, against the recorded baseline (airline: the mean of its two trials). 21 passed, against 24; twenty-eight tasks cannot separate an effect on passes from the agent’s own variance. Paper, Table 5 and §4.4.', { x: 200, y: 810, w: 860, size: 16 });

      // Learning: turns saved against the agent's own sessions, retail.
      const own = M.learning['retail/own'].filter(([n]) => n > 0);
      const all = own[own.length - 1];
      const LX = 1260, LW = 540, LY = 300, LH = 300;
      const sxL = n => LX + LW * Math.log10(n) / Math.log10(400);
      const syL = v => LY + LH - LH * v / 32;
      const lg = el('g', {}, svg);
      el('line', { x1: LX, x2: LX + LW, y1: LY + LH, y2: LY + LH, stroke: C.borderStrong }, lg);
      for (const n of [10, 30, 100, 300]) el('text', { x: sxL(n), y: LY + LH + 26, class: 'axis', 'text-anchor': 'middle' }, lg, String(n));
      for (const v of [0, 10, 20, 30]) { el('line', { x1: LX, x2: LX + LW, y1: syL(v), y2: syL(v), stroke: C.border }, lg); el('text', { x: LX - 12, y: syL(v) + 5, class: 'axis', 'text-anchor': 'end' }, lg, `${v}%`); }
      const lline = arrow(svg, own.map(([n, v], i) => `${i ? 'L' : 'M'} ${sxL(n)} ${syL(v)}`).join(' '), { color: T.p, width: 3, head: false });
      const ld = dots(svg, own.map(([n, v]) => [sxL(n), syL(v)]), { color: T.p, r: 6 });
      const allL = el('line', { x1: LX, x2: LX + LW, y1: syL(all[1]), y2: syL(all[1]), stroke: C.subtle, 'stroke-dasharray': '4 5' }, svg);
      const allT = box(layer, { x: LX + LW, y: syL(all[1]) - 8, ax: 1, ay: 1, cls: 'code', text: `all ${all[0]}`, style: { fontSize: '15px', color: C.subtle } });
      const ten = el('circle', { cx: sxL(10), cy: syL(own[0][1]), r: 15, fill: 'none', stroke: T.p, 'stroke-width': 2 }, svg);
      const tenL = box(layer, { x: sxL(10) + 4, y: syL(own[0][1]) + 26, cls: 'anno', html: `ten sessions: ${P('96%')} of what all give`, style: { color: C.text } });
      const lhead = label(layer, 'LLM turns saved, retail, against the agent’s own sessions', { x: LX, y: LY - 50, size: 15 });
      const lx = label(layer, 'sessions learned from, log scale', { x: LX + LW, y: LY + LH + 50, ax: 1, size: 14 });
      const lscope = note(layer, 'Replay at θ = 0.3: GLM-5, Claude Sonnet 4.5 and Qwen3.5-397B, three random orders each (paper, Table 4 and Figure 9).', { x: LX, y: 700, w: 540, size: 16 });

      const tLive = at('live1', 'live'), tCut = at('live1', 'cut'), tPct = at('live1', 'twenty'), tEach = at('live2', 'each line'), tBefore = at('live2', 'before and after'), tTwenty3 = at('live2', 'twenty-three'), tLearns = at('live3', 'learns fast'), tTen = at('live3', 'ten of'), tAll = at('live3', 'all of them');
      return t => {
        axis.setAttribute('opacity', ramp(t, tLive - 0.2, tLive + 0.3).toFixed(3));
        colL.forEach(c => show(c, t, tLive, { d: 0.4, dy: 0 })); show(yl, t, tLive, { d: 0.4, dy: 0 });
        ls.forEach((l, i) => {
          const s = tCut + i * 0.05;
          l.a.setAttribute('opacity', ramp(t, s, s + 0.2).toFixed(3));
          l.p.set(ramp(t, s + 0.1, s + 0.7, E.io2));
          l.c.setAttribute('opacity', ramp(t, s + 0.6, s + 0.8).toFixed(3));
        });
        const u = ramp(t, tPct, tPct + 1.0, E.out3);
        big.textContent = `−${(27.9 * u).toFixed(1)}%`; show(big, t, tPct, { d: 0.3, dy: 0 });
        show(bigL, t, tPct + 0.6, { d: 0.4, dy: 0 });
        show(cnt, t, tTwenty3, { d: 0.4, dy: 6 });
        show(scope, t, tEach, { d: 0.5, dy: 0 });
        lg.setAttribute('opacity', ramp(t, tLearns - 0.2, tLearns + 0.3).toFixed(3));
        show(lhead, t, tLearns - 0.2, { d: 0.4, dy: 0 }); show(lx, t, tLearns, { d: 0.4, dy: 0 });
        lline.set(ramp(t, tLearns, tLearns + 1.2)); ld.set(t, tLearns, 0.15);
        allL.setAttribute('opacity', ramp(t, tAll, tAll + 0.3).toFixed(3)); show(allT, t, tAll, { d: 0.3, dy: 0 });
        ten.setAttribute('opacity', ramp(t, tTen, tTen + 0.3).toFixed(3)); show(tenL, t, tAll + 0.3, { d: 0.4, dy: 0 });
        show(lscope, t, tAll + 0.6, { d: 0.5, dy: 0 });
        void tBefore;
      };
    });
  }

  // ================================================================ audit
  function sceneAudit(ctx) {
    scene(ctx, 'audit', (layer, b) => {
      const at = ctx.at;
      const src = card(layer, { x: 120, y: 160, w: 980, h: 540, title: 'crates/stretto-report/src/audit.rs', tag: 'fugue' });
      const c = code(src.body, [
        `${D('/// The flow\'s decisions as one fugue program: a categorical choice per')}`,
        `${D('/// decision, at `decision#i`, returning the options picked.')}`,
        `<span class="k">pub fn</span> model(decisions: &amp;[Decision]) -&gt; Model&lt;Vec&lt;usize&gt;&gt; {`,
        `    traverse_vec(sites, |(i, probs)| {`,
        `        ${P('sample')}(`,
        `            ${P('addr!("decision", i)')},`,
        `            ${P('Categorical')}::new(probs).expect(<span class="s">"…"</span>),`,
        `        )`,
        `    })`,
        `}`,
        ``,
        `${D('// the agent\'s own steps, scored under the flow')}`,
        `run(${P('ScoreGivenTrace')} { base, trace }, model(decisions))`,
      ], { x: 22, y: 10, size: 18, lh: 1.65 });
      const out = card(layer, { x: 1160, y: 160, w: 640, h: 540, title: '$ stretto audit', tag: 'notes flow', accent: 'petrol' });
      const o = code(out.body, [
        `${D('3 episodes, 8 decisions scored')}`,
        ``,
        `Agreement     <span class="s">50.0%</span>`,
        `${D('  (60.6% expected)')}`,
        `Surprise      ${P('0.563')} nats per decision`,
        `${D('  log-probability −4.5')}`,
        `Calibration   ECE <span class="s">0.178</span>`,
      ], { x: 22, y: 14, size: 20, lh: 1.8 });
      const svg = svgOf(layer);
      const surp = tex(layer, String.raw`\text{surprise} \;=\; -\tfrac{1}{N}\textstyle\sum_i \log \htmlClass{tp k-p}{p(\text{the agent's choice at } i)}`, { x: 120, y: 760, size: 36 });
      const aS = annotate(layer, svg, surp, 'p', 'under the flow’s categorical at decision i', { pos: 'below', dist: 26, color: T.p });
      const scope = note(layer, 'The walkthrough’s notes flow, scored on three sessions it never saw, as the walkthrough records it: −4.5 nats over 8 decisions is 0.563 a decision. The options at each decision: the lookups offered, handing back, and anything else.', { x: 120, y: 880, w: 1680, size: 16 });
      const tProg = at('audit1', 'program'), tCat = at('audit2', 'categorical'), tAddr = at('audit2', 'address'), tScore = at('audit3', 'scores'), tNats = at('audit3', 'nats');
      return t => {
        show(src, t, tProg - 0.4, { d: 0.6, dy: 10 });
        c.rows.forEach((r, i) => show(r, t, tProg - 0.2 + i * 0.05, { d: 0.3, dy: 0 }));
        const k = t >= tScore ? [12] : t >= tAddr ? [5] : t >= tCat ? [4, 6] : [];
        c.rows.forEach((r, i) => {
          const on = k.includes(i);
          r.style.background = on ? 'rgba(51,192,199,0.10)' : 'transparent';
          r.style.boxShadow = on ? `inset 3px 0 0 ${T.p}` : 'none';
        });
        show(out, t, tScore, { d: 0.6, dy: 10 });
        o.rows.forEach((r, i) => show(r, t, tScore + 0.3 + i * 0.12, { d: 0.3, dy: 0 }));
        o.rows[4].style.background = t > tNats ? 'rgba(51,192,199,0.10)' : 'transparent';
        show(surp, t, tNats, { d: 0.6, dy: 8 });
        aS.set(t, tNats + 0.5);
        show(scope, t, tNats + 0.9, { d: 0.5, dy: 0 });
      };
    });
  }

  // ================================================================== end
  function sceneEnd(ctx) {
    scene(ctx, 'end', (layer, b) => {
      const g = box(layer, { x: 960, y: 430, ax: 0.5, ay: 0.5, html: `<div style="display:flex;align-items:center;gap:30px">${markSvg(132)}${wordmarkSvg(82)}</div>` });
      const links = box(layer, { x: 960, y: 600, ax: 0.5, ay: 0.5, cls: 'code', html: `<span style="color:${C.petrol}">stretto.alexnodeland.com/research/paper</span>  <span style="color:${C.faint}">·</span>  <span style="color:${C.subtle}">github.com/alexnodeland/fugue</span>`, style: { fontSize: '22px' } });
      const L = ctx.L.end1;
      ctx.music.end = +(L.t1 + 0.4).toFixed(3);
      ctx.poster = ctx.at('fugue6', 'learned with') + 1.0;
      return t => {
        show(g, t, b.t0 + 0.1, { d: 0.8, dy: 0, scale: 0.04 });
        show(links, t, L.t0 + 0.4, { d: 0.6, dy: 6 });
      };
    }, { fout: 0 });
  }

  function video(ctx) {
    sceneTitle(ctx);
    sceneEvent(ctx);
    sceneSafety(ctx);
    sceneRule(ctx);
    sceneHabit(ctx);
    sceneFugue(ctx);
    sceneBindings(ctx);
    sceneCalibration(ctx);
    sceneSweep(ctx);
    sceneLive(ctx);
    sceneAudit(ctx);
    sceneEnd(ctx);
  }

  const fonts = ['400 40px KaTeX_Main', 'italic 400 40px KaTeX_Main', '700 40px KaTeX_Main', 'italic 400 40px KaTeX_Math', '400 40px KaTeX_Size1', '400 40px KaTeX_Size2', '400 40px KaTeX_AMS', '400 40px KaTeX_Caligraphic'];
  window.Film = { video, titles: TITLES, fonts };
})();
