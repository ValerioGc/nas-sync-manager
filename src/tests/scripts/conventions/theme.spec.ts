import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

const ROOT = resolve(__dirname, '../../../..');

describe('the theme of the shell', () => {
  it('keeps the dark palette in the build, which the sources only set at runtime', () => {
    const config = readFileSync(resolve(ROOT, 'vite.config.common.ts'), 'utf8');
    const styles = readFileSync(resolve(ROOT, 'src/styles/main.scss'), 'utf8');
    const palette = readFileSync(
      resolve(ROOT, 'node_modules/@valeriogc/nas-base-app/dist/styles/_nas-theme.scss'),
      'utf8',
    );

    expect(styles).toContain("@use '@valeriogc/nas-base-app/styles/_nas-theme.scss';");
    expect(palette).toContain(":root[data-theme='dark']");
    expect(config).toMatch(/greedy:\s*\[\/data-theme\/\]/);
  });
});
