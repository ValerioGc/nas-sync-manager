import { fileURLToPath, URL } from 'node:url';
import { readFileSync } from 'node:fs';
import { request } from 'node:http';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import vue from '@vitejs/plugin-vue';
import purgecss from '@fullhuman/postcss-purgecss';
import cssnano from 'cssnano';
import combineSelectors from 'postcss-combine-duplicated-selectors';
import type { Plugin, UserConfig } from 'vite';
import {
  frontendBoundary,
  nasPurgeSafelist,
  stripTestIds,
  stripTestIdsNodeTransform,
} from '@valeriogc/nas-base-app/vite';

const { app } = JSON.parse(readFileSync(new URL('./nas-app.json', import.meta.url), 'utf8')) as {
  app: { id: string };
};
export const commonConfig: UserConfig = {
  plugins: [vue()],
  define: { 'import.meta.env.VITE_NAS_APP_ID': JSON.stringify(app.id) },
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('./src', import.meta.url)),
      '@tests': fileURLToPath(new URL('./src/tests', import.meta.url)),
    },
  },
  css: {
    preprocessorOptions: {
      scss: {
        additionalData: (source, filename) =>
          filename.endsWith('.vue') ? '@use "@/styles/global.scss" as *;\n' + source : source,
      },
    },
  },
};

function socketProxy(): Plugin {
  return {
    name: 'nas-api-development-proxy',
    apply: 'serve',
    configureServer(server) {
      server.middlewares.use((incoming, outgoing, next) => {
        if (!incoming.url?.startsWith('/api/')) {
          return next();
        }
        const upstream = request(
          {
            socketPath: process.env.NAS_APP_SOCKET ?? join(tmpdir(), `${app.id}-dev.sock`),
            path: incoming.url,
            method: incoming.method,
            headers: incoming.headers,
          },
          (response) => {
            outgoing.writeHead(response.statusCode ?? 502, response.headers);
            response.pipe(outgoing);
          },
        );
        upstream.on('error', () => {
          if (!outgoing.headersSent) {
            outgoing.writeHead(502, { 'Content-Type': 'application/json' });
          }
          outgoing.end(JSON.stringify({ error: 'Application backend unavailable' }));
        });
        incoming.pipe(upstream);
      });
    },
  };
}

export function frontendConfig(
  command: 'build' | 'serve',
  target: 'axum' | 'tauri' = 'axum',
): UserConfig {
  return {
    ...commonConfig,
    base: './',
    define: { ...commonConfig.define, 'import.meta.env.VITE_APP_MODE': JSON.stringify(target) },
    plugins: [
      frontendBoundary(target),
      vue(
        command === 'build'
          ? { template: { compilerOptions: { nodeTransforms: [stripTestIdsNodeTransform] } } }
          : {},
      ),
      ...(command === 'build' ? [stripTestIds()] : [socketProxy()]),
    ],
    css: {
      ...commonConfig.css,
      ...(command === 'build'
        ? {
            postcss: {
              plugins: [
                combineSelectors({ removeDuplicatedProperties: true }),
                purgecss({
                  content: ['./index.html', './src/**/*.vue', './src/**/*.ts'],
                  safelist: {
                    standard: [
                      // Classes used only inside the @valeriogc/nas-base-app components.
                      ...nasPurgeSafelist,
                      /^v-/,
                      /^dark/,
                      /^tone_/,
                      /^app_/,
                      /^nas_/,
                      /^icon_/,
                      /^is_/,
                      /^theme_/,
                      /^preview_/,
                      /^no_/,
                      /^has_/,
                    ],
                    greedy: [/data-theme/],
                  },
                  defaultExtractor: (content) =>
                    content
                      .replace(/<style\b[^>]*>[\s\S]*?<\/style>/gi, '')
                      .match(/[\w-/:]+(?<!:)/g) ?? [],
                }),
                cssnano({ preset: 'default' }),
              ],
            },
          }
        : {}),
    },
    build: { outDir: `dist/${target}`, emptyOutDir: true, sourcemap: false },
    server: {
      host: '127.0.0.1',
      port: 1420,
      strictPort: true,
      watch: { ignored: ['**/backend/target/**', '**/.git/**'] },
    },
  };
}
