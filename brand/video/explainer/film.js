// The explainer's chapters. Each is one diagram on a still stage, built up as
// the voice names its parts: every time below is a word of the voice-over
// (ctx.at(line, phrase)), never a hand-typed second, so a new voice-over
// re-times the film by itself.
//
// What the frame shows is the real thing: τ²-bench retail's own tools and
// their read and write marks, the proxy's own wording for the reads it adds
// to a result, counts and bindings from docs/examples/retail-10-sessions.flow.json,
// the commands from docs/install.md and the walkthrough, and the numbers of
// docs/results/claims.md with their scope.
(() => {
  const { C, clamp, lerp, E, ramp, fade, el, box, show, card, chip, arrow, packet, code, type } = window.ST;

  const markSvg = (h, ink = '#eceff0', acc = C.petrolG) => {
    const w = (h * 31) / 30;
    return `<svg width="${w}" height="${h}" viewBox="0 0 31 30" style="display:block"><rect x="0" y="0.5" width="20" height="7" rx="1.5" fill="${ink}"/><rect x="6" y="11.5" width="20" height="7" rx="1.5" fill="${acc}"/><rect x="11" y="22.5" width="20" height="7" rx="1.5" fill="${acc}"/></svg>`;
  };
  const wordmarkSvg = (h, fill = '#eceff0') => {
    const W = window.STRETTO_WORDMARK;
    const [, , vw, vh] = W.viewBox.split(' ').map(Number);
    return `<svg width="${(h * vw) / vh}" height="${h}" viewBox="${W.viewBox}" style="display:block"><path fill="${fill}" transform="${W.transform}" d="${W.d}"/></svg>`;
  };

  const TITLES = {
    turns: 'How an agent works', decided: 'Turns the tools decide', idea: 'Reading ahead', reads: 'Reads only',
    learn: 'Learning a flow', console: 'The console', rule: 'The reach decider', results: 'Live results', prompt: 'Against a prompt', start: 'Get started',
  };

  // The τ²-bench retail customer of the example: a record with two orders.
  const RECORD = [
    '<span class="p">{</span>',
    '  <span class="k">"user_id"</span><span class="p">:</span> <span class="s">"mei_tan_7"</span><span class="p">,</span>',
    '  <span class="k">"name"</span><span class="p">:</span> <span class="s">"Mei Tan"</span><span class="p">,</span>',
    '  <span class="k">"orders"</span><span class="p">:</span> <span class="p">[</span><span class="s" data-id="a">"#W7a"</span><span class="p">,</span> <span class="s" data-id="b">"#W7b"</span><span class="p">],</span>',
    '  <span class="k">"payment_methods"</span><span class="p">:</span> <span class="p">{ … }</span>',
    '<span class="p">}</span>',
  ];

  // ------------------------------------------------------------ helpers
  const scene = (ctx, key, build, opt = {}) => {
    const b = ctx.B[key];
    ctx.scene({ key, t0: b.t0, t1: b.t1, build: layer => build(layer, b), ...opt });
  };
  const svgOf = layer => el('svg', { width: 1920, height: 1080, viewBox: '0 0 1920 1080' }, layer);
  /** A turn: a numbered pill on the turns row. */
  function turnPill(parent, n, label, { x, y, kind = 'call', w = 300 }) {
    const p = box(parent, { x, y, w: w ?? undefined, ay: 0.5, cls: `chip ${kind}` });
    p.style.boxSizing = 'border-box';
    p.style.fontSize = '19px';
    p.style.display = 'flex';
    p.style.gap = '12px';
    p.style.alignItems = 'center';
    p.innerHTML = `<span style="color:${C.faint};font-size:15px;letter-spacing:.08em">TURN ${n}</span><span>${label}</span>`;
    return p;
  }
  /** Highlight a span of code (data-id) by adding the hl class from time `at`. */
  const hl = (n, on) => n && n.classList.toggle('hl', on);

  // ============================================================== cold open
  // No words: a call goes out, its result comes back with the two reads the
  // agent would have made next riding in it, and the turns close up.
  function coldOpen(ctx, layer, t0, { loop = null } = {}) {
    const svg = svgOf(layer);
    const res = card(layer, { x: 960, y: 190, w: 1000, h: 420, ax: 0.5, title: '← get_user_details <span style="color:#606b6f">{"user_id": "mei_tan_7"}</span>', tag: 'tool result' });
    const rec = code(res.body, [
      '<span class="p">{</span> <span class="k">"user_id"</span><span class="p">:</span> <span class="s">"mei_tan_7"</span><span class="p">,</span> <span class="k">"orders"</span><span class="p">:</span> <span class="p">[</span><span class="s hl-a">"#W7a"</span><span class="p">,</span> <span class="s hl-b">"#W7b"</span><span class="p">], … }</span>',
    ], { x: 22, y: 20, size: 23 });
    const sep = box(res.body, { x: 22, y: 86, w: 930, cls: 'code', html: '<span class="pr">--- Also looked up automatically (current results; no need to repeat these calls) ---</span>' });
    sep.style.fontSize = '19px';
    const look = [
      code(res.body, ['<span class="pr">get_order_details</span> <span class="p">{"order_id":"#W7a"}:</span>', '<span class="p">{</span> <span class="k">"status"</span><span class="p">:</span> <span class="s">"pending"</span><span class="p">,</span> <span class="k">"items"</span><span class="p">: [ … ] }</span>'], { x: 22, y: 140, size: 21 }),
      code(res.body, ['<span class="pr">get_order_details</span> <span class="p">{"order_id":"#W7b"}:</span>', '<span class="p">{</span> <span class="k">"status"</span><span class="p">:</span> <span class="s">"delivered"</span><span class="p">,</span> <span class="k">"items"</span><span class="p">: [ … ] }</span>'], { x: 22, y: 248, size: 21 }),
    ];
    const rail = box(res.body, { x: 0, y: 128, w: 4, h: 0, style: { background: C.petrolG, borderRadius: '2px' } });
    // The turns row: five turns, then three.
    const labels = ['get_user_details', 'get_order_details #W7a', 'get_order_details #W7b', 'cancel_pending_order', 'reply'];
    const kinds = ['call', 'call', 'call', 'write', 'call'];
    const pills = labels.map((l, i) => turnPill(layer, i + 1, l, { x: 0, y: 760, kind: kinds[i], w: null }));
    const ws = pills.map(p => p.getBoundingClientRect().width + 2);
    const counter = box(layer, { x: 960, y: 840, ax: 0.5, ay: 0.5, cls: 'label', html: '' });
    const T = t0;
    const tRes = T + 0.5, tLook = T + 2.0, tFold = T + 4.1;
    return t => {
      const tl = loop ? T + ((t - T) % loop) : t;
      show(res, tl, tRes, { d: 0.6, dy: 16 });
      rec.style.opacity = 1;
      const u1 = ramp(tl, tLook, tLook + 0.6, E.out4), u2 = ramp(tl, tLook + 0.35, tLook + 0.95, E.out4);
      show(sep, tl, tLook - 0.2, { d: 0.4, dy: 0 });
      show(look[0], tl, tLook, { d: 0.55, dy: -18 });
      show(look[1], tl, tLook + 0.35, { d: 0.55, dy: -18 });
      rail.style.height = `${(u1 * 100 + u2 * 108).toFixed(1)}px`;
      res.querySelectorAll('.hl-a, .hl-b').forEach(n => n.classList.toggle('hl', tl > tLook - 0.4));
      // Close up: turns 2 and 3 fold away, the rest slide left.
      const fold = ramp(tl, tFold, tFold + 0.9, E.io3);
      const gap = 18;
      const widths = ws.map((w, i) => (i === 1 || i === 2 ? w * (1 - fold) : w));
      const total = widths.reduce((a, b) => a + b, 0) + gap * (ws.length - 1 - 2 * fold);
      let x = 960 - total / 2;
      pills.forEach((p, i) => {
        const folding = i === 1 || i === 2;
        p.style.left = `${x.toFixed(1)}px`;
        p.style.width = `${widths[i].toFixed(1)}px`;
        const o = ramp(tl, T + 0.9 + i * 0.12, T + 1.4 + i * 0.12, E.out3) * (folding ? 1 - ramp(tl, tFold, tFold + 0.5) : 1);
        p.style.opacity = o.toFixed(3);
        p.style.overflow = 'hidden';
        if (folding) { p.classList.toggle('read', tl > tLook); p.classList.toggle('call', tl <= tLook); }
        x += widths[i] + (folding ? gap * (1 - fold) : gap);
      });
      const n = tl < tFold + 0.45 ? 5 : 3;
      counter.innerHTML = `<span class="num" style="color:${n === 3 ? C.petrol : C.subtle}">${n}</span>&nbsp; LLM TURNS`;
      counter.style.opacity = ramp(tl, T + 1.2, T + 1.7).toFixed(3);
      void svg;
    };
  }

  function sceneCold(ctx) {
    scene(ctx, 'cold', (layer, b) => coldOpen(ctx, layer, b.t0 + 0.3), { fin: 0, fout: 0.6 });
  }

  // ================================================================ title
  function sceneTitle(ctx) {
    scene(ctx, 'title', (layer, b) => {
      const T = b.t0 + 0.35;
      // The mark: three bars, each entering before the last has finished.
      const S = 5.2; // px per mark unit
      const g = box(layer, { x: 960, y: 440, ax: 0.5, ay: 0.5, w: 31 * S + 24 + 600, h: 30 * S });
      const bars = [[0, 0.5, '#eceff0'], [6, 11.5, C.petrolG], [11, 22.5, C.petrolG]].map(([x, y, fill]) =>
        box(g, { x: x * S, y: y * S, w: 20 * S, h: 7 * S, style: { background: fill, borderRadius: `${1.5 * S}px`, transformOrigin: 'left center' } }));
      const word = box(g, { x: 31 * S + 34, y: 15 * S, ay: 0.5, html: wordmarkSvg(30 * S * 0.62) });
      const tag = box(layer, { x: 960, y: 600, ax: 0.5, ay: 0.5, text: 'Read ahead of your agent.', style: { font: '500 44px/1 Inter, sans-serif', letterSpacing: '-0.02em', color: C.text } });
      const sub = box(layer, { x: 960, y: 664, ax: 0.5, ay: 0.5, cls: 'label', text: 'an MCP proxy · open source · Rust' });
      const entries = [0, 0.35, 0.65];
      ctx.music.open = entries.map(e => +(T + e).toFixed(3));
      return t => {
        bars.forEach((bar, i) => {
          const u = ramp(t, T + entries[i], T + entries[i] + 0.7, E.out5);
          bar.style.transform = `scaleX(${u.toFixed(4)})`;
          bar.style.opacity = ramp(t, T + entries[i], T + entries[i] + 0.15).toFixed(3);
        });
        show(word, t, T + 1.1, { d: 0.8, dy: 0, dx: -16 });
        show(tag, t, T + 1.7, { d: 0.7, dy: 10 });
        show(sub, t, T + 2.2, { d: 0.7, dy: 6 });
        // On the second line, the lockup rises a little to make room for its words.
        const lift = ramp(t, ctx.L.title2.t0 - 0.4, ctx.L.title2.t0 + 0.6, E.io3);
        layer.style.transform = `translateY(${(-lift * 30).toFixed(2)}px)`;
      };
    }, { fin: 0.2 });
  }

  // ================================================================ turns
  function sceneTurns(ctx) {
    scene(ctx, 'turns', (layer, b) => {
      const at = ctx.at;
      const L1 = ctx.L.turns1, L4 = ctx.L.turns4;
      // Left: the conversation, which grows a line each turn.
      const conv = card(layer, { x: 120, y: 150, w: 470, h: 680, title: 'conversation', tag: 'context' });
      const bubble = box(conv.body, { x: 22, y: 22, w: 400, cls: 'note', html: '<span style="color:#909a9d;font:500 15px/1 JetBrains Mono;letter-spacing:.1em">CUSTOMER</span><br><span style="color:#eceff0">Please cancel my pending order.</span>' });
      const convRows = ['→ get_user_details', '← { "orders": [ … ] }', '→ get_order_details', '← { "status": "pending" }', '→ get_order_details', '← { "status": "delivered" }', '→ cancel_pending_order', '← { "status": "cancelled" }'].map((s, i) =>
        box(conv.body, { x: 22, y: 118 + i * 52, cls: 'code', text: s, style: { fontSize: '19px', color: s[0] === '→' ? C.muted : C.faint } }));
      // Right: the turns. Each reads the whole conversation (its bar, longer
      // each turn), then picks one step.
      const X0 = 700, X1 = 1800;
      const head = [box(layer, { x: X0, y: 170, cls: 'label', text: 'LLM turn' }), box(layer, { x: X0 + 150, y: 170, cls: 'label', text: 'reads the whole conversation' }), box(layer, { x: 1390, y: 170, cls: 'label', text: 'then one step' })];
      const steps = ['get_user_details', 'get_order_details', 'get_order_details', 'cancel_pending_order', 'reply'];
      const rows = steps.map((s, i) => {
        const y = 240 + i * 92;
        const num = box(layer, { x: X0 + 10, y: y + 20, ay: 0.5, cls: 'code', text: String(i + 1), style: { fontSize: '26px', color: C.subtle } });
        const track = box(layer, { x: X0 + 150, y: y + 6, w: 470, h: 28, style: { borderRadius: '6px', background: C.surface2, border: `1px solid ${C.border}` } });
        const fill = box(track, { x: 0, y: 0, h: 26, w: 0, style: { borderRadius: '5px', background: 'linear-gradient(90deg, #2a3538, #46565a)' } });
        const stepChip = chip(layer, s, { x: 1390, y: y + 20, ay: 0.5, kind: s === 'cancel_pending_order' ? 'write' : 'call', size: 19 });
        return { num, track, fill, stepChip, y, full: 120 + i * 80 };
      });
      const more = box(layer, { x: X0 + 10, y: 240 + 5 * 92 + 4, cls: 'code', text: '⋮', style: { fontSize: '30px', color: C.faint } });
      // turns3: the two kinds of step.
      const either = [chip(layer, 'answer the customer', { x: 1390, y: 336, ay: 0.5, kind: 'ghost', size: 18 }), chip(layer, 'or call a tool', { x: 1390, y: 388, ay: 0.5, kind: 'ghost', size: 18 })];
      const cost = box(layer, { x: X0 + 150, y: 790, ay: 0.5, cls: 'note', html: '' });
      const tRow = [at('turns1', 'turns'), ...[1, 2, 3, 4].map(i => lerp(at('turns4', 'Each'), at('turns4', 'twenty', 'end'), i / 4))];
      const tRead = at('turns2', 'reads'), tWhole = at('turns2', 'conversation', 'end'), tStep = at('turns2', 'one next step');
      const tAns = at('turns3', 'answers'), tTool = at('turns3', 'calls a tool');
      const tCost = at('turns4', 'seconds'), tDozens = at('turns4', 'ten or twenty');
      return t => {
        show(conv, t, b.t0 + 0.1, { d: 0.6 });
        show(bubble, t, b.t0 + 0.4);
        head.forEach((h, i) => show(h, t, L1.t0 + 0.1 + i * 0.1, { d: 0.5, dy: 6 }));
        rows.forEach((r, i) => {
          const tr = i === 0 ? tRow[0] : tRow[i];
          const fillAt = i === 0 ? tRead : tr + 0.1;
          const fillEnd = i === 0 ? tWhole : tr + 0.6;
          const stepAt = i === 0 ? tStep : tr + 0.55;
          show(r.num, t, tr, { d: 0.4, dy: 8 });
          show(r.track, t, tr, { d: 0.4, dy: 8 });
          r.fill.style.width = `${(ramp(t, fillAt, fillEnd, E.io2) * (r.full + 140)).toFixed(1)}px`;
          show(r.stepChip, t, stepAt, { d: 0.45, dy: 0, dx: -10 });
          // The conversation grows by the call and its result.
          if (convRows[2 * i]) show(convRows[2 * i], t, stepAt + 0.1, { d: 0.4, dy: 6 });
          if (convRows[2 * i + 1]) show(convRows[2 * i + 1], t, stepAt + 0.4, { d: 0.4, dy: 6 });
        });
        show(either[0], t, tAns, { d: 0.4, dy: 6, out: tCost - 0.2 });
        show(either[1], t, tTool, { d: 0.4, dy: 6, out: tCost - 0.2 });
        cost.innerHTML = `<span style="color:#eceff0">each turn:</span> seconds, and thousands of input tokens`;
        show(cost, t, tCost, { d: 0.5, dy: 6 });
        show(more, t, tDozens, { d: 0.5, dy: 6 });
      };
    });
  }

  // ============================================================== decided
  function sceneDecided(ctx) {
    scene(ctx, 'decided', (layer, b) => {
      const at = ctx.at;
      const svg = svgOf(layer);
      const res = card(layer, { x: 120, y: 230, w: 780, h: 420, title: '← get_user_details', tag: 'result of turn 1' });
      const rec = code(res.body, RECORD, { x: 24, y: 22, size: 25, lh: 1.75 });
      const ids = [...rec.querySelectorAll('[data-id]')];
      const T2 = turnPill(layer, 2, 'get_order_details <span style="color:#909a9d">{"order_id": "#W7a"}</span>', { x: 1080, y: 380, w: 680 });
      const T3 = turnPill(layer, 3, 'get_order_details <span style="color:#909a9d">{"order_id": "#W7b"}</span>', { x: 1080, y: 520, w: 680 });
      // From each order id in the record to the turn that reads it.
      const r = ids.map(n => n.getBoundingClientRect());
      // Both from the record's orders line, at the card's edge, to the turns that read them.
      const oy = (r[0].top + r[0].bottom) / 2, ox = 904;
      const a1 = arrow(svg, `M ${ox} ${oy} C ${ox + 90} ${oy}, 980 380, 1068 380`, { color: C.petrolLine, width: 2 });
      const a2 = arrow(svg, `M ${ox} ${oy} C ${ox + 90} ${oy}, 980 520, 1068 520`, { color: C.petrolLine, width: 2 });
      const stub = arrow(svg, `M ${r[1].right + 6} ${oy} L ${ox} ${oy}`, { color: C.petrolLine, width: 2, head: false });
      layer.appendChild(svg);
      const lbl = box(layer, { x: 1080, y: 610, cls: 'label petrol', text: 'decided by the result, not the customer' });
      const cost = [box(layer, { x: 1760, y: 432, ax: 1, ay: 0.5, cls: 'label', text: '' }), box(layer, { x: 1760, y: 572, ax: 1, ay: 0.5, cls: 'label', text: '' })];
      const ghostT = [box(layer, { x: 1080, y: 300, cls: 'label', text: 'the next two turns' })];
      const typeT = [T2.lastElementChild, T3.lastElementChild];
      const full = typeT.map(n => n.innerHTML);
      const tIds = at('decided2', 'two orders'), tNext = at('decided3', 'next two turns'), tRead = at('decided3', 'read those');
      const tTools = at('decided4', 'The tools'), tTyping = at('decided4', 'typing');
      return t => {
        show(res, t, b.t0 + 0.1, { d: 0.6 });
        ids.forEach(n => hl(n, t > tIds));
        show(ghostT[0], t, tNext, { d: 0.4, dy: 6 });
        show(T2, t, tNext + 0.1, { d: 0.5, dx: -12, dy: 0 });
        show(T3, t, tNext + 0.3, { d: 0.5, dx: -12, dy: 0 });
        stub.set(ramp(t, tRead - 0.25, tRead, E.io2));
        a1.set(ramp(t, tRead, tRead + 0.6, E.io2));
        a2.set(ramp(t, tRead + 0.2, tRead + 0.8, E.io2));
        T2.classList.toggle('read', t > tRead + 0.5);
        T3.classList.toggle('read', t > tRead + 0.7);
        show(lbl, t, tTools, { d: 0.5, dy: 6 });
        // "typing it out": each turn's call is typed again, one character at a time.
        const plain = ['get_order_details {"order_id": "#W7a"}', 'get_order_details {"order_id": "#W7b"}'];
        typeT.forEach((n, i) => {
          if (t < tTyping) n.innerHTML = full[i];
          else type(n, plain[i], t, tTyping + i * 0.5, 30);
        });
        cost.forEach((c, i) => { c.textContent = 'one LLM turn'; show(c, t, tTyping + 0.3 + i * 0.5, { d: 0.4, dy: 0 }); });
      };
    });
  }

  // ================================================================= idea
  function sceneIdea(ctx) {
    scene(ctx, 'idea', (layer, b) => {
      const at = ctx.at;
      const svg = svgOf(layer);
      const Y = 330;
      const agent = card(layer, { x: 330, y: Y, w: 330, h: 230, ax: 0.5, ay: 0.5, title: 'agent', tag: 'any MCP host' });
      box(agent.body, { x: 22, y: 22, w: 286, cls: 'note', html: '<span style="color:#eceff0">LLM</span><br>reads the conversation,<br>picks a step' });
      const proxy = card(layer, { x: 960, y: Y, w: 330, h: 230, ax: 0.5, ay: 0.5, title: 'stretto-proxy', tag: 'MCP proxy', accent: 'petrol' });
      box(proxy.body, { x: 22, y: 26, html: markSvg(52) });
      box(proxy.body, { x: 100, y: 22, w: 210, cls: 'note', html: '<span style="color:#5dd2d8">the flow</span><br>makes the reads<br>that come next' });
      const server = card(layer, { x: 1590, y: Y, w: 330, h: 230, ax: 0.5, ay: 0.5, title: 'MCP server', tag: 'your tools' });
      code(server.body, ['get_user_details', 'get_order_details', '<span class="am">cancel_pending_order</span>'], { x: 22, y: 12, size: 19, lh: 1.8 });
      // Wires: agent ↔ server first; stretto slides into the middle.
      const direct = arrow(svg, `M 495 ${Y} L 1425 ${Y}`, { color: C.border, width: 2, head: false });
      const w1 = arrow(svg, `M 495 ${Y - 40} L 795 ${Y - 40}`, { color: C.faint, width: 2 });
      const w2 = arrow(svg, `M 1125 ${Y - 40} L 1425 ${Y - 40}`, { color: C.faint, width: 2 });
      const r2 = arrow(svg, `M 1425 ${Y + 40} L 1125 ${Y + 40}`, { color: C.faint, width: 2 });
      const r1 = arrow(svg, `M 795 ${Y + 40} L 495 ${Y + 40}`, { color: C.petrolG, width: 2.5 });
      const q1 = arrow(svg, `M 1125 ${Y - 8} L 1425 ${Y - 8}`, { color: C.petrolG, width: 2.5 });
      const q2 = arrow(svg, `M 1425 ${Y + 12} L 1125 ${Y + 12}`, { color: C.petrolG, width: 2.5, head: false });
      const callTag = chip(layer, 'get_user_details', { x: 645, y: Y - 72, ax: 0.5, ay: 0.5, size: 16 });
      const readTag = chip(layer, 'get_order_details ×2', { x: 1275, y: Y - 92, ax: 0.5, ay: 0.5, kind: 'read', size: 16 });
      const pk = [packet(svg, { color: C.subtle, r: 6 }), packet(svg, { color: C.petrolG }), packet(svg, { color: C.petrolG }), packet(svg, { color: C.petrolG, r: 8 })];
      // The bundle that comes back: the result, with the two reads in it.
      const bundle = card(layer, { x: 120, y: 560, w: 820, h: 196, title: '← get_user_details', tag: 'one tool result' });
      code(bundle.body, ['<span class="p">{ </span><span class="k">"user_id"</span><span class="p">: </span><span class="s">"mei_tan_7"</span><span class="p">, </span><span class="k">"orders"</span><span class="p">: [</span><span class="s">"#W7a"</span><span class="p">, </span><span class="s">"#W7b"</span><span class="p">], … }</span>'], { x: 22, y: 14, size: 19 });
      const bundleReads = code(bundle.body, ['<span class="pr">--- Also looked up automatically ---</span>', '<span class="pr">get_order_details</span> <span class="p">{"order_id":"#W7a"}: { … }</span>', '<span class="pr">get_order_details</span> <span class="p">{"order_id":"#W7b"}: { … }</span>'], { x: 22, y: 54, size: 19, lh: 1.55 });
      // The turns, five, then three.
      const labels = ['get_user_details', 'get_order_details', 'get_order_details', 'cancel_pending_order', 'reply'];
      const ws = [190, 196, 196, 236, 92];
      const tl = labels.map((l, i) => {
        const p = box(layer, { x: 0, y: 640, ay: 0.5, cls: `chip ${i === 3 ? 'write' : 'call'}`, html: `<span style="color:#606b6f;font-size:13px">${i + 1}</span>&nbsp; ${l}` });
        p.style.fontSize = '15px'; p.style.overflow = 'hidden'; p.style.padding = '9px 12px';
        return p;
      });
      const turnsLbl = box(layer, { x: 1040, y: 585, cls: 'label', html: '' });
      const checks = [box(layer, { x: 1040, y: 720, cls: 'note', html: '<span style="color:#5dd2d8">✓</span>&nbsp; no new tools' }), box(layer, { x: 1040, y: 766, cls: 'note', html: '<span style="color:#5dd2d8">✓</span>&nbsp; the same prompt' })];
      checks.forEach(c => { c.style.fontSize = '24px'; c.style.color = C.text; });

      const tSlide = at('idea1', 'between'), tProxy = at('idea1', 'MCP proxy');
      const tCall = at('idea2', 'When'), tNames = at('idea2', 'names'), tReads = at('idea2', 'makes those reads');
      const tRide = at('idea3', 'ride back'), tInside = at('idea3', 'inside');
      const tSkip = at('idea4', 'skips'), tNoTools = at('idea4', 'No new tools'), tPrompt = at('idea4', 'same prompt');
      ctx.poster = tSkip + 1.2;
      return t => {
        show(agent, t, b.t0 + 0.1, { d: 0.6, dy: 10 });
        show(server, t, b.t0 + 0.25, { d: 0.6, dy: 10 });
        direct.set(ramp(t, b.t0 + 0.3, b.t0 + 0.9), 1 - ramp(t, tSlide, tSlide + 0.4));
        show(proxy, t, tSlide, { d: 0.7, dy: -40 });
        w1.set(ramp(t, tProxy - 0.1, tProxy + 0.4)); w2.set(ramp(t, tProxy, tProxy + 0.5));
        r2.set(ramp(t, tProxy + 0.1, tProxy + 0.6)); r1.set(ramp(t, tProxy + 0.2, tProxy + 0.7));
        // The call goes out: agent → stretto → server; its result comes back to stretto.
        const c1 = ramp(t, tCall, tCall + 0.6, E.io2), c2 = ramp(t, tCall + 0.6, tCall + 1.1, E.io2);
        show(callTag, t, tCall, { d: 0.3, dy: 4 });
        if (t < tCall + 0.6) pk[0].set(w1.at(c1), c1 > 0 && c1 < 1 ? 1 : 0);
        else if (t < tCall + 1.1) pk[0].set(w2.at(c2), 1);
        else { const c3 = ramp(t, tCall + 1.1, tNames + 0.2, E.io2); pk[0].set(r2.at(c3), c3 < 1 ? 1 : 0); }
        // stretto makes the two reads, and their results come back.
        show(readTag, t, tReads, { d: 0.35, dy: 4 });
        q1.set(ramp(t, tReads, tReads + 0.5, E.io2));
        q2.set(ramp(t, tReads + 0.5, tReads + 1.0, E.io2));
        [1, 2].forEach(k => {
          const go = ramp(t, tReads + (k - 1) * 0.18, tReads + 0.55 + (k - 1) * 0.18, E.io2);
          const back = ramp(t, tReads + 0.6 + (k - 1) * 0.18, tReads + 1.15 + (k - 1) * 0.18, E.io2);
          if (back > 0) pk[k].set(q2.at(back), back < 1 ? 1 : 0);
          else pk[k].set(q1.at(go), go > 0 ? 1 : 0);
        });
        // One result rides back to the agent, the reads inside it.
        const rb = ramp(t, tRide, tRide + 0.8, E.io2);
        pk[3].set(r1.at(rb), rb > 0 && rb < 1 ? 1 : 0);
        show(bundle, t, tInside - 0.1, { d: 0.6, dy: 14 });
        show(bundleReads, t, tInside + 0.25, { d: 0.5, dy: -10 });
        // The turns close up.
        const fold = ramp(t, tSkip, tSkip + 0.9, E.io3);
        let x = 1040;
        tl.forEach((p, i) => {
          const f = i === 1 || i === 2;
          const w = f ? ws[i] * (1 - fold) : ws[i];
          p.style.left = `${x.toFixed(1)}px`;
          p.style.width = `${w.toFixed(1)}px`;
          p.style.opacity = (ramp(t, tInside + 0.4 + i * 0.08, tInside + 0.8 + i * 0.08) * (f ? 1 - ramp(t, tSkip, tSkip + 0.5) : 1)).toFixed(3);
          if (f) p.classList.toggle('read', t > tReads + 1);
          x += w + (f ? 12 * (1 - fold) : 12);
        });
        turnsLbl.innerHTML = t < tSkip + 0.5 ? 'LLM turns: <span class="num" style="color:#eceff0">5</span>' : 'LLM turns: <span class="num" style="color:#5dd2d8">3</span>';
        show(turnsLbl, t, tInside + 0.4, { d: 0.4, dy: 0 });
        show(checks[0], t, tNoTools, { d: 0.45, dy: 8 });
        show(checks[1], t, tPrompt, { d: 0.45, dy: 8 });
      };
    });
  }

  // ================================================================ reads
  function sceneReads(ctx) {
    scene(ctx, 'reads', (layer, b) => {
      const at = ctx.at;
      const tools = [
        ['get_user_details', 'read'], ['get_order_details', 'read'], ['get_product_details', 'read'], ['list_all_product_types', 'read'],
        ['cancel_pending_order', 'write'], ['modify_pending_order_items', 'write'], ['return_delivered_order_items', 'write'],
      ];
      const tbl = card(layer, { x: 120, y: 150, w: 900, h: 640, title: 'τ²-bench retail · some of the server\'s tools', tag: 'readOnlyHint' });
      const rows = tools.map(([name, kind], i) => {
        const y = 22 + i * 76;
        const r = box(tbl.body, { x: 0, y, w: 856, h: 62, style: { borderRadius: '10px' } });
        box(r, { x: 20, y: 31, ay: 0.5, cls: 'code', text: name, style: { fontSize: '23px', color: kind === 'read' ? C.text : C.subtle } });
        const k = box(r, { x: 836, y: 31, ax: 1, ay: 0.5, cls: `label ${kind === 'read' ? '' : 'amber'}`, html: kind === 'read' ? 'read' : '🔒&nbsp; write' });
        return { r, kind, k };
      });
      const bracket = box(layer, { x: 1048, y: 172, w: 4, h: 4 * 76 - 14, style: { background: C.petrolG, borderRadius: '2px', transformOrigin: 'top' } });
      const bLbl = box(layer, { x: 1076, y: 172 + (4 * 76 - 14) / 2, ay: 0.5, cls: 'label petrol', html: 'a flow may call these' });
      const never = box(layer, { x: 1076, y: 172 + 4 * 76 + (3 * 76 - 14) / 2, ay: 0.5, cls: 'label amber', html: 'never these' });
      // A detour: a lookup the agent did not use.
      const det = card(layer, { x: 1330, y: 520, w: 470, h: 270, title: 'a detour', tag: 'not used', accent: 'petrol' });
      code(det.body, ['<span class="pr">get_product_details</span>', '<span class="p">{"product_id": "…"}: { … }</span>'], { x: 22, y: 12, size: 19 });
      const dRows = [
        box(det.body, { x: 22, y: 110, cls: 'note', html: '<span style="color:#d9a35b">+ tokens</span> in the agent\'s context' }),
        box(det.body, { x: 22, y: 150, cls: 'note', html: '<span style="color:#5dd2d8">✓</span> state unchanged' }),
      ];
      const tRead = at('reads1', 'only reads'), tWrites = at('reads1', 'a write'), tDetour = at('reads2', 'detour'), tTokens = at('reads2', 'costs tokens'), tState = at('reads2', 'changes no state');
      return t => {
        show(tbl, t, b.t0 + 0.1, { d: 0.6 });
        rows.forEach((r, i) => {
          show(r.r, t, b.t0 + 0.3 + i * 0.07, { d: 0.4, dy: 6 });
          const isRead = r.kind === 'read';
          const lit = isRead ? ramp(t, tRead, tRead + 0.5) : ramp(t, tWrites, tWrites + 0.5);
          r.r.style.background = isRead ? `rgba(51,192,199,${(0.08 * lit).toFixed(3)})` : `rgba(217,163,91,${(0.06 * lit).toFixed(3)})`;
          r.r.style.boxShadow = isRead ? `inset 0 0 0 1px rgba(93,210,216,${(0.35 * lit).toFixed(3)})` : 'none';
        });
        bracket.style.transform = `scaleY(${ramp(t, tRead, tRead + 0.5, E.out3).toFixed(3)})`;
        show(bLbl, t, tRead + 0.25, { d: 0.4, dx: -8, dy: 0 });
        show(never, t, tWrites, { d: 0.4, dx: -8, dy: 0 });
        show(det, t, tDetour, { d: 0.6, dy: 14 });
        show(dRows[0], t, tTokens, { d: 0.4, dy: 6 });
        show(dRows[1], t, tState, { d: 0.4, dy: 6 });
      };
    });
  }

  // ================================================================ learn
  function sceneLearn(ctx) {
    scene(ctx, 'learn', (layer, b) => {
      const at = ctx.at;
      const svg = svgOf(layer);
      // Recorded sessions: each a strip of calls.
      const R = rng(11);
      const pool = [['get_user_details', 'r'], ['get_order_details', 'r'], ['get_order_details', 'r'], ['get_product_details', 'r'], ['cancel_pending_order', 'w'], ['find_user_id_by_email', 'r'], ['return_delivered_order_items', 'w'], ['reply', 'n']];
      const sess = card(layer, { x: 120, y: 150, w: 560, h: 640, title: '~/.stretto/logs/retail', tag: 'sessions' });
      const strips = Array.from({ length: 8 }, (_, i) => {
        const s = box(sess.body, { x: 22, y: 18 + i * 70, w: 514, h: 54 });
        box(s, { x: 0, y: 27, ay: 0.5, cls: 'code', text: `${String(i + 1).padStart(2, '0')}`, style: { fontSize: '16px', color: C.faint } });
        const seq = [pool[5], pool[0], pool[1], ...(R() < 0.6 ? [pool[2]] : []), ...(R() < 0.5 ? [pool[3]] : []), pool[R() < 0.5 ? 4 : 6], pool[7]];
        let x = 44;
        seq.forEach(([name, k]) => {
          const w = 10 + name.length * 2.2;
          box(s, { x, y: 15, w, h: 24, style: { borderRadius: '5px', background: k === 'r' ? 'rgba(93,210,216,0.22)' : k === 'w' ? 'rgba(217,163,91,0.28)' : '#2a3336' } });
          x += w + 6;
        });
        return s;
      });
      // What stretto counts: from docs/examples/retail-10-sessions.flow.json.
      const counts = card(layer, { x: 740, y: 150, w: 520, h: 640, title: 'stretto learn', tag: 'counts' });
      const seenHd = box(counts.body, { x: 492, y: 0, ax: 1, cls: 'label', text: 'times seen', style: { fontSize: '13px', top: '-6px' } });
      const cRows = [
        ['after', 'get_user_details'], ['next', 'get_order_details', '7'],
        ['after', 'get_order_details'], ['next', 'get_order_details', '20'], ['next', 'get_product_details', '3'],
      ].map(([k, v, n], i) => {
        const y = 22 + i * 48 + (i >= 2 ? 22 : 0);
        return box(counts.body, { x: 22, y, w: 470, cls: 'code', html: `<span class="k">${k === 'after' ? 'after ' : '  → '}</span><span class="${k === 'after' ? 's' : 'pr'}">${v}</span>${n ? `<span class="p" style="float:right">${n}</span>` : ''}`, style: { fontSize: '19px' } });
      });
      const bHead = box(counts.body, { x: 22, y: 314, cls: 'label', text: 'where each argument came from' });
      const bRows = [
        box(counts.body, { x: 22, y: 354, w: 470, cls: 'code', html: '<span class="pr">order_id</span> <span class="p">←</span> <span class="s">get_user_details</span><br><span class="k">   at $.orders[*]</span><span class="p" style="float:right">27 of 28</span>', style: { fontSize: '19px', lineHeight: '1.6' } }),
        box(counts.body, { x: 22, y: 444, w: 470, cls: 'code', html: '<span class="pr">product_id</span> <span class="p">←</span> <span class="s">get_order_details</span><br><span class="k">   at $.items[*].product_id</span><span class="p" style="float:right">9 of 9</span>', style: { fontSize: '19px', lineHeight: '1.6' } }),
      ];
      const src = box(counts.body, { x: 22, y: 548, w: 470, cls: 'code', text: 'docs/examples/retail-10-sessions.flow.json', style: { fontSize: '14px', color: C.faint } });
      // The flow: a file, reviewed like code.
      // The flow, reviewed: stretto flow-diff's own output for five sessions
      // against ten (docs/review.md, diff-5-10).
      const file = card(layer, { x: 1320, y: 150, w: 480, h: 640, title: 'stretto flow-diff', tag: 'review', accent: 'petrol' });
      const diff = code(file.body, [
        '<span class="c">$ stretto flow-diff \\</span>',
        '<span class="c">    retail-5-sessions.flow.json \\</span>',
        '<span class="c">    retail-10-sessions.flow.json</span>',
        '',
        '<span class="s"># Flow diff: retail</span>',
        '<span class="k">Needs review:</span>',
        '<span class="pr">- a new lookup: get_user_details</span>',
        '<span class="p">  after find_user_id_by_name_zip</span>',
        '<span class="pr">- a new lookup: get_product_details</span>',
        '<span class="p">  after get_item_details</span>',
      ], { x: 22, y: 14, size: 17, lh: 1.75 });
      const scope = box(layer, { x: 1320, y: 820, w: 480, cls: 'note', html: '' });
      scope.style.fontSize = '17px';
      const fl = arrow(svg, 'M 690 470 L 732 470', { color: C.faint });
      const fr = arrow(svg, 'M 1270 470 L 1312 470', { color: C.petrolG });
      const tOwn = at('learn1', 'recorded sessions'), tCounts = at('learn2', 'counts'), tArg = at('learn2', 'where each argument'), tFlow = at('learn3', 'a flow'), tReview = at('learn3', 'review'), tDiff = at('learn3', 'diff'), tTen = at('learn4', 'ten of');
      return t => {
        show(sess, t, b.t0 + 0.1, { d: 0.6 });
        strips.forEach((s, i) => show(s, t, Math.min(tOwn, b.t0 + 0.4) + i * 0.12, { d: 0.4, dx: -10, dy: 0 }));
        fl.set(ramp(t, tCounts - 0.3, tCounts));
        show(counts, t, tCounts - 0.2, { d: 0.5, dy: 10 });
        cRows.forEach((r, i) => show(r, t, tCounts + 0.1 + i * 0.22, { d: 0.35, dy: 6 }));
        show(seenHd, t, tCounts + 0.2, { d: 0.35, dy: 0 });
        show(bHead, t, tArg, { d: 0.4, dy: 6 });
        bRows.forEach((r, i) => show(r, t, tArg + 0.3 + i * 0.35, { d: 0.4, dy: 6 }));
        show(src, t, tArg + 1.0, { d: 0.4, dy: 0 });
        fr.set(ramp(t, tFlow - 0.3, tFlow));
        show(file, t, tFlow - 0.1, { d: 0.5, dy: 10 });
        diff.rows.forEach((r, i) => show(r, t, (i < 4 ? tReview : tDiff) + (i % 6) * 0.1, { d: 0.3, dy: 0 }));
        scope.innerHTML = '<span style="color:#b3bcbf">Replay, three agents, three orders:</span> ten of an agent\'s own sessions give 96% (retail) and 93% (airline) of what all of them do.';
        show(scope, t, tTen, { d: 0.5, dy: 6 });
      };
    });
  }

  // ============================================================== console
  // The real console, from brand/media/console/ (Playwright screenshots of
  // stretto-console on its test fixtures, 1440 x 900): the overview, a served
  // session, and a flow. The frame moves in on what each line names.
  function sceneConsole(ctx) {
    scene(ctx, 'console', (layer, b) => {
      const at = ctx.at;
      const K = 0.8, IW = 1440 * K, IH = 900 * K, BAR = 40;
      const frame = box(layer, { x: 960, y: 128, w: IW + 2, h: IH + BAR + 2, ax: 0.5, cls: 'card' });
      const bar = box(frame, { x: 0, y: 0, w: IW, h: BAR, style: { borderBottom: `1px solid ${C.border}`, background: C.surface2 } });
      box(bar, { x: 18, y: 20, ay: 0.5, html: '<span style="color:#3b4649;font-size:14px;letter-spacing:4px">●●●</span>' });
      const url = box(bar, { x: IW / 2, y: 20, ax: 0.5, ay: 0.5, cls: 'code', text: '', style: { fontSize: '15px', color: C.subtle, padding: '5px 18px', background: C.bg, borderRadius: '7px', border: `1px solid ${C.border}` } });
      const view = box(frame, { x: 0, y: BAR, w: IW, h: IH, style: { overflow: 'hidden' } });
      // Each shot: an image, the page's address, and a focus: where to move in
      // (image pixels), how far, and a ring around what the line names.
      const shots = [
        { src: 'overview', url: '127.0.0.1:7878', fx: 720, fy: 450, s: 1, ring: [10, 66, 214, 196] },
        { src: 'session', url: '127.0.0.1:7878/sessions/shop/20260928T020401.195Z-14715', fx: 760, fy: 800, s: 1.26, ring: [346, 676, 1040, 224] },
        { src: 'flow', url: '127.0.0.1:7878/flows/shop', fx: 840, fy: 640, s: 1.24, ring: [270, 462, 1138, 370] },
      ].map(sh => {
        const holder = box(view, { x: 0, y: 0, w: IW, h: IH, style: { transformOrigin: `${sh.fx * K}px ${sh.fy * K}px` } });
        const img = el('img', { src: `../../media/console/${sh.src}-dark.png`, width: IW, height: IH, style: { display: 'block', width: `${IW}px`, height: `${IH}px` } }, holder);
        ctx.waits.push(img.decode());
        const [rx, ry, rw, rh] = sh.ring;
        const ring = box(holder, { x: rx * K - 6, y: ry * K - 6, w: rw * K + 12, h: rh * K + 12, style: { borderRadius: '12px', boxShadow: `0 0 0 2px ${C.petrolG}, 0 0 0 9999px rgba(11,15,17,0.45)` } });
        return { ...sh, holder, ring };
      });
      const tOne = at('console1', 'servers'), tPlace = at('console1', 'one place');
      const tSess = at('console2', 'A session'), tLook = at('console2', 'the lookups'), tWhy = at('console2', 'why');
      const tFlow = at('console3', 'A flow'), tThr = at('console3', 'another threshold');
      const starts = [b.t0, tSess - 0.35, tFlow - 0.35];
      const rings = [tOne + 0.1, tLook, tThr - 0.2];
      return t => {
        show(frame, t, b.t0 + 0.05, { d: 0.7, dy: 18 });
        shots.forEach((sh, i) => {
          const on = i === 0 ? 1 : ramp(t, starts[i], starts[i] + 0.45, E.io2);
          const off = i + 1 < shots.length ? ramp(t, starts[i + 1] + 0.2, starts[i + 1] + 0.45) : 0;
          sh.holder.style.opacity = (on * (1 - off)).toFixed(3);
          sh.holder.style.visibility = on * (1 - off) < 0.002 ? 'hidden' : '';
          const end = i + 1 < shots.length ? starts[i + 1] : b.t1;
          const z = 1 + (sh.s - 1) * ramp(t, rings[i] - 0.2, rings[i] + 1.2, E.io3) + 0.012 * ramp(t, starts[i], end + 0.5, E.lin);
          sh.holder.style.transform = `scale(${z.toFixed(4)})`;
          const r = ramp(t, rings[i], rings[i] + 0.5, E.out3) * (1 - ramp(t, i === 0 ? tPlace + 0.9 : end - 0.2, i === 0 ? tPlace + 1.3 : end + 0.2));
          sh.ring.style.opacity = r.toFixed(3);
        });
        const k = t < starts[1] + 0.2 ? 0 : t < starts[2] + 0.2 ? 1 : 2;
        url.textContent = shots[k].url;
        void tWhy;
      };
    });
  }

  // ================================================================= rule
  function sceneRule(ctx) {
    scene(ctx, 'rule', (layer, b) => {
      const at = ctx.at;
      const svg = svgOf(layer);
      const X0 = 260, X1 = 1660, Y = 650;
      const px = p => X0 + (X1 - X0) * p;
      const P = 2530 / (2530 + 6000);
      // The two costs.
      const saved = box(layer, { x: 260, y: 170, cls: 'note', html: '<span class="label petrol">a turn saved</span><br><span class="big num" style="color:#5dd2d8">6,000</span> <span style="font-size:22px">input tokens</span>' });
      const wasted = box(layer, { x: 720, y: 170, cls: 'note', html: '<span class="label amber">a detour</span><br><span class="big num" style="color:#d9a35b">2,530</span> <span style="font-size:22px">input tokens</span>' });
      const eq = box(layer, { x: 1180, y: 214, cls: 'code', html: '<span class="k">line</span> <span class="p">=</span> <span class="am">2,530</span> <span class="p">/ (</span><span class="am">2,530</span> <span class="p">+</span> <span class="pr">6,000</span><span class="p">)</span><br><span class="p">     ≈</span> <span class="s">0.30</span>', style: { fontSize: '26px', lineHeight: '1.6' } });
      const live = box(layer, { x: 260, y: 296, cls: 'note', text: 'Live, GLM-5.3 on τ²-bench retail and airline: what each cost over the rest of its episode.', style: { fontSize: '17px', color: C.faint } });
      // The axis.
      const axis = arrow(svg, `M ${X0} ${Y} L ${X1} ${Y}`, { color: C.borderStrong, width: 2, head: false });
      const ticks = [0, 0.25, 0.5, 0.75, 1].map(p => box(layer, { x: px(p), y: Y + 30, ax: 0.5, cls: 'code', text: p.toFixed(p % 1 ? 2 : 0), style: { fontSize: '17px', color: C.faint } }));
      const axisLbl = box(layer, { x: X1, y: Y + 74, ax: 1, cls: 'label', text: 'chance the lookup is used before the next write' });
      const line = box(layer, { x: px(P) - 1, y: Y - 250, w: 2, h: 270, style: { background: C.text, transformOrigin: 'bottom' } });
      const lineLbl = box(layer, { x: px(P) + 14, y: Y - 250, cls: 'label', html: '<span style="color:#eceff0">the line ≈ 0.3</span>' });
      // Candidate lookups, at their chances (illustrative).
      const cands = [['get_order_details', 0.92, 0], ['get_product_details', 0.61, 1], ['find_user_id_by_email', 0.17, 2], ['list_all_product_types', 0.06, 3]];
      const cc = cands.map(([name, p, i]) => {
        const y = Y - 170 + (i % 2) * 70;
        const dot = box(layer, { x: px(p), y: Y, ax: 0.5, ay: 0.5, w: 16, h: 16, style: { borderRadius: '50%', background: C.subtle } });
        const c = chip(layer, `${name} <span style="opacity:.7">${p.toFixed(2)}</span>`, { x: px(p), y, ax: 0.5, ay: 0.5, size: 18, html: true });
        const stem = box(layer, { x: px(p) - 0.5, y: y + 22, w: 1, h: Y - y - 30, style: { background: C.border } });
        return { dot, c, stem, p };
      });
      const verdict = [box(layer, { x: px(0.65), y: Y + 30, ax: 0.5, cls: 'label petrol', text: 'made', style: { top: `${Y - 236}px` } }), box(layer, { x: px(0.13), y: Y - 236, ax: 0.5, cls: 'label', text: 'left out' })];
      const example = box(layer, { x: X0, y: Y + 74, cls: 'note', text: 'Illustrative chances, not measured.', style: { fontSize: '17px', color: C.faint } });
      const noModel = chip(layer, 'no model · no key', { x: 1660, y: 214, ax: 1, kind: 'read', size: 20 });
      const tSaved = at('rule1', 'a turn saved'), tWasted = at('rule1', 'a detour'), tChance = at('rule2', 'chance'), tWrite = at('rule2', 'next write'), tLine = at('rule2', 'clears the line');
      const tLive = at('rule3', 'Measured'), tThirty = at('rule3', 'thirty'), tNo = at('rule4', 'No model'), tCounts = at('rule4', 'counts');
      return t => {
        show(saved, t, tSaved, { d: 0.5 });
        show(wasted, t, tWasted, { d: 0.5 });
        axis.set(ramp(t, tChance - 0.2, tChance + 0.6, E.io3));
        ticks.forEach((k, i) => show(k, t, tChance + i * 0.08, { d: 0.3, dy: 0 }));
        show(axisLbl, t, tWrite, { d: 0.4, dy: 0 });
        cc.forEach(({ dot, c, stem, p }, i) => {
          show(dot, t, tChance + 0.3 + i * 0.12, { d: 0.35, dy: 0, scale: 0.6 });
          show(c, t, tChance + 0.35 + i * 0.12, { d: 0.4, dy: 8 });
          show(stem, t, tChance + 0.35 + i * 0.12, { d: 0.4, dy: 0 });
          const made = p > P;
          const k = ramp(t, tLine + 0.3, tLine + 0.7);
          c.classList.toggle('read', made && k > 0.5);
          c.classList.toggle('ghost', !made && k > 0.5);
          dot.style.background = k > 0.5 ? (made ? C.petrolG : C.dim) : C.subtle;
        });
        line.style.transform = `scaleY(${ramp(t, tLine - 0.2, tLine + 0.4, E.out3).toFixed(3)})`;
        line.style.opacity = ramp(t, tLine - 0.2, tLine).toFixed(3);
        show(lineLbl, t, tLine + 0.2, { d: 0.4, dy: 0 });
        verdict.forEach(v => show(v, t, tLine + 0.6, { d: 0.4, dy: 0 }));
        show(example, t, tChance + 0.8, { d: 0.4, dy: 0 });
        show(eq, t, tLive, { d: 0.5 });
        show(live, t, tLive + 0.2, { d: 0.5, dy: 0 });
        lineLbl.style.color = t > tThirty ? C.petrol : '';
        show(noModel, t, tNo, { d: 0.45, dy: 0 });
        void tCounts;
      };
    });
  }

  // ============================================================== results
  /** A bar chart of fewer LLM turns: rows of [name, pct, ci]. */
  function bars(layer, rows, { x, y, w, rowH = 104, max = 30, kind = 'petrol', label }) {
    const head = box(layer, { x, y: y - 56, cls: 'label', text: label });
    const els = rows.map(([name, pct, ci], i) => {
      const yy = y + i * rowH;
      const n = box(layer, { x, y: yy, cls: 'note', text: name, style: { color: C.text, fontSize: '25px', fontWeight: 500 } });
      const track = box(layer, { x, y: yy + 42, w, h: 22, style: { borderRadius: '6px', background: C.surface2, border: `1px solid ${C.border}` } });
      const fill = box(track, { x: 0, y: 0, h: 20, w: 0, style: { borderRadius: '5px', background: kind === 'petrol' ? C.petrolG : '#5d6a6e' } });
      const v = box(layer, { x: x + w + 24, y: yy + 53, ay: 0.5, cls: 'big num', text: '', style: { fontSize: '40px', color: kind === 'petrol' ? C.petrol : C.muted } });
      const c = ci ? box(layer, { x: x + w + 24, y: yy + 88, ay: 0.5, cls: 'code', text: `95% CI ${ci}`, style: { fontSize: '15px', color: C.faint } }) : null;
      return { n, track, fill, v, c, pct };
    });
    return {
      head, els,
      set(t, starts) {
        show(head, t, starts[0] - 0.4, { d: 0.4, dy: 0 });
        els.forEach((e, i) => {
          const s = starts[i];
          show(e.n, t, s, { d: 0.4, dy: 6 });
          show(e.track, t, s, { d: 0.4, dy: 6 });
          const u = ramp(t, s + 0.15, s + 1.1, E.out3);
          e.fill.style.width = `${((w - 2) * (e.pct / max) * u).toFixed(1)}px`;
          e.v.textContent = `−${(e.pct * u).toFixed(1)}%`;
          show(e.v, t, s + 0.15, { d: 0.3, dy: 0 });
          if (e.c) show(e.c, t, s + 0.8, { d: 0.4, dy: 0 });
        });
      },
    };
  }

  function sceneResults(ctx) {
    scene(ctx, 'results', (layer, b) => {
      const at = ctx.at;
      const title = box(layer, { x: 240, y: 168, html: '<span style="font:600 46px/1.1 Inter;letter-spacing:-.025em;color:#eceff0">Fewer LLM turns, with a flow</span>' });
      const scope = box(layer, { x: 240, y: 236, cls: 'note', html: 'Live · 28 τ²-bench retail and airline tasks · the reach decider, no model of its own' });
      const chart = bars(layer, [['Claude Sonnet 5', 20.5, '16.5–24.4%'], ['Claude Haiku 4.5', 22.4, '15.8–29.4%'], ['GLM-5.3', 27.9, '19.1–35.9%']], { x: 240, y: 380, w: 1100, max: 32, label: 'LLM turns saved, against the same agent without stretto' });
      const foot = box(layer, { x: 240, y: 742, w: 1440, cls: 'note', html: 'Claude models: pre-registered, three trials, paired by task. GLM-5.3: one trial, against its recorded baseline.<br>The savings are in retail; in airline they are not established. Pass rates are underpowered.' });
      foot.style.fontSize = '18px';
      foot.style.color = C.faint;
      const tLive = at('results1', 'Live'), tSon = at('results2', 'Claude'), tHai = at('results3', 'Claude'), tGlm = at('results3', 'and');
      return t => {
        show(title, t, b.t0 + 0.1, { d: 0.5 });
        show(scope, t, tLive, { d: 0.5, dy: 6 });
        chart.set(t, [tSon, tHai, tGlm]);
        show(foot, t, tSon + 1.4, { d: 0.6, dy: 0 });
      };
    });
  }

  function scenePrompt(ctx) {
    scene(ctx, 'prompt', (layer, b) => {
      const at = ctx.at;
      const L = box(layer, { x: 120, y: 160, w: 800, cls: 'note', html: '<span style="font:600 32px/1.2 Inter;color:#eceff0;letter-spacing:-.02em">A prompt</span><br>Anthropic\'s sample prompt for parallel tool calls' });
      const R = box(layer, { x: 1000, y: 160, w: 800, cls: 'note', html: '<span style="font:600 32px/1.2 Inter;color:#eceff0;letter-spacing:-.02em">The prompt, plus a flow</span><br>the prompt in both arms; what the flow adds' });
      const lb = bars(layer, [['Claude Sonnet 5', 3.4, '−1.2–7.8%'], ['Claude Haiku 4.5', 5.9, '0.9–10.9%'], ['GLM-5.3', 7.6, '−1.8–15.0%']], { x: 120, y: 340, w: 520, max: 26, kind: 'gray', label: 'LLM turns saved', rowH: 96 });
      const rb = bars(layer, [['Claude Sonnet 5', 22.9, '19.5–26.1%'], ['Claude Haiku 4.5', 17.2, '10.2–24.0%']], { x: 1000, y: 340, w: 520, max: 26, label: 'LLM turns saved, against the prompt alone', rowH: 120 });
      const knows = box(layer, { x: 120, y: 690, w: 760, cls: 'code', html: '<span class="k">one turn, what the agent knows:</span><br><span class="s">get_user_details</span> <span class="p">+</span> <span class="s">list_all_product_types</span>', style: { fontSize: '19px', lineHeight: '1.7' } });
      const reveal = box(layer, { x: 1000, y: 690, w: 760, cls: 'code', html: '<span class="k">what the result reveals:</span><br><span class="pr">get_order_details #W7a</span> <span class="p">+</span> <span class="pr">get_order_details #W7b</span>', style: { fontSize: '19px', lineHeight: '1.7' } });
      const live = box(layer, { x: 120, y: 820, cls: 'note', text: 'Live, 28 τ²-bench retail and airline tasks. Claude models: three trials, paired by task. GLM-5.3: one trial, against its recorded arms.', style: { fontSize: '17px', color: C.faint } });
      const divider = box(layer, { x: 940, y: 170, w: 1, h: 640, style: { background: C.border } });
      const tBatch = at('prompt1', 'batch'), tSaved = at('prompt1', 'saved'), tKnows = at('prompt2', 'already knows'), tReveal = at('prompt2', 'what results reveal'), tBoth = at('prompt3', 'both arms'), tStill = at('prompt3', 'still saved');
      return t => {
        show(L, t, tBatch - 0.3, { d: 0.5 });
        lb.set(t, [tSaved, tSaved + 0.25, tSaved + 0.5]);
        show(live, t, tSaved + 1.2, { d: 0.5, dy: 0 });
        show(divider, t, tKnows - 0.3, { d: 0.5, dy: 0 });
        show(knows, t, tKnows - 0.4, { d: 0.5, dy: 6 });
        show(R, t, tReveal - 0.4, { d: 0.5 });
        show(reveal, t, tReveal, { d: 0.5, dy: 6 });
        show(rb.head, t, tBoth, { d: 0.4, dy: 0 });
        rb.set(t, [tStill, tStill + 0.35]);
      };
    });
  }

  // ================================================================ start
  function sceneStart(ctx) {
    scene(ctx, 'start', (layer, b) => {
      const at = ctx.at;
      const term = card(layer, { x: 120, y: 170, w: 1680, h: 380, title: '<span style="color:#606b6f">●&nbsp;●&nbsp;●</span>&nbsp;&nbsp; ~', tag: 'zsh' });
      const cmds = [
        ['$ ', 'cargo install --locked --git https://github.com/alexnodeland/stretto stretto-report stretto-proxy'],
        ['$ ', 'stretto init --host claude-code --domain notes -- npx -y @modelcontextprotocol/server-filesystem ~/notes 2>/dev/null'],
      ];
      const out = 'claude mcp add notes -- stretto-proxy --record ~/.stretto/logs/notes --domain notes -- npx -y @modelcontextprotocol/server-filesystem /home/me/notes';
      const lines = cmds.map((c, i) => {
        const row = box(term.body, { x: 26, y: 18 + i * 96, w: 1620, cls: 'code', style: { fontSize: '22px', lineHeight: '1.55', whiteSpace: 'pre-wrap' } });
        const p = el('span', { style: { color: C.petrol } }, row, c[0]);
        const s = el('span', { style: { color: C.text } }, row);
        return { row, p, s, text: c[1] };
      });
      const o = box(term.body, { x: 26, y: 18 + 96 + 86, w: 1620, cls: 'code', text: out, style: { fontSize: '20px', lineHeight: '1.55', whiteSpace: 'pre-wrap', color: C.subtle } });
      const badges = [
        chip(layer, 'open source · MIT', { x: 120, y: 610, kind: 'call', size: 20 }),
        chip(layer, 'Rust', { x: 386, y: 610, kind: 'call', size: 20 }),
        chip(layer, 'github.com/alexnodeland/stretto', { x: 500, y: 610, kind: 'read', size: 20 }),
      ];
      const after = box(layer, { x: 120, y: 710, cls: 'note', html: 'then: use the agent as usual, <span class="code" style="color:#5dd2d8">stretto learn</span>, review the flow, and serve it' });
      after.style.fontSize = '22px';
      const tOpen = at('start1', 'open source'), tRun = at('start2', 'Run'), tLearn = at('start3', 'learns');
      const T1 = Math.max(b.t0 + 0.6, tOpen - 1.2);
      return t => {
        show(term, t, b.t0 + 0.1, { d: 0.6 });
        type(lines[0].s, lines[0].text, t, T1, 70, t < tRun);
        show(lines[0].row, t, T1 - 0.2, { d: 0.2, dy: 0 });
        type(lines[1].s, lines[1].text, t, tRun, 60, t >= tRun);
        show(lines[1].row, t, tRun - 0.2, { d: 0.2, dy: 0 });
        show(o, t, tRun + lines[1].text.length / 60 + 0.3, { d: 0.3, dy: 0 });
        badges.forEach((bd, i) => show(bd, t, tOpen + i * 0.25, { d: 0.4, dy: 6 }));
        show(after, t, tLearn, { d: 0.5, dy: 6 });
      };
    });
  }

  // ================================================================== end
  function sceneEnd(ctx) {
    scene(ctx, 'end', (layer, b) => {
      const S = 4.4;
      const g = box(layer, { x: 960, y: 430, ax: 0.5, ay: 0.5, w: 31 * S + 30 + 512, h: 30 * S });
      box(g, { x: 0, y: 0, html: markSvg(30 * S) });
      box(g, { x: 31 * S + 30, y: 15 * S, ay: 0.5, html: wordmarkSvg(30 * S * 0.62) });
      const tag = box(layer, { x: 960, y: 580, ax: 0.5, ay: 0.5, text: 'Read ahead of your agent.', style: { font: '500 42px/1 Inter, sans-serif', letterSpacing: '-0.02em', color: C.text } });
      const url = box(layer, { x: 960, y: 664, ax: 0.5, ay: 0.5, cls: 'code', html: '<span style="color:#5dd2d8">stretto.alexnodeland.com</span>  <span style="color:#606b6f">·</span>  <span style="color:#909a9d">github.com/alexnodeland/stretto</span>', style: { fontSize: '22px' } });
      const L = ctx.L.end1;
      ctx.music.end = +(L.t1 + 0.4).toFixed(3);
      return t => {
        show(g, t, b.t0 + 0.1, { d: 0.8, dy: 0, scale: 0.04 });
        show(tag, t, L.t0 + 0.6, { d: 0.6, dy: 8 });
        show(url, t, L.t1 + 0.3, { d: 0.6, dy: 6 });
      };
    }, { fout: 0 });
  }

  // ================================================================= cuts
  function video(ctx) {
    sceneCold(ctx);
    sceneTitle(ctx);
    sceneTurns(ctx);
    sceneDecided(ctx);
    sceneIdea(ctx);
    sceneReads(ctx);
    sceneLearn(ctx);
    sceneConsole(ctx);
    sceneRule(ctx);
    sceneResults(ctx);
    scenePrompt(ctx);
    sceneStart(ctx);
    sceneEnd(ctx);
  }

  // The teaser: the cold open alone, as an 8-second loop for the README.
  function teaser(ctx) {
    const D = 8;
    ctx.scene({ key: 'teaser', t0: 0, t1: D, fin: 0, fout: 0, build: layer => {
      const up = coldOpen(ctx, layer, 0, { loop: D });
      return t => {
        up(t);
        // Fade out and back in across the loop's seam.
        layer.style.opacity = (ramp(t, 0, 0.35) * (1 - ramp(t, D - 0.6, D))).toFixed(3);
      };
    } });
    return { duration: D };
  }

  function rng(seed) { return window.ST.rng(seed); }
  void clamp; void fade;
  window.Film = { video, teaser, titles: TITLES, markSvg };
})();
