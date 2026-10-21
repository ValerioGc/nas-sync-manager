import eslint from '@eslint/js';
import vue from 'eslint-plugin-vue';
import globals from 'globals';
import tseslint from 'typescript-eslint';

export default tseslint.config(
  {
    ignores: [
      'dist',
      'backend',
      '.claude',
      'src/tests/e2e/.features-gen',
      'src/tests/e2e/allure-results',
      'src/tests/e2e/allure-report',
      'test-results',
      'coverage',
      'release-assets',
      'build-reports',
      '.playwright-results',
    ],
  },
  eslint.configs.recommended,
  ...tseslint.configs.recommended,
  ...vue.configs['flat/recommended'],
  {
    files: ['src/tests/**/*.mjs', 'src/tests/scripts/**/*.ts', '*.config.ts'],
    languageOptions: {
      globals: { ...globals.node, URL: 'readonly' },
    },
    rules: { '@typescript-eslint/no-require-imports': 'off' },
  },
  {
    files: ['**/*.{ts,vue}'],
    languageOptions: {
      globals: globals.browser,
      parserOptions: {
        parser: tseslint.parser,
      },
    },
    rules: {
      curly: ['error', 'all'],
      'nonblock-statement-body-position': ['error', 'below'],
      semi: ['error', 'always'],
      'vue/multi-word-component-names': 'off',
      'vue/html-closing-bracket-newline': 'off',
      'vue/html-indent': 'off',
      'vue/html-self-closing': 'off',
      'vue/max-attributes-per-line': 'off',
      'vue/multiline-html-element-content-newline': 'off',
      'vue/singleline-html-element-content-newline': 'off',
      '@typescript-eslint/no-unused-vars': ['error', { argsIgnorePattern: '^_' }],
    },
  },
);
