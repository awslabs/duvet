import test from 'node:test';
import report from '../src/util/report.mjs';
import { remove as removeFrontMatter } from '../src/util/front-matter.mjs';
import { readdirSync } from 'node:fs';
import { readFile } from 'node:fs/promises';
import { basename, join } from 'node:path';

const __dirname = import.meta.dirname;

test('empty report', () => {
  report({});
});

const jsonSnapshotSuffix = '_json.snap';
const snapshotDir = join(__dirname, `../../../integration/snapshots`);
readdirSync(snapshotDir).forEach((file) => {
  if (!file.endsWith(jsonSnapshotSuffix)) {
    return;
  }

  const name = basename(file, jsonSnapshotSuffix);

  test(`snapshot ${name}`, async () => {
    let input = await readFile(join(snapshotDir, file), 'utf8');
    // trim the front matter
    input = removeFrontMatter(input);
    input = JSON.parse(input);
    report(input);
  });
});
