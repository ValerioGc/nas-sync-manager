import { spawn } from 'node:child_process';
import { createServer } from 'node:http';
import { copyFileSync, chmodSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
const root = mkdtempSync(join(tmpdir(), 'nas-sync-e2e-'));
const storage = join(root, 'disk');
mkdirSync(join(storage, 'Foto'), { recursive: true });
writeFileSync(join(storage, 'Foto/a.txt'), 'current');
writeFileSync(join(storage, 'Foto/a.sync-conflict-20261010-120000-ABCDEFG.txt'), 'other');
const program = join(root, 'syncthing');
copyFileSync('src/tests/e2e/fixtures/syncthing.mjs', program);
chmodSync(program, 0o755);
const launcher = createServer(async (req, res) => {
  for await (const chunk of req) void chunk;
  res.writeHead(204);
  res.end();
});
launcher.listen(join(root, 'launcher.sock'));
const child = spawn(
  'cargo',
  [
    'run',
    '--locked',
    '--manifest-path',
    'backend/axum/Cargo.toml',
    '--',
    '--listen',
    process.env.NAS_APP_SOCKET,
  ],
  {
    stdio: 'inherit',
    env: {
      ...process.env,
      NAS_UI_DIR: resolve('dist/axum'),
      NAS_DB_PATH: join(root, 'state/app.db'),
      NAS_STORAGE_ROOT: storage,
      NAS_STORAGE_REGISTRY: '',
      NAS_LAUNCHER_SOCKET: join(root, 'launcher.sock'),
      NAS_SYNCTHING: program,
    },
  },
);
let stopping = false;
function stop(signal) {
  if (stopping) return;
  stopping = true;
  child.kill(signal);
}
for (const signal of ['SIGINT', 'SIGTERM']) process.on(signal, () => stop(signal));
child.on('exit', (code) => {
  launcher.close();
  rmSync(root, { recursive: true, force: true });
  process.exitCode = stopping ? 0 : (code ?? 1);
});
