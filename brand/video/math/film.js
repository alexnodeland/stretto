// The math film's chapters. Each formula is the paper's (paper/stretto.md:
// the event §2.3, Proposition 1, Propositions 3 and 4, the habit §2.4, the
// thresholds counted per domain in Appendix B, Table 2) or the code's (alpha's
// posterior in crates/stretto-model/src/alpha.rs, the binding's chance in
// crates/stretto-report/src/flow.rs, the audit's program in
// crates/stretto-report/src/audit.rs). The worked values are the walkthrough's
// own flow-show and audit output (docs/walkthrough.md). Times are words of the
// voice-over, never hand-typed seconds.
(() => {
  const { C, E, ramp, el, box, show, card, chip, arrow, code, markSvg, wordmarkSvg } = window.ST;

  const TITLES = {
    event: 'The event', safety: 'Reads only', rule: 'The rule', habit: 'The counts', fugue: 'Alpha, with fugue',
    bindings: 'Arguments', calibration: 'Calibration', audit: 'The audit',
  };

  const scene = (ctx, key, build, opt = {}) => {
    const b = ctx.B[key];
    ctx.scene({ key, t0: b.t0, t1: b.t1, build: layer => build(layer, b), ...opt });
  };
  const svgOf = layer => el('svg', { width: 1920, height: 1080, viewBox: '0 0 1920 1080' }, layer);
  /** A formula, in the mono face. */
  const math = (parent, html, { x, y, size = 40, ax = 0, ay = 0, color = C.text } = {}) =>
    box(parent, { x, y, ax, ay, cls: 'code', html, style: { fontSize: `${size}px`, color, lineHeight: '1.3', whiteSpace: 'nowrap' } });
  const note = (parent, text, { x, y, w, size = 17, color = C.faint, ax = 0 } = {}) =>
    box(parent, { x, y, w, ax, cls: 'note', html: text, style: { fontSize: `${size}px`, color } });
  const P = s => `<span style="color:${C.petrol}">${s}</span>`;
  const A = s => `<span style="color:${C.amber}">${s}</span>`;
  const D = s => `<span style="color:${C.faint}">${s}</span>`;

  // ============================================================ cold open
  // No words: the rule, and a chance sliding across its line.
  function sceneCold(ctx) {
    scene(ctx, 'cold', (layer, b) => {
      const T = b.t0 + 0.3;
      const f = math(layer, `${P('q')}  ≥  ${A('δ')} / (β + ${A('δ')})`, { x: 960, y: 380, ax: 0.5, ay: 0.5, size: 84 });
      const X0 = 560, X1 = 1360, Y = 600, th = 0.3;
      const axis = box(layer, { x: X0, y: Y, w: X1 - X0, h: 2, style: { background: C.borderStrong } });
      const line = box(layer, { x: X0 + (X1 - X0) * th - 1, y: Y - 60, w: 2, h: 76, style: { background: C.text } });
      const lbl = box(layer, { x: X0 + (X1 - X0) * th, y: Y + 34, ax: 0.5, cls: 'code', text: '0.3', style: { fontSize: '20px', color: C.subtle } });
      const ends = [box(layer, { x: X0, y: Y + 34, ax: 0.5, cls: 'code', text: '0', style: { fontSize: '20px', color: C.faint } }), box(layer, { x: X1, y: Y + 34, ax: 0.5, cls: 'code', text: '1', style: { fontSize: '20px', color: C.faint } })];
      const dot = box(layer, { x: 0, y: Y + 1, ax: 0.5, ay: 0.5, w: 22, h: 22, style: { borderRadius: '50%' } });
      const tag = chip(layer, 'looks up', { x: 0, y: Y - 64, ax: 0.5, ay: 0.5, size: 18 });
      return t => {
        show(f, t, T, { d: 0.9, dy: 14 });
        show(axis, t, T + 0.8, { d: 0.6, dy: 0 });
        show(line, t, T + 1.1, { d: 0.5, dy: 0 });
        show(lbl, t, T + 1.1, { d: 0.5, dy: 0 });
        ends.forEach(e => show(e, t, T + 0.9, { d: 0.5, dy: 0 }));
        const u = ramp(t, T + 1.6, T + 5.0, E.io3);
        const q = 0.06 + 0.86 * u;
        const x = X0 + (X1 - X0) * q;
        const over = q >= th;
        dot.style.left = `${x}px`;
        dot.style.background = over ? C.petrolG : C.subtle;
        dot.style.boxShadow = over ? '0 0 0 8px rgba(51,192,199,0.16)' : 'none';
        show(dot, t, T + 1.5, { d: 0.4, dy: 0 });
        tag.style.left = `${x}px`;
        tag.textContent = over ? `looks up · q = ${q.toFixed(2)}` : `hands back · q = ${q.toFixed(2)}`;
        tag.className = `chip ${over ? 'read' : 'ghost'}`;
        tag.style.fontSize = '18px';
        show(tag, t, T + 1.7, { d: 0.4, dy: 0 });
      };
    }, { fin: 0, fout: 0.6 });
  }

  // ================================================================ title
  function sceneTitle(ctx) {
    scene(ctx, 'title', (layer, b) => {
      const T = b.t0 + 0.3;
      const g = box(layer, { x: 960, y: 360, ax: 0.5, ay: 0.5, html: `<div style="display:flex;align-items:center;gap:26px">${markSvg(84)}${wordmarkSvg(54)}</div>` });
      const h = box(layer, { x: 960, y: 500, ax: 0.5, ay: 0.5, text: 'The math, and the probabilistic programs', style: { font: '600 56px/1.1 Inter, sans-serif', letterSpacing: '-0.03em', color: C.text, whiteSpace: 'nowrap' } });
      const sub = box(layer, { x: 960, y: 580, ax: 0.5, ay: 0.5, cls: 'label', text: 'the event · the rule · the counts · the checks' });
      const bars = g.querySelectorAll('rect');
      ctx.music.open = [0, 0.35, 0.65].map(e => +(T + e).toFixed(3));
      return t => {
        bars.forEach((r, i) => {
          const u = ramp(t, T + [0, 0.35, 0.65][i], T + [0, 0.35, 0.65][i] + 0.7, E.out5);
          r.setAttribute('transform', `translate(${r.getAttribute('x')} 0) scale(${u.toFixed(4)} 1) translate(${-r.getAttribute('x')} 0)`);
        });
        show(g, t, T, { d: 0.3, dy: 0 });
        show(h, t, T + 1.1, { d: 0.8, dy: 12 });
        show(sub, t, T + 1.7, { d: 0.7, dy: 6 });
      };
    }, { fin: 0.2 });
  }

  // ================================================================ event
  function sceneEvent(ctx) {
    scene(ctx, 'event', (layer, b) => {
      const at = ctx.at;
      const steps = [
        ['get_user_details', 'call'], ['reply', 'call'], ['get_order_details', 'read'], ['cancel_pending_order', 'write'],
      ];
      let x = 200;
      const chips = steps.map(([s, k], i) => {
        const c = chip(layer, s, { x, y: 300, ay: 0.5, kind: k === 'read' ? 'call' : k, size: 22 });
        const w = c.getBoundingClientRect().width;
        x += w + 70;
        return { c, k, w, x0: x - w - 70 };
      });
      const svg = svgOf(layer);
      const arrows = chips.slice(1).map((ch, i) => arrow(svg, `M ${chips[i].x0 + chips[i].w + 10} 300 L ${ch.x0 - 10} 300`, { color: C.faint }));
      const now = box(layer, { x: chips[1].x0 - 35, y: 220, ax: 0.5, cls: 'label petrol', text: 'stretto decides here' });
      const tick = box(layer, { x: chips[1].x0 - 36, y: 250, w: 2, h: 70, style: { background: C.petrolG } });
      const bracket = box(layer, { x: chips[1].x0 - 20, y: 360, h: 10, style: { borderLeft: `2px solid ${C.petrolG}`, borderRight: `2px solid ${C.petrolG}`, borderBottom: `2px solid ${C.petrolG}` } });
      bracket.style.width = `${chips[3].x0 - chips[1].x0 + 4}px`;
      const uLbl = box(layer, { x: (chips[1].x0 + chips[3].x0) / 2, y: 392, ax: 0.5, cls: 'note', html: `${P('U(x)')}: the reads the agent makes before its next write`, style: { fontSize: '22px', color: C.muted } });
      const q = math(layer, `${P('q<sub>c</sub>')} = Pr( c ∈ ${P('U(x)')} | x )`, { x: 200, y: 520, size: 54 });
      const ineq = math(layer, `Pr( c next | x )  ≤  ${P('q<sub>c</sub>')}`, { x: 200, y: 640, size: 40, color: C.muted });
      // event4: the paper's numbers, after a user's details.
      const B0 = 1080, BW = 640;
      const rows = [['the habit\'s chance an order is read next', 0.64, C.subtle], ['orders read before the next write', 0.94, C.petrolG]].map(([label, v, col], i) => {
        const y = 520 + i * 90;
        const l = box(layer, { x: B0, y, cls: 'note', text: label, style: { fontSize: '22px', color: C.muted } });
        const tr = box(layer, { x: B0, y: y + 36, w: BW, h: 20, style: { background: C.surface2, borderRadius: '5px', border: `1px solid ${C.border}` } });
        const f = box(tr, { x: 0, y: 0, h: 18, w: 0, style: { background: col, borderRadius: '4px' } });
        const n = box(layer, { x: B0 + BW + 20, y: y + 46, ay: 0.5, cls: 'big num', text: '', style: { fontSize: '36px', color: i ? C.petrol : C.muted } });
        return { l, tr, f, n, v };
      });
      const scope = note(layer, 'GLM-5 and Claude Sonnet 4.5, after a user\'s details, under the habit. Replay (paper §4.2).', { x: B0, y: 720, w: 700 });
      const tSteps = b.t0 + 0.2, tPays = at('event1', 'pays'), tWrite = at('event1', 'next write'), tQ = at('event2', 'estimates q'), tAmong = at('event2', 'among'), tNext = at('event3', 'comes next'), tLarger = at('event3', 'far larger');
      const t64 = at('event4', 'the habit'), t94 = at('event4', 'The agents');
      return t => {
        chips.forEach((ch, i) => show(ch.c, t, tSteps + i * 0.25, { d: 0.45, dy: 8 }));
        arrows.forEach((a, i) => a.set(ramp(t, tSteps + 0.2 + i * 0.25, tSteps + 0.6 + i * 0.25)));
        show(now, t, tPays, { d: 0.4, dy: 6 });
        show(tick, t, tPays, { d: 0.4, dy: 0 });
        chips[2].c.className = `chip ${t > tPays + 0.3 ? 'read' : 'call'}`;
        chips[2].c.style.fontSize = '22px';
        bracket.style.transformOrigin = 'left';
        bracket.style.transform = `scaleX(${ramp(t, tWrite, tWrite + 0.6, E.out3).toFixed(3)})`;
        bracket.style.opacity = ramp(t, tWrite, tWrite + 0.2).toFixed(3);
        show(uLbl, t, tAmong, { d: 0.5, dy: 6 });
        show(q, t, tQ, { d: 0.6, dy: 10 });
        show(ineq, t, tNext, { d: 0.6, dy: 10 });
        rows.forEach((r, i) => {
          const s = i ? t94 : t64;
          show(r.l, t, s, { d: 0.4, dy: 6 });
          show(r.tr, t, s, { d: 0.4, dy: 6 });
          const u = ramp(t, s + 0.1, s + 1.0, E.out3);
          r.f.style.width = `${((BW - 2) * r.v * u).toFixed(1)}px`;
          r.n.textContent = (r.v * u).toFixed(2);
          show(r.n, t, s + 0.1, { d: 0.3, dy: 0 });
        });
        show(scope, t, t94 + 0.8, { d: 0.5, dy: 0 });
        void tLarger;
      };
    });
  }

  // =============================================================== safety
  function sceneSafety(ctx) {
    scene(ctx, 'safety', (layer, b) => {
      const at = ctx.at;
      // Two runs of one episode: without stretto, and with it. Reads (petrol)
      // leave the state as it was; writes (amber) move it.
      const runs = [
        { label: 'without stretto', y: 300, steps: ['r', 'r', 'r', 'w', 'r', 'w'] },
        { label: 'with stretto', y: 470, steps: ['r', 'R', 'R', 'w', 'r', 'w'] },
      ];
      const X0 = 420, DX = 190;
      const state = ['S₀', 'S₀', 'S₀', 'S₀', 'S₁', 'S₁', 'S₂'];
      const els = runs.map((run, ri) => {
        const l = box(layer, { x: 120, y: run.y, ay: 0.5, cls: 'label', text: run.label });
        const cells = run.steps.map((k, i) => {
          const c = box(layer, { x: X0 + i * DX, y: run.y, ax: 0.5, ay: 0.5, w: 130, h: 54, cls: 'chip', html: k === 'w' ? 'write' : k === 'R' ? 'read ahead' : 'read', style: { textAlign: 'center', display: 'grid', placeItems: 'center', padding: '0', fontSize: '18px' } });
          c.className = `chip ${k === 'w' ? 'write' : k === 'R' ? 'read' : ''}`;
          return c;
        });
        const sts = state.map((s, i) => box(layer, { x: X0 - DX / 2 + i * DX, y: run.y + 62, ax: 0.5, cls: 'code', text: s, style: { fontSize: '20px', color: C.subtle } }));
        return { l, cells, sts };
      });
      const eq = [3, 5].map(i => box(layer, { x: X0 + (i + 0.5) * DX, y: 418, ax: 0.5, ay: 0.5, cls: 'label petrol', html: '↕ same state' }));
      const prop = box(layer, { x: 120, y: 650, w: 1680, cls: 'note', html: `<span style="color:${C.text};font-weight:600">Proposition 1 (safety).</span> If σ(x) ⊆ 𝓡 for all x, and the agent's calls are those it would make without σ except that answered calls are omitted, then the environment's state after every write is the same with and without σ.`, style: { fontSize: '24px', color: C.muted, lineHeight: '1.5' } });
      const src = note(layer, 'paper/stretto.md §2.2; the proof is in Appendix A.', { x: 120, y: 770 });
      const det = box(layer, { x: X0 + 2 * DX, y: 470, ax: 0.5, ay: 0.5, w: 150, h: 70, style: { border: `2px dashed ${C.petrolLine}`, borderRadius: '12px' } });
      const detL = box(layer, { x: X0 + 2 * DX, y: 540, ax: 0.5, cls: 'label petrol', text: 'a detour: tokens only' });
      const tReads = at('safety1', 'only reads'), tState = at('safety1', 'state'), tWrite = at('safety2', 'same calls'), tSame = at('safety2', 'same state'), tWrong = at('safety3', 'wrong'), tDet = at('safety3', 'detour');
      return t => {
        els.forEach((e, ri) => {
          show(e.l, t, b.t0 + 0.2 + ri * 0.2, { d: 0.4, dy: 0 });
          e.cells.forEach((c, i) => show(c, t, b.t0 + 0.3 + ri * 0.2 + i * 0.1, { d: 0.4, dy: 6 }));
          e.sts.forEach((s, i) => show(s, t, tState + i * 0.08, { d: 0.35, dy: 0 }));
        });
        show(prop, t, tWrite, { d: 0.6, dy: 8 });
        show(src, t, tWrite + 0.4, { d: 0.5, dy: 0 });
        eq.forEach((e, i) => show(e, t, tSame + i * 0.2, { d: 0.4, dy: 0 }));
        show(det, t, tWrong, { d: 0.4, dy: 0, scale: 0.1 });
        show(detL, t, tDet, { d: 0.4, dy: 0 });
        void tReads;
      };
    });
  }

  // ================================================================= rule
  function sceneRule(ctx) {
    scene(ctx, 'rule', (layer, b) => {
      const at = ctx.at;
      const svg = svgOf(layer);
      // A lookup's expected value, in thousands of input tokens, against q:
      // q·β − (1 − q)·δ, crossing zero at θ* = δ/(β + δ) (paper, Figure 2).
      const PX0 = 980, PX1 = 1780, PY0 = 250, PY1 = 760;
      const vmin = -3, vmax = 9;
      const px = q => PX0 + (PX1 - PX0) * q, py = v => PY1 - (PY1 - PY0) * (v - vmin) / (vmax - vmin);
      const zero = arrow(svg, `M ${PX0} ${py(0)} L ${PX1} ${py(0)}`, { color: C.borderStrong, head: false });
      const yax = arrow(svg, `M ${PX0} ${PY1} L ${PX0} ${PY0}`, { color: C.border, head: false });
      const lines = [
        { key: 'live', beta: 6.0, delta: 2.53, color: C.petrolG, w: 3.5, label: 'live: β 6,000 · δ 2,530 → 0.30' },
        { key: 'airline', beta: 7.05, delta: 1.02, color: C.subtle, w: 2, label: 'airline, counted → 0.13' },
        { key: 'telecom', beta: 8.93, delta: 1.18, color: C.faint, w: 2, label: 'telecom, counted → 0.12' },
      ].map((L, i) => {
        const a = arrow(svg, `M ${px(0)} ${py(-L.delta)} L ${px(1)} ${py(L.beta)}`, { color: L.color, width: L.w, head: false });
        const th = L.delta / (L.beta + L.delta);
        const mark = el('circle', { cx: px(th), cy: py(0), r: 7, fill: L.color }, svg);
        const lab = box(layer, { x: PX0 + 24, y: PY0 + 8 + i * 30, cls: 'code', html: `<span style="display:inline-block;width:22px;height:3px;background:${L.color};vertical-align:middle;margin-right:10px"></span>${L.label}`, style: { fontSize: '17px', color: L.color } });
        return { ...L, a, th, mark, lab };
      });
      const xl = box(layer, { x: PX1, y: py(-3) + 16, ax: 1, cls: 'label', text: 'q, the chance of use' });
      const yl = box(layer, { x: PX0 - 14, y: PY0 - 34, cls: 'label', text: 'expected value, thousand input tokens' });
      const f1 = math(layer, `used: ${P('+β')}     unused: ${A('−δ')}`, { x: 120, y: 260, size: 34, color: C.muted });
      const f2 = math(layer, `E u = ${P('q')}·β − (1 − ${P('q')})·${A('δ')}`, { x: 120, y: 380, size: 44 });
      const f3 = math(layer, `act  ⇔  ${P('q')} ≥ θ* = ${A('δ')} / (β + ${A('δ')})`, { x: 120, y: 480, size: 44 });
      const prop = note(layer, 'Propositions 3 and 4, paper §2.3', { x: 120, y: 556 });
      const live = note(layer, 'The live paired run (GLM-5.3, τ²-bench retail and airline, an earlier flow): 260 saved turns saved 1.56M input tokens; 17 detours carried 43,000. Airline and telecom: counted in recorded episodes (paper, Appendix B).', { x: 120, y: 640, w: 760, size: 18 });
      const tSaves = at('rule1', 'saves'), tCosts = at('rule1', 'costs'), tEV = at('rule2', 'expected'), tClears = at('rule3', 'clears'), tLive = at('rule4', 'Live'), tLower = at('rule5', 'airline');
      return t => {
        show(f1, t, tSaves, { d: 0.5, dy: 8 });
        show(f2, t, tEV, { d: 0.6, dy: 10 });
        show(f3, t, tClears - 0.3, { d: 0.6, dy: 10 });
        show(prop, t, tClears + 0.4, { d: 0.5, dy: 0 });
        zero.set(ramp(t, tEV, tEV + 0.6)); yax.set(ramp(t, tEV, tEV + 0.6));
        show(xl, t, tEV + 0.3, { d: 0.4, dy: 0 }); show(yl, t, tEV + 0.3, { d: 0.4, dy: 0 });
        lines.forEach((L, i) => {
          const s = i === 0 ? tLive : tLower + (i - 1) * 0.5;
          L.a.set(ramp(t, s, s + 0.8, E.io2));
          L.mark.setAttribute('opacity', ramp(t, s + 0.7, s + 1.0).toFixed(3));
          show(L.lab, t, s + 0.6, { d: 0.4, dy: 0 });
        });
        show(live, t, tLive + 0.6, { d: 0.6, dy: 0 });
        void tCosts;
      };
    });
  }

  // ================================================================ habit
  function sceneHabit(ctx) {
    scene(ctx, 'habit', (layer, b) => {
      const at = ctx.at;
      const svg = svgOf(layer);
      const step = card(layer, { x: 120, y: 160, w: 760, h: 150, title: 'a step, abstracted', tag: 'the habit' });
      code(step.body, [`${P('tool')}      get_order_details`, `${P('outcome')}   returned`, `${P('feature')}   a feature of its result`], { x: 22, y: 6, size: 19, lh: 1.55 });
      // The contexts: the last two steps, the last one, none, uniform; each
      // the prior mean of the one before it.
      const ctxs = [['h₂', 'the last two steps'], ['h₁', 'the last step'], ['h₀', 'no context'], ['p₋₁', 'uniform']];
      const nodes = ctxs.map(([k, d], i) => box(layer, { x: 120 + i * 330, y: 390, ay: 0.5, cls: 'chip', html: `<span style="color:${C.petrol}">${k}</span>&nbsp; ${d}`, style: { fontSize: '18px' } }));
      const rects = nodes.map(n => n.getBoundingClientRect());
      const links = nodes.slice(1).map((n, i) => arrow(svg, `M ${rects[i].right + 8} 390 L ${120 + (i + 1) * 330 - 10} 390`, { color: C.petrolLine, width: 1.5 }));
      const pm = box(layer, { x: 1460, y: 390, ay: 0.5, cls: 'label petrol', text: '← each backs off to the next, its prior mean' });
      const fp = math(layer, `p<sub>j</sub>(t | h<sub>j</sub>) = ( n(h<sub>j</sub>, t) + ${P('α')} · p<sub>j−1</sub>(t | h<sub>j−1</sub>) ) / ( n(h<sub>j</sub>) + ${P('α')} )`, { x: 120, y: 490, size: 38 });
      const fpN = note(layer, 'a hierarchical Dirichlet back-off model of the next step over the last j ≤ 2 steps: the posterior predictive of a Dirichlet–multinomial at each context, with the shorter context as its prior mean', { x: 120, y: 556, w: 1500, size: 19 });
      const fr = math(layer, `r<sub>j</sub>(t | h<sub>j</sub>) = ( ${P('m(h<sub>j</sub>, t)')} + ${P('α')} · r<sub>j−1</sub>(t | h<sub>j−1</sub>) ) / ( n(h<sub>j</sub>) + ${P('α')} )`, { x: 120, y: 660, size: 38 });
      const frN = note(layer, `${P('m(h, t)')}: the times t was used before the next write. Each tool is its own Beta–Bernoulli, so the r<sub>j</sub> need not sum to one.`, { x: 120, y: 726, w: 1500, size: 19 });
      const src = note(layer, 'paper §2.4 · crates/stretto-model/src/world.rs', { x: 120, y: 790 });
      const tStep = at('habit1', 'Each step'), tTwo = at('habit2', 'back-off'), tDir = at('habit3', 'Dirichlet'), tPrior = at('habit3', 'prior mean'), tReach = at('habit4', 'reach'), tUsed = at('habit4', 'used');
      return t => {
        show(step, t, tStep - 0.2, { d: 0.6, dy: 10 });
        nodes.forEach((n, i) => show(n, t, tTwo + i * 0.2, { d: 0.45, dy: 6 }));
        links.forEach((l, i) => l.set(ramp(t, tPrior + i * 0.2, tPrior + 0.5 + i * 0.2)));
        show(pm, t, tPrior + 0.3, { d: 0.4, dy: 0 });
        show(fp, t, tDir - 0.2, { d: 0.6, dy: 10 });
        show(fpN, t, tDir + 0.6, { d: 0.5, dy: 0 });
        show(fr, t, tReach, { d: 0.6, dy: 10 });
        show(frN, t, tUsed, { d: 0.5, dy: 0 });
        show(src, t, tUsed + 0.4, { d: 0.5, dy: 0 });
      };
    });
  }

  // ================================================================ fugue
  function sceneFugue(ctx) {
    scene(ctx, 'fugue', (layer, b) => {
      const at = ctx.at;
      const src = card(layer, { x: 120, y: 160, w: 1000, h: 640, title: 'crates/stretto-model/src/alpha.rs', tag: 'fugue' });
      const lines = [
        `${D('/// Posterior over `α` given `episodes`, with a `Normal(0, 2)` prior on')}`,
        `${D('/// `log α`, from `n_samples` adaptive-MH draws after `n_samples / 2` warm-up.')}`,
        `<span class="k">let</span> draws = ${P('adaptive_mcmc_chain')}(`,
        `    &amp;mut rng,`,
        `    <span class="k">move</span> || {`,
        `        <span class="k">let</span> episodes = episodes.clone();`,
        `        ${P('sample')}(addr!(<span class="s">"log_alpha"</span>), Normal::new(0.0, 2.0).unwrap())`,
        `            .bind(<span class="k">move</span> |log_alpha| {`,
        `                <span class="k">let</span> alpha = log_alpha.exp();`,
        `                ${P('factor')}(prequential_loglik(&amp;episodes, order, vocab_size, alpha))`,
        `                    .map(<span class="k">move</span> |_| alpha)`,
        `            })`,
        `    },`,
        `    n_samples,`,
        `    n_samples / 2,`,
        `);`,
      ];
      const c = code(src.body, lines, { x: 22, y: 8, size: 18, lh: 1.62 });
      const hl = [[6], [9], [2, 3, 4, 13, 14, 15]];
      const what = [
        box(layer, { x: 1180, y: 230, w: 620, cls: 'note', html: `${P('fugue-ppl')} · a monadic probabilistic programming library for Rust`, style: { fontSize: '24px', color: C.text, lineHeight: '1.4' } }),
        box(layer, { x: 1180, y: 350, w: 620, cls: 'note', html: `${P('sample')} at an address: the prior, log α ~ Normal(0, 2)`, style: { fontSize: '22px', color: C.muted } }),
        box(layer, { x: 1180, y: 430, w: 620, cls: 'note', html: `${P('factor')}: the episodes' likelihood, each step predicted from those before it`, style: { fontSize: '22px', color: C.muted } }),
        box(layer, { x: 1180, y: 530, w: 620, cls: 'note', html: `${P('adaptive_mcmc_chain')}: Metropolis–Hastings draws, after a warm-up; the flow keeps the median, and the report gives the 5th and 95th percentiles`, style: { fontSize: '22px', color: C.muted } }),
      ];
      const tHand = at('fugue1', 'tuned'), tFugue = at('fugue2', 'fugue'), tPrior = at('fugue3', 'normal prior'), tScore = at('fugue3', 'scores'), tMH = at('fugue4', 'Metropolis'), tMed = at('fugue4', 'median');
      return t => {
        show(src, t, b.t0 + 0.15, { d: 0.6, dy: 10 });
        c.rows.forEach((r, i) => show(r, t, b.t0 + 0.3 + i * 0.05, { d: 0.3, dy: 0 }));
        show(what[0], t, tFugue, { d: 0.5, dy: 8 });
        show(what[1], t, tPrior, { d: 0.5, dy: 8 });
        show(what[2], t, tScore, { d: 0.5, dy: 8 });
        show(what[3], t, tMH, { d: 0.5, dy: 8 });
        const k = t >= tMH ? 2 : t >= tScore ? 1 : t >= tPrior ? 0 : -1;
        c.rows.forEach((r, i) => {
          const on = k >= 0 && hl[k].includes(i);
          r.style.background = on ? 'rgba(51,192,199,0.10)' : 'transparent';
          r.style.boxShadow = on ? `inset 3px 0 0 ${C.petrolG}` : 'none';
        });
        void tHand; void tMed;
      };
    });
  }

  // ============================================================= bindings
  function sceneBindings(ctx) {
    scene(ctx, 'bindings', (layer, b) => {
      const at = ctx.at;
      const tbl = card(layer, { x: 120, y: 170, w: 1680, h: 230, title: '$ stretto flow-show notes.flow.json', tag: 'bindings' });
      code(tbl.body, [
        `${D('Lookup')}            ${D('Argument')}   ${D('Bound from')}                                      ${D('Chance it is the agent\'s')}`,
        `read_text_file    path       search_files at ${P('$')} (3 of 14); at ${P('$[*]')} (11 of 14)   ${P('0.81')} (12/14)`,
      ], { x: 22, y: 14, size: 20, lh: 1.9 });
      const f = math(layer, `chance = ( k + 1 ) / ( n + 2 ) = ( 12 + 1 ) / ( 14 + 2 ) = ${P('0.81')}`, { x: 120, y: 470, size: 40 });
      const fN = note(layer, 'right over tried, shrunk toward ½ by a uniform prior: with nothing tried, the chance is ½ · crates/stretto-report/src/flow.rs', { x: 120, y: 534, w: 1600, size: 19 });
      const d = math(layer, `after search_files:  looks up read_text_file  ${P('1.00')} × ${P('0.81')} = ${P('0.81')}  ≥ 0.3`, { x: 120, y: 650, size: 32 });
      const dN = note(layer, 'flow-show\'s own line: the likeliest lookup\'s share of the times the agent made it before its next write, times the chance its bound arguments are the agent\'s. The walkthrough\'s flow, on the official MCP filesystem server (docs/walkthrough.md).', { x: 120, y: 710, w: 1600, size: 18 });
      const tWhere = at('bind1', 'where'), tRight = at('bind2', 'right over tried'), tHalf = at('bind2', 'one half'), tMult = at('bind3', 'multiplies');
      return t => {
        show(tbl, t, tWhere - 0.3, { d: 0.6, dy: 10 });
        show(f, t, tRight, { d: 0.6, dy: 10 });
        show(fN, t, tHalf, { d: 0.5, dy: 0 });
        show(d, t, tMult, { d: 0.6, dy: 10 });
        show(dN, t, tMult + 0.8, { d: 0.5, dy: 0 });
      };
    });
  }

  // ========================================================== calibration
  function sceneCalibration(ctx) {
    scene(ctx, 'calibration', (layer, b) => {
      const at = ctx.at;
      // Paper, Table 2: expected calibration error of the lookups' score,
      // against the next step and against use before the next write.
      const rows = [['Retail', 0.164, 0.083], ['Airline', 0.106, 0.068], ['Telecom', 0.069, 0.049], ['Solo telecom', 0.056, 0.014]];
      const X = 420, W = 900, MAX = 0.18;
      const head = box(layer, { x: 120, y: 180, cls: 'label', text: 'expected calibration error, 10 bins · lower is better' });
      const legend = box(layer, { x: X, y: 230, cls: 'note', html: `<span style="color:${C.subtle}">■</span> scored on the next step&nbsp;&nbsp;&nbsp;&nbsp;<span style="color:${C.petrolG}">■</span> scored on use before the next write`, style: { fontSize: '19px', color: C.muted } });
      const els = rows.map(([name, a, bb], i) => {
        const y = 300 + i * 120;
        const n = box(layer, { x: 120, y: y + 18, cls: 'note', text: name, style: { fontSize: '26px', color: C.text } });
        const mk = (v, col, dy) => {
          const tr = box(layer, { x: X, y: y + dy, w: W, h: 22, style: { background: C.surface2, borderRadius: '5px' } });
          const f = box(tr, { x: 0, y: 0, h: 22, w: 0, style: { background: col, borderRadius: '5px' } });
          const t = box(layer, { x: X + W + 18, y: y + dy + 11, ay: 0.5, cls: 'code', text: v.toFixed(3), style: { fontSize: '20px', color: col === C.petrolG ? C.petrol : C.subtle } });
          return { tr, f, t, v };
        };
        return { n, a: mk(a, C.subtle, 0), b: mk(bb, C.petrolG, 34) };
      });
      const scope = note(layer, 'Replay, nine agents; solo telecom, two. Each difference\'s interval excludes zero (paper, Table 2; scripts/calibration.py).', { x: 120, y: 790, w: 1600, size: 18 });
      const tHonest = at('cal1', 'calibrates'), tNew = at('cal2', 'calibration error'), tOld = at('cal3', 'next step');
      return t => {
        show(head, t, b.t0 + 0.2, { d: 0.5, dy: 0 });
        show(legend, t, b.t0 + 0.4, { d: 0.5, dy: 0 });
        els.forEach((e, i) => {
          show(e.n, t, b.t0 + 0.4 + i * 0.12, { d: 0.4, dy: 6 });
          [[e.a, tOld], [e.b, tNew]].forEach(([s, ts]) => {
            show(s.tr, t, b.t0 + 0.5 + i * 0.12, { d: 0.4, dy: 6 });
            const u = ramp(t, ts + i * 0.12, ts + 0.8 + i * 0.12, E.out3);
            s.f.style.width = `${(W * (s.v / MAX) * u).toFixed(1)}px`;
            show(s.t, t, ts + 0.5 + i * 0.12, { d: 0.3, dy: 0 });
          });
        });
        show(scope, t, tOld + 1.0, { d: 0.5, dy: 0 });
        void tHonest;
      };
    });
  }

  // ================================================================ audit
  function sceneAudit(ctx) {
    scene(ctx, 'audit', (layer, b) => {
      const at = ctx.at;
      const src = card(layer, { x: 120, y: 160, w: 980, h: 520, title: 'crates/stretto-report/src/audit.rs', tag: 'fugue' });
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
      const out = card(layer, { x: 1160, y: 160, w: 640, h: 520, title: '$ stretto audit', tag: 'notes flow', accent: 'petrol' });
      const o = code(out.body, [
        `${D('3 episodes, 8 decisions scored')}`,
        ``,
        `Agreement     <span class="s">50.0%</span>`,
        `${D('  (60.6% expected)')}`,
        `Surprise      ${P('0.563')} nats per decision`,
        `${D('  log-probability −4.5')}`,
        `Calibration   ECE <span class="s">0.178</span>`,
      ], { x: 22, y: 14, size: 20, lh: 1.8 });
      const scope = note(layer, 'The walkthrough\'s notes flow, scored on three sessions it never saw, as the walkthrough video records it. Options at each decision: the lookups offered, handing back, and anything else.', { x: 120, y: 720, w: 1680, size: 18 });
      const tProg = at('audit1', 'program'), tCat = at('audit2', 'categorical'), tAddr = at('audit2', 'address'), tScore = at('audit3', 'scores'), tNats = at('audit3', 'nats');
      return t => {
        show(src, t, tProg - 0.4, { d: 0.6, dy: 10 });
        c.rows.forEach((r, i) => show(r, t, tProg - 0.2 + i * 0.05, { d: 0.3, dy: 0 }));
        const k = t >= tScore ? [12] : t >= tAddr ? [5] : t >= tCat ? [4, 6] : [];
        c.rows.forEach((r, i) => {
          const on = k.includes(i);
          r.style.background = on ? 'rgba(51,192,199,0.10)' : 'transparent';
          r.style.boxShadow = on ? `inset 3px 0 0 ${C.petrolG}` : 'none';
        });
        show(out, t, tScore, { d: 0.6, dy: 10 });
        o.rows.forEach((r, i) => show(r, t, tScore + 0.3 + i * 0.12, { d: 0.3, dy: 0 }));
        o.rows[4].style.background = t > tNats ? 'rgba(51,192,199,0.10)' : 'transparent';
        show(scope, t, tNats + 0.6, { d: 0.5, dy: 0 });
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
      ctx.poster = ctx.at('rule5', 'lower') + 0.9;
      return t => {
        show(g, t, b.t0 + 0.1, { d: 0.8, dy: 0, scale: 0.04 });
        show(links, t, L.t0 + 0.4, { d: 0.6, dy: 6 });
      };
    }, { fout: 0 });
  }

  function video(ctx) {
    sceneCold(ctx);
    sceneTitle(ctx);
    sceneEvent(ctx);
    sceneSafety(ctx);
    sceneRule(ctx);
    sceneHabit(ctx);
    sceneFugue(ctx);
    sceneBindings(ctx);
    sceneCalibration(ctx);
    sceneAudit(ctx);
    sceneEnd(ctx);
  }

  window.Film = { video, titles: TITLES };
})();
