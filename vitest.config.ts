import { defineConfig } from 'vitest/config';
import { commonConfig } from './vite.config.common.ts';

export default defineConfig({
  ...commonConfig,
  test: {
    projects: [
      {
        extends: true,
        test: { name: 'unit', environment: 'jsdom', include: ['src/tests/unit/**/*.unit.spec.ts'] },
      },
      {
        extends: true,
        test: {
          name: 'component',
          environment: 'jsdom',
          include: ['src/tests/component/**/*.component.spec.ts'],
          setupFiles: ['src/tests/component/setup.ts'],
        },
      },
      {
        extends: true,
        test: {
          name: 'conventions',
          environment: 'node',
          include: ['src/tests/scripts/**/*.spec.ts'],
        },
      },
    ],
    coverage: {
      provider: 'v8',
      reporter: ['text', 'json-summary', 'lcov'],
      reportsDirectory: 'coverage',
      include: ['src/**/*.{ts,vue}'],
      exclude: ['src/tests/**', 'src/main.ts', 'src/types/**', 'src/**/*.d.ts'],
      thresholds: { lines: 80, functions: 80, branches: 80, statements: 80 },
    },
  },
});
