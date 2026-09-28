// The scenes, part two: learning a flow, the decision rule, the results, the
// prompt baseline, getting started and the end card; then the teaser, and
// the order the scenes run in.
(() => {
  const { s } = E;
  const { T, TM, label } = A;
  const Sc = window.Scenes;
  const H = Sc.H;
  const { N, G } = H;
  const L = E.lerp;

  // -------------------------------------------------------- learning a flow
  // A tilted flyover across recorded sessions, counts ticking up on the
  // transitions; the stream condenses into a flow graph, which folds into a
  // file that opens as a reviewed diff.
  const KIND = {
    user: { text: 'user', fill: '#1d2629', stroke: '#8b989c', ink: C.text },
    order: { text: 'order', fill: '#0f2a2d', stroke: C.petrol, ink: C.petrolText },
    product: { text: 'product', fill: '#112528', stroke: '#4fa7ad', ink: '#9fd8dc' },
    reply: { text: 'reply', fill: '#161d1f', stroke: '#3a4548', ink: C.subtle },
    cancel: { text: 'cancel', fill: '#231a0e', stroke: C.warm, ink: C.warm },
    exchange: { text: 'exchange', fill: '#231a0e', stroke: C.warm, ink: C.warm },
    return: { text: 'return', fill: '#231a0e', stroke: C.warm, ink: C.warm },
  };
  const READS = new Set(['user', 'order', 'product']);

  function learn(ctx, st) {
    const { S, word, cam, ev } = ctx;
    const L0 = S.learn.t0, R0 = S.rule.t0;
    const w = p => word('learn', p);
    const wWhere = w('where each argument'), wFlow = w('becomes a flow'), wFile = w('small file');
    const wRead = w('read, review'), wDiff = w('diff'), wCode = w('like code'), wTen = w('About ten sessions');

    // The sessions: rows of calls, one row per recorded session.
    const streamG = G(W.root, { id: 'stream' });
    const rnd = E.rng(4242);
    const patterns = [
      ['user', 'order', 'order', 'reply', 'cancel'],
      ['user', 'order', 'product', 'reply', 'exchange'],
      ['user', 'order', 'reply', 'cancel'],
      ['user', 'order', 'order', 'order', 'reply'],
      ['user', 'order', 'product', 'product', 'reply', 'return'],
      ['user', 'order', 'product', 'reply'],
    ];
    const ROWS = 16, RY0 = -1000, RDY = -190;
    const rows = [];
    for (let r = 0; r < ROWS; r++) {
      const pat = patterns[(r * 5 + rnd.int(0, 5)) % patterns.length];
      const y = RY0 + r * RDY;
      const g = G(streamG);
      let x = -820 + rnd.range(-40, 40);
      g.appendChild(T(`session ${String(r + 1).padStart(2, '0')}`, { x: -1000, y: y + 6, size: 16, mono: true, fill: '#4f5b5f' }));
      const chips = [], links = [];
      pat.forEach((k, i) => {
        const K = KIND[k];
        const cw = K.text.length * 14.4 + 50;
        if (i) {
          const x0 = x - 44, line = s('line', { x1: x0 + 4, y1: y, x2: x - 9, y2: y, stroke: '#34424a', 'stroke-width': 3 });
          const head = s('path', { d: `M ${x - 17} ${y - 7} L ${x - 9} ${y} L ${x - 17} ${y + 7}`, fill: 'none', stroke: '#34424a', 'stroke-width': 3, 'stroke-linecap': 'round', 'stroke-linejoin': 'round' });
          const plus = T('+1', { x: x - 24, y: y - 48, size: 32, weight: 700, fill: C.petrolText, anchor: 'middle' });
          E.add(g, line, head, plus);
          const counted = READS.has(pat[i - 1]) && READS.has(k);
          links.push({ line, head, plus, counted, pn: N(plus, { o: 0 }) });
        }
        const chip = s('g', {},
          s('rect', { x, y: -31 + y, width: cw, height: 62, rx: 15, fill: K.fill, stroke: K.stroke, 'stroke-width': 2.2 }),
          T(K.text, { x: x + cw / 2, y: y + 8.5, size: 24, mono: true, fill: K.ink, anchor: 'middle' }));
        g.appendChild(chip);
        chips.push({ k, x: x + cw / 2, y, el: chip, n: N(chip) });
        x += cw + 44;
      });
      // Where the arguments came from: the user's record to each order read.
      const arcs = [];
      const u = chips.find(c => c.k === 'user');
      chips.filter(c => c.k === 'order').forEach((c, i) => {
        const d = `M ${u.x} ${y - 33} C ${u.x} ${y - 100 - i * 18}, ${c.x} ${y - 100 - i * 18}, ${c.x} ${y - 35}`;
        const p = s('path', { d, fill: 'none', stroke: C.petrol, 'stroke-width': 3, 'stroke-dasharray': '2 9', 'stroke-linecap': 'round', opacity: 0 });
        g.insertBefore(p, g.firstChild);
        arcs.push(p);
      });
      rows.push({ y, g, chips, links, arcs, n: N(g) });
    }

    // The camera tilts and flies over them.
    const fly1 = Math.max(L0 + 5.4, wFlow - 0.35);
    const d1 = fly1 - (L0 - 0.3);
    const tiltUp = p => E.ease.io3(Math.min(1, (p * d1) / 2.6));
    cam.go(L0 - 0.3, d1, { x: -120, y: -3150, z: 1.05, tilt: 60 }, 'smooth', 'io2', { et: tiltUp, ez: tiltUp });
    cam.go(fly1, 2.4, { x: 0, y: -4450, z: 1.0, tilt: 0 }, 'smooth', 'io3');
    ev(L0 - 0.25, 'whoosh-soft', 0.4);
    ev(fly1, 'whoosh-soft', 0.26);
    // Counting, as each row passes: by the camera's distance, not a clock.
    const counted = { rows: 0, uo: 0, op: 0 };
    ctx.fn((t, v) => {
      counted.rows = 0; counted.uo = 0; counted.op = 0;
      const live = t > L0 && t < wFlow + 1.5;
      for (const r of rows) {
        const q = live ? E.clamp((300 - (v.y - r.y)) / 240) : t >= wFlow + 1.5 ? 1 : 0;
        if (q >= 0.5) {
          counted.rows++;
          r.chips.forEach((c, i) => {
            if (!i) return;
            const a = r.chips[i - 1].k;
            if (a === 'user' && c.k === 'order') counted.uo++;
            if (a === 'order' && c.k === 'product') counted.op++;
          });
        }
        r.links.forEach((lk, i) => {
          const qq = E.clamp(q * 1.6 - i * 0.12);
          const on = lk.counted ? qq : 0;
          lk.pn.p.o = lk.counted ? Math.sin(Math.PI * E.clamp(qq)) : 0;
          lk.pn.p.dy = -26 * qq;
          const col = E.mixColor('#34424a', C.petrol, on);
          E.attr(lk.line, 'stroke', col);
          E.attr(lk.head, 'stroke', col);
        });
        const aon = E.P(t, wWhere, 0.6) * E.clamp(q * 2) * (1 - E.P(t, wFlow - 0.1, 0.35, 'lin'));
        r.arcs.forEach(a => E.attr(a, 'opacity', aon.toFixed(3)));
      }
    });
    // The rows' first tick, for the sound.
    const camY = t => cam.at(t).y;
    ctx.after(() => {
      let n = 0;
      for (const r of rows) {
        let lo = L0, hi = fly1;
        if (camY(hi) - r.y > 150) continue;
        for (let i = 0; i < 30; i++) { const m = (lo + hi) / 2; if (camY(m) - r.y > 150) lo = m; else hi = m; }
        if (hi < wFlow && n++ % 2 === 0) ev(hi, 'tick', 0.12);
      }
    });
    // The tally, pinned to the screen while the sessions pass.
    const panel = G(st.layers.panel);
    const pw = 700;
    E.add(panel,
      s('rect', { x: 0, y: 0, width: pw, height: 246, rx: 22, fill: '#11171a', 'fill-opacity': 0.9, stroke: '#2a3437', 'stroke-width': 1.6 }),
      T('counted in the sessions', { x: 30, y: 48, size: 22, weight: 640, fill: C.subtle, ls: 0.12, caps: true }));
    const tallies = [['sessions', 'rows'], ['get_user_details → get_order_details', 'uo'], ['get_order_details → get_product_details', 'op']].map(([name, key], i) => {
      const y = 106 + i * 52;
      panel.appendChild(T(name, { x: 30, y, size: i ? 23 : 26, mono: !!i, weight: i ? 400 : 580, fill: i ? C.muted : C.text }));
      const val = T('0', { x: pw - 30, y, size: 32, weight: 660, fill: i ? C.petrolText : C.text, anchor: 'end', tnum: true });
      panel.appendChild(val);
      return { val, key };
    });
    ctx.fn(() => tallies.forEach(tl => E.text(tl.val, String(counted[tl.key] * (tl.key === 'rows' ? 1 : 3) + (tl.key === 'uo' ? 2 : 0)))));
    const panelN = N(panel, { x: 48, y: 44, o: 0 });
    H.fadeIn(panelN, L0 + 1.6, 0.5, 10);
    H.fadeOut(panelN, wFlow + 0.9, 0.5);
    // Fog towards the horizon.
    const fog = s('rect', { x: 0, y: 0, width: 1920, height: 440, fill: 'url(#gFog)' });
    st.layers.fog.appendChild(fog);
    ctx.fn((t, v) => E.attr(fog, 'opacity', E.clamp(v.tilt / 50).toFixed(3)));

    // The flow: the stream condenses into a graph.
    const FC = [0, -4450];
    const flowG = G(W.root, { id: 'flow' });
    const graphG = G(flowG);
    const graphN = N(graphG, { x: FC[0], y: FC[1] });
    const NODES = [
      { k: 'user', name: 'get_user_details', x: -640, out: 'orders[*]' },
      { k: 'order', name: 'get_order_details', x: 0, arg: 'order_id', out: 'items[*].product_id' },
      { k: 'product', name: 'get_product_details', x: 640, arg: 'product_id' },
    ];
    const NW = 400, NH = 122;
    // Edges first, under the nodes.
    const edges = [[0, 1, 0.94, 47, 50], [1, 2, 0.61, 22, 36]].map(([a, b, p, used, of], i) => {
      const x0 = NODES[a].x + NW / 2 + 8, x1 = NODES[b].x - NW / 2 - 14, y = -28;
      const line = s('path', { d: `M ${x0} ${y} L ${x1} ${y}`, fill: 'none', stroke: C.petrol, 'stroke-width': 3, 'stroke-linecap': 'round' });
      const head = s('path', { d: `M ${x1 - 6} ${y - 14} L ${x1 + 8} ${y} L ${x1 - 6} ${y + 14} Z`, fill: C.petrol });
      const pl = T('0.00', { x: (x0 + x1) / 2, y: y - 32, size: 38, weight: 700, fill: C.petrolText, anchor: 'middle', tnum: true });
      const cl = T('', { x: (x0 + x1) / 2, y: y + 48, size: 24, mono: true, fill: C.muted, anchor: 'middle' });
      E.add(graphG, line, head, pl, cl);
      return { line, head, pl, cl, p, used, of, a: wFlow + 1.05 + i * 0.3 };
    });
    const nodeEls = NODES.map((nd, i) => {
      const K = KIND[nd.k];
      const g = s('g', {},
        s('rect', { x: nd.x - NW / 2, y: -NH / 2 - 28, width: NW, height: NH, rx: 20, fill: '#131b1d', stroke: K.stroke, 'stroke-width': 2.2 }),
        T(nd.name, { x: nd.x, y: -36, size: 26, mono: true, fill: C.text, anchor: 'middle' }),
        T(nd.arg ? `${nd.arg} ←` : 'from the customer', { x: nd.x, y: 8, size: 24, mono: !!nd.arg, fill: C.muted, anchor: 'middle' }));
      graphG.appendChild(g);
      const n = N(g, { o: 0 });
      const a = wFlow + 0.62 + i * 0.14;
      H.popIn(n, a, 0.7, 0.7);
      ev(a, 'pop', 0.2);
      return n;
    });
    edges.forEach(e => {
      H.drawOn(ctx, e.line, e.a, 0.5, 'io3');
      const hn = N(e.head, { o: 0 });
      hn.key('o', [e.a + 0.45, 0], [e.a + 0.6, 1]);
      ctx.fn(t => {
        const q = E.P(t, e.a + 0.2, 1.3, 'io2');
        E.attr(e.line, 'stroke-width', (3 + 9 * e.p * q).toFixed(2));
        // The chance is what the counts say so far: it settles as they arrive.
        const used = Math.round(e.used * q), of = Math.round(e.of * q);
        E.text(e.pl, of ? (used / of).toFixed(2) : '0.00');
        E.text(e.cl, q > 0 ? `used ${used} of ${of}` : '');
        E.attr(e.pl, 'display', q > 0 ? 'inline' : 'none');
      });
    });
    ev(wFlow + 1.05, 'swoosh-in', 0.2);
    // Where the arguments come from: dotted lines from result fields.
    const srcs = [[0, 1, 'orders[*]', 'order_id ← $.orders[*]'], [1, 2, 'items[*].product_id', 'product_id ← $.items[*].product_id']].map(([ia, ib, from, text], i) => {
      const x0 = NODES[ia].x, x1 = NODES[ib].x, y0 = NH / 2 - 28 + 8;
      const d = `M ${x0} ${y0} C ${x0} ${y0 + 120}, ${x1} ${y0 + 120}, ${x1} ${y0 + 4}`;
      const p = s('path', { d, fill: 'none', stroke: C.petrol, 'stroke-width': 3, 'stroke-dasharray': '2 9', 'stroke-linecap': 'round' });
      const lab = T(text, { x: (x0 + x1) / 2, y: y0 + 144, size: 25, mono: true, fill: C.petrolText, anchor: 'middle' });
      const dot = s('circle', { cx: x0, cy: y0, r: 6, fill: C.petrol });
      E.add(graphG, p, dot, lab);
      const a = wFlow + 1.7 + i * 0.3;
      ctx.fn(t => {
        const q = E.P(t, a, 0.7, 'io3');
        const len = p.__len || (p.__len = p.getTotalLength());
        E.attr(p, 'stroke-dasharray', q >= 1 ? '2 9' : `2 9`);
        E.attr(p, 'display', q > 0 ? 'inline' : 'none');
        E.attr(p, 'stroke-dashoffset', '0');
        E.attr(p, 'opacity', q.toFixed(3));
        E.attr(dot, 'opacity', q.toFixed(3));
        void len;
      });
      H.fadeIn(N(lab, { o: 0 }), a + 0.35, 0.5, 8);
      return p;
    });
    // The chips of the nearest sessions fly into the graph's nodes.
    const toNode = { user: 0, order: 1, product: 2 };
    rows.slice(9).forEach((r, ri) => {
      r.chips.forEach((c, ci) => {
        const a = wFlow + 0.02 + ri * 0.035 + ci * 0.02;
        if (c.k in toNode) {
          const nd = NODES[toNode[c.k]];
          c.n.key('dx', [a, 0], [a + 0.75, FC[0] + nd.x - c.x, 'io3']);
          c.n.key('dy', [a, 0], [a + 0.75, FC[1] - 28 - c.y, 'io3']);
          c.n.key('o', [a + 0.5, 1], [a + 0.8, 0, 'in2']);
        } else c.n.key('o', [a, 1], [a + 0.4, 0, 'in2']);
      });
    });
    rows.forEach((r, ri) => {
      const a = wFlow + 0.05;
      const fade = [r.g.querySelector('text')].concat(r.links.flatMap(lk => [lk.line, lk.head]));
      ctx.fn(t => { const k = 1 - E.P(t, a, 0.35, 'out2'); fade.forEach(el => E.attr(el, 'opacity', k.toFixed(3))); });
      if (ri < 9) r.chips.forEach(c => c.n.key('o', [a, 1], [a + 0.5, 0, 'in2']));
    });
    ev(wFlow, 'shimmer', 0.3);

    // The graph folds into a file.
    const fileG = G(flowG);
    const fileN = N(fileG, { x: FC[0], y: FC[1] - 20, o: 0 });
    E.add(fileG,
      s('path', { d: 'M -80 -104 H 38 L 80 -62 V 104 H -80 Z', fill: '#141c1e', stroke: C.petrol, 'stroke-width': 3, 'stroke-linejoin': 'round' }),
      s('path', { d: 'M 38 -104 V -62 H 80', fill: 'none', stroke: C.petrol, 'stroke-width': 3, 'stroke-linejoin': 'round' }),
      ...[0, 1, 2, 3, 4].map(i => s('rect', { x: -56, y: -52 + i * 30, width: [96, 70, 104, 60, 84][i], height: 10, rx: 5, fill: i % 2 ? '#2a6f73' : C.petrol, opacity: 0.8 })),
      T('retail.flow.json', { x: 0, y: 154, size: 27, mono: true, fill: C.text, anchor: 'middle' }));
    const wFold = Math.max(wFile, wFlow + 2.4);
    graphN.key('s', [wFold, 1], [wFold + 0.55, 0.12, 'in3']);
    graphN.key('o', [wFold + 0.25, 1], [wFold + 0.55, 0, 'lin']);
    H.popIn(fileN, wFold + 0.4, 0.75, 0.3);
    ev(wFold, 'swoosh-in', 0.26);
    ev(wFold + 0.45, 'pop', 0.3);
    // It opens as a diff, reviewed.
    const DX = -150, DW = 1110, DH = 590;
    const diffG = G(flowG);
    const diffN = N(diffG, { x: FC[0] + DX, y: FC[1], o: 0 });
    const lines = [
      [' ', '12', '  "sites": { "next": ['],
      [' ', '13', '    [["get_user_details", false],'],
      ['−', '14', '      {"get_order_details": 31}],'],
      ['+', '14', '      {"get_order_details": 47,'],
      ['+', '15', '       "get_product_details": 6}],'],
      [' ', '··', ''],
      [' ', '31', '  "bindings": { "sources": ['],
      [' ', '32', '    [["get_order_details", "order_id"],'],
      ['+', '33', '      {"values": 47,'],
      ['+', '34', '       "found": [[["get_user_details", "$.orders[*]"], 44]]}],'],
    ];
    const x0 = -DW / 2, y0 = -DH / 2;
    E.add(diffG,
      s('rect', { x: x0 + 6, y: y0 + 16, width: DW, height: DH, rx: 22, fill: '#000', opacity: 0.4 }),
      s('rect', { x: x0, y: y0, width: DW, height: DH, rx: 22, fill: '#10171a', stroke: '#2b3639', 'stroke-width': 1.8 }),
      s('path', { d: `M ${x0} ${y0 + 66} H ${x0 + DW}`, stroke: '#2b3639', 'stroke-width': 1.8 }),
      s('path', { d: `M ${x0 + 30} ${y0 + 22} H ${x0 + 50} L ${x0 + 58} ${y0 + 30} V ${y0 + 46} H ${x0 + 30} Z`, fill: 'none', stroke: C.petrol, 'stroke-width': 2 }),
      T('retail.flow.json', { x: x0 + 74, y: y0 + 43, size: 24, mono: true, fill: C.text }),
      TM([['+4', C.petrolText], ['  ', null], ['−1', C.warm]], { x: x0 + 360, y: y0 + 43, size: 22, mono: true, weight: 700 }));
    const LH = 42;
    const lineEls = lines.map(([sign, num, text], i) => {
      const y = y0 + 110 + i * LH;
      const bg = s('rect', { x: x0 + 1, y: y - 28, width: DW - 2, height: LH, fill: sign === '+' ? '#0e3134' : sign === '−' ? '#2e2311' : 'none', opacity: 0 });
      const g = s('g', {},
        bg,
        T(num, { x: x0 + 58, y, size: 19, mono: true, fill: '#56646a', anchor: 'end' }),
        T(sign, { x: x0 + 82, y, size: 22, mono: true, fill: sign === '+' ? C.petrolText : sign === '−' ? C.warm : '#56646a' }),
        T(text, { x: x0 + 110, y, size: 22, mono: true, fill: sign === ' ' ? C.muted : sign === '+' ? '#d8f4f5' : '#e9cf9f' }));
      diffG.appendChild(g);
      return { g, bg, sign, n: N(g, { o: 0 }) };
    });
    const wOpen = Math.max(wRead - 0.15, wFold + 1.2);
    fileN.key('s', [wOpen, 1], [wOpen + 0.5, 2.6, 'in3']);
    H.fadeOut(fileN, wOpen + 0.15, 0.3);
    diffN.key('s', [wOpen, 0.16], [wOpen + 0.75, 1, 'io4']);
    diffN.key('o', [wOpen, 0], [wOpen + 0.3, 1, 'out2']);
    diffN.key('dx', [wOpen, -DX], [wOpen + 0.75, 0, 'io4']);
    lineEls.forEach((le, i) => le.n.key('o', [wOpen + 0.4 + i * 0.06, 0], [wOpen + 0.7 + i * 0.06, 1, 'out2']));
    ev(wOpen, 'swoosh-in', 0.24);
    const wD = Math.max(wDiff, wOpen + 1.0);
    lineEls.filter(le => le.sign !== ' ').forEach((le, i) => ctx.fn(t => E.attr(le.bg, 'opacity', E.P(t, wD + i * 0.1, 0.35, 'out2').toFixed(3))));
    // Reviewed.
    const badge = s('g', {},
      s('rect', { x: -104, y: -26, width: 208, height: 52, rx: 26, fill: C.soft, stroke: C.petrol, 'stroke-width': 2 }),
      s('circle', { cx: -72, cy: 0, r: 15, fill: C.petrol }),
      s('path', { d: 'M -79 0 l 5 5 l 9 -10', fill: 'none', stroke: '#06201f', 'stroke-width': 3.2, 'stroke-linecap': 'round', 'stroke-linejoin': 'round' }),
      T('reviewed', { x: -48, y: 10, size: 27, weight: 640, fill: C.petrolText }));
    diffG.appendChild(badge);
    const badgeN = N(badge, { x: x0 + DW - 128, y: y0 + 34, o: 0 });
    const wOk = Math.max(wCode + 0.15, wD + 0.6);
    H.popIn(badgeN, wOk, 0.8, 0.4);
    ev(wOk, 'chime', 0.4);
    // About ten sessions: a learning curve.
    const CX = 690, CY = 0, CWd = 460, CHd = 340;
    const curveG = G(flowG);
    const curveN = N(curveG, { x: FC[0] + CX, y: FC[1] + CY, o: 0 });
    const px = v => -CWd / 2 + 40 + (v / 40) * (CWd - 70), py = f => CHd / 2 - 60 - f * (CHd - 120);
    const learned = v => 1 - Math.exp(-v / 3.4);
    let d = '';
    for (let v = 0; v <= 40; v += 0.5) d += `${v ? 'L' : 'M'} ${px(v).toFixed(1)} ${py(learned(v)).toFixed(1)} `;
    const curve = s('path', { d, fill: 'none', stroke: C.petrol, 'stroke-width': 4, 'stroke-linecap': 'round', 'stroke-linejoin': 'round' });
    const mark10 = s('g', {},
      s('line', { x1: px(10), y1: py(0), x2: px(10), y2: py(learned(10)), stroke: C.petrol, 'stroke-width': 2, 'stroke-dasharray': '4 6' }),
      s('circle', { cx: px(10), cy: py(learned(10)), r: 9, fill: C.petrolText, stroke: '#0b0f11', 'stroke-width': 3 }),
      T('10 sessions', { x: px(10) + 16, y: py(learned(10)) + 44, size: 27, weight: 640, fill: C.petrolText }));
    E.add(curveG,
      s('rect', { x: -CWd / 2, y: -CHd / 2, width: CWd, height: CHd, rx: 20, fill: '#10171a', stroke: '#2b3639', 'stroke-width': 1.6 }),
      T('what it has learned', { x: -CWd / 2 + 24, y: -CHd / 2 + 44, size: 24, weight: 640, fill: C.subtle, ls: 0.1, caps: true }),
      s('line', { x1: px(0), y1: py(0), x2: px(40), y2: py(0), stroke: '#3a474b', 'stroke-width': 2 }),
      s('line', { x1: px(0), y1: py(0), x2: px(0), y2: py(1.02), stroke: '#3a474b', 'stroke-width': 2 }),
      T('sessions', { x: px(40), y: py(0) + 40, size: 26, fill: C.subtle, anchor: 'end' }),
      curve, mark10);
    const wT = Math.max(wTen, wOk + 0.6);
    H.fadeIn(curveN, wT - 0.1, 0.5, 16);
    H.drawOn(ctx, curve, wT + 0.2, 1.3, 'io2');
    H.popIn(N(mark10, { o: 0 }), wT + 1.0, 0.7, 0.6);
    ev(wT + 1.0, 'pop', 0.24);
    cam.go(wT - 0.3, 1.4, { x: 190, y: -4450, z: 0.96 }, 'smooth', 'io3');

    H.during(ctx, streamG, L0 - 0.6, wFlow + 1.6);
    H.during(ctx, flowG, wFlow - 3, R0 + 0.1);
    st.learnEnd = { flowG };
  }

  // ------------------------------------------------------------- the rule
  // A bar wipe of the mark's three bars. Then a balance: a saved turn on one
  // end, a wasted lookup on the other; its fulcrum slides to where it
  // balances, at 0.3, and the beam stands up as the gauge lookups are
  // weighed on.
  function rule(ctx, st) {
    const { S, word, cam, ev } = ctx;
    const R0 = S.rule.t0, Q0 = S.results.t0;
    const w = p => word('rule', p);
    const RC = [3600, -4000];

    // The wipe: the mark's bars sweep across, each before the last is done.
    const tw = R0 - 0.64;
    const cols = [C.petrol, '#2aabb2', '#20949b'];
    const off = [0, 110, 180];
    cols.forEach((col, i) => {
      const r = s('rect', { x: 0, y: i * 360 - 6, width: 2600, height: 372, rx: 70, fill: col });
      st.layers.wipe.appendChild(r);
      const a = tw + i * 0.07, b = tw + 0.68 + i * 0.07;
      ctx.fn(t => {
        if (t < a || t > b + 0.55) { E.attr(r, 'display', 'none'); return; }
        E.attr(r, 'display', 'inline');
        const x = t < b ? L(-2700, -340 + off[i], E.P(t, a, 0.46, 'io3')) : L(-340 + off[i], 1960 + off[i], E.P(t, b, 0.5, 'io3'));
        E.attr(r, 'x', x.toFixed(1));
      });
    });
    const tc = tw + 0.62;
    ev(tw, 'whoosh-fast', 0.42);
    cam.cut(tc, { x: RC[0], y: RC[1] + 20, z: 1.0 });
    cam.go(tc + 0.01, Q0 - tc - 0.5, { x: RC[0], y: RC[1] + 14, z: 1.05 }, 'smooth', 'io2');

    const ruleG = G(W.root, { id: 'rule' });
    ruleG.setAttribute('transform', `translate(${RC[0]} ${RC[1]})`);
    const BL = 1000, BY = -60;
    const wChance = Math.max(w('chance that the agent'), R0 + 0.5);
    const wSaved = Math.max(w('a turn saved'), wChance + 1.0);
    const wWasted = Math.max(w('a lookup wasted'), wSaved + 0.7);
    const wThirty = Math.max(w('thirty percent'), wWasted + 2.0);
    const fMove = wWasted + 0.45, fDur = Math.max(1.2, Math.min(1.8, wThirty - fMove - 0.2));
    const F = t => L(0.5, 0.2966, E.P(t, fMove, fDur, 'io3'));
    const tRot = wThirty + 1.0;
    // The beam's angle: a damped spring towards where the torques point it,
    // simulated once at 240 Hz, then read back by time.
    const Wl = 6000, Wr = 2530;
    const dropL = wSaved + 0.35, dropR = wWasted + 0.35;
    const target = t => {
      const f = F(t), l = t >= dropL ? Wl : 0, r = t >= dropR ? Wr : 0;
      const tau = l * f - r * (1 - f);
      return -Math.max(-11, Math.min(11, tau / 260));
    };
    const sim = [];
    { let th = 0, om = 0; const dt = 1 / 240; for (let t = R0 - 1; t <= tRot + 2; t += dt) { const acc = -58 * (th - target(t)) - 6.2 * om; om += acc * dt; th += om * dt; sim.push(th); } }
    const theta = t => { const i = (t - (R0 - 1)) * 240; if (i <= 0) return 0; if (i >= sim.length - 1) return sim[sim.length - 1]; const j = Math.floor(i); return L(sim[j], sim[j + 1], i - j); };

    // The balance builds as the wipe clears, so no frame after it is empty:
    // the fulcrum, then the beam drawn out from it both ways, then the pans.
    // The words it weighs come with the voice.
    const tb = R0 + 0.12;
    const beamWrap = G(ruleG);
    const beamG = G(beamWrap);
    const beamN = N(beamWrap, { o: 0 });
    H.fadeIn(beamN, tb + 0.08, 0.2, 0);
    const drawRect = s('rect', { x: 0, y: BY - 80, width: 0, height: 120 });
    const beamBody = G(beamG, { 'clip-path': 'url(#clipBeamDraw)' });
    ruleG.appendChild(s('clipPath', { id: 'clipBeamDraw' }, drawRect));
    ctx.fn(t => {
      const q = E.P(t, tb + 0.08, 0.8, 'out3');
      const half = q >= 1 ? 4000 : q * (BL / 2 + 26);
      E.attr(drawRect, 'x', (-half).toFixed(1));
      E.attr(drawRect, 'width', (2 * half).toFixed(1));
      E.attr(drawRect, 'y', q >= 1 ? -4000 : BY - 80);
      E.attr(drawRect, 'height', q >= 1 ? 8000 : 120);
    });
    E.add(beamBody, s('rect', { x: -BL / 2 - 22, y: BY - 9, width: BL + 44, height: 18, rx: 9, fill: '#263033', stroke: '#3c494d', 'stroke-width': 1.6 }));
    for (let i = 0; i <= 10; i++) {
      const x = -BL / 2 + (i * BL) / 10, big = i % 5 === 0;
      beamBody.appendChild(s('line', { x1: x, y1: BY - (big ? 9 : 5), x2: x, y2: BY + (big ? 9 : 5), stroke: big ? '#8a969a' : '#56646a', 'stroke-width': big ? 3 : 2 }));
    }
    const tickLabs = [[0, '0'], [0.5, '0.5'], [1, '1']].map(([p, text]) => {
      const g = s('g', {}, T(text, { x: 0, y: 9, size: 27, weight: 600, fill: C.subtle, anchor: 'middle', tnum: true }));
      beamBody.appendChild(g);
      return { g, x: -BL / 2 + p * BL, y: BY - 34 };
    });
    // The scale's measure, on its own line below the balance: nothing crosses it.
    const axis = T('the chance a lookup is used before the next write', { x: 0, y: 360, size: 30, weight: 540, fill: C.muted, anchor: 'middle' });
    ruleG.appendChild(axis);
    const axisN = N(axis, { o: 0 });
    H.fadeIn(axisN, Math.max(wChance + 0.3, tb + 0.9), 0.6, 10);
    // The fulcrum.
    const fulG = G(ruleG);
    const ful = s('path', { d: 'M 0 -2 L -46 78 L 46 78 Z', fill: '#1b2427', stroke: '#56646a', 'stroke-width': 2, 'stroke-linejoin': 'round' });
    const fulDot = s('circle', { cx: 0, cy: 0, r: 7, fill: C.text });
    E.add(fulG, ful, fulDot);
    const fulN = N(fulG, { o: 0 });
    H.fadeIn(fulN, tb, 0.35, 12);
    // The pans and their weights.
    const pan = (color, wdt, hgt, lines, ink) => {
      const g = s('g', {});
      const strings = s('path', { d: '', fill: 'none', stroke: '#56646a', 'stroke-width': 2 });
      const dish = s('path', { d: 'M -150 0 Q 0 28 150 0', fill: 'none', stroke: '#8a969a', 'stroke-width': 4, 'stroke-linecap': 'round' });
      const wt = s('g', {},
        s('rect', { x: -wdt / 2, y: -hgt - 4, width: wdt, height: hgt, rx: 14, fill: color }),
        ...lines.map((ln, i) => T(ln[0], { x: 0, y: -hgt - 4 + hgt / 2 + (i - (lines.length - 1) / 2) * ln[2] * 1.25 + ln[2] * 0.36, size: ln[2], weight: ln[1], fill: ink, anchor: 'middle' })));
      E.add(g, strings, dish, wt);
      ruleG.appendChild(g);
      return { g, strings, dish, wt, n: N(g, { o: 0 }), wn: N(wt, { o: 0 }) };
    };
    const left = pan(C.petrol, 282, 140, [['a turn saved', 680, 32], ['≈ 6,000 tokens', 560, 27]], '#06201f');
    const right = pan(C.warm, 234, 72, [['a lookup wasted', 680, 25], ['≈ 2,530 tokens', 560, 23]], '#2a1804');
    [left, right].forEach((p, i) => H.fadeIn(p.n, tb + 0.5 + i * 0.08, 0.45, 0));
    left.wn.key('o', [wSaved, 0], [wSaved + 0.1, 1, 'lin']); left.wn.key('dy', [wSaved, -560], [dropL + 0.55, 0, 'springHard']);
    right.wn.key('o', [wWasted, 0], [wWasted + 0.1, 1, 'lin']); right.wn.key('dy', [wWasted, -560], [dropR + 0.55, 0, 'springHard']);
    ev(dropL, 'thump', 0.4); ev(dropR, 'thump', 0.3);
    ev(fMove + fDur - 0.05, 'click', 0.3);
    // Standing up: the beam turns into the gauge; pivot, angle and scale.
    const PV1 = [-380, 132], SC1 = 0.66;
    ctx.fn(t => {
      const f = F(t), fx = -BL / 2 + f * BL;
      const q = E.P(t, tRot, 1.25, 'io3');
      const th = L(theta(t), -90, q), sc = L(1, SC1, q);
      const px = L(fx, PV1[0], q), py = L(BY, PV1[1], q);
      E.attr(beamG, 'transform', `translate(${px.toFixed(2)} ${py.toFixed(2)}) rotate(${th.toFixed(3)}) scale(${sc.toFixed(4)}) translate(${(-fx).toFixed(2)} ${-BY})`);
      // Keep the tick labels upright.
      tickLabs.forEach(tl => E.attr(tl.g, 'transform', `translate(${tl.x} ${tl.y}) rotate(${(-th).toFixed(3)}) scale(${L(1, 1.45, q).toFixed(4)})`));
      fulN.p.x = fx; fulN.p.y = BY + 9;
      fulN.p.o *= 1 - E.P(t, tRot - 0.2, 0.4, 'lin');
      axisN.p.o *= 1 - E.P(t, tRot - 0.3, 0.3, 'lin');
      // The pans hang from the beam's ends.
      const rad = (th * Math.PI) / 180;
      [[left, -BL / 2], [right, BL / 2]].forEach(([p, ex]) => {
        const dx = ex - fx;
        const x = fx + dx * Math.cos(rad), y = BY + dx * Math.sin(rad);
        p.n.p.x = x; p.n.p.y = y + 170;
        E.attr(p.strings, 'd', `M 0 -170 L -142 0 M 0 -170 L 142 0`);
        p.n.p.o *= 1 - E.P(t, tRot - 0.25, 0.35, 'lin');
      });
    });
    // The balance point, named.
    const frac = s('g', {},
      T('δ', { x: 0, y: -26, size: 34, weight: 600, fill: C.warm, anchor: 'middle' }),
      s('line', { x1: -62, y1: -12, x2: 62, y2: -12, stroke: C.text, 'stroke-width': 2.5 }),
      TM([['β', C.petrolText], [' + ', C.text], ['δ', C.warm]], { x: 0, y: 26, size: 34, weight: 600, anchor: 'middle' }),
      T('≈ 0.3', { x: 84, y: 12, size: 40, weight: 700, fill: C.text }));
    ruleG.appendChild(frac);
    const fracN = N(frac, { o: 0 });
    ctx.fn(t => { fracN.p.x = -BL / 2 + F(t) * BL - 30; fracN.p.y = -240; });
    H.fadeIn(fracN, wThirty - 0.1, 0.5, 12);
    H.fadeOut(fracN, tRot - 0.1, 0.35);
    ev(wThirty, 'chime', 0.28);

    // The threshold, across the gauge.
    const gy = p => PV1[1] - (p - 0.2966) * BL * SC1;
    const thr = s('path', { d: '', fill: 'none', stroke: C.text, 'stroke-width': 2.5, 'stroke-dasharray': '10 9' });
    const thrLab = s('g', {},
      T('0.3', { x: PV1[0] - 52, y: gy(0.3) + 15, size: 46, weight: 700, fill: C.text, anchor: 'end' }),
      T('threshold', { x: PV1[0] - 52, y: gy(0.3) + 50, size: 24, weight: 640, fill: C.subtle, anchor: 'end', ls: 0.1, caps: true }));
    E.add(ruleG, thr, thrLab);
    // Drawn on by its end, so it keeps its dashes.
    ctx.fn(t => { const q = E.P(t, tRot + 1.0, 0.7, 'io3'); E.attr(thr, 'd', q > 0 ? `M ${PV1[0] - 30} ${gy(0.3).toFixed(1)} H ${L(PV1[0] - 30, 660, q).toFixed(1)}` : ''); });
    H.fadeIn(N(thrLab, { o: 0 }), tRot + 1.0, 0.5, 0);
    // Candidate lookups, placed by their chance, then made or not.
    const wModel = Math.max(w('No extra model'), tRot + 1.5);
    const wCounts = Math.max(w('just counts'), wModel + 1.3);
    const cands = [['get_order_details', 0.94, '47 / 50'], ['get_product_details', 0.61, '22 / 36'], ['list_all_product_types', 0.33, '5 / 15'], ['find_user_id_by_email', 0.12, '3 / 25']];
    const slotY = [-300, -120, 40, 250];
    cands.forEach(([tool, p, cnt], i) => {
      const y = gy(p), cy = slotY[i], cx = -210, cw = 700, ch = 76;
      const above = p >= 0.3;
      const g = s('g', {});
      const lead = s('path', { d: `M ${PV1[0]} ${y} H ${PV1[0] + 70} L ${cx - 20} ${cy} H ${cx}`, fill: 'none', stroke: '#56646a', 'stroke-width': 2 });
      const dot = s('circle', { cx: PV1[0], cy: y, r: 8, fill: C.text });
      const box = s('rect', { x: cx, y: cy - ch / 2, width: cw, height: ch, rx: 16, fill: '#151c1f', stroke: '#3a474b', 'stroke-width': 2 });
      const name = T(tool, { x: cx + 26, y: cy + 10, size: 27, mono: true, fill: C.text });
      const cntT = T(cnt, { x: cx + cw - 136, y: cy + 10, size: 25, mono: true, fill: C.subtle, anchor: 'end' });
      const pT = T(p.toFixed(2), { x: cx + cw - 24, y: cy + 11, size: 32, weight: 700, fill: C.text, anchor: 'end', tnum: true });
      E.add(g, lead, dot, box, name, cntT, pT);
      ruleG.appendChild(g);
      const n = N(g, { o: 0 });
      const a = wModel + i * 0.2;
      n.key('o', [a, 0], [a + 0.3, 1, 'out2']);
      n.key('dx', [a, 700], [a + 0.8, 0, 'out4']);
      ev(a, 'tick', 0.18);
      // Counted uses light up when the voice says it counts.
      ctx.fn(t => E.attr(cntT, 'fill', E.mixColor(C.subtle, C.text, E.pulse(t, wCounts - 0.1, wCounts + 1.6, 0.2, 0.5))));
      const f0 = wCounts + 0.35 + i * 0.14;
      if (above) {
        ctx.fn(t => {
          const k = E.P(t, f0, 0.35, 'out2');
          E.attr(box, 'stroke', E.mixColor('#3a474b', C.petrol, k));
          E.attr(box, 'fill', E.mixColor('#151c1f', '#0c2528', k));
          E.attr(pT, 'fill', E.mixColor(C.text, C.petrolText, k));
          E.attr(dot, 'fill', E.mixColor(C.text, C.petrol, k));
        });
        // It fires: a lookup leaves for the server.
        const shot = s('g', {}, s('rect', { x: -40, y: -13, width: 80, height: 26, rx: 13, fill: C.petrol }), s('circle', { cx: -54, cy: 0, r: 5, fill: C.petrol, opacity: 0.6 }));
        ruleG.appendChild(shot);
        H.flight(N(shot, { o: 0 }), [
          { t: f0, x: cx + cw - 40, y: cy, s: 0.4, o: 0 },
          { t: f0 + 0.15, x: cx + cw + 30, y: cy, s: 1, o: 1, ease: 'out2' },
          { t: f0 + 0.75, x: cx + cw + 420, y: cy, s: 0.7, o: 0, ease: 'in2' },
        ]);
        ev(f0, 'pop', 0.26);
      } else {
        ctx.fn(t => { const k = E.P(t, f0, 0.5, 'io2'); n.p.o *= 1 - 0.6 * k; });
      }
    });
    H.during(ctx, ruleG, tc - 0.05, Q0 + 0.7);
  }

  // ----------------------------------------------------------- the results
  function results(ctx, st) {
    const { S, word, cam, ev } = ctx;
    const Q0 = S.results.t0, P0 = S.prompt.t0;
    const w = p => word('results', p);
    const RS = [7200, -4000];
    // The whip pan, over faint shapes that streak.
    cam.go(Q0 - 0.34, 0.66, { x: RS[0], y: RS[1] + 10, z: 1.0 }, 'smooth', 'io4');
    cam.go(Q0 + 0.4, Math.max(1, P0 - Q0 - 0.8), { x: RS[0] + 16, y: RS[1] + 6, z: 1.03 }, 'smooth', 'io2');
    ev(Q0 - 0.36, 'whoosh-fast', 0.5);
    const decor = G(W.root);
    const rnd = E.rng(515);
    for (let i = 0; i < 26; i++) {
      const x = rnd.range(4200, 6600), y = rnd.range(-4460, -3540), wd = rnd.range(90, 260);
      decor.appendChild(s('rect', { x, y, width: wd, height: rnd.pick([10, 18, 44, 64]), rx: 9, fill: rnd.pick([C.petrol, '#2a3437', '#3a474b', C.warm]), opacity: rnd.range(0.12, 0.35) }));
    }
    H.during(ctx, decor, Q0 - 0.5, Q0 + 0.5);

    const chartG = G(W.root, { id: 'results' });
    const chart = G(chartG);
    chartG.setAttribute('transform', `translate(${RS[0]} ${RS[1]})`);
    const chartN = N(chart);
    const title = T('Fewer LLM turns with stretto', { x: -820, y: -318, size: 62, weight: 650, ls: -0.03 });
    const sub = T('Live · τ²-bench retail and airline · 28 tasks', { x: -818, y: -256, size: 33, weight: 520, fill: C.muted });
    const legend = s('g', {},
      s('rect', { x: 0, y: -21, width: 28, height: 28, rx: 6, fill: C.base }), T('without', { x: 42, y: 2, size: 28, fill: C.muted }),
      s('rect', { x: 196, y: -21, width: 28, height: 28, rx: 6, fill: C.petrol }), T('with stretto', { x: 238, y: 2, size: 28, fill: C.muted }));
    const foot = T('Live, against runs without stretto · three trials of each task for the Claude models', { x: -818, y: 380, size: 27, fill: C.subtle });
    E.add(chart, title, sub, legend, foot);
    const tN = N(title, { o: 0 }), sN = N(sub, { o: 0 }), lN = N(legend, { x: 370, y: -300, o: 0 }), fN = N(foot, { o: 0 });
    H.fadeIn(tN, Q0 + 0.25, 0.6, 16);
    H.fadeIn(sN, Q0 + 0.45, 0.6, 12);
    H.fadeIn(lN, Q0 + 0.8, 0.5, 0);
    const rowsSpec = [['Claude Sonnet 5', 20.5, w('Sonnet')], ['Claude Haiku 4.5', 22.4, w('Haiku')], ['GLM-5.3', 27.9, w('G L M')]];
    const BX = -380, BW = 880, BH = 56;
    rowsSpec.forEach(([name, pct, at], i) => {
      const y = -150 + i * 170;
      const a = Math.max(at, Q0 + 1.0 + i * 0.6);
      const nameEl = T(name, { x: -820, y: y + 10, size: 34, weight: 600 });
      const gray = s('rect', { x: BX, y: y - BH / 2, width: BW, height: BH, rx: 12, fill: C.base });
      const hatch = s('rect', { x: BX, y: y - BH / 2, width: BW, height: BH, rx: 12, fill: 'url(#pHatch)' });
      const pet = s('rect', { x: BX, y: y - BH / 2, width: BW, height: BH, rx: 12, fill: C.petrol });
      const pctEl = T('', { x: BX + BW + 36, y: y + 20, size: 58, weight: 700, fill: C.petrolText, tnum: true, ls: -0.02 });
      const savedLab = T('turns saved', { x: BX + BW - 12, y: y + 9, size: 26, weight: 640, fill: C.text, anchor: 'end' });
      E.add(chart, nameEl, gray, hatch, pet, savedLab, pctEl);
      // The bar without stretto draws in as the pan settles; the bar with it
      // lands when the voice names the model.
      const g0 = Q0 + 0.55 + i * 0.14;
      H.fadeIn(N(nameEl, { o: 0 }), g0 - 0.1, 0.5, 10);
      const ratio = 1 - pct / 100;
      ctx.fn(t => {
        const g = E.P(t, g0, 0.6, 'out3');
        const q = E.P(t, a + 0.3, 1.1, 'io3');
        E.attr(gray, 'width', (BW * g).toFixed(1));
        E.attr(gray, 'display', g > 0 ? 'inline' : 'none');
        E.attr(hatch, 'display', q > 0 ? 'inline' : 'none');
        E.attr(hatch, 'x', (BX + BW * (1 - (1 - ratio) * q)).toFixed(1));
        E.attr(hatch, 'width', (BW * (1 - ratio) * q).toFixed(1));
        E.attr(pet, 'display', q > 0 ? 'inline' : 'none');
        E.attr(pet, 'width', (BW * (1 - (1 - ratio) * q)).toFixed(1));
        E.text(pctEl, q > 0 ? `−${(pct * q).toFixed(1)}%` : '');
        E.attr(savedLab, 'opacity', (i === 0 ? E.P(t, a + 1.2, 0.5) : 0).toFixed(3));
      });
      ev(g0 + 0.05, 'tick', 0.12);
      ev(a + 0.25, 'thump', 0.24);
      ev(a + 1.35, 'tick', 0.2);
    });
    H.fadeIn(fN, Math.max(rowsSpec[2][2] + 1.6, Q0 + 4), 0.6, 8);
    st.chartN = chartN;
    st.resultsG = chartG;
    H.during(ctx, chartG, Q0 - 0.6, S.start.t0 + 0.1);
  }

  // ------------------------------------------------ the prompt, split screen
  function prompt(ctx, st) {
    const { S, word, cam, ev } = ctx;
    const P0 = S.prompt.t0;
    const RS = [7200, -4000];
    const w = p => word('prompt', p);
    const chartG = st.resultsG;
    // The chart goes up and out as the two halves come in.
    st.chartN.key('o', [P0 - 0.5, 1], [P0 + 0.1, 0, 'in2']);
    st.chartN.key('dy', [P0 - 0.5, 0], [P0 + 0.1, -80, 'in2']);
    const split = G(chartG);
    const div = s('line', { x1: 0, y1: -470, x2: 0, y2: 470, stroke: '#2f3b3f', 'stroke-width': 2.5 });
    split.appendChild(div);
    const divN = N(div, { o: 0 });
    divN.key('o', [P0 - 0.2, 0], [P0 + 0.05, 1]);
    divN.key('sy', [P0 - 0.2, 0], [P0 + 0.5, 1, 'io3']);
    ev(P0 - 0.35, 'swoosh-in', 0.36);
    const panel = (x, from, delay) => {
      const g = G(split);
      g.appendChild(s('rect', { x: -425, y: -470, width: 850, height: 940, rx: 28, fill: '#0e1417', stroke: '#222c2f', 'stroke-width': 1.6 }));
      const n = N(g, { x, o: 0 });
      n.key('o', [P0 - 0.35 + delay, 0], [P0 - 0.1 + delay, 1, 'lin']);
      n.key('dy', [P0 - 0.35 + delay, from], [P0 + 0.6 + delay, 0, 'io4']);
      return g;
    };
    const Lp = panel(-455, -1150, 0), Rp = panel(455, 1150, 0.1);
    cam.go(P0 - 0.3, 1.2, { x: RS[0], y: RS[1], z: 0.985 }, 'smooth', 'io3');
    // Titles.
    E.add(Lp, T('A prompt', { x: -390, y: -386, size: 48, weight: 650, ls: -0.025 }),
      T('“Make independent calls at once.”', { x: -390, y: -334, size: 30, weight: 520, fill: C.muted }));
    E.add(Rp, TM([['The prompt, plus ', C.text], ['stretto', C.petrolText]], { x: -390, y: -386, size: 48, weight: 650, ls: -0.025 }),
      T('Reads what the results reveal.', { x: -390, y: -334, size: 30, weight: 520, fill: C.muted }));
    // What each can do: a prompt batches only calls the agent already knows.
    const chip = (g, x, y, text, kind, wd) => {
      const col = kind === 'q' ? '#56646a' : kind === 'l' ? C.petrol : '#8b989c';
      const el = s('g', {},
        s('rect', { x, y: y - 27, width: wd, height: 54, rx: 13, fill: kind === 'l' ? '#0c2528' : '#151c1f', stroke: col, 'stroke-width': 2, 'stroke-dasharray': kind === 'q' ? '8 7' : 'none' }),
        T(text, { x: x + 18, y: y + 8, size: 24, mono: true, fill: kind === 'q' ? C.muted : kind === 'l' ? C.petrolText : C.text }));
      g.appendChild(el);
      return N(el, { o: 0 });
    };
    const bracket = (g, y0, y1, text, col) => {
      const el = s('g', {},
        s('path', { d: `M 330 ${y0} h 14 V ${y1} h -14`, fill: 'none', stroke: col, 'stroke-width': 2.5, 'stroke-linecap': 'round' }),
        T(text, { x: 356, y: (y0 + y1) / 2 + 8, size: 22, weight: 620, fill: col }));
      g.appendChild(el);
      return N(el, { o: 0 });
    };
    const tL = P0 + 0.9;
    const lA = chip(Lp, -390, -250, 'get_user_details', 'k', 268), lB = chip(Lp, -104, -250, 'list_all_product_types', 'k', 356);
    const lC = chip(Lp, -390, -160, 'get_order_details(?)', 'q', 326), lD = chip(Lp, -46, -160, 'get_order_details(?)', 'q', 326);
    [lA, lB].forEach((n, i) => H.fadeIn(n, tL + i * 0.12, 0.4, 10));
    [lC, lD].forEach((n, i) => H.fadeIn(n, tL + 0.5 + i * 0.12, 0.4, 10));
    const bl1 = s('path', { d: 'M -400 -286 h -14 V -214 h 14', fill: 'none', stroke: '#8b989c', 'stroke-width': 2.5 });
    const bl2 = s('path', { d: 'M -400 -196 h -14 V -124 h 14', fill: 'none', stroke: '#56646a', 'stroke-width': 2.5 });
    E.add(Lp, bl1, bl2);
    const cap = (g, text, y) => { const el = T(text, { x: -390, y, size: 28, weight: 540, fill: C.muted }); g.appendChild(el); return N(el, { o: 0 }); };
    const lcap1 = cap(Lp, 'one turn: the calls it already knows', -84);
    const lcap2 = cap(Lp, 'the order ids come in a result it has not seen', -44);
    [N(bl1, { o: 0 }), N(bl2, { o: 0 })].forEach((n, i) => H.fadeIn(n, tL + 0.3 + i * 0.5, 0.4, 0));
    H.fadeIn(lcap1, tL + 0.6, 0.5, 8); H.fadeIn(lcap2, tL + 0.9, 0.5, 8);
    // stretto reads what the results reveal.
    const wEven = Math.max(w('Even with'), tL + 1.4);
    const tR = Math.max(P0 + 1.2, wEven - 1.4);
    const rA = chip(Rp, -390, -250, 'get_user_details', 'k', 268);
    const rC = chip(Rp, -390, -160, 'get_order_details #W7a', 'l', 356), rD = chip(Rp, -16, -160, 'get_order_details #W7b', 'l', 356);
    H.fadeIn(rA, tR, 0.4, 10);
    [rC, rD].forEach((n, i) => { H.popIn(n, tR + 0.6 + i * 0.16, 0.6, 0.7); ev(tR + 0.6 + i * 0.16, 'pop', 0.2); });
    const br = s('path', { d: 'M -400 -286 h -14 V -124 h 14', fill: 'none', stroke: C.petrol, 'stroke-width': 2.5 });
    Rp.appendChild(br);
    H.fadeIn(N(br, { o: 0 }), tR + 0.9, 0.4, 0);
    const rcap1 = cap(Rp, 'one turn: the reads ride in the same result', -84);
    H.fadeIn(rcap1, tR + 1.0, 0.5, 8);
    // The numbers.
    const bars = (g, rows, at, color, labColor, caption) => {
      const capEl = T(caption, { x: -390, y: 38, size: 28, weight: 580, fill: C.subtle });
      g.appendChild(capEl);
      H.fadeIn(N(capEl, { o: 0 }), at - 0.3, 0.4, 0);
      rows.forEach(([name, pct], i) => {
        const y = 116 + i * 124, bx = -390, bw = 470, bh = 42;
        const a = at + i * 0.22;
        const nm = T(name, { x: bx, y: y - 18, size: 28, weight: 620, fill: C.text });
        const base = s('rect', { x: bx, y, width: bw, height: bh, rx: 10, fill: '#2a3437' });
        const bar = s('rect', { x: bx, y, width: bw, height: bh, rx: 10, fill: color });
        const lab = T('', { x: bx + bw + 30, y: y + 35, size: 46, weight: 700, fill: labColor, tnum: true, ls: -0.02 });
        E.add(g, nm, base, bar, lab);
        H.fadeIn(N(nm, { o: 0 }), a - 0.25, 0.4, 6);
        ctx.fn(t => {
          const q = E.P(t, a, 1.0, 'io3');
          E.attr(bar, 'width', (bw * (1 - (pct / 100) * q)).toFixed(1));
          E.text(lab, q > 0 ? `−${(pct * q).toFixed(1)}%` : '');
          E.attr(base, 'opacity', E.P(t, a - 0.3, 0.3).toFixed(3));
          E.attr(bar, 'opacity', E.P(t, a - 0.3, 0.3).toFixed(3));
        });
        ev(a, 'tick', 0.16);
      });
    };
    bars(Lp, [['Claude Sonnet 5', 3.4], ['Claude Haiku 4.5', 5.9], ['GLM-5.3', 7.6]], Math.max(w('saved only'), tL + 1.0), '#8b989c', C.muted, 'LLM turns, against no prompt');
    bars(Rp, [['Claude Sonnet 5', 22.9], ['Claude Haiku 4.5', 17.2]], Math.max(wEven + 0.5, tR + 1.3), C.petrol, C.petrolText, 'LLM turns, against the prompt alone');
  }

  // ------------------------------------------------------ getting started
  const INIT = 'stretto init --host claude-code --domain notes -- npx @modelcontextprotocol/server-filesystem ./notes';
  // What it prints (stdout, then its next steps on stderr), as a terminal
  // 120 columns wide shows it.
  const OUT = [
    ['o', 'claude mcp add notes -- stretto-proxy --record ~/.stretto/logs/notes --domain notes -- npx @modelcontextprotocol/server-filesystem ./notes'],
    ['e', ''],
    ['e', 'Run the command in the project where you use Claude Code. With `--scope user` it applies to every project; `--scope project` writes it to .mcp.json, to share.'],
    ['e', ''],
    ['h', 'Next steps (docs/walkthrough.md runs them on a real server):'],
    ['e', ''],
    ['e', '1. Use the agent as usual. The proxy records each session in ~/.stretto/logs/notes.'],
    ['e', ''],
    ['e', '2. Learn a flow from the sessions, with no key:'],
    ['c', '     stretto learn --sessions ~/.stretto/logs/notes --domain notes --habit-only --out ~/.stretto/notes.flow.json'],
    ['e', ''],
    ['e', '3. Review what it may do: the tools it may call, the lookups it may make and where their arguments come from:'],
    ['c', '     stretto flow-show ~/.stretto/notes.flow.json'],
    ['e', ''],
    ['e', '4. Run it in shadow: it decides and logs, but looks nothing up. Replace the configuration above with what this prints:'],
    ['c', '     stretto init --host claude-code --domain notes --flow ~/.stretto/notes.flow.json --shadow -- npx @modelcontextprotocol/server-filesystem ./notes'],
  ];
  const wrap = (text, cols) => { const out = []; for (let i = 0; i < Math.max(1, text.length); i += cols) out.push(text.slice(i, i + cols)); return out; };

  function start(ctx, st) {
    const { S, word, cam, ev } = ctx;
    const St0 = S.start.t0, En0 = S.end.t0;
    const w = p => word('start', p);
    const TS = [0, 2600];
    // A dip through black.
    const dip = s('rect', { x: 0, y: 0, width: 1920, height: 1080, fill: '#040607' });
    st.layers.dip.appendChild(dip);
    st.dipN = N(dip, { o: 0 });
    st.dipN.key('o', [St0 - 0.6, 0], [St0 - 0.08, 1, 'io2'], [St0 + 0.12, 1], [St0 + 0.8, 0, 'io2']);
    cam.cut(St0 - 0.04, { x: TS[0], y: TS[1] - 30, z: 1.0 });
    cam.go(St0, Math.max(1, En0 - St0 - 0.4), { x: TS[0], y: TS[1] - 24, z: 1.035 }, 'smooth', 'io2');
    ev(St0 - 0.55, 'whoosh-soft', 0.2);

    const startG = G(W.root, { id: 'start' });
    startG.setAttribute('transform', `translate(${TS[0]} ${TS[1]})`);
    // The world, in miniature: agent, then server; stretto drops in between.
    const mini = G(startG);
    const my = -330;
    const lane = s('line', { x1: -400, y1: my, x2: 400, y2: my, stroke: '#2a3538', 'stroke-width': 3, 'stroke-dasharray': '1 12', 'stroke-linecap': 'round' });
    mini.appendChild(lane);
    const ag = A.agent(), sv = A.server(), px = A.proxy();
    for (const a of [ag, sv, px]) a.label.setAttribute('display', 'none');
    const agG = s('g', {}, s('g', { transform: 'scale(0.5)' }, ag.g)), svG = s('g', {}, s('g', { transform: 'scale(0.4)' }, sv.g)), pxG = s('g', {}, s('g', { transform: 'scale(0.5)' }, px.g));
    E.add(mini, agG, pxG, svG);
    const agN = N(agG, { x: -470, y: my, o: 0 }), svN = N(svG, { x: 470, y: my, o: 0 }), pxN = N(pxG, { x: 0, y: my, o: 0 });
    const labs = [['agent', -470, my + 92], ['stretto', 0, my + 100], ['MCP server', 470, my + 110]].map(([text, x, y]) => {
      const el = label(text, x, y + 6, { size: 28, fill: text === 'stretto' ? C.petrolText : C.subtle });
      mini.appendChild(el);
      return N(el, { o: 0 });
    });
    ctx.fn(t => ag.set(t, 0.15 + 0.5 * E.pulse(t, St0 + 1, En0, 0.4, 0.4), t * 0.3, 0));
    H.popIn(agN, St0 + 0.55, 0.8, 0.3);
    H.popIn(svN, St0 + 0.7, 0.8, 0.3);
    H.fadeIn(labs[0], St0 + 0.8, 0.4, 6); H.fadeIn(labs[2], St0 + 0.95, 0.4, 6);
    const laneN = N(lane, { o: 0 });
    H.fadeIn(laneN, St0 + 0.9, 0.5, 0);
    // The terminal.
    const TW = 1520, TH = 470, tx0 = -TW / 2, ty0 = -150;
    const term = G(startG);
    const termN = N(term, { o: 0 });
    E.add(term,
      s('rect', { x: tx0 + 8, y: ty0 + 20, width: TW, height: TH, rx: 20, fill: '#000', opacity: 0.45 }),
      s('rect', { x: tx0, y: ty0, width: TW, height: TH, rx: 20, fill: '#0d1214', stroke: '#2b3639', 'stroke-width': 1.8 }),
      s('path', { d: `M ${tx0} ${ty0 + 50} H ${tx0 + TW}`, stroke: '#222c2f', 'stroke-width': 1.8 }),
      ...[0, 1, 2].map(i => s('circle', { cx: tx0 + 30 + i * 24, cy: ty0 + 25, r: 7, fill: '#2d383b' })),
      T('~/notes', { x: 0, y: ty0 + 32, size: 18, weight: 520, fill: C.subtle, anchor: 'middle' }));
    const badge = s('g', {},
      s('rect', { x: -160, y: -23, width: 320, height: 46, rx: 23, fill: C.soft, stroke: C.petrol, 'stroke-width': 1.6 }),
      T('open source · MIT', { x: 0, y: 9, size: 26, weight: 640, fill: C.petrolText, anchor: 'middle' }));
    term.appendChild(badge);
    const badgeN = N(badge, { x: tx0 + TW - 184, y: ty0 + 25, o: 0 });
    const tIn = St0 + 0.35;
    termN.key('o', [tIn, 0], [tIn + 0.3, 1, 'lin']);
    termN.key('dy', [tIn, 260], [tIn + 1.0, 0, 'springSoft']);
    const wOpen = Math.max(w('open source'), tIn + 0.4);
    H.popIn(badgeN, wOpen, 0.7, 0.6);
    ev(wOpen, 'pop', 0.24);
    // Its body: the command typed, then what it prints, scrolling.
    const clip = s('clipPath', { id: 'clipTerm' }, s('rect', { x: tx0 + 2, y: ty0 + 56, width: TW - 4, height: TH - 60 }));
    const body = s('g', {});
    E.add(term, clip, s('g', { 'clip-path': 'url(#clipTerm)' }, body));
    const bodyN = N(body);
    const FS = 19.5, LHt = 30, COLS = 121, bx = tx0 + 30, by = ty0 + 92;
    const cmdEl = TM([['$ ', C.petrolText, 700], ['', C.text]], { x: bx, y: by, size: FS, mono: true });
    body.appendChild(cmdEl);
    const typed = cmdEl.childNodes[1];
    const tType = Math.max(w('run stretto init') - 0.1, tIn + 1.0);
    const CPS = 62;
    H.type(ctx, typed, INIT, tType, CPS, true, tType + INIT.length / CPS + 0.25);
    for (let i = 0; i < INIT.length; i += 7) ev(tType + i / CPS, 'click', 0.07);
    const tEnter = tType + INIT.length / CPS + 0.35;
    ev(tEnter, 'click', 0.26);
    const rows = [];
    for (const [kind, text] of OUT) for (const ln of wrap(text, COLS)) rows.push([kind, ln]);
    const rowEls = rows.map(([kind, text], i) => {
      const el = T(text, { x: bx, y: by + (i + 1) * LHt, size: FS, mono: true, fill: kind === 'o' ? C.text : kind === 'h' ? C.muted : kind === 'c' ? C.petrolText : C.subtle });
      body.appendChild(el);
      const n = N(el, { o: 0 });
      const a = tEnter + 0.1 + i * 0.05;
      n.key('o', [a, 0], [a + 0.08, 1, 'lin']);
      return { a, el };
    });
    // Scroll by whole lines to keep the last printed line in view; a line
    // scrolled past the top is gone, not cut in half.
    const visible = Math.floor((TH - 70) / LHt) - 1;
    ctx.fn(t => {
      let shown = 0;
      rowEls.forEach(r => { if (t >= r.a) shown++; });
      const q = Math.max(0, shown + 1 - visible);
      bodyN.p.y = -LHt * q;
      E.attr(cmdEl, 'display', q >= 1 ? 'none' : 'inline');
      rowEls.forEach((r, i) => E.attr(r.el, 'display', i + 1 < q ? 'none' : 'inline'));
    });
    // stretto goes in front of the server.
    const tPx = Math.max(w('in front of'), tEnter + 0.3);
    pxN.key('o', [tPx, 0], [tPx + 0.12, 1, 'lin']);
    pxN.key('dy', [tPx, -300], [tPx + 0.9, 0, 'springHard']);
    H.fadeIn(labs[1], tPx + 0.4, 0.4, 6);
    ev(tPx + 0.25, 'thump', 0.3);
    // It learns from the agent's own sessions: calls flow through it.
    const wLearns = Math.max(w('learns from'), tPx + 0.9);
    for (let i = 0; i < 9; i++) {
      const a = wLearns + i * 0.42;
      const dot = s('rect', { x: -12, y: -7, width: 24, height: 14, rx: 7, fill: i % 3 === 2 ? C.petrol : '#8b989c' });
      mini.appendChild(dot);
      H.flight(N(dot, { o: 0 }), [
        { t: a, x: -400, y: my - 18, s: 0.6, o: 0 },
        { t: a + 0.2, x: -330, y: my - 18, s: 1, o: 1, ease: 'out2' },
        { t: a + 0.9, x: 0, y: my - 18, s: 0.4, o: 0.2, ease: 'io2' },
      ]);
      st.miniHits = (st.miniHits || []).concat(a + 0.85);
    }
    ctx.fn(t => px.bars.forEach((b, i) => {
      let k = 0;
      for (const h of st.miniHits || []) k = Math.max(k, E.pulse(t, h + i * 0.08, h + i * 0.08 + 0.5, 0.08, 0.35));
      E.attr(b.glow, 'opacity', (0.55 * k).toFixed(3));
    }));
    st.start = { termN, agN, svN, pxN, labs, laneN, my, TS, startG };
    H.during(ctx, startG, St0 - 0.1, S.end.t1 + 1);
  }

  // ------------------------------------------------------------- the end
  // The world pulls back into the mark: agent, stretto and server fly into
  // the three bars, each before the last has landed; then the wordmark.
  function end(ctx, st) {
    const { S, word, cam, ev } = ctx;
    const En0 = S.end.t0, TOT = S.end.t1;
    const { termN, agN, svN, pxN, labs, laneN, my, TS } = st.start;
    H.fadeOut(termN, En0 - 0.3, 0.5, 'in2');
    termN.key('dy', [En0 - 0.3, 0], [En0 + 0.3, 90, 'in2']);
    labs.forEach(n => H.fadeOut(n, En0 - 0.2, 0.3));
    H.fadeOut(laneN, En0 - 0.2, 0.3);
    cam.go(En0 - 0.3, 1.6, { x: TS[0], y: TS[1] + my, z: 1.0 }, 'smooth', 'io3');
    // Darken the world behind the card.
    const shade = s('rect', { x: 0, y: 0, width: 1920, height: 1080, fill: '#050809' });
    st.layers.dip.appendChild(shade);
    const shadeN = N(shade, { o: 0 });
    shadeN.key('o', [En0 + 0.3, 0], [En0 + 1.4, 0.8, 'io2']);

    const { LX, LY, k } = st.lockup;
    const lk = A.lockup(k);
    const card = G(st.layers.title);
    const lockG = G(card);
    lockG.appendChild(lk.g);
    N(lockG, { x: LX, y: LY });
    const cardN = N(card);
    ctx.fn(t => { const sc = 1 + 0.03 * E.P(t, En0 + 1, TOT - En0, 'io2'); cardN.p.s = sc; cardN.p.x = 960 * (1 - sc); cardN.p.y = 540 * (1 - sc); });
    // Each mini flies from where it is on screen to its bar.
    const minis = [[agN, -470, 0], [pxN, 0, 1], [svN, 470, 2]];
    const t0s = [En0 + 0.2, En0 + 0.36, En0 + 0.52];
    // The closing motif resolves (its last note, about 1.05 s in) just after
    // the voice's last word, and its chord rings under the end card.
    ctx.music.end = +Math.max(t0s[0], S.end.vend - 0.9).toFixed(3);
    minis.forEach(([mn, mx, bi], i) => {
      const a = t0s[i];
      mn.key('o', [a, 1], [a + 0.25, 0, 'in2']);
      const b = lk.mark.bars[bi];
      const bn = N(b.node);
      let from = null;
      b.node.on((p, t) => {
        if (t < a) { p.o = 0; return; }
        if (!from) {
          // Where the mini is on screen when it leaves, in the card's coordinates.
          const v = ctx.cam.at(a);
          const [sx, sy] = ctx.toScreen(v, TS[0] + mx, TS[1] + my);
          from = [sx, sy];
        }
        const q = E.P(t, a, 0.95, 'io3');
        const sc = cardN.p.s;
        const fx = (from[0] - cardN.p.x) / sc - LX, fy = (from[1] - cardN.p.y) / sc - LY;
        const w0 = 70, h0 = 70;
        const ww = L(w0, b.w, q), hh = L(h0, b.h, q);
        p.x = L(fx - w0 / 2, b.x, q); p.y = L(fy - h0 / 2, b.y, q);
        p.sx = ww / b.w; p.sy = hh / b.h; p.s = 1; p.o = Math.min(1, (t - a) / 0.12);
      });
      void bn;
      ev(a, 'swoosh-in', 0.22);
    });
    lk.word.letters.forEach((l, j) => {
      const n = N(l.node);
      const a = En0 + 1.05 + j * 0.055;
      n.key('o', [a, 0], [a + 0.35, 1, 'out2']);
      n.key('dy', [a, 520], [a + 0.7, 0, 'out4']);
    });
    ev(En0 + 1.05, 'shimmer', 0.3);
    const tag = T('Read ahead of your agent.', { x: 960, y: 668, size: 58, weight: 620, anchor: 'middle', ls: -0.028 });
    const url = TM([['alexnodeland.github.io/stretto', C.text], ['  ·  open source, MIT', C.subtle]], { x: 960, y: 760, size: 30, weight: 500, anchor: 'middle' });
    E.add(card, tag, url);
    H.fadeIn(N(tag, { o: 0 }), Math.max(word('end', 'Read ahead') - 0.15, En0 + 1.4), 0.8, 16);
    H.fadeIn(N(url, { o: 0 }), En0 + 2.1, 0.8, 10);
    H.during(ctx, card, En0, TOT + 1);
  }

  // ----------------------------------------------------------- the teaser
  // Eight seconds that loop: a call through stretto, two reads riding back
  // in its result, and the turns closing up.
  function teaser(ctx) {
    const { cam, ev } = ctx;
    const st = Sc.shared(ctx);
    const D = 8;
    E.LOOP = D;
    // A GIF is paid for by every pixel that changes: no vignette, no wide glows.
    document.getElementById('vignette').style.display = 'none';
    for (const el of [W.agent.g.querySelector('circle'), W.proxy.halo]) el.style.display = 'none';
    ctx.setDrift(() => 0);
    ctx.setDust(() => 0);
    cam.start = { x: 190, y: 80, z: 1.12, r: 0, tilt: 0 };
    W.fork.user.n.base.o = 0;
    W.fork.tool.n.base.o = 0;
    // Only what the loop needs: agent, stretto, server, five turns.
    W.customer.n.base.o = 0;
    W.scroll.g.base.o = 0;
    W.proxy.n.base.o = 1;
    W.P.scroll = [-420, 0];
    W.timeline.label.base.o = 1;
    const specs = [
      { tool: 'get user', arg: 'user_7' }, { tool: 'get order', arg: '#W7a' }, { tool: 'get order', arg: '#W7b' },
      { tool: 'reply', kind: 'reply' }, { tool: 'cancel order', arg: '#W7a', kind: 'write' },
    ];
    const x0 = -560;
    W.timeline.label.base.x = x0 - W.TL.x0;
    W.timeline.rail.setAttribute('x1', x0);
    const chips = specs.map((sp, i) => {
      const c = W.timeline.add(sp, i);
      c.n.base.o = 1;
      c.n.base.x = x0 + i * (W.TL.w + W.TL.gap);
      return c;
    });
    const brand = A.lockup(1.05);
    const bG = s('g', {}, brand.g);
    st.layers.title.appendChild(bG);
    N(bG, { x: 56, y: 48 });
    // The call, through stretto, to the users drawer.
    const c = 0.55;
    st.thinks.push([0.08, c + 0.1, 0.9]);
    const call = H.flyer(A.card({ dir: '→', title: 'get_user_details', sub: '{"user_id":"user_7"}' }).g);
    H.flight(call, [
      { t: c, x: H.agentOut[0] - 10, y: H.agentOut[1], s: 0.55, o: 0 },
      { t: c + 0.18, x: H.agentOut[0] + 40, y: H.agentOut[1], s: 1, o: 1, ease: 'out3' },
      { t: c + 0.65, x: W.P.proxy[0], y: W.laneY.call, s: 0.9, o: 1, ease: 'io3' },
      { t: c + 1.05, x: H.serverIn[0] - 150, y: W.laneY.call, s: 1, o: 1, ease: 'io3' },
      { t: c + 1.25, x: H.drawerAt(0)[0], y: H.drawerAt(0)[1], s: 0.3, o: 0, ease: 'in2' },
    ]);
    st.proxyHits.push(c + 0.55);
    st.access.push({ i: 0, t: c + 1.1, d: 1.1 });
    const r = c + 1.4;
    const { g: bundleG, riderNs } = H.bundle({ dir: '←', title: 'get_user_details', kind: 'result', sub: [['{"orders":["', null], ['#W7a', C.petrolText], ['","', null], ['#W7b', C.petrolText], ['"],…}', null]] });
    const bn = H.flyer(bundleG);
    const hold = [W.P.proxy[0] - 70, -292];
    const home = 4.35;
    H.flight(bn, [
      { t: r, x: H.drawerAt(0)[0], y: H.drawerAt(0)[1], s: 0.3, o: 0 },
      { t: r + 0.22, x: H.serverOut[0] - 150, y: W.laneY.result, s: 1, o: 1, ease: 'out3' },
      { t: r + 0.8, x: hold[0], y: hold[1], s: 1, o: 1, ease: 'io3', arc: 40 },
      { t: 3.35, x: hold[0], y: hold[1], s: 1, o: 1, ease: 'lin' },
      { t: home, x: H.agentIn[0] + 150, y: W.laneY.result + 6, s: 1, o: 1, ease: 'io3', arc: 30 },
      { t: home + 0.55, x: H.agentIn[0] + 150, y: W.laneY.result + 6, s: 1, o: 1, ease: 'lin' },
      { t: home + 1.05, x: W.P.agent[0] + 10, y: W.P.agent[1] + 4, s: 0.12, o: 0, ease: 'in3', oe: 'in4' },
    ]);
    st.glows.push([home + 0.9, 1.0]);
    st.proxyHits.push(r + 0.7);
    const [odx, ody] = H.drawerAt(1);
    const px = W.P.proxy[0];
    ['get_order_details #W7a', 'get_order_details #W7b'].forEach((text, i) => {
      const n = H.flyer(A.lookup(text).g);
      const a = r + 0.85 + i * 0.18;
      H.flight(n, [
        { t: a, x: px + 20, y: -20 + i * 10, s: 0.4, o: 0 },
        { t: a + 0.18, x: px + 130, y: -70 + i * 50, s: 1, o: 1, ease: 'out3' },
        { t: a + 0.6, x: odx - 60, y: ody - 10 + i * 12, s: 0.9, o: 1, ease: 'io3' },
        { t: a + 0.78, x: odx + 10, y: ody, s: 0.3, o: 0, ease: 'in2' },
      ]);
      st.access.push({ i: 1, t: a + 0.62, d: 0.9 });
      const rn = riderNs[i];
      const b0 = a + 0.85;
      rn.key('o', [b0, 0], [b0 + 0.22, 1, 'out2']);
      rn.key('dx', [b0, (odx - hold[0]) * 0.9], [b0 + 0.6, 0, 'io3']);
      rn.key('dy', [b0, ody - hold[1] - 60], [b0 + 0.6, i * 54, 'io3']);
    });
    // The turns close up, then open again for the loop.
    const cl = 4.6, op = 7.05;
    [1, 2].forEach((i, k) => {
      const ch = chips[i];
      ctx.fn(t => E.attr(ch.ring, 'opacity', E.pulse(t, cl - 0.5 + k * 0.1, cl + 0.3, 0.2, 0.25).toFixed(3)));
      ch.n.key('o', [cl + k * 0.08, 1], [cl + 0.45 + k * 0.08, 0, 'in2'], [op, 0], [op + 0.5, 1, 'out2']);
    });
    chips.slice(3).forEach((ch, k) => {
      const a = cl + 0.35 + k * 0.06;
      const x = x0 + (k + 1) * (W.TL.w + W.TL.gap);
      ch.n.key('x', [a, ch.n.base.x], [a + 0.75, x, 'io3'], [op - 0.2, x], [op + 0.5, ch.n.base.x, 'io3']);
      ctx.fn(t => E.text(ch.num, t >= a + 0.38 && t < op + 0.15 ? `TURN ${k + 2}` : `TURN ${k + 4}`));
    });
    const note = T('2 turns the agent no longer takes', { x: x0 + 2, y: W.TL.y + 150, size: 28, weight: 600, fill: C.petrolText });
    W.timeline.g.el.appendChild(note);
    const noteN = N(note, { o: 0 });
    noteN.key('o', [cl + 1.0, 0], [cl + 1.4, 1], [op - 0.4, 1], [op, 0]);
    void ev;
    return { duration: D };
  }

  // ------------------------------------------------------------ the order
  function video(ctx) {
    const st = Sc.shared(ctx);
    Sc.open(ctx, st);
    Sc.turns(ctx, st);
    Sc.decided(ctx, st);
    Sc.idea(ctx, st);
    Sc.safe(ctx, st);
    learn(ctx, st);
    rule(ctx, st);
    results(ctx, st);
    prompt(ctx, st);
    start(ctx, st);
    end(ctx, st);
    const { S } = ctx;
    H.during(ctx, W.tableau, S.turns.t0 - 0.3, S.learn.t0 + 3.2);
    return { duration: ctx.TOTAL };
  }

  Object.assign(Sc, { learn, rule, results, prompt, start, end, teaser, video });
})();
