// The explainer's clock. Every property of every frame is a function of t:
// no CSS transitions, no timers, randomness only from seeded generators.
//
//   window.__cut        the cut this page renders (?cut=video|teaser)
//   window.__duration   its length in seconds
//   window.__marks      when each chapter starts
//   window.__lines      when each line of the voice-over starts, and its length
//   window.__events     sound effects: none (PACING.md: the voice, the bed and two marks)
//   window.__music      the music bed's cues: {open: [a, b, c], end}
//   window.__poster     the poster's time
//   window.__voice      (set by render.mjs) each line's length and word timings, and the script
//   window.__render(t)  draw the frame at t seconds
//   window.__captions(on) show or hide the captions (off for the poster)
//   window.__ready      resolves once the fonts have loaded and the film is built
//
// The timeline is laid out from the voice: each chapter waits `lead`, then
// holds each of its lines for as long as the voice takes to say it, plus its
// `post` pause, then `tail`; a chapter is at least `min` long. A line not
// voiced yet is estimated at RATE words a second, so the film previews
// without a voice.
(() => {
  const { el, box, fade, ramp, E, caption, lightCaption } = window.ST;
  const params = new URLSearchParams(location.search);
  const CUT = params.get('cut') || 'video';
  const FPS = 30;
  const RATE = 2.6;

  let script = window.__voice && window.__voice.script;
  if (!script) {
    // Served over http, the page reads the script itself.
    const x = new XMLHttpRequest();
    x.open('GET', 'narration.json', false);
    x.send();
    script = JSON.parse(x.responseText);
  }
  const VOICE = (window.__voice && window.__voice.lines) || {};
  const norm = w => w.toLowerCase().normalize('NFKD').replace(/[^a-z0-9]/g, '');

  // A line's spoken words with their times from its start: the voice's own,
  // where the manifest has them, else an estimate.
  function spokenWords(line) {
    const said = line.say || line.text;
    const v = VOICE[line.key];
    if (v && Array.isArray(v.words) && v.words.length) return { words: v.words.map(w => ({ w: w.w, start: +w.start, end: +w.end })), duration: +v.duration, voiced: true };
    const raw = said.split(/\s+/).filter(Boolean);
    const weight = raw.map(w => 0.7 + norm(w).length * 0.075 + (/[.!?]$/.test(w) ? 0.55 : /[,;:]$/.test(w) ? 0.28 : 0));
    const total = weight.reduce((a, b) => a + b, 0);
    const duration = v && v.duration ? +v.duration : raw.length / RATE;
    let acc = 0;
    return {
      words: raw.map((w, i) => { const start = (acc / total) * duration; acc += weight[i]; return { w, start, end: (acc / total) * duration }; }),
      duration, voiced: !!(v && v.duration),
    };
  }

  // ---------------------------------------------------------------- layout
  const B = {}, L = {}, ORDER = [];
  let t = 0;
  for (const beat of script.beats) {
    const b = { key: beat.key, t0: t, lines: [] };
    t += beat.lines.length ? beat.lead ?? 0.7 : 0;
    for (const line of beat.lines) {
      const sw = spokenWords(line);
      const l = { ...line, beat: beat.key, t0: t, t1: t + sw.duration, duration: sw.duration, words: sw.words, voiced: sw.voiced };
      // The written words' times, for the captions: the spoken words where
      // the two match one for one, else each written word placed where its
      // share of the line falls among the spoken ones.
      const written = line.text.split(/\s+/).filter(Boolean);
      l.textTimes = written.length === sw.words.length
        ? sw.words.map(w => t + w.start)
        : written.map((_, i) => t + sw.words[Math.min(sw.words.length - 1, Math.floor((i / written.length) * sw.words.length))].start);
      L[line.key] = l;
      b.lines.push(l);
      t = l.t1 + (line.post ?? 0.45);
    }
    if (beat.lines.length) t += beat.tail ?? 0.7;
    b.t1 = Math.max(t, b.t0 + (beat.min || 0));
    t = b.t1;
    B[beat.key] = b;
    ORDER.push(b);
  }
  const TOTAL = t;

  /** The time (absolute) a phrase starts (or ends) in a line, by the voice's words. */
  function at(key, phrase, which = 'start') {
    const l = L[key];
    if (!l) throw new Error(`explainer: no line ${key}`);
    let joined = '';
    const idx = [];
    l.words.forEach((w, i) => { for (const _ of norm(w.w)) idx.push(i); joined += norm(w.w); });
    const q = norm(phrase);
    const k = joined.indexOf(q);
    if (k >= 0) {
      const i = idx[k], j = idx[k + q.length - 1];
      return l.t0 + (which === 'end' ? l.words[j].end : l.words[i].start);
    }
    // Not in the spoken form (a number written as digits): place it by where
    // it sits in the written text.
    if (l.voiced) (window.__missing || (window.__missing = [])).push(`${key}: "${phrase}"`);
    const text = norm(l.text), m = text.indexOf(q);
    const frac = m < 0 ? 0.5 : (m + (which === 'end' ? q.length : 0)) / Math.max(1, text.length);
    return l.t0 + frac * l.duration;
  }

  // ---------------------------------------------------------------- scenes
  const stage = document.getElementById('stage');
  const scenesRoot = document.getElementById('scenes');
  const over = document.getElementById('over');
  const scenes = [];
  const ctx = {
    CUT, FPS, B, L, ORDER, TOTAL, at,
    /** A chapter's layer: shown over [t0 - fin, t1 + fout], faded in and out. */
    scene({ key, t0, t1, fin = 0.45, fout = 0.45, build }) {
      const layer = el('div', { class: 'layer', 'data-scene': key }, scenesRoot);
      const update = build(layer) || (() => {});
      layer.style.display = 'none';
      scenes.push({ key, t0, t1, fin, fout, layer, update });
    },
    music: { open: [0.4, 0.75, 1.05], end: TOTAL - 4 },
    poster: null,
    waits: [],
  };

  // The frame's furniture: the chapter's name, top left, and the mark, top right.
  const chapters = [];
  const captions = [];
  function furniture(titles) {
    const mark = box(over, { x: 1800, y: 74, ax: 1, ay: 0.5, html: window.ST.markSvg(30), style: { height: '30px' } });
    const numbered = ORDER.filter(b => titles[b.key]);
    numbered.forEach((b, i) => {
      const n = box(over, { x: 120, y: 74, ay: 0.5, cls: 'chapter', html: `<b>${String(i + 1).padStart(2, '0')}</b><i></i>${titles[b.key]}` });
      chapters.push({ n, b });
    });
    const shown = ORDER.filter(b => titles[b.key]);
    const first = shown.length ? shown[0].t0 : Infinity;
    const last = shown.length ? shown[shown.length - 1].t1 : -Infinity;
    chapters.mark = { n: mark, first, last };
    // Each caption holds a little into its pause, and is gone before the next
    // one comes in: two lines never share the frame.
    const all = Object.values(L);
    all.forEach((l, i) => {
      const next = all[i + 1];
      const out = Math.min(l.t1 + Math.min(0.6, (l.post ?? 0.45) + 0.1), next ? next.t0 - 0.32 : Infinity);
      captions.push({ cap: caption(over, l.text, l.em || []), l, out });
    });
  }

  function render(t) {
    for (const s of scenes) {
      const on = t >= s.t0 - s.fin && t < s.t1 + s.fout;
      s.layer.style.display = on ? '' : 'none';
      if (!on) continue;
      s.layer.style.opacity = (ramp(t, s.t0 - s.fin, s.t0, E.io2) * (1 - ramp(t, s.t1, s.t1 + s.fout, E.io2))).toFixed(4);
      s.update(t);
    }
    if (CUT !== 'video') return;
    for (const { n, b } of chapters) {
      const o = fade(t, b.t0 - 0.1, b.t0 + 0.5, b.t1 - 0.2, b.t1 + 0.3);
      n.style.opacity = o.toFixed(3);
      n.style.visibility = o < 0.002 ? 'hidden' : '';
    }
    const m = chapters.mark;
    if (m) m.n.style.opacity = (0.9 * fade(t, m.first, m.first + 0.6, m.last - 0.2, m.last + 0.4)).toFixed(3);
    for (const { cap, l, out } of captions) lightCaption(cap, t, l.textTimes, l.t0, out);
  }

  let duration = TOTAL;
  window.__cut = CUT;
  window.__ready = Promise.all([
    document.fonts.load('400 20px "JetBrains Mono"'),
    document.fonts.load('700 20px "JetBrains Mono"'),
    document.fonts.load('400 20px Inter'),
    document.fonts.load('600 20px Inter'),
  ]).then(() => document.fonts.ready).then(() => {
    window.__hasTeaser = typeof window.Film.teaser === 'function';
    const built = CUT === 'teaser' ? (window.__hasTeaser ? window.Film.teaser(ctx) : { duration: 0 }) : window.Film.video(ctx);
    if (built && built.duration) duration = built.duration;
    if (CUT === 'video') furniture(window.Film.titles);
    return Promise.all(ctx.waits);
  }).then(() => {
    window.__duration = duration;
    window.__marks = Object.fromEntries(ORDER.map(b => [b.key, +b.t0.toFixed(3)]));
    window.__lines = Object.fromEntries(Object.values(L).map(l => [l.key, { t: +l.t0.toFixed(3), duration: l.duration, voiced: l.voiced }]));
    window.__events = [];
    window.__music = ctx.music;
    window.__poster = ctx.poster;
    window.__render = render;
    // The poster is drawn without the caption: a still of a line half said reads as a glitch.
    window.__captions = on => { for (const c of captions) c.cap.n.style.display = on ? '' : 'none'; };
    window.__at = at;
    render(params.has('t') ? +params.get('t') : 0);
    if (params.has('play')) {
      const start = performance.now() - (params.has('t') ? +params.get('t') * 1000 : 0);
      const loop = now => { render(((now - start) / 1000) % duration); requestAnimationFrame(loop); };
      requestAnimationFrame(loop);
    }
  });
  void stage;
})();
