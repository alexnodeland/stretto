// The world: every station of the explainer, laid out on one plane in world
// units (px at zoom 1), built from art.js. The scenes (scenes.js) move the
// camera across it and animate what is here.
//
//   the tableau        (0, 0)          customer, conversation, agent, stretto, server, turns
//   the first result   (-360, -300)    a card the camera dives into
//   the sessions       (0, -1000 .. -3850)  recorded sessions, flown over
//   the flow           (0, -4450)      the flow graph, its file and the diff
//   the rule           (3600, -4000)   the balance and the gauge
//   the results        (7200, -4000)   the chart, then the split screen
//   the start          (0, 2600)       the terminal and the world in miniature
(() => {
  const { s } = E;
  const { T, TM, label } = A;
  const root = document.getElementById('root');
  const ovRoot = document.getElementById('ovRoot');
  A.defs(document.getElementById('defs'));

  const nodes = [];
  const node = (el, base) => { const n = el instanceof E.Node ? el : new E.Node(el, base); if (base && el instanceof E.Node) Object.assign(n.base, base); nodes.push(n); return n; };
  const group = (parent, attrs = {}) => { const g = s('g', attrs); parent.appendChild(g); return g; };

  const W = { nodes, node, group, root, ovRoot };

  // ------------------------------------------------------------ the tableau
  const P = { customer: [-800, 24], scroll: [-520, 0], agent: [-200, 0], proxy: [215, 0], server: [640, 0] };
  W.P = P;
  const tab = group(root, { id: 'tableau' });
  W.tableau = tab;

  // Lanes: calls go right above the axis, results come back below it.
  const laneY = { call: -46, result: 46 };
  W.laneY = laneY;
  const lanes = group(tab);
  const lane = (y, dir) => {
    const x1 = -100, x2 = 498;
    const g = s('g', {},
      s('line', { x1, y1: y, x2, y2: y, stroke: '#2a3538', 'stroke-width': 2.5, 'stroke-dasharray': '1 11', 'stroke-linecap': 'round' }),
      s('path', { d: dir > 0 ? `M ${x2 - 12} ${y - 7} L ${x2} ${y} L ${x2 - 12} ${y + 7}` : `M ${x1 + 12} ${y - 7} L ${x1} ${y} L ${x1 + 12} ${y + 7}`, fill: 'none', stroke: '#3a474b', 'stroke-width': 2.5, 'stroke-linecap': 'round', 'stroke-linejoin': 'round' }));
    lanes.appendChild(g);
    return g;
  };
  W.lanes = { g: node(lanes), call: lane(laneY.call, 1), result: lane(laneY.result, -1) };

  // The conversation: what the model reads, whole, every turn.
  const scrollG = group(tab);
  const SC = { w: 184, h: 330 };
  const scrollClip = s('clipPath', { id: 'clipScroll' }, s('rect', { x: -SC.w / 2 + 6, y: -SC.h / 2 + 8, width: SC.w - 12, height: SC.h - 16, rx: 12 }));
  const scrollPanel = s('rect', { x: -SC.w / 2, y: -SC.h / 2, width: SC.w, height: SC.h, rx: 18, fill: '#10171a', stroke: '#2b3639', 'stroke-width': 1.6 });
  const scrollRead = s('rect', { x: -SC.w / 2 + 6, y: 0, width: SC.w - 12, height: 40, rx: 8, fill: C.petrol, 'fill-opacity': 0.13, stroke: C.petrol, 'stroke-opacity': 0.8, 'stroke-width': 1.5, opacity: 0 });
  const scrollEntries = s('g', {});
  const scrollInner = s('g', { 'clip-path': 'url(#clipScroll)' }, scrollEntries);
  const scrollLabel = label('conversation', 0, SC.h / 2 + 50);
  E.add(scrollG, scrollClip, A.shadow(0, SC.h / 2 + 12, 100, 13), scrollPanel, scrollInner, scrollRead, scrollLabel);
  W.scroll = { g: node(scrollG, { x: P.scroll[0], y: P.scroll[1] }), panel: scrollPanel, read: scrollRead, entries: [], label: scrollLabel, ...SC };
  // An entry: a kind of message, shown from tIn (and gone at tOut).
  const ENTRY = {
    user: { h: 14, bars: [[118, '#dfe5e6']] },
    call: { h: 12, bars: [[92, '#8a969a']], arrow: true },
    result: { h: 28, bars: [[128, C.petrol], [84, '#237f84']] },
    bundle: { h: 28, bars: [[128, C.petrol], [84, '#237f84']] },
    rider: { h: 12, bars: [[112, '#7fdde2']], clip: true },
    reply: { h: 14, bars: [[104, '#aab4b7']] },
    write: { h: 12, bars: [[100, C.warm]], arrow: true },
    tokens: { h: 10, bars: [], tokens: true },
  };
  W.scroll.push = (kind, tIn, tOut = Infinity) => {
    const spec = ENTRY[kind];
    const g = s('g', {});
    let y = 0;
    const x0 = -SC.w / 2 + 22;
    if (spec.arrow) g.appendChild(s('path', { d: `M ${x0} ${spec.h / 2} h 10 m -4 -4 l 4 4 l -4 4`, fill: 'none', stroke: spec.bars[0][1], 'stroke-width': 2, 'stroke-linecap': 'round', 'stroke-linejoin': 'round' }));
    if (spec.clip) g.appendChild(s('path', { d: `M ${x0 + 2} -8 v ${spec.h / 2 + 8} h 8`, fill: 'none', stroke: C.petrol, 'stroke-width': 2 }));
    for (const [w, fill] of spec.bars) {
      const off = spec.arrow ? 18 : spec.clip ? 16 : 0;
      g.appendChild(s('rect', { x: x0 + off, y, width: w - off, height: y ? 9 : 12, rx: y ? 4.5 : 6, fill }));
      y += 16;
    }
    if (spec.tokens) for (let i = 0; i < 6; i++) g.appendChild(s('rect', { x: x0 + i * 13, y: 1, width: 8, height: 8, rx: 2, fill: i % 2 ? '#8a969a' : '#6d797c' }));
    scrollEntries.appendChild(g);
    const e = { kind, tIn, tOut, h: spec.h, g, n: node(g) };
    W.scroll.entries.push(e);
    return e;
  };

  // The customer.
  const cust = A.customer(['Please cancel my', 'pending order.']);
  tab.appendChild(cust.g);
  W.customer = { ...cust, n: node(cust.g, { x: P.customer[0], y: P.customer[1] }), bubbleN: node(cust.bubble, { y: -84 }) };

  // The agent.
  const ag = A.agent();
  tab.appendChild(ag.g);
  W.agent = { ...ag, n: node(ag.g, { x: P.agent[0], y: P.agent[1] }) };
  // Its reading beam, from the core to the band of the conversation it reads.
  const beam = s('path', { d: 'M 0 0', fill: C.petrol, 'fill-opacity': 0.11, stroke: 'none' });
  tab.insertBefore(beam, ag.g);
  W.beam = beam;

  // The server.
  const sv = A.server();
  tab.appendChild(sv.g);
  W.server = { ...sv, n: node(sv.g, { x: P.server[0], y: P.server[1] }) };
  sv.drawers.forEach(d => node(d.node));
  node(sv.lock.node);

  // stretto, the proxy: off stage until the idea.
  const px = A.proxy();
  tab.appendChild(px.g);
  W.proxy = { ...px, n: node(px.g, { x: P.proxy[0], y: P.proxy[1], o: 0 }) };

  // The turns, as a timeline of chips under the tableau.
  const TL = { x0: -905, y: 336, w: 230, gap: 16, small: 50 };
  W.TL = TL;
  const tlG = group(tab);
  const tlLabel = label('LLM turns', TL.x0, TL.y - 52, { anchor: 'start', size: 30, fill: C.subtle });
  const tlRail = s('line', { x1: TL.x0, y1: TL.y + 104, x2: TL.x0 + 1810, y2: TL.y + 104, stroke: '#1f292c', 'stroke-width': 2 });
  E.add(tlG, tlRail);
  W.timeline = { g: node(tlG), label: node(tlLabel), rail: tlRail, chips: [] };
  tlG.appendChild(tlLabel);
  // The x of each turn's slot: five full chips, then small ones.
  W.slotX = i => (i < 5 ? TL.x0 + i * (TL.w + TL.gap) : TL.x0 + 5 * (TL.w + TL.gap) + (i - 5) * (TL.small + 11));
  W.timeline.add = (spec, i) => {
    const c = A.turnChip({ ...spec, n: i + 1, w: i < 5 ? TL.w : TL.small });
    tlG.appendChild(c.g);
    const n = node(c.node, { x: W.slotX(i), y: TL.y, o: 0 });
    const chip = { ...c, n, i, spec };
    W.timeline.chips.push(chip);
    return chip;
  };

  // The choice each turn makes: answer the user, or call a tool.
  const fork = group(tab);
  const forkArm = (d, text, tx, ty, anchor) => {
    const path = s('path', { d, fill: 'none', stroke: C.muted, 'stroke-width': 2.5, 'stroke-dasharray': '7 8', 'stroke-linecap': 'round' });
    const head = s('path', { d: 'M -9 -7 L 0 0 L -9 7', fill: 'none', stroke: C.muted, 'stroke-width': 2.5, 'stroke-linecap': 'round', 'stroke-linejoin': 'round' });
    const txt = T(text, { x: tx, y: ty, size: 28, weight: 580, fill: C.muted, anchor });
    const g = s('g', {}, path, head, txt);
    fork.appendChild(g);
    return { g, path, head, txt, n: node(g), len: path };
  };
  W.fork = {
    user: forkArm('M -232 -104 C -262 -180 -330 -214 -420 -214', 'answer the user', -436, -204, 'end'),
    tool: forkArm('M -168 -104 C -138 -180 -70 -214 20 -214', 'call a tool', 38, -204, 'start'),
  };

  // Things in flight: cards, lookups and riders are made by the scenes.
  W.flyers = group(tab);

  window.W = W;
})();
