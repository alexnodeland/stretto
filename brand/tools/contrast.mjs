// Check the WCAG 2.2 contrast of brand/tokens.css. No dependencies:
//
//   node brand/tools/contrast.mjs            # prints a Markdown table
//   node brand/tools/contrast.mjs --check    # also exits 1 if a pair fails
//
// Text pairs need 4.5:1 (AA, normal text); graphic pairs (marks, bars, icons,
// control outlines, focus rings) need 3:1 (AA, non-text contrast, SC 1.4.11).
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));
const css = fs.readFileSync(path.join(here, '..', 'tokens.css'), 'utf8').replace(/\/\*[\s\S]*?\*\//g, '');

function block(selector) {
  const start = css.indexOf(selector);
  if (start < 0) throw new Error(`no block ${selector}`);
  const open = css.indexOf('{', start + selector.length - 1);
  let depth = 0;
  for (let i = open; i < css.length; i++) {
    if (css[i] === '{') depth++;
    if (css[i] === '}' && --depth === 0) return css.slice(open + 1, i);
  }
  throw new Error(`unclosed block ${selector}`);
}
function tokens(body) {
  const out = {};
  for (const m of body.matchAll(/--stretto-([a-z0-9-]+)\s*:\s*(#[0-9a-fA-F]{6})\s*;/g)) out[m[1]] = m[2].toLowerCase();
  return out;
}

const light = tokens(block(':root {'));
const darkMedia = tokens(block(':root:not([data-theme="light"]) {'));
const darkAttr = tokens(block(':root[data-theme="dark"] {'));
const dark = { ...light, ...darkMedia };

let failed = false;
for (const [k, v] of Object.entries(darkMedia)) {
  if (darkAttr[k] !== v) { console.error(`dark tokens differ: --stretto-${k} is ${v} in the media query and ${darkAttr[k]} under [data-theme="dark"]`); failed = true; }
}
for (const k of Object.keys(darkAttr)) if (!(k in darkMedia)) { console.error(`--stretto-${k} is set only under [data-theme="dark"]`); failed = true; }

function lum(hex) {
  const c = [1, 3, 5].map(i => parseInt(hex.slice(i, i + 2), 16) / 255)
    .map(x => (x <= 0.04045 ? x / 12.92 : ((x + 0.055) / 1.055) ** 2.4));
  return 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
}
function ratio(a, b) {
  const [x, y] = [lum(a), lum(b)].sort((p, q) => q - p);
  return (x + 0.05) / (y + 0.05);
}

const TEXT = 4.5, GRAPHIC = 3;
const surfaces = ['bg', 'surface', 'surface-2', 'accent-soft'];
const pairs = [];
for (const fg of ['text', 'text-muted', 'text-subtle', 'accent', 'accent-hover']) {
  for (const bg of surfaces) pairs.push([fg, bg, TEXT, 'text']);
}
pairs.push(['on-accent', 'accent-fill', TEXT, 'text']);
for (const fg of ['accent-graphic', 'border-strong', 'focus', 'chart-baseline', 'chart-accent']) {
  for (const bg of ['bg', 'surface', 'surface-2']) pairs.push([fg, bg, GRAPHIC, 'graphic']);
}

const rows = [];
for (const [theme, t] of [['light', light], ['dark', dark]]) {
  for (const [fg, bg, min, kind] of pairs) {
    const r = ratio(t[fg], t[bg]);
    const ok = r >= min;
    if (!ok) failed = true;
    rows.push(`| ${theme} | \`--stretto-${fg}\` ${t[fg]} | \`--stretto-${bg}\` ${t[bg]} | ${kind} | ${r.toFixed(2)}:1 | ${min}:1 | ${ok ? 'pass' : 'FAIL'} |`);
  }
}
// The logo files use fixed colors, whatever the page's theme.
const brand = [
  ['logo on light: ink bar', light.ink, light.paper, GRAPHIC],
  ['logo on light: petrol bars', light['petrol-light'], light.paper, GRAPHIC],
  ['logo on light: petrol bars on white', light['petrol-light'], '#ffffff', GRAPHIC],
  ['logo on dark: paper bar', light.paper, dark.bg, GRAPHIC],
  ['logo on dark: petrol bars', light['petrol-dark'], dark.bg, GRAPHIC],
  ['icon tile: paper bars on petrol', light.paper, light.petrol, GRAPHIC],
  ['wordmark: ink on paper', light.ink, light.paper, TEXT],
  ['wordmark: paper on dark bg', light.paper, dark.bg, TEXT],
];
for (const [what, fg, bg, min] of brand) {
  const r = ratio(fg, bg);
  const ok = r >= min;
  if (!ok) failed = true;
  rows.push(`| brand | ${what} ${fg} | ${bg} | ${min === TEXT ? 'text' : 'graphic'} | ${r.toFixed(2)}:1 | ${min}:1 | ${ok ? 'pass' : 'FAIL'} |`);
}

console.log('| Theme | Foreground | Background | Kind | Ratio | Needs | Result |');
console.log('|---|---|---|---|---|---|---|');
for (const r of rows) console.log(r);
if (process.argv.includes('--check') && failed) process.exit(1);
