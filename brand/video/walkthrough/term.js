// The walkthrough video: a timeline built from the recorded session, and a
// small terminal emulator that draws it. Every frame is a pure function of t.
//
//   window.__load(castText)   build the timeline from walkthrough.cast
//   window.__duration          its length in seconds
//   window.__posterTime        a frame to use as the poster: the served result
//   window.__render(t)         draw the frame at t seconds
//   window.__ready             resolves once fonts and images load
(() => {
  const $ = id => document.getElementById(id);

  // Pacing, in seconds.
  const INTRO = 5.5, CARD = 3.6, OUTRO = 6.5;
  const TYPE_CPS = 42, TYPE_MAX = 2.4, ENTER = 0.3;
  const GAP = 0.6, GAP_KEEP = 0.3, GAP_MAX = 1.4;  // recorded pauses above GAP shrink to 30%
  const STEP_TAIL = 0.6, FADE = 0.35;
  const hold = (lines, newCaption) => Math.max(2.8, Math.min(8.5, 2.2 + 0.22 * lines)) + (newCaption ? 1.2 : 0);

  const PROMPT = '\x1b[1;36m$\x1b[0m ';
  const TYPED = '\x1b[97m';
  const PALETTE = {
    30: '#4d575b', 31: '#e98a82', 32: '#93c9a1', 33: '#dcc07a', 34: '#86b3e3', 35: '#c7a2dd', 36: '#5dd2d8', 37: '#d5dcde',
    90: '#7d898d', 91: '#f2a39c', 92: '#aee0bb', 93: '#ead69b', 94: '#a6c8f0', 95: '#dabbef', 96: '#93e5e9', 97: '#f1f4f5',
  };

  let plan = null;

  function build(castText) {
    const rows = castText.trim().split('\n');
    const header = JSON.parse(rows[0]);
    const steps = [];
    let step = null, cmd = null, caption = '';
    for (const row of rows.slice(1)) {
      const [t, kind, data] = JSON.parse(row);
      if (kind === 'm') {
        const m = JSON.parse(data);
        if ('step' in m) { step = { title: m.step, caption: '', cmds: [] }; steps.push(step); caption = ''; }
        else if ('caption' in m) { caption = m.caption; if (step && !step.caption) step.caption = m.caption; }
        else if ('cmd' in m) { cmd = { text: m.cmd, t0: t, out: [], caption }; step.cmds.push(cmd); }
        else if ('exit' in m) { if (cmd) cmd.exit = m.exit; cmd = null; }
      } else if (kind === 'o' && cmd) {
        cmd.out.push([t - cmd.t0, data]);
      }
    }

    const segs = [{ kind: 'intro', t0: 0, t1: INTRO }];
    let t = INTRO, poster = null;
    steps.forEach((s, i) => {
      segs.push({ kind: 'card', t0: t, t1: t + CARD, step: s, index: i });
      t += CARD;
      const scene = { kind: 'term', t0: t, step: s, index: i, cmds: [] };
      let promptAt = t;
      let lastCaption = s.caption;
      for (const c of s.cmds) {
        const typing = Math.min(TYPE_MAX, c.text.length / TYPE_CPS);
        const x = { text: c.text, caption: c.caption, promptAt, typeStart: t + 0.4, out: [] };
        x.typeEnd = x.typeStart + typing;
        x.outStart = x.typeEnd + ENTER;
        let clock = x.outStart, prev = 0;
        for (const [rel, data] of c.out) {
          let gap = rel - prev;
          prev = rel;
          if (gap > GAP) gap = Math.min(GAP_MAX, GAP + (gap - GAP) * GAP_KEEP);
          clock += gap;
          x.out.push([clock, data]);
        }
        x.outEnd = clock + 0.12;
        const lines = c.out.map(o => o[1]).join('').split('\n').length;
        x.end = x.outEnd + hold(lines, c.caption !== lastCaption);
        lastCaption = c.caption;
        if (!poster && c.text.includes('--flow ')) poster = x.outEnd + 1.2;
        scene.cmds.push(x);
        promptAt = x.outEnd;
        t = x.end;
      }
      scene.idlePrompt = promptAt;
      t += STEP_TAIL;
      scene.t1 = t;
      segs.push(scene);
    });
    segs.push({ kind: 'outro', t0: t, t1: t + OUTRO });
    t += OUTRO;
    $('intro-meta').innerHTML = `${escapeHtml(header.stretto?.version || 'stretto')} · recorded ${new Date(header.timestamp * 1000).toISOString().slice(0, 10)} · the commands are in <code>brand/video/walkthrough/script.sh</code>`;
    $('tb-right').textContent = `${header.width}×${header.height}`;
    plan = { header, steps, segs, duration: t, poster: poster ?? t / 2 };
    window.__duration = plan.duration;
    window.__posterTime = plan.poster;
    return plan;
  }

  // ------------------------------------------------------------ the terminal
  function emulate(stream, cols) {
    const lines = [[]];
    let x = 0, style = {};
    const newline = () => { lines.push([]); x = 0; };
    const chars = Array.from(stream);
    for (let i = 0; i < chars.length; i++) {
      const c = chars[i];
      if (c === '\x1b') {
        if (chars[i + 1] === '[') {
          let j = i + 2, params = '';
          while (j < chars.length && !/[@-~]/.test(chars[j])) params += chars[j++];
          const fin = chars[j];
          if (fin === 'm') style = sgr(style, params);
          else if (fin === 'K') lines[lines.length - 1].length = x;
          else if (fin === 'J' && params === '2') { lines.length = 0; lines.push([]); x = 0; }
          i = j;
        } else if (chars[i + 1] === ']') {
          let j = i + 2;
          while (j < chars.length && chars[j] !== '\x07' && !(chars[j] === '\x1b' && chars[j + 1] === '\\')) j++;
          i = chars[j] === '\x07' ? j : j + 1;
        } else {
          i += 1;
        }
        continue;
      }
      if (c === '\r') { x = 0; continue; }
      if (c === '\n') { newline(); continue; }
      if (c === '\b') { x = Math.max(0, x - 1); continue; }
      if (c === '\t') { const n = 8 - (x % 8); for (let k = 0; k < n; k++) put(' '); continue; }
      if (c < ' ') continue;
      put(c);
    }
    function put(ch) {
      if (x >= cols) newline();
      const line = lines[lines.length - 1];
      while (line.length < x) line.push({ ch: ' ', st: {} });
      line[x] = { ch, st: style };
      x++;
    }
    return { lines, x };
  }

  function sgr(style, params) {
    const s = { ...style };
    const p = params === '' ? [0] : params.split(';').map(Number);
    for (let k = 0; k < p.length; k++) {
      const n = p[k];
      if (n === 0) { for (const key of Object.keys(s)) delete s[key]; }
      else if (n === 1) s.b = true;
      else if (n === 2) s.dim = true;
      else if (n === 22) { delete s.b; delete s.dim; }
      else if ((n >= 30 && n <= 37) || (n >= 90 && n <= 97)) s.fg = PALETTE[n];
      else if (n === 39) delete s.fg;
      else if (n === 38 && p[k + 1] === 5) { s.fg = xterm256(p[k + 2]); k += 2; }
    }
    return s;
  }

  function xterm256(n) {
    if (n < 8) return PALETTE[30 + n];
    if (n < 16) return PALETTE[90 + n - 8];
    if (n >= 232) { const v = 8 + (n - 232) * 10; return `rgb(${v},${v},${v})`; }
    const i = n - 16, r = Math.floor(i / 36), g = Math.floor(i / 6) % 6, b = i % 6;
    const lv = v => (v ? 55 + v * 40 : 0);
    return `rgb(${lv(r)},${lv(g)},${lv(b)})`;
  }

  function escapeHtml(s) { return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;'); }

  function drawScreen(stream, cursorOn) {
    const cols = plan.header.width, rows = plan.header.height;
    const { lines, x } = emulate(stream, cols);
    const first = Math.max(0, lines.length - rows);
    let html = '';
    for (let r = first; r < lines.length; r++) {
      const line = lines[r];
      let out = '', run = '', cur = null;
      const flush = () => {
        if (!run) return;
        const st = cur || {};
        const css = [st.fg ? `color:${st.fg}` : '', st.dim ? 'opacity:.7' : ''].filter(Boolean).join(';');
        out += st.b || css ? `<span${st.b ? ' class="b"' : ''}${css ? ` style="${css}"` : ''}>${escapeHtml(run)}</span>` : escapeHtml(run);
        run = '';
      };
      for (const cell of line) {
        if (cell.st !== cur) { flush(); cur = cell.st; }
        run += cell.ch;
      }
      flush();
      if (cursorOn && r === lines.length - 1) {
        out += (x > line.length ? ' '.repeat(x - line.length) : '') + '<span class="cursor"></span>';
      }
      html += `<div class="l">${out}</div>`;
    }
    $('screen').innerHTML = html;
  }

  // ------------------------------------------------------------ frames
  function fade(t, t0, t1, fin = FADE, fout = FADE) {
    if (t < t0 || t >= t1) return 0;
    return Math.max(0, Math.min(1, (t - t0) / fin, (t1 - t) / fout));
  }

  function render(t) {
    const layers = { intro: 0, stepcard: 0, term: 0, outro: 0 };
    for (const seg of plan.segs) {
      if (t < seg.t0 || t >= seg.t1) continue;
      if (seg.kind === 'intro') layers.intro = fade(t, seg.t0, seg.t1, 0.6, FADE);
      if (seg.kind === 'outro') layers.outro = Math.min(1, (t - seg.t0) / 0.6);
      if (seg.kind === 'card') {
        layers.stepcard = fade(t, seg.t0, seg.t1);
        $('sc-no').textContent = `Step ${seg.index + 1} of ${plan.steps.length}`;
        $('sc-title').textContent = seg.step.title;
        $('sc-caption').textContent = seg.step.caption;
      }
      if (seg.kind === 'term') {
        layers.term = fade(t, seg.t0, seg.t1);
        drawTerm(seg, t);
      }
    }
    for (const [id, o] of Object.entries(layers)) $(id).style.opacity = o.toFixed(3);
  }

  function drawTerm(scene, t) {
    let stream = '', cursor = false, caption = scene.step.caption;
    const blink = Math.floor(t * 1.6) % 2 === 0;
    for (let i = 0; i < scene.cmds.length; i++) {
      const c = scene.cmds[i];
      if (t < c.promptAt) break;
      stream += PROMPT;
      if (t < c.typeStart) { cursor = blink; break; }
      caption = c.caption;
      const n = t >= c.typeEnd ? c.text.length : Math.floor(((t - c.typeStart) / (c.typeEnd - c.typeStart)) * c.text.length);
      stream += TYPED + c.text.slice(0, n) + '\x1b[0m';
      if (t < c.outStart) { cursor = true; break; }
      stream += '\r\n';
      for (const [ot, data] of c.out) if (ot <= t) stream += data;
      if (t < c.outEnd) break;
      if (i === scene.cmds.length - 1 && t >= scene.idlePrompt) { stream += PROMPT; cursor = blink; }
    }
    drawScreen(stream, cursor);
    $('cb-no').textContent = `${scene.index + 1} / ${plan.steps.length}`;
    $('cb-title').textContent = scene.step.title;
    $('cb-caption').textContent = caption;
  }

  window.__load = text => { build(text); render(0); return plan.duration; };
  window.__render = t => render(t);
  window.__ready = Promise.all([
    document.fonts.load('400 20px "JetBrains Mono"'),
    document.fonts.load('700 20px "JetBrains Mono"'),
    document.fonts.load('400 20px Inter'),
    document.fonts.load('640 20px Inter'),
    ...[...document.images].map(img => (img.complete ? Promise.resolve() : img.decode().catch(() => {}))),
  ]).then(() => document.fonts.ready);
})();
