#!/usr/bin/env node
// Only a REST stand-in. No file transport and no network ports.
import { createServer } from 'node:http';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
const home = process.argv.find((arg) => arg.startsWith('--home=')).slice(7);
const key = readFileSync(join(home, 'api-key'), 'utf8').trim();
export const NAS_ID = 'NASNASN-AAAAAAA-BBBBBBB-CCCCCCC-DDDDDDD-EEEEEEE-FFFFFFF-GGGGGGG';
const devices = new Map();
const folders = new Map();
const ignores = new Map();
createServer(async (req, res) => {
  if (req.headers['x-api-key'] !== key) {
    res.writeHead(403);
    res.end();
    return;
  }
  const url = new URL(req.url, 'http://localhost');
  const path = url.pathname;
  let raw = '';
  for await (const chunk of req) raw += chunk;
  const body = raw ? JSON.parse(raw) : null;
  let answer = null;
  let status = 200;
  if (path === '/rest/system/ping') answer = { ping: 'pong' };
  else if (path === '/rest/system/status') answer = { myID: NAS_ID };
  else if (path === '/rest/config/options') answer = null;
  else if (path === '/rest/config/devices') answer = [{ deviceID: NAS_ID }, ...devices.values()];
  else if (path === '/rest/config/folders') answer = [...folders.values()];
  else if (path.startsWith('/rest/config/devices/')) {
    const id = path.split('/').at(-1);
    if (req.method === 'DELETE') devices.delete(id);
    else devices.set(id, body);
  } else if (path.startsWith('/rest/config/folders/')) {
    const id = path.split('/').at(-1);
    if (req.method === 'DELETE') folders.delete(id);
    else folders.set(id, body);
  } else if (path === '/rest/system/connections')
    answer = {
      connections: Object.fromEntries([...devices.keys()].map((id) => [id, { connected: true }])),
    };
  else if (path === '/rest/db/status')
    answer = { state: 'idle', globalBytes: 1024, globalFiles: 1, needBytes: 0, needFiles: 0 };
  else if (path === '/rest/db/completion')
    answer = {
      remoteState: 'valid',
      globalBytes: 1024,
      needBytes: 0,
      needItems: 0,
      needDeletes: 0,
    };
  else if (path === '/rest/stats/device')
    answer = Object.fromEntries(
      [...devices.keys()].map((id) => [id, { lastSeen: '2026-10-11T12:00:00Z' }]),
    );
  else if (path === '/rest/stats/folder')
    answer = Object.fromEntries(
      [...folders.keys()].map((id) => [id, { lastFile: { at: '2026-10-11T12:00:00Z' } }]),
    );
  else if (path === '/rest/db/ignores') {
    const id = url.searchParams.get('folder');
    if (req.method === 'POST') ignores.set(id, body.ignore);
    answer = { ignore: ignores.get(id) ?? ['*.tmp'] };
  } else if (path === '/rest/events/disk')
    answer = [...folders.keys()].map((folder) => ({
      type: 'RemoteChangeDetected',
      time: '2026-10-11T12:00:00Z',
      data: { folder, action: 'modified', path: 'a.txt' },
    }));
  else status = 404;
  res.writeHead(status, { 'Content-Type': 'application/json' });
  res.end(answer === null ? '' : JSON.stringify(answer));
}).listen(join(home, 'api.sock'));
