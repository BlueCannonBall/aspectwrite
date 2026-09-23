// node --experimental-strip-types tests/pi-extension.mjs [profile.json]
import assert from 'node:assert/strict';
import { resolve } from 'node:path';
import { readFileSync, readdirSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync as read, rmSync } from 'node:fs';
import { homedir, tmpdir } from 'node:os';
import { join } from 'node:path';
import aspectwrite from '../.pi/extensions/aspectwrite.ts';
const profile = resolve(process.argv[2] || '.local/aspectwrite-strokes.json');
// Exercise the default path, without an environment variable, when no path is supplied.
if (process.argv[2]) process.env.ASPECTWRITE_STROKES = profile;
else delete process.env.ASPECTWRITE_STROKES;
let tool;
aspectwrite({ registerTool(t) { tool = t; } });
assert.equal(tool.name, 'render_latex');
assert.match(tool.description, /CLOSED.*SUBSET/);
assert.match(tool.description, /\\xrightarrow\{label\}/);
assert(tool.promptGuidelines.some(line => line.includes('not arbitrary LaTeX')));
for (const latex of readdirSync('examples').filter(f => f.endsWith('.tex')).map(f => readFileSync(join('examples', f), 'utf8'))) {
  const result = await tool.execute('test', { latex, seed: 7 }, undefined, undefined, { cwd: process.cwd() });
  const image = result.content[0];
  assert.equal(image.type, 'image');
  assert.equal(image.mimeType, 'image/png');
  assert.equal(typeof image.data, 'string');
  assert.equal(image.source, undefined); // pi-ai ImageContent: {type,data,mimeType}
  const png = Buffer.from(image.data, 'base64');
  assert.equal(png.subarray(0, 8).toString('hex'), '89504e470d0a1a0a');
  const dir = mkdtempSync(join(tmpdir(), 'aspectwrite-pi-test-'));
  try {
    const output = join(dir, 'out.png');
    const exe = resolve('target', 'debug', process.platform === 'win32' ? 'aspectwrite.exe' : 'aspectwrite');
    const cli = spawnSync(exe, ['render', profile, output, latex, '--seed', '7']);
    assert.equal(cli.status, 0, cli.stderr.toString());
    assert.deepEqual(png, read(output));
  } finally { rmSync(dir, { recursive: true, force: true }); }
}
const elsewhere = mkdtempSync(join(tmpdir(), 'aspectwrite-other-project-'));
try {
  const result = await tool.execute('test', { latex: 'xx' }, undefined, undefined, { cwd: elsewhere });
  assert.equal(result.content[0].mimeType, 'image/png');
  await assert.rejects(() => tool.execute('test', { latex: String.raw`\unknown` }, undefined, undefined, { cwd: elsewhere }), /unsupported command/);
  await assert.rejects(() => tool.execute('test', { latex: String.raw`\begin{aligned}x&=1\\\text{note}\end{aligned}` }, undefined, undefined, { cwd: elsewhere }), /every row in an aligned block must include '&'/);
  await assert.rejects(() => tool.execute('test', { latex: String.raw`C_{10}H_{18}O\xrightarrow[\Delta]{H^+}C_{10}H_{16}+H_2O` }, undefined, undefined, { cwd: elsewhere }), /optional \[below\] arrow labels/);
  await assert.rejects(() => tool.execute('test', { latex: 'xx', download_filename: '../escape.png' }, undefined, undefined, { cwd: elsewhere }), /simple \.png filename/);
  const relative = 'assets/equation.png';
  const saved = await tool.execute('test', { latex: 'xx', output_path: relative }, undefined, undefined, { cwd: elsewhere });
  const onDisk = join(elsewhere, relative);
  assert.equal(saved.details.outputPath, onDisk);
  assert.match(saved.content[0].text, /Saved PNG to/);
  assert.deepEqual(read(onDisk), Buffer.from(saved.content[1].data, 'base64'));
  await assert.rejects(() => tool.execute('test', { latex: 'xx', output_path: relative }, undefined, undefined, { cwd: elsewhere }), /PNG already exists/);
  await assert.rejects(() => tool.execute('test', { latex: 'xx', output_path: relative, download_filename: 'foo.png' }, undefined, undefined, { cwd: elsewhere }), /Choose output_path OR download_filename/);
  const absolute = join(elsewhere, 'assets with spaces', 'reaction.png');
  const other = await tool.execute('test', { latex: 'xx', output_path: absolute }, undefined, undefined, { cwd: elsewhere });
  assert.equal(other.details.outputPath, absolute);
  assert.deepEqual(read(absolute), Buffer.from(other.content[1].data, 'base64'));
} finally { rmSync(elsewhere, { recursive: true, force: true }); }
const filename = `aspectwrite-test-${process.pid}.png`;
const download = join(homedir(), 'Downloads', filename);
try {
  const latex = String.raw`\text{isoborneol }C_{10}H_{18}O\xrightarrow{H^{+},\,\Delta}\text{camphene }C_{10}H_{16}+H_2O`;
  const result = await tool.execute('test', { latex, download_filename: filename }, undefined, undefined, { cwd: elsewhere });
  assert.match(result.content[0].text, /Saved PNG to/);
  assert.equal(result.content[1].mimeType, 'image/png');
  assert.deepEqual(read(download), Buffer.from(result.content[1].data, 'base64'));
  await assert.rejects(() => tool.execute('test', { latex, download_filename: filename }, undefined, undefined, { cwd: elsewhere }), /PNG already exists/);
} finally { rmSync(download, { force: true }); }
console.log('Pi MCP bridge registered tool, Pi image shape, path and Downloads saves, CLI parity and errors: OK');
