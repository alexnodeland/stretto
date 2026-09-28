// The explainer's clock. Every property of every frame is a function of t:
// no CSS transitions, no timers, randomness only from seeded generators.
//
//   window.__cut        the cut this page renders (?cut=video|teaser)
//   window.__duration   its length in seconds
//   window.__marks      when each scene starts
//   window.__lines      when each line of the voice-over starts, and its length
//   window.__events     the sound effects: [{t, sfx, gain}]
//   window.__music      the music bed's cues: {open: [a, b, c], end}
//   window.__voice      (set by render.mjs) each line's length and word timings
//   window.__script     (set by render.mjs) narration.json's lines
//   window.__render(t)  draw the frame at t seconds
//   window.__ready      resolves once the fonts have loaded and the world is built
(() => {
  const params = new URLSearchParams(location.search);
  const CUT = params.get('cut') || 'video';
  const FPS = 30;

  // ------------------------------------------------------------ the script
  // render.mjs passes the script with the voice (window.__voice.script).
  let script = window.__script || (window.__voice && window.__voice.script);
  if (!script) {
    // Served over http, the page can read the script itself.
    try {
      const x = new XMLHttpRequest();
      x.open('GET', 'narration.json', false);
      x.send();
      script = JSON.parse(x.responseText);
    } catch (e) { throw new Error('explainer: no script (render.mjs injects window.__script)'); }
  }
  const VOICE = (window.__voice && window.__voice.lines) || {};
  const RATE = 2.5; // words per second, for lines not voiced yet

  // Each line's words with their times from the line's start: the voice's
  // own timings where the manifest has them, else an estimate at RATE.
  const norm = w => w.toLowerCase().normalize('NFKD').replace(/[^a-z0-9]/g, '');
  function wordsOf(line) {
    const v = VOICE[line.key];
    const said = line.say || line.text;
    if (v && Array.isArray(v.words) && v.words.length) {
      return { words: v.words.map(w => ({ w: w.w ?? w.word, start: +w.start, end: +w.end })), duration: +v.duration, voiced: true };
    }
    const raw = said.split(/\s+/).filter(Boolean);
    const weight = raw.map(w => 0.7 + norm(w).length * 0.075 + (/[.!?]$/.test(w) ? 0.55 : /[,;:]$/.test(w) ? 0.28 : 0));
    const total = weight.reduce((a, b) => a + b, 0);
    const duration = v && v.duration ? +v.duration : raw.length / RATE;
    let acc = 0;
    const words = raw.map((w, i) => {
      const start = (acc / total) * duration;
      acc += weight[i];
      return { w, start, end: (acc / total) * duration };
    });
    return { words, duration, voiced: !!(v && v.duration) };
  }

  // ------------------------------------------------------------- the scenes
  // lead: when the line starts after the scene does; min: the scene's least
  // length; breath: the pause held after the line.
  const SPEC = {
    open: { lead: 1.75, min: 5.6, breath: 0.9 },
    turns: { lead: 1.5, min: 17, breath: 0.9 },
    decided: { lead: 0.9, min: 14, breath: 1.0 },
    idea: { lead: 1.5, min: 18, breath: 1.0 },
    safe: { lead: 1.0, min: 10, breath: 1.0 },
    learn: { lead: 1.1, min: 17, breath: 1.2 },
    rule: { lead: 1.0, min: 13, breath: 1.2 },
    results: { lead: 0.9, min: 13, breath: 1.4 },
    prompt: { lead: 1.2, min: 9, breath: 1.3 },
    start: { lead: 1.2, min: 10, breath: 1.0 },
    end: { lead: 1.1, min: 5.5, breath: 3.4 },
  };
  const LINES = {};
  for (const l of script.lines) LINES[l.key] = { ...l, ...wordsOf(l) };

  const S = {};
  let t = 0;
  for (const l of script.lines) {
    const sp = SPEC[l.key] || { lead: 1, min: 6, breath: 1 };
    const L = LINES[l.key];
    const dur = Math.max(sp.min, sp.lead + L.duration + sp.breath);
    S[l.key] = { key: l.key, t0: t, t1: t + dur, v0: t + sp.lead, vd: L.duration, vend: t + sp.lead + L.duration, line: L };
    t += dur;
  }
  const TOTAL = t;

  // The time a phrase starts in a line (absolute), searching from `after`.
  function wordTime(key, phrase, after = -Infinity, which = 'start') {
    const sc = S[key], L = sc.line;
    let joined = '';
    const at = [];
    L.words.forEach((w, i) => { const n = norm(w.w); for (const _ of n) at.push(i); joined += n; });
    const q = norm(phrase);
    let from = 0;
    for (;;) {
      const k = joined.indexOf(q, from);
      if (k < 0) break;
      const i = at[k], j = at[k + q.length - 1];
      const tt = sc.v0 + (which === 'end' ? L.words[j].end : L.words[i].start);
      if (tt >= after - 1e-6) return tt;
      from = k + 1;
    }
    // Not found: place it by where the phrase sits in the written text.
    if (L.voiced) (window.__missing || (window.__missing = [])).push(`${key}: "${phrase}"`);
    const text = norm(L.say || L.text), k = text.indexOf(q);
    const frac = k < 0 ? 0.5 : (k + (which === 'end' ? q.length : 0)) / Math.max(1, text.length);
    return Math.max(after, sc.v0 + frac * L.duration);
  }

  // ------------------------------------------------------------- the camera
  // Segments in order, each from where the last left the camera. A segment's
  // target is a view, or a function of t (to follow something).
  const V0 = { x: 0, y: 0, z: 1, r: 0, tilt: 0 };
  const cam = {
    segs: [],
    start: { ...V0 },
    cut(t0, view) { this.segs.push({ t0, t1: t0, to: { ...V0, ...view }, mode: 'cut' }); return this; },
    go(t0, d, view, mode = 'smooth', ease = 'io3', opts = {}) { this.segs.push({ t0, t1: t0 + d, to: typeof view === 'function' ? view : { ...V0, ...view }, mode, ease, opts }); return this; },
    // Each segment starts from wherever the camera is when it begins, so a
    // move can take over from one still under way.
    resolve() {
      this.segs.sort((a, b) => a.t0 - b.t0);
      this.segs.forEach((sg, i) => {
        sg.from = this.upTo(i, sg.t0);
        if (sg.mode === 'zoom') sg.path = E.zoomPath(sg.from, sg.to, sg.opts.rho || 1.2);
        if (sg.mode === 'dive') sg.path = E.divePath(sg.from, sg.to, sg.to.z);
      });
    },
    upTo(n, t) {
      let v = { ...this.start };
      for (let i = 0; i < n; i++) {
        const sg = this.segs[i];
        if (t < sg.t0) break;
        v = this.seg(sg, t);
      }
      return v;
    },
    seg(sg, t) {
      const to = typeof sg.to === 'function' ? { ...V0, ...sg.to(t) } : sg.to;
      const p = sg.t1 > sg.t0 ? E.clamp((t - sg.t0) / (sg.t1 - sg.t0)) : 1;
      if (sg.mode === 'cut' || p >= 1) return { ...to };
      const f = sg.from, e = E.easeOf(sg.ease)(p);
      const ez = sg.opts.ez ? E.easeOf(sg.opts.ez)(p) : e;
      const et = sg.opts.et ? E.easeOf(sg.opts.et)(p) : e;
      if (sg.mode === 'zoom' || sg.mode === 'dive') {
        const q = sg.path(e, ez);
        return { x: q.x, y: q.y, z: q.z, r: E.lerp(f.r, to.r, e), tilt: E.lerp(f.tilt, to.tilt, et) };
      }
      return {
        x: E.lerp(f.x, to.x, e), y: E.lerp(f.y, to.y, e),
        z: Math.exp(E.lerp(Math.log(f.z), Math.log(to.z), ez)),
        r: E.lerp(f.r, to.r, e), tilt: E.lerp(f.tilt, to.tilt, et),
      };
    },
    at(t) { return this.upTo(this.segs.length, t); },
  };
  // A gentle drift, in screen pixels, so no frame is ever dead still.
  let driftScale = t => 1;
  const drift = (v, t) => {
    const k = driftScale(t);
    return {
      ...v,
      x: v.x + (9 * k * E.noise(t * 0.21, 1)) / v.z,
      y: v.y + (6 * k * E.noise(t * 0.18, 2)) / v.z,
      z: v.z * (1 + 0.006 * k * E.noise(t * 0.12, 3)),
      r: v.r + 0.14 * k * E.noise(t * 0.15, 4),
    };
  };

  // --------------------------------------------------------- the renderer
  const world = document.getElementById('world');
  const gridL = document.getElementById('gridL');
  const scene = document.getElementById('scene');
  const blurG = document.getElementById('mblurG');
  const PERSP = 1500;
  const camCss = (v, f = 1) => {
    const z = Math.pow(v.z, f);
    let tf = 'translate(960px, 540px)';
    if (Math.abs(v.tilt) > 0.01) tf += ` perspective(${PERSP}px) rotateX(${v.tilt.toFixed(3)}deg)`;
    if (v.r) tf += ` rotate(${v.r.toFixed(4)}deg)`;
    tf += ` scale(${z.toFixed(5)}) translate(${(-v.x * f).toFixed(2)}px, ${(-v.y * f).toFixed(2)}px)`;
    return tf;
  };
  // World point to screen point, for a flat camera.
  const toScreen = (v, x, y) => {
    const r = (v.r * Math.PI) / 180, c = Math.cos(r), s = Math.sin(r);
    const dx = (x - v.x) * v.z, dy = (y - v.y) * v.z;
    return [960 + dx * c - dy * s, 540 + dx * s + dy * c];
  };

  // Dust: specks at several depths, from a seeded generator.
  const dustBack = document.getElementById('dustBack').getContext('2d');
  const dustFront = document.getElementById('dustFront').getContext('2d');
  const rnd = E.rng(20260927);
  const TW = 2600, TH = 1600;
  const specks = (n, f, r0, r1, a0, a1) => Array.from({ length: n }, () => ({
    x: rnd() * TW, y: rnd() * TH, r: rnd.range(r0, r1), a: rnd.range(a0, a1), f,
    vx: rnd.range(-6, 6), vy: rnd.range(-4, 3), ph: rnd() * 6.28,
  }));
  const BACK = [...specks(90, 0.3, 0.6, 1.4, 0.08, 0.26), ...specks(40, 0.55, 0.9, 1.8, 0.1, 0.3)];
  const FRONT = [...specks(22, 1.35, 1.2, 2.4, 0.12, 0.3), ...specks(9, 1.8, 10, 26, 0.025, 0.05)];
  let dustOn = t => 1;
  function drawDust(ctx, list, v, t, k) {
    ctx.clearRect(0, 0, 1920, 1080);
    if (k <= 0.001) return;
    for (const p of list) {
      const zf = Math.pow(v.z, p.f * 0.5);
      let x = (p.x + p.vx * t - v.x * p.f * 0.35) * zf, y = (p.y + p.vy * t - v.y * p.f * 0.35) * zf;
      x = ((x % TW) + TW) % TW - (TW - 1920) / 2;
      y = ((y % TH) + TH) % TH - (TH - 1080) / 2;
      const tw = 0.75 + 0.25 * Math.sin(t * 0.9 + p.ph);
      const a = p.a * tw * k;
      if (p.r > 6) {
        const g = ctx.createRadialGradient(x, y, 0, x, y, p.r);
        g.addColorStop(0, `rgba(140,215,220,${a})`);
        g.addColorStop(1, 'rgba(140,215,220,0)');
        ctx.fillStyle = g;
      } else ctx.fillStyle = `rgba(190,225,228,${a})`;
      ctx.beginPath();
      ctx.arc(x, y, p.r, 0, Math.PI * 2);
      ctx.fill();
    }
  }

  // ----------------------------------------------------------------- build
  const fns = [];
  const events = [];
  const afters = [];
  const ctx = {
    CUT, FPS, S, TOTAL, LINES, cam, fns, events,
    word: wordTime,
    fn: f => fns.push(f),
    after: f => afters.push(f),
    ev: (t, sfx, gain = 0.5) => { if (t >= 0) events.push({ t: +t.toFixed(3), sfx, gain: +gain.toFixed(3) }); },
    toScreen,
    setDrift: f => { driftScale = f; },
    setDust: f => { dustOn = f; },
    music: { open: [0.5, 0.85, 1.15], end: TOTAL - 4 },
  };

  let duration = TOTAL;
  const build = () => {
    const built = CUT === 'teaser' ? Scenes.teaser(ctx) : Scenes.video(ctx);
    if (built && built.duration) duration = built.duration;
    cam.resolve();
    for (const f of afters) f();
    events.sort((a, b) => a.t - b.t);
  };

  let lastBlur = '';
  function render(t) {
    const base = cam.at(t);
    const v = drift(base, t);
    ctx.view = v;
    E.css(world, 'transform', camCss(v));
    const gf = E.lerp(0.62, 1, E.clamp(v.tilt / 20));
    E.css(gridL, 'transform', camCss(v, gf));
    // Motion blur along the camera's motion, when it moves fast.
    const b0 = cam.at(t - 1 / FPS);
    const vx = Math.abs(base.x - b0.x) * base.z, vy = Math.abs(base.y - b0.y) * base.z;
    const bx = Math.min(46, Math.max(0, vx - 26) * 0.34), by = Math.min(46, Math.max(0, vy - 26) * 0.34);
    const blur = bx > 0.3 || by > 0.3 ? `${bx.toFixed(1)} ${by.toFixed(1)}` : '';
    if (blur !== lastBlur) {
      lastBlur = blur;
      if (blur) { blurG.setAttribute('stdDeviation', blur); scene.style.filter = 'url(#mblur)'; }
      else scene.style.filter = 'none';
    }
    for (const n of W.nodes) { n.reset(); n.eval(t); }
    for (const f of fns) f(t, v);
    for (const n of W.nodes) n.apply(t);
    const d = dustOn(t);
    drawDust(dustBack, BACK, v, t, d);
    drawDust(dustFront, FRONT, v, t, d);
  }

  window.__cut = CUT;
  window.__ready = Promise.all([
    document.fonts.load('400 20px "JetBrains Mono"'),
    document.fonts.load('700 20px "JetBrains Mono"'),
    document.fonts.load('400 20px Inter'),
    document.fonts.load('640 20px Inter'),
  ]).then(() => document.fonts.ready).then(() => {
    build();
    window.__duration = duration;
    window.__marks = Object.fromEntries(Object.values(S).map(s => [s.key, +s.t0.toFixed(3)]));
    window.__lines = Object.fromEntries(Object.values(S).map(s => [s.key, { t: +s.v0.toFixed(3), duration: s.vd, voiced: s.line.voiced }]));
    window.__events = events;
    window.__music = ctx.music;
    window.__poster = ctx.poster;
    window.__render = render;
    window.__word = wordTime;
    render(params.has('t') ? +params.get('t') : 0);
    if (params.has('play')) {
      const start = performance.now();
      const loop = now => { render(((now - start) / 1000) % duration); requestAnimationFrame(loop); };
      requestAnimationFrame(loop);
    }
  });
})();
