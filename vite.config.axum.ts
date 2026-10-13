import { defineConfig } from 'vite';
import { frontendConfig } from './vite.config.common.ts';
export default defineConfig(({ command }) => frontendConfig(command));
