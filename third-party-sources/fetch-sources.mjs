// Reproduce the pinned decoder source archives supplied with this source release.
// No source is downloaded by the installed viewer.
import fs from 'node:fs/promises';
import path from 'node:path';
import crypto from 'node:crypto';
import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';
const dir = path.dirname(fileURLToPath(import.meta.url));
const manifestPath = path.join(dir, 'manifest.json');
const manifest = JSON.parse(await fs.readFile(manifestPath, 'utf8'));
let next = 0;
const download = (url, out) => new Promise((resolve, reject) => {
  const p = spawn('curl.exe', ['--silent', '--show-error', '--fail', '--location', '--retry', '2', '--max-time', '240', '--output', out, url], { windowsHide: true });
  let error = '';
  p.stderr.on('data', data => error += data);
  p.on('error', reject);
  p.on('exit', code => code === 0 ? resolve() : reject(new Error(`${code}: ${error}`)));
});
async function worker() {
  while (next < manifest.sources.length) {
    const s = manifest.sources[next++];
    const file = path.join(dir, s.filename);
    try {
      let existing;
      try { existing = await fs.readFile(file); } catch {}
      if (!existing || (s.sha256 && crypto.createHash('sha256').update(existing).digest('hex') !== s.sha256)) await download(s.archiveUrl, file);
      const bytes = await fs.readFile(file);
      const sha256 = crypto.createHash('sha256').update(bytes).digest('hex');
      if (s.sha256 && sha256 !== s.sha256) throw new Error('SHA256 mismatch');
      s.sha256 = sha256; s.bytes = bytes.length; s.status = 'downloaded';
      console.log(`${s.name}: ${bytes.length} bytes`);
    } catch (error) { s.status = 'failed'; s.error = String(error); console.error(`${s.name}: ${error}`); }
  }
}
await Promise.all(Array.from({length: 4}, worker));
await fs.writeFile(manifestPath, JSON.stringify(manifest, null, 2) + '\n');
if (manifest.sources.some(s => s.status !== 'downloaded')) process.exitCode = 1;
