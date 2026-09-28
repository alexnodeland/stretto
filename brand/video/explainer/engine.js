// The explainer's engine: easing, springs, a seeded RNG, keyframe tracks,
// SVG builders and the camera's interpolations. Nothing here reads a clock:
// every value is a function of the time it is given, so a frame at t is the
// same frame however, and in whatever order, it is drawn.
(() => {
  const E = {};
  const { min, max, abs, sqrt, exp, log, cos, sin, cosh, sinh, tanh, PI } = Math;

  // ------------------------------------------------------------------ math
  E.clamp = (x, a = 0, b = 1) => min(b, max(a, x));
  E.lerp = (a, b, p) => a + (b - a) * p;
  E.inv = (t, a, b) => (b === a ? (t >= b ? 1 : 0) : E.clamp((t - a) / (b - a)));
  E.smooth = (a, b, x) => { const p = E.inv(x, a, b); return p * p * (3 - 2 * p); };
  // A gentle, deterministic wobble: a few incommensurate sines.
  E.noise = (t, seed = 0) => (sin(t * 0.73 + seed * 12.9898) * 0.5 + sin(t * 1.37 + seed * 78.233) * 0.3 + sin(t * 2.11 + seed * 37.719) * 0.2);

  // ---------------------------------------------------------------- easing
  const io = (fin, fout) => p => (p < 0.5 ? fin(p * 2) / 2 : 1 - fout((1 - p) * 2) / 2);
  const pw = n => p => Math.pow(p, n);
  E.ease = {
    lin: p => p,
    in2: pw(2), in3: pw(3), in4: pw(4), in5: pw(5),
    out2: p => 1 - Math.pow(1 - p, 2), out3: p => 1 - Math.pow(1 - p, 3), out4: p => 1 - Math.pow(1 - p, 4), out5: p => 1 - Math.pow(1 - p, 5),
    io2: io(pw(2), pw(2)), io3: io(pw(3), pw(3)), io4: io(pw(4), pw(4)), io5: io(pw(5), pw(5)),
    outExpo: p => (p >= 1 ? 1 : 1 - Math.pow(2, -10 * p)),
    inExpo: p => (p <= 0 ? 0 : Math.pow(2, 10 * p - 10)),
    ioSine: p => -(cos(PI * p) - 1) / 2,
    outSine: p => sin((p * PI) / 2),
    outBack: p => { const c1 = 1.5, c3 = c1 + 1; return 1 + c3 * Math.pow(p - 1, 3) + c1 * Math.pow(p - 1, 2); },
    outBackSoft: p => { const c1 = 0.9, c3 = c1 + 1; return 1 + c3 * Math.pow(p - 1, 3) + c1 * Math.pow(p - 1, 2); },
    // A damped spring settling on 1 by p = 1; a lower zeta overshoots more.
    spring: p => E.spring(p, 0.5),
    springSoft: p => E.spring(p, 0.72),
    springHard: p => E.spring(p, 0.38),
  };
  E.spring = (p, zeta = 0.5) => {
    if (p <= 0) return 0;
    if (p >= 1) return 1;
    const w = 6.4 / zeta, wd = w * sqrt(1 - zeta * zeta);
    return 1 - exp(-zeta * w * p) * (cos(wd * p) + ((zeta * w) / wd) * sin(wd * p));
  };
  const easeOf = e => (typeof e === 'function' ? e : E.ease[e || 'io3']);
  E.easeOf = easeOf;
  // Eased progress of t through [t0, t0 + d].
  E.P = (t, t0, d, ease = 'io3') => easeOf(ease)(E.inv(t, t0, t0 + d));
  // 0 -> 1 over [a, a + din], held, then 1 -> 0 over [b - dout, b].
  E.pulse = (t, a, b, din = 0.3, dout = 0.3, ein = 'out3', eout = 'in3') => {
    if (t <= a || t >= b) return 0;
    const up = E.P(t, a, din, ein), down = 1 - E.P(t, b - dout, dout, eout);
    return min(up, down);
  };

  // ---------------------------------------------------------------- colour
  const hex = h => { const n = parseInt(h.slice(1), 16); return [(n >> 16) & 255, (n >> 8) & 255, n & 255]; };
  E.mixColor = (a, b, p) => {
    const A = hex(a), B = hex(b), q = E.clamp(p);
    return `rgb(${A.map((v, i) => Math.round(v + (B[i] - v) * q)).join(',')})`;
  };
  E.rgba = (h, a) => { const [r, g, b] = hex(h); return `rgba(${r},${g},${b},${a.toFixed(3)})`; };

  // ------------------------------------------------------------------- rng
  E.rng = seed => {
    let a = seed >>> 0;
    const next = () => {
      a = (a + 0x6d2b79f5) >>> 0;
      let t = a;
      t = Math.imul(t ^ (t >>> 15), t | 1);
      t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
      return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
    };
    next.range = (lo, hi) => lo + (hi - lo) * next();
    next.int = (lo, hi) => Math.floor(lo + (hi + 1 - lo) * next());
    next.pick = arr => arr[Math.floor(next() * arr.length)];
    return next;
  };

  // ---------------------------------------------------------------- tracks
  // A track is [[t, value, ease], ...]: before its first key it holds the
  // first value, after its last the last; between two keys it moves with the
  // later key's ease.
  E.trackAt = (keys, t) => {
    if (t <= keys[0][0]) return keys[0][1];
    for (let i = 1; i < keys.length; i++) {
      const k = keys[i];
      if (t < k[0]) {
        const k0 = keys[i - 1];
        const p = easeOf(k[2])((t - k0[0]) / (k[0] - k0[0]));
        return k0[1] + (k[1] - k0[1]) * p;
      }
    }
    return keys[keys.length - 1][1];
  };

  // ------------------------------------------------------------------- DOM
  const NS = 'http://www.w3.org/2000/svg';
  E.NS = NS;
  // s(tag, attrs, ...children): an SVG element. Strings are text nodes.
  E.s = (tag, attrs = {}, ...kids) => {
    const el = document.createElementNS(NS, tag);
    for (const k in attrs) {
      const v = attrs[k];
      if (v === undefined || v === null || v === false) continue;
      if (k === 'class') el.setAttribute('class', v);
      else if (k === 'style' && typeof v === 'object') Object.assign(el.style, v);
      else if (k === 'text') el.textContent = v;
      else el.setAttribute(k, String(v));
    }
    for (const c of kids.flat()) if (c !== null && c !== undefined && c !== false) el.appendChild(typeof c === 'string' ? document.createTextNode(c) : c);
    return el;
  };
  E.add = (parent, ...kids) => { for (const c of kids.flat()) if (c) parent.appendChild(c); return parent; };
  // Attribute and style writes, skipped when the value has not changed.
  E.attr = (el, name, v) => {
    const c = el.__a || (el.__a = {});
    if (c[name] !== v) { c[name] = v; el.setAttribute(name, v); }
  };
  E.css = (el, name, v) => {
    const c = el.__c || (el.__c = {});
    if (c[name] !== v) { c[name] = v; el.style.setProperty(name, v); }
  };
  E.text = (el, v) => { if (el.__t !== v) { el.__t = v; el.textContent = v; } };
  E.f = (x, d = 2) => (Math.abs(x) < 1e-9 ? '0' : x.toFixed(d));

  // A node: an SVG group whose transform and opacity are set each frame from
  // its base values, its tracks, then whatever the scenes' functions set.
  class Node {
    constructor(el, base = {}) {
      this.el = el;
      this.base = Object.assign({ x: 0, y: 0, dx: 0, dy: 0, s: 1, sx: 1, sy: 1, r: 0, o: 1 }, base);
      this.p = {};
      this.tr = {};
      this.fx = [];
      Object.assign(this.p, this.base);
    }
    key(prop, ...frames) {
      const list = this.tr[prop] || (this.tr[prop] = []);
      list.push(...frames);
      list.sort((a, b) => a[0] - b[0]);
      return this;
    }
    on(fn) { this.fx.push(fn); return this; }
    reset() { Object.assign(this.p, this.base); }
    eval(t) { for (const k in this.tr) this.p[k] = E.trackAt(this.tr[k], t); }
    apply(t) {
      for (const fn of this.fx) fn(this.p, t, this);
      const p = this.p, s = p.s;
      let tf = `translate(${E.f(p.x + p.dx)} ${E.f(p.y + p.dy)})`;
      if (p.r) tf += ` rotate(${E.f(p.r, 3)})`;
      if (s !== 1 || p.sx !== 1 || p.sy !== 1) tf += ` scale(${E.f(s * p.sx, 4)} ${E.f(s * p.sy, 4)})`;
      E.attr(this.el, 'transform', tf);
      const o = E.clamp(p.o);
      if (o <= 0.002) E.attr(this.el, 'visibility', 'hidden');
      else {
        // inherit, not visible: a hidden ancestor keeps its descendants hidden.
        E.attr(this.el, 'visibility', 'inherit');
        E.attr(this.el, 'opacity', o >= 0.998 ? '1' : o.toFixed(3));
      }
    }
  }
  E.Node = Node;

  // ---------------------------------------------------------------- camera
  // A view is {x, y, z, r, tilt}: the world point at the centre of the
  // frame, the zoom, the roll in degrees and the flyover's tilt in degrees.
  // van Wijk and Nuij's smooth zoom and pan (as d3.interpolateZoom), on the
  // view's width in world units.
  E.zoomPath = (a, b, rho = 1.2, W = 1920) => {
    const ux0 = a.x, uy0 = a.y, w0 = W / a.z, ux1 = b.x, uy1 = b.y, w1 = W / b.z;
    const dx = ux1 - ux0, dy = uy1 - uy0, d2 = dx * dx + dy * dy;
    if (d2 < 1e-9) {
      const S = log(w1 / w0) / rho;
      return p => ({ x: ux0 + p * dx, y: uy0 + p * dy, z: W / (w0 * exp(rho * p * S)) });
    }
    const d1 = sqrt(d2), r2 = rho * rho, r4 = r2 * r2;
    const b0 = (w1 * w1 - w0 * w0 + r4 * d2) / (2 * w0 * r2 * d1);
    const b1 = (w1 * w1 - w0 * w0 - r4 * d2) / (2 * w1 * r2 * d1);
    const q0 = log(sqrt(b0 * b0 + 1) - b0), q1 = log(sqrt(b1 * b1 + 1) - b1);
    const S = (q1 - q0) / rho;
    return p => {
      const s = p * S, c0 = cosh(q0);
      const u = (w0 / (r2 * d1)) * (c0 * tanh(rho * s + q0) - sinh(q0));
      return { x: ux0 + u * dx, y: uy0 + u * dy, z: W / ((w0 * c0) / cosh(rho * s + q0)) };
    };
  };
  // A dive: the target point slides to the centre of the frame along a
  // straight line on screen while the zoom grows geometrically.
  E.divePath = (a, target, z1, ezoom = p => p) => {
    const sx = (target.x - a.x) * a.z, sy = (target.y - a.y) * a.z;
    return (p, pz = p) => {
      const z = exp(E.lerp(log(a.z), log(z1), ezoom(pz)));
      return { x: target.x - (sx * (1 - p)) / z, y: target.y - (sy * (1 - p)) / z, z };
    };
  };

  window.E = E;
})();
