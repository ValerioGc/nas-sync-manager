import { defineConfig, devices } from '@playwright/test';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
const socket = join(tmpdir(), `nas-sync-manager-e2e-${process.pid}.sock`);
export default defineConfig({
  testDir: 'src/tests/e2e',
  fullyParallel: false,
  workers: 1,
  reporter: 'list',
  use: { ...devices['Desktop Chrome'], baseURL: 'http://127.0.0.1:1420', trace: 'on-first-retry' },
  projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'] } }],
  webServer: [
    {
      command: 'npm run dev -- --host 127.0.0.1 --port 1420 --strictPort',
      env: { CHOKIDAR_USEPOLLING: 'true', NAS_APP_SOCKET: socket },
      url: 'http://127.0.0.1:1420',
      reuseExistingServer: false,
      timeout: 120_000,
    },
    {
      command: 'npm run build && node src/tests/e2e/fixtures/start.mjs',
      env: { NAS_APP_SOCKET: socket },
      url: 'http://127.0.0.1:1420/api/settings',
      reuseExistingServer: false,
      timeout: 180_000,
    },
  ],
});
