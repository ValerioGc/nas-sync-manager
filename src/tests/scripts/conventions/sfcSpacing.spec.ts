import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';

function findVueFiles(directory: string): string[] {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const path = join(directory, entry.name);

    if (entry.isDirectory()) {
      return findVueFiles(path);
    }

    return path.endsWith('.vue') ? [path] : [];
  });
}

describe('single-file component spacing', () => {
  it('keeps one blank line around script and style contents', () => {
    const files = findVueFiles('src');

    files.forEach((file) => {
      const source = readFileSync(file, 'utf8');
      const blocks = source.matchAll(/<(script|style)\b[^>]*>([\s\S]*?)<\/\1>/g);

      for (const block of blocks) {
        const content = block[2];

        if (content.trim() === '') {
          continue;
        }

        expect(content, file).toMatch(/^\r?\n\r?\n[^\r\n]/);
        expect(content, file).toMatch(/[^\r\n]\r?\n\r?\n$/);
      }
    });
  });
});
