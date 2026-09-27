// The launch video's clock. Every animated property is a keyframed function of
// time, so window.__render(t) draws the same frame for the same t, every time:
// no CSS transitions, no timers, no randomness.
//
//   window.__cut          the cut this page renders (?cut=launch|teaser)
//   window.__duration     its length in seconds
//   window.__render(t)    draw the frame at t seconds
//   window.__ready        a promise that resolves once every font face and image has loaded
(() => {
  const $ = id => {
    const el = typeof id === 'string' ? document.getElementById(id) : id;
    if (!el) throw new Error(`timeline: no element #${id}`);
    return el;
  };
  const EASE = {
    lin: p => p,
    out: p => 1 - Math.pow(1 - p, 3),
    in: p => p * p * p,
    io: p => (p < 0.5 ? 4 * p * p * p : 1 - Math.pow(-2 * p + 2, 3) / 2),
  };

  class Timeline {
    constructor() { this.tracks = new Map(); this.duration = 0; }
    // key(el, prop, [t, value, ease], ...): the value at each time; between two
    // frames it moves with the second frame's ease; strings switch at their frame.
    key(id, prop, ...frames) {
      const el = $(id);
      let props = this.tracks.get(el);
      if (!props) this.tracks.set(el, (props = {}));
      const list = props[prop] || (props[prop] = []);
      list.push(...frames);
      list.sort((a, b) => a[0] - b[0]);
      return this;
    }
    render(t) {
      for (const [el, props] of this.tracks) {
        const v = {};
        for (const prop in props) v[prop] = valueAt(props[prop], t);
        apply(el, v);
      }
    }
  }

  function valueAt(frames, t) {
    if (t <= frames[0][0]) return frames[0][1];
    for (let i = 1; i < frames.length; i++) {
      const [t1, v1, ease] = frames[i];
      if (t < t1) {
        const [t0, v0] = frames[i - 1];
        if (typeof v1 !== 'number') return v0;
        const p = (t - t0) / (t1 - t0);
        return v0 + (v1 - v0) * EASE[ease || 'out'](Math.min(1, Math.max(0, p)));
      }
    }
    return frames[frames.length - 1][1];
  }

  function apply(el, v) {
    if ('opacity' in v) el.style.opacity = String(Math.round(v.opacity * 1000) / 1000);
    if ('x' in v || 'y' in v || 'sx' in v || 'scale' in v) {
      let tr = `translate(${(v.x || 0).toFixed(2)}px, ${(v.y || 0).toFixed(2)}px)`;
      if ('scale' in v) tr += ` scale(${v.scale.toFixed(4)})`;
      if ('sx' in v) tr += ` scaleX(${v.sx.toFixed(4)})`;
      el.style.transform = tr;
    }
    if ('chars' in v) el.textContent = el.dataset.full.slice(0, Math.round(v.chars));
    if ('text' in v) el.textContent = v.text;
    if ('hl' in v) el.style.setProperty('--hl', v.hl.toFixed(3));
    if ('reveal' in v) el.style.clipPath = `inset(-4px ${(100 - v.reveal * 100).toFixed(2)}% -4px -80px round 9px)`;
  }

  // Helpers. Times are seconds; d is a duration.
  const fadeIn = (tl, id, t, d = 0.6, dy = 18) => {
    tl.key(id, 'opacity', [t, 0], [t + d, 1]);
    if (dy) tl.key(id, 'y', [t, dy], [t + d, 0]);
  };
  const fadeOut = (tl, id, t, d = 0.45) => tl.key(id, 'opacity', [t, 1], [t + d, 0, 'in']);
  // A scene is visible from t0, and fades out over its last `out` seconds.
  const scene = (tl, id, t0, t1, out = 0.5) =>
    tl.key(id, 'opacity', [t0 - 0.001, 0], [t0, 1, 'lin'], [t1 - out, 1], [t1, 0, 'in']);
  const hide = (tl, ...ids) => ids.forEach(id => tl.key(id, 'opacity', [0, 0]));
  // Move an element along x (and y) between two times.
  const move = (tl, id, t, d, dx, dy = 0, ease = 'io') => {
    tl.key(id, 'x', [t, 0], [t + d, dx, ease]);
    if (dy) tl.key(id, 'y', [t, 0], [t + d, dy, ease]);
  };

  // ---------------------------------------------------------------- launch
  function launch() {
    const tl = new Timeline();
    let t = 0;

    // 0. Open: the three bars of the mark enter as a stretto, each before the
    //    last has finished, and each sooner than the one before.
    {
      const t0 = t, end = t0 + 5.6;
      scene(tl, 'open', t0, end, 0.6);
      for (const [id, s] of [['b1', 0.35], ['b2', 0.85], ['b3', 1.1]]) {
        tl.key(id, 'opacity', [t0 + s, 0], [t0 + s + 0.2, 1, 'lin']);
        tl.key(id, 'sx', [t0 + s, 0], [t0 + s + 0.9, 1]);
        tl.key(id, 'x', [t0 + s, -80], [t0 + s + 0.9, 0]);
      }
      tl.key('word', 'opacity', [t0 + 1.75, 0], [t0 + 2.45, 1]);
      tl.key('word', 'x', [t0 + 1.75, -24], [t0 + 2.45, 0]);
      fadeIn(tl, 'tagline', t0 + 2.8, 0.7, 20);
      fadeIn(tl, 'namenote', t0 + 3.7, 0.7, 12);
      t = end;
    }

    // 1–2. Problem, then the idea, on one episode's turns.
    {
      const t0 = t, end = t0 + 23.4;
      scene(tl, 'episode', t0, end, 0.6);
      fadeIn(tl, 'p-t1', t0 + 0.3, 0.7);
      fadeIn(tl, 'p-lane', t0 + 0.8, 0.5, 0);
      ['k1', 'k2', 'k3', 'k4', 'k5'].forEach((id, i) => fadeIn(tl, id, t0 + 1.1 + i * 0.5, 0.5, 24));
      fadeIn(tl, 'p-c1', t0 + 2.4, 0.7, 12);
      hide(tl, 'ride1', 'ride2', 'rail', 'wl', 'wlab');
      tl.key('r2', 'opacity', [0, 0]); tl.key('r3', 'opacity', [0, 0]);

      // The problem, sharpened: the first result named what turns 2 and 3 read.
      const a = t0 + 6.8;
      fadeOut(tl, 'p-t1', a); fadeOut(tl, 'p-c1', a);
      fadeIn(tl, 'p-t2', a + 0.4, 0.7);
      fadeIn(tl, 'p-res', a + 0.9, 0.6, 10);
      tl.key('p-res', 'opacity', [a + 0.9, 0], [a + 1.5, 1]);
      tl.key('r2', 'opacity', [a + 1.8, 0], [a + 2.3, 1]);
      tl.key('r3', 'opacity', [a + 2.05, 0], [a + 2.55, 1]);
      fadeIn(tl, 'p-c2', a + 2.2, 0.7, 12);

      // The idea: those reads ride in the first result; the turns close up.
      const b = t0 + 13.6;
      fadeOut(tl, 'p-t2', b); fadeOut(tl, 'p-c2', b); fadeOut(tl, 'p-res', b);
      fadeIn(tl, 'p-t3', b + 0.4, 0.7);
      tl.key('k2', 'opacity', [b + 1.2, 1], [b + 1.8, 0, 'in']);
      move(tl, 'k2', b + 1.2, 0.6, -150, 60, 'in');
      tl.key('k3', 'opacity', [b + 1.4, 1], [b + 2.0, 0, 'in']);
      move(tl, 'k3', b + 1.4, 0.6, -420, 110, 'in');
      tl.key('rail', 'opacity', [b + 1.6, 0], [b + 1.9, 0.7]);
      for (const [id, s] of [['ride1', 1.7], ['ride2', 1.95]]) {
        tl.key(id, 'opacity', [b + s, 0], [b + s + 0.2, 1, 'lin']);
        tl.key(id, 'reveal', [b + s, 0], [b + s + 0.7, 1]);
      }
      hide(tl, 'g1', 'g2', 'glab');
      tl.key('g1', 'opacity', [b + 3.4, 0], [b + 3.9, 1]);
      tl.key('g2', 'opacity', [b + 3.5, 0], [b + 4.0, 1]);
      tl.key('glab', 'opacity', [b + 3.7, 0], [b + 4.2, 1]);
      move(tl, 'k4', b + 2.4, 1.0, -668);
      move(tl, 'k5', b + 2.5, 1.0, -668);
      tl.key('n4', 'text', [0, 'turn 4'], [b + 3.0, 'turn 2']);
      tl.key('n5', 'text', [0, 'write · turn 5'], [b + 3.1, 'write · turn 3']);
      fadeIn(tl, 'p-c3', b + 2.2, 0.7, 12);
      tl.key('wl', 'x', [0, -668]); tl.key('wlab', 'x', [0, -668]);
      tl.key('wl', 'opacity', [b + 4.4, 0], [b + 5.0, 1]);
      tl.key('wlab', 'opacity', [b + 4.4, 0], [b + 5.0, 1]);
      t = end;
    }

    // 3a. Record.
    {
      const t0 = t, end = t0 + 7.2;
      scene(tl, 'record', t0, end);
      tl.key('record', 'y', [0, 0]);
      fadeIn(tl, document.querySelector('#record .kicker'), t0 + 0.2, 0.5, 0);
      fadeIn(tl, document.querySelector('#record .title'), t0 + 0.3, 0.6);
      fadeIn(tl, 'n-host', t0 + 0.7, 0.5, 16);
      fadeIn(tl, 'w1', t0 + 0.95, 0.4, 0);
      fadeIn(tl, 'n-proxy', t0 + 1.0, 0.5, 16);
      fadeIn(tl, 'w2', t0 + 1.25, 0.4, 0);
      fadeIn(tl, 'n-server', t0 + 1.3, 0.5, 16);
      const p = t0 + 2.0;
      for (const [id, s, dx] of [['q1', 0, 170], ['q2', 0.7, 170], ['q3', 1.5, -170], ['q4', 2.2, -170]]) {
        tl.key(id, 'opacity', [p + s, 0], [p + s + 0.1, 1, 'lin'], [p + s + 0.6, 1], [p + s + 0.7, 0, 'lin']);
        tl.key(id, 'x', [p + s, 0], [p + s + 0.7, dx, 'io']);
      }
      ['l1', 'l2', 'l3', 'l4', 'l5'].forEach((id, i) => fadeIn(tl, id, t0 + 4.3 + i * 0.16, 0.35, 10));
      fadeIn(tl, 'loglabel', t0 + 4.9, 0.5, 0);
      fadeIn(tl, 'rc', t0 + 1.8, 0.7, 12);
      t = end;
    }

    // 3b. Learn.
    {
      const t0 = t, end = t0 + 7.4;
      scene(tl, 'learn', t0, end);
      fadeIn(tl, document.querySelector('#learn .kicker'), t0 + 0.2, 0.5, 0);
      fadeIn(tl, document.querySelector('#learn .title'), t0 + 0.3, 0.6);
      fadeIn(tl, 'lc1', t0 + 0.7, 0.6, 16);
      tl.key('h1', 'hl', [t0 + 1.8, 0], [t0 + 2.3, 1]);
      tl.key('h2', 'hl', [t0 + 2.2, 0], [t0 + 2.7, 1]);
      fadeIn(tl, 'larrow', t0 + 2.9, 0.4, 0);
      tl.key('larrow', 'x', [t0 + 2.9, -16], [t0 + 3.3, 0]);
      fadeIn(tl, 'lc2', t0 + 3.2, 0.6, 16);
      fadeIn(tl, 'lcap', t0 + 1.4, 0.7, 12);
      t = end;
    }

    // 3c. Serve.
    {
      const t0 = t, end = t0 + 7.6;
      scene(tl, 'serve', t0, end);
      fadeIn(tl, document.querySelector('#serve .kicker'), t0 + 0.2, 0.5, 0);
      fadeIn(tl, document.querySelector('#serve .title'), t0 + 0.3, 0.6);
      fadeIn(tl, 'sc', t0 + 0.7, 0.6, 16);
      fadeIn(tl, 'sl1', t0 + 1.3, 0.4, 0);
      fadeIn(tl, 'sl3', t0 + 2.2, 0.4, 0);
      fadeIn(tl, 'sl4', t0 + 2.8, 0.4, 0);
      fadeIn(tl, 'sl2', t0 + 3.4, 0.5, 0);
      fadeIn(tl, 'scap', t0 + 1.8, 0.7, 12);
      t = end;
    }

    // 3d. The rule.
    {
      const t0 = t, end = t0 + 8.4;
      scene(tl, 'rule', t0, end);
      fadeIn(tl, document.querySelector('#rule .kicker'), t0 + 0.2, 0.5, 0);
      fadeIn(tl, document.querySelector('#rule .title'), t0 + 0.3, 0.6);
      fadeIn(tl, 'fm', t0 + 0.9, 0.7, 16);
      fadeIn(tl, 'fdefs', t0 + 2.0, 0.7, 12);
      fadeIn(tl, 'rcap', t0 + 3.2, 0.7, 12);
      t = end;
    }

    // 4. Results, with their scope.
    {
      const t0 = t, end = t0 + 13.6;
      scene(tl, 'results', t0, end, 0.6);
      fadeIn(tl, document.querySelector('#results .kicker'), t0 + 0.2, 0.5, 0);
      fadeIn(tl, 'hero', t0 + 0.4, 0.8, 24);
      fadeIn(tl, 'herosub', t0 + 0.8, 0.7, 16);
      fadeIn(tl, 'rscope', t0 + 1.4, 0.7, 12);
      fadeIn(tl, 'chart', t0 + 2.0, 0.6, 0);
      tl.key('cb1', 'sx', [t0 + 2.3, 0], [t0 + 3.3, 1]);
      fadeIn(tl, 'cv1', t0 + 3.0, 0.4, 0);
      tl.key('cb2', 'sx', [t0 + 3.1, 0], [t0 + 4.1, 1]);
      fadeIn(tl, 'cv2', t0 + 3.8, 0.4, 0);
      fadeIn(tl, 'rfoot', t0 + 4.6, 0.6, 8);
      fadeIn(tl, 'f1', t0 + 6.4, 0.7, 16);
      fadeIn(tl, 'f2', t0 + 7.2, 0.7, 16);
      t = end;
    }

    // 5. Get started.
    {
      const t0 = t, end = t0 + 10.0;
      scene(tl, 'start', t0, end);
      fadeIn(tl, document.querySelector('#start .title'), t0 + 0.2, 0.6);
      fadeIn(tl, 'cmd1', t0 + 0.6, 0.5, 12);
      tl.key('cmd1t', 'chars', [t0 + 1.0, 0], [t0 + 2.6, $('cmd1t').dataset.full.length, 'lin']);
      fadeIn(tl, 'cmd2', t0 + 2.9, 0.5, 12);
      tl.key('cmd2t', 'chars', [t0 + 3.3, 0], [t0 + 4.5, $('cmd2t').dataset.full.length, 'lin']);
      fadeIn(tl, 'gcap', t0 + 4.6, 0.7, 12);
      fadeIn(tl, 'lk1', t0 + 5.6, 0.6, 12);
      fadeIn(tl, 'lk2', t0 + 6.0, 0.6, 12);
      t = end;
    }

    // End card: the lockup, the tagline and the address.
    {
      const t0 = t, end = t0 + 4.0;
      scene(tl, 'open', t0, end + 0.001, 0.001);
      // The open's elements, hidden again while its scene is invisible, then back.
      tl.key('namenote', 'opacity', [t0 - 0.02, 1], [t0 - 0.01, 0, 'lin']);
      tl.key('lockup', 'opacity', [0, 1], [t0 - 0.02, 1], [t0 - 0.01, 0, 'lin'], [t0 + 0.7, 1]);
      tl.key('tagline', 'opacity', [t0 - 0.02, 1], [t0 - 0.01, 0, 'lin'], [t0 + 0.3, 0], [t0 + 1.0, 1]);
      scene(tl, 'endcard', t0, end + 0.001, 0.001);
      fadeIn(tl, 'endurl', t0 + 0.8, 0.7, 10);
      t = end;
    }
    hide(tl, 'teaser');
    tl.duration = t;
    return tl;
  }

  // ---------------------------------------------------------------- teaser
  // An 8-second loop for the README: the turns close up as stretto's reads
  // ride in the first result. The last frames fade back to the first.
  function teaser() {
    const tl = new Timeline();
    const D = 8.0;
    for (const id of ['open', 'record', 'learn', 'serve', 'rule', 'results', 'start', 'endcard']) hide(tl, id);
    hide(tl, 'p-t1', 'p-t2', 'p-t3', 'p-c1', 'p-c2', 'p-c3', 'p-res', 'p-lane', 'wl', 'wlab');
    tl.key('episode', 'opacity', [0, 1]);
    tl.key('teaser', 'opacity', [0, 1]);
    tl.key('r2', 'opacity', [0, 0], [0.6, 0], [1.0, 1], [5.8, 1], [6.2, 0]);
    tl.key('r3', 'opacity', [0, 0], [0.8, 0], [1.2, 1], [5.8, 1], [6.2, 0]);
    const b = 1.4;
    tl.key('k2', 'opacity', [b, 1], [b + 0.6, 0, 'in'], [7.2, 0], [7.8, 1]);
    tl.key('k2', 'x', [b, 0], [b + 0.6, -150, 'in'], [7.1, -150], [7.2, 0, 'lin']);
    tl.key('k2', 'y', [b, 0], [b + 0.6, 60, 'in'], [7.1, 60], [7.2, 0, 'lin']);
    tl.key('k3', 'opacity', [b + 0.2, 1], [b + 0.8, 0, 'in'], [7.2, 0], [7.8, 1]);
    tl.key('k3', 'x', [b + 0.2, 0], [b + 0.8, -420, 'in'], [7.1, -420], [7.2, 0, 'lin']);
    tl.key('k3', 'y', [b + 0.2, 0], [b + 0.8, 110, 'in'], [7.1, 110], [7.2, 0, 'lin']);
    tl.key('rail', 'opacity', [b + 0.4, 0], [b + 0.7, 0.7], [6.6, 0.7], [7.0, 0]);
    for (const [id, s] of [['ride1', 0.5], ['ride2', 0.75]]) {
      tl.key(id, 'opacity', [b + s, 0], [b + s + 0.2, 1, 'lin'], [6.6, 1], [7.0, 0]);
      tl.key(id, 'reveal', [b + s, 0], [b + s + 0.7, 1]);
    }
    tl.key('g1', 'opacity', [0, 0], [b + 2.4, 0], [b + 2.9, 1], [6.6, 1], [7.0, 0]);
    tl.key('g2', 'opacity', [0, 0], [b + 2.5, 0], [b + 3.0, 1], [6.6, 1], [7.0, 0]);
    tl.key('glab', 'opacity', [0, 0], [b + 2.7, 0], [b + 3.2, 1], [6.6, 1], [7.0, 0]);
    for (const id of ['k4', 'k5']) {
      const s = id === 'k4' ? 1.2 : 1.3;
      tl.key(id, 'x', [b + s, 0], [b + s + 1.0, -668, 'io'], [6.6, -668], [7.4, 0, 'io']);
    }
    tl.key('n4', 'text', [0, 'turn 4'], [b + 1.8, 'turn 2'], [7.0, 'turn 4']);
    tl.key('n5', 'text', [0, 'write · turn 5'], [b + 1.9, 'write · turn 3'], [7.0, 'write · turn 5']);
    tl.key('tz-t', 'opacity', [0, 1]);
    tl.key('tz-lane', 'opacity', [0, 1]);
    tl.key('tz-c', 'opacity', [0, 1]);
    tl.duration = D;
    return tl;
  }

  const cuts = { launch, teaser };
  const params = new URLSearchParams(location.search);
  const name = params.get('cut') || 'launch';
  const tl = cuts[name]();
  window.__cut = name;
  window.__duration = tl.duration;
  window.__render = t => tl.render(t);
  window.__ready = Promise.all([
    document.fonts.load('400 20px "JetBrains Mono"'),
    document.fonts.load('700 20px "JetBrains Mono"'),
    document.fonts.load('400 20px Inter'),
    document.fonts.load('640 20px Inter'),
    ...[...document.images].map(img => (img.complete ? Promise.resolve() : img.decode().catch(() => {}))),
  ]).then(() => document.fonts.ready);

  // Preview in a browser: ?t=12.5 draws one frame; ?play runs in real time.
  window.__render(params.has('t') ? +params.get('t') : 0);
  if (params.has('play')) {
    const start = performance.now();
    const loop = now => {
      const t = ((now - start) / 1000) % tl.duration;
      tl.render(t);
      requestAnimationFrame(loop);
    };
    requestAnimationFrame(loop);
  }
})();
