#!/usr/bin/env node
// Checagens de higiene do OpenSpec (pendencias-e-encerramento.md):
//  1. Em PR para main: falha se uma change entregue (presente fora de
//     archive/ na main) continua fora de archive/ no head do PR. O PR [SPEC]
//     que introduz a change e o PR que a arquiva passam.
//  2. Sempre: toda change arquivada tem linha no registro de entregas.
//
// Uso: node scripts/check-openspec.mjs [--base <ref>]
//   --base: ref da branch de destino (ex.: origin/main) quando o PR é para main.

import { execFileSync } from 'node:child_process';
import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';

const CHANGES = 'openspec/changes';
const ARCHIVE = `${CHANGES}/archive`;
const REGISTRY = 'docs/entregas.md';

export function activeChangesFromNames(names) {
  return names.filter((n) => n !== 'archive' && !n.startsWith('.')).sort();
}

export function activeChangesInWorkingTree(root = '.') {
  const dir = join(root, CHANGES);
  if (!existsSync(dir)) return [];
  const names = readdirSync(dir, { withFileTypes: true })
    .filter((d) => d.isDirectory())
    .map((d) => d.name);
  return activeChangesFromNames(names);
}

export function activeChangesAtRef(ref) {
  let out;
  try {
    out = execFileSync('git', ['ls-tree', '-d', '--name-only', `${ref}:${CHANGES}`], {
      encoding: 'utf8',
    });
  } catch {
    return [];
  }
  return activeChangesFromNames(out.split('\n').map((s) => s.trim()).filter(Boolean));
}

/** Changes entregues na base que o PR não arquiva. */
export function deliveredNotArchived(baseChanges, headChanges) {
  const head = new Set(headChanges);
  return baseChanges.filter((c) => head.has(c));
}

/** Nome da change a partir da pasta arquivada `AAAA-MM-DD-<change>`. */
export function changeFromArchiveDir(dir) {
  const m = /^\d{4}-\d{2}-\d{2}-(.+)$/.exec(dir);
  return m ? m[1] : null;
}

/**
 * Changes arquivadas sem linha de tabela no registro. Uma linha conta quando
 * alguma célula é exatamente o nome da change, com ou sem crases (`app` não é
 * registrada por uma linha de `app-android`).
 */
export function archivedWithoutRegistry(archiveDirs, registryText) {
  const cells = new Set(
    registryText
      .split('\n')
      .map((l) => l.trim())
      .filter((l) => l.startsWith('|'))
      .flatMap((l) => l.split('|'))
      .map((c) => c.trim().replace(/^`(.*)`$/, '$1').trim()),
  );
  return archiveDirs
    .map(changeFromArchiveDir)
    .filter(Boolean)
    .filter((change) => !cells.has(change));
}

function main(argv) {
  const baseIdx = argv.indexOf('--base');
  const base = baseIdx >= 0 ? argv[baseIdx + 1] : null;
  const errors = [];

  if (base) {
    const pending = deliveredNotArchived(activeChangesAtRef(base), activeChangesInWorkingTree());
    for (const c of pending) {
      errors.push(`change entregue e não arquivada na base ${base}: ${c}`);
    }
  }

  const archiveDirs = existsSync(ARCHIVE)
    ? readdirSync(ARCHIVE, { withFileTypes: true }).filter((d) => d.isDirectory()).map((d) => d.name)
    : [];
  const registry = existsSync(REGISTRY) ? readFileSync(REGISTRY, 'utf8') : '';
  for (const c of archivedWithoutRegistry(archiveDirs, registry)) {
    errors.push(`change arquivada sem linha em ${REGISTRY}: ${c}`);
  }

  if (errors.length) {
    for (const e of errors) console.error(`ERRO: ${e}`);
    process.exit(1);
  }
  console.log('openspec: sem pendências de arquivamento ou de registro.');
}

if (process.argv[1]?.endsWith('check-openspec.mjs')) {
  main(process.argv.slice(2));
}
