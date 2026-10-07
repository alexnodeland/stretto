// The console film's chapters: stretto-console as a user drives it, in one
// browser window. Every picture is a shot of the real console over its test
// fixtures (capture.sh, capture.mjs: shots/*.png, and in shots/shots.js the
// boxes of what each line points at). The film moves in on those boxes, rings
// them, and moves a cursor to the controls the capture clicked. Times are words
// of the voice-over, never hand-typed seconds.
(() => {
  const { C, E, ramp, el, box, show, markSvg, wordmarkSvg, lerp } = window.ST;
  const SH = Object.fromEntries(window.SHOTS.shots.map(s => [s.id, s]));
  const [SW, SHH] = window.SHOTS.size;

  const TITLES = { overview: 'Overview', servers: 'Servers', sessions: 'Sessions', flows: 'Flows', jobs: 'Jobs', more: 'Everywhere' };

  // The window: the shots at 0.8 of their size, under an address bar.
  const K = 0.8, IW = SW * K, IH = SHH * K, BAR = 40;
  const FX = (1920 - IW) / 2, FY = 124;

  /** A box of a shot, grown by `pad` (shot pixels) on each side, or cut to `h`. */
  const bx = (id, name, { pad = 8, h = null } = {}) => {
    const b = SH[id].boxes[name];
    if (!b) throw new Error(`console: no box ${name} in ${id}`);
    return [b[0] - pad, b[1] - pad, b[2] + 2 * pad, (h ?? b[3]) + 2 * pad];
  };
  const centre = b => [b[0] + b[2] / 2, b[1] + b[3] / 2];

  /** The browser window, with one layer per shot, a camera, rings and a cursor. */
  function browser(ctx, layer) {
    const frame = box(layer, { x: FX, y: FY, w: IW + 2, h: IH + BAR + 2, cls: 'card' });
    const bar = box(frame, { x: 0, y: 0, w: IW, h: BAR, style: { borderBottom: `1px solid ${C.border}`, background: C.surface2 } });
    box(bar, { x: 18, y: 20, ay: 0.5, html: '<span style="color:#3b4649;font-size:14px;letter-spacing:4px">●●●</span>' });
    const url = box(bar, { x: IW / 2, y: 20, ax: 0.5, ay: 0.5, cls: 'code', text: '', style: { fontSize: '15px', color: C.subtle, padding: '5px 18px', background: C.bg, borderRadius: '7px', border: `1px solid ${C.border}`, whiteSpace: 'nowrap' } });
    const view = box(frame, { x: 0, y: BAR, w: IW, h: IH, style: { overflow: 'hidden' } });
    const cam = box(view, { x: 0, y: 0, w: IW, h: IH, style: { transformOrigin: '0 0' } });
    const shots = {};
    for (const id of Object.keys(SH)) {
      const h = box(cam, { x: 0, y: 0, w: IW, h: IH });
      const img = el('img', { src: `shots/${id}.png`, width: IW, height: IH, style: { display: 'block', width: `${IW}px`, height: `${IH}px` } }, h);
      // Loaded, not decoded: decode() refuses an image under a hidden layer.
      ctx.waits.push(new Promise((res, rej) => { if (img.complete && img.naturalWidth) res(); else { img.onload = res; img.onerror = () => rej(new Error(`console: shots/${id}.png did not load`)); } }));
      h.style.display = 'none';
      shots[id] = h;
    }
    // Rings, in the camera's space (shot pixels at K), with the page dimmed around them.
    const ring = box(cam, { x: 0, y: 0, w: 0, h: 0, style: { borderRadius: '10px', boxShadow: `0 0 0 2px ${C.petrolG}, 0 0 0 9999px rgba(11,15,17,0.42)` } });
    // The cursor and its click, in the window's space.
    const cursor = box(view, { x: 0, y: 0, html: '<svg width="26" height="30" viewBox="0 0 26 30"><path d="M2 2 L2 24 L8 18.5 L12.5 28 L16.5 26.2 L12 16.8 L20 16.8 Z" fill="#f4f6f6" stroke="#0b0f11" stroke-width="1.6" stroke-linejoin="round"/></svg>', style: { filter: 'drop-shadow(0 2px 3px rgba(0,0,0,0.5))' } });
    const ripple = box(view, { x: 0, y: 0, w: 44, h: 44, ax: 0.5, ay: 0.5, style: { borderRadius: '50%', border: `2px solid ${C.petrolG}` } });
    return { frame, url, view, cam, shots, ring, cursor, ripple };
  }

  /**
   * The window's timeline: steps in order, each from time `t`: the shot, the
   * camera (`z` and the `focus` box or point), a `ring` box, and a `click` on a
   * box (the cursor gets there just before `t`, and the shot changes at `t`),
   * or a `drag` from one point to another over [t, t + d].
   */
  function play(ctx, w, steps) {
    steps.sort((a, b) => a.t - b.t);
    // Fill each step from the one before it.
    let prev = { shot: steps[0].shot, z: 1, focus: [SW / 2, SHH / 2] };
    for (const s of steps) {
      s.shot = s.shot ?? prev.shot;
      s.z = s.z ?? (s.shot === prev.shot ? prev.z : 1);
      s.focus = s.focus ?? (s.shot === prev.shot ? prev.focus : [SW / 2, SHH / 2]);
      if (s.focus.length === 4) s.focus = centre(s.focus);
      s.ringKeep = s.ring === undefined && s.shot === prev.shot ? prev.ringKeep : s.ring ?? null;
      prev = s;
    }
    const viewOf = s => {
      const z = s.z, [fx, fy] = s.focus;
      let tx = IW / 2 - fx * K * z, ty = IH / 2 - fy * K * z;
      tx = Math.min(0, Math.max(IW - IW * z, tx));
      ty = Math.min(0, Math.max(IH - IH * z, ty));
      return { z, tx, ty };
    };
    steps.forEach(s => { s.view = viewOf(s); });
    const at = t => {
      let i = 0;
      while (i + 1 < steps.length && steps[i + 1].t <= t) i++;
      return i;
    };
    // The cursor's path: to each click's box, and along each drag.
    const moves = [];
    steps.forEach((s, i) => {
      const before = steps[Math.max(0, i - 1)];
      if (s.click) moves.push({ t0: s.t - 1.0, t1: s.t - 0.2, shot: before.shot, to: centre(s.click), click: s.t - 0.15 });
      if (s.drag) moves.push({ t0: s.t - 0.8, t1: s.t, shot: s.shot, to: s.drag[0] }, { t0: s.t, t1: s.t + s.drag[2], shot: s.shot, to: s.drag[1], hold: true });
    });
    let last = [SW * 0.62, SHH * 0.78];
    moves.forEach(m => { m.from = last; last = m.to; });
    return t => {
      const i = at(t), s = steps[i], p = steps[Math.max(0, i - 1)];
      // The camera eases from the last step's view to this one's.
      const u = i > 0 ? ramp(t, s.t, s.t + 0.8, E.io3) : 1;
      const v0 = s.shot === p.shot ? p.view : s.view;
      const z = lerp(v0.z, s.view.z, u), tx = lerp(v0.tx, s.view.tx, u), ty = lerp(v0.ty, s.view.ty, u);
      w.cam.style.transform = `translate(${tx.toFixed(2)}px, ${ty.toFixed(2)}px) scale(${z.toFixed(4)})`;
      // The shot, crossfading from the last one.
      const fadeIn = s.shot === p.shot ? 1 : ramp(t, s.t, s.t + 0.35, E.io2);
      for (const [id, h] of Object.entries(w.shots)) {
        const o = id === s.shot ? 1 : id === p.shot && fadeIn < 1 ? 1 : 0;
        h.style.display = o ? '' : 'none';
        h.style.zIndex = id === s.shot ? 2 : 1;
        h.style.opacity = id === s.shot ? fadeIn.toFixed(3) : '1';
      }
      w.url.textContent = SH[s.shot].url;
      // The ring.
      const r = s.ringKeep;
      if (r) {
        const ru = ramp(t, s.ringAt ?? s.t + 0.5, (s.ringAt ?? s.t + 0.5) + 0.45, E.out3);
        Object.assign(w.ring.style, { left: `${r[0] * K}px`, top: `${r[1] * K}px`, width: `${r[2] * K}px`, height: `${r[3] * K}px` });
        w.ring.style.opacity = (s.ring ? ru : 1).toFixed(3);
      } else w.ring.style.opacity = '0';
      w.ring.style.zIndex = 3;
      // The cursor: on screen only around a click or a drag.
      let on = 0, pos = last, rip = 0;
      for (const m of moves) {
        if (t >= m.t0 - 0.4 && t <= (m.click ?? m.t1) + 0.6) {
          on = ramp(t, m.t0 - 0.4, m.t0, E.io2) * (1 - ramp(t, (m.click ?? m.t1) + 0.3, (m.click ?? m.t1) + 0.6, E.io2));
          const k = ramp(t, m.t0, m.t1, m.hold ? E.io2 : E.io3);
          pos = [lerp(m.from[0], m.to[0], k), lerp(m.from[1], m.to[1], k)];
          if (m.click) rip = t >= m.click ? 1 - ramp(t, m.click, m.click + 0.45) : 0;
          if (m.hold || (m.click && t > m.t1)) on = Math.max(on, m.hold ? 1 : on);
          break;
        }
      }
      const [cx, cy] = [pos[0] * K * z + tx, pos[1] * K * z + ty];
      Object.assign(w.cursor.style, { left: `${cx.toFixed(1)}px`, top: `${cy.toFixed(1)}px`, opacity: on.toFixed(3), zIndex: 5 });
      Object.assign(w.ripple.style, { left: `${cx.toFixed(1)}px`, top: `${cy.toFixed(1)}px`, opacity: (rip * on).toFixed(3), transform: `scale(${(1.6 - rip).toFixed(3)})`, zIndex: 4 });
    };
  }

  // ================================================================ title
  function sceneTitle(ctx) {
    const b = ctx.B.title;
    ctx.scene({ key: 'title', t0: b.t0, t1: b.t1, fin: 0.2, build: layer => {
      const T = b.t0 + 0.3;
      const g = box(layer, { x: 960, y: 380, ax: 0.5, ay: 0.5, html: `<div style="display:flex;align-items:center;gap:26px">${markSvg(84)}${wordmarkSvg(54)}<span class="chip" style="font-size:26px;margin-left:6px">console</span></div>` });
      const sub = box(layer, { x: 960, y: 500, ax: 0.5, ay: 0.5, cls: 'label', text: 'servers · sessions · flows · jobs' });
      const bars = g.querySelectorAll('rect');
      ctx.music.open = [0, 0.35, 0.65].map(e => +(T + e).toFixed(3));
      return t => {
        bars.forEach((r, i) => {
          const u = ramp(t, T + [0, 0.35, 0.65][i], T + [0, 0.35, 0.65][i] + 0.7, E.out5);
          r.setAttribute('transform', `translate(${r.getAttribute('x')} 0) scale(${u.toFixed(4)} 1) translate(${-r.getAttribute('x')} 0)`);
        });
        show(g, t, T, { d: 0.3, dy: 0 });
        show(sub, t, T + 1.4, { d: 0.7, dy: 6 });
      };
    } });
  }

  // ============================================================== the tour
  function sceneTour(ctx) {
    const at = ctx.at, B = ctx.B;
    const t0 = B.overview.t0, t1 = B.more.t1;
    ctx.scene({ key: 'tour', t0, t1, build: layer => {
      const w = browser(ctx, layer);
      const knob = v => { const s = SH.flow.boxes.slider; return [s[0] + v * s[2], s[1] + s[3] / 2]; };
      const steps = [
        // The overview, and its domains.
        { t: t0, shot: 'overview', z: 1 },
        { t: at('ov1', 'counts'), z: 1.22, focus: bx('overview', 'totals'), ring: bx('overview', 'totals') },
        { t: at('ov2', 'Below'), shot: 'domains', z: 1.35, focus: bx('domains', 'domains'), ring: bx('domains', 'domains') },
        // Servers: the registry, a server, its connection test.
        { t: at('sv1', 'Servers'), shot: 'servers', z: 1.15, focus: bx('servers', 'shop'), ring: bx('servers', 'shop'), ringAt: at('sv1', 'fronts') },
        { t: at('sv2', 'Each'), shot: 'server', z: 1.45, focus: bx('server', 'snippet'), ring: bx('server', 'snippet'), ringAt: at('sv2', 'paste') },
        { t: at('sv3', 'Test') - 0.2, z: 1.1, focus: bx('server', 'probe'), ring: null },
        { t: at('sv3', 'connection', 'end') + 0.2, shot: 'probe', click: bx('server', 'probe'), z: 1.2, focus: bx('probe', 'tools'), ring: bx('probe', 'tools'), ringAt: at('sv3', 'tools') },
        // Sessions: the list, a served one, its decisions, a shadow one.
        { t: at('se1', 'Sessions'), shot: 'sessions', z: 1, ring: bx('sessions', 'modes'), ringAt: at('se1', 'served') },
        { t: at('se2', 'served one', 'end'), shot: 'session', click: bx('sessions', 'served'), z: 1 },
        { t: at('se2', 'read ahead'), z: 1.32, focus: bx('session', 'lookups', { h: 205 }), ring: bx('session', 'lookups', { h: 205 }) },
        { t: at('se3', 'chance'), z: 1.8, focus: bx('session', 'score'), ring: bx('session', 'score', { pad: 6 }) },
        { t: at('se4', 'Decisions') - 0.3, z: 1, focus: [SW / 2, SHH / 2], ring: null },
        { t: at('se4', 'lists') + 0.5, shot: 'decisions', click: bx('session', 'decisionsTab'), z: 1.3, focus: bx('decisions', 'table') },
        { t: at('se4', 'handed back'), ring: bx('decisions', 'handback', { pad: 6 }) },
        { t: at('se5', 'In shadow'), shot: 'shadow', z: 1.32, focus: bx('shadow', 'would'), ring: bx('shadow', 'would'), ringAt: at('se5', 'would have read') },
        // Flows: the graph, the threshold moved, the review and the comparison.
        { t: at('fl1', 'flow'), shot: 'flow', z: 1.15, focus: bx('flow', 'graph'), ring: bx('flow', 'graph'), ringAt: at('fl1', 'graph') },
        { t: at('fl2', 'Move'), z: 1.12, focus: [SW / 2, 560], ring: null, drag: [knob(0.3), knob(0.5), 0.9] },
        { t: at('fl2', 'Move') + 0.95, shot: 'flow50', z: 1.12, focus: [SW / 2, 560] },
        { t: at('fl2', 'hands back'), ring: bx('flow50', 'acting', { pad: 4 }) },
        { t: at('fl3', 'Review') + 0.3, shot: 'review', click: bx('flow', 'reviewTab'), z: 1, ring: bx('review', 'review', { pad: 6 }) },
        { t: at('fl3', 'Compare') + 0.3, shot: 'compare', click: bx('flow', 'compareTab'), z: 1.25, focus: bx('compare', 'verdict'), ring: bx('compare', 'verdict', { pad: 8 }) },
        // Jobs: an audit started from the page, and its report.
        { t: at('jb1', 'Jobs'), shot: 'jobnew', z: 1.12, focus: bx('jobnew', 'kinds'), ring: bx('jobnew', 'kinds'), ringAt: at('jb1', 'learn') },
        { t: at('jb2', 'audit'), z: 1.25, focus: bx('jobnew', 'command'), ring: bx('jobnew', 'command', { pad: 10 }) },
        { t: at('jb2', 'report') - 0.2, shot: 'job', click: bx('jobnew', 'start'), z: 1.15, focus: bx('job', 'report'), ring: bx('job', 'report', { pad: 10 }) },
        // Everywhere: the palette, and read-only.
        { t: at('mo1', 'jumps'), shot: 'palette', z: 1.1, focus: bx('palette', 'palette'), ring: bx('palette', 'palette') },
        { t: at('mo2', 'read-only'), shot: 'readonly', z: 1.7, focus: [1080, 160], ring: bx('readonly', 'badge', { pad: 6 }) },
      ];
      const step = play(ctx, w, steps);
      // Ctrl K, shown as the keys pressed, before the palette opens.
      const keys = box(layer, { x: 960, y: 520, ax: 0.5, ay: 0.5, html: '<span class="chip solid" style="font-size:40px;padding:16px 26px">Ctrl</span>&nbsp;&nbsp;<span class="chip solid" style="font-size:40px;padding:16px 30px">K</span>', style: { zIndex: 9, whiteSpace: 'nowrap' } });
      const tK = at('mo1', 'Control');
      ctx.poster = at('se2', 'read ahead') + 1.4;
      return t => {
        show(w.frame, t, t0 - 0.2, { d: 0.6, dy: 14 });
        step(t);
        show(keys, t, tK - 0.1, { d: 0.25, dy: 0, scale: 0.08, out: at('mo1', 'jumps') - 0.05, od: 0.2 });
      };
    } });
  }

  // ================================================================== end
  function sceneEnd(ctx) {
    const b = ctx.B.end;
    ctx.scene({ key: 'end', t0: b.t0, t1: b.t1, fout: 0, build: layer => {
      const g = box(layer, { x: 960, y: 400, ax: 0.5, ay: 0.5, html: `<div style="display:flex;align-items:center;gap:30px">${markSvg(132)}${wordmarkSvg(82)}</div>` });
      const cmds = box(layer, { x: 960, y: 560, ax: 0.5, ay: 0.5, cls: 'code', html: `<span style="color:${C.petrol}">$</span> make console&nbsp;&nbsp;&nbsp;<span style="color:${C.faint}">·</span>&nbsp;&nbsp;&nbsp;<span style="color:${C.petrol}">$</span> docker compose up -d --build`, style: { fontSize: '26px', color: C.text, whiteSpace: 'nowrap' } });
      const link = box(layer, { x: 960, y: 630, ax: 0.5, ay: 0.5, cls: 'code', html: `<span style="color:${C.petrol}">stretto.alexnodeland.com/guide/console</span>`, style: { fontSize: '22px', whiteSpace: 'nowrap' } });
      const L = ctx.L.end1;
      ctx.music.end = +(L.t1 + 0.4).toFixed(3);
      return t => {
        show(g, t, b.t0 + 0.1, { d: 0.8, dy: 0, scale: 0.04 });
        show(cmds, t, ctx.at('end1', 'make console'), { d: 0.5, dy: 6 });
        show(link, t, L.t1 + 0.3, { d: 0.6, dy: 6 });
      };
    } });
  }

  function video(ctx) {
    sceneTitle(ctx);
    sceneTour(ctx);
    sceneEnd(ctx);
  }

  window.Film = { video, titles: TITLES };
})();
