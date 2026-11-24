import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  activeChangesFromNames,
  archivedWithoutRegistry,
  changeFromArchiveDir,
  deliveredNotArchived,
} from './check-openspec.mjs';

test('ignora archive e pastas ocultas', () => {
  assert.deepEqual(activeChangesFromNames(['archive', '.x', 'b', 'a']), ['a', 'b']);
});

test('PR [SPEC] que introduz a change passa', () => {
  assert.deepEqual(deliveredNotArchived([], ['nova']), []);
});

test('PR que arquiva a change passa', () => {
  assert.deepEqual(deliveredNotArchived(['entregue'], []), []);
});

test('PR comum com change entregue sem arquivar falha', () => {
  assert.deepEqual(deliveredNotArchived(['entregue'], ['entregue', 'outra']), ['entregue']);
});

test('extrai o nome da change da pasta arquivada', () => {
  assert.equal(changeFromArchiveDir('2026-11-24-mvp-desktop-rtsp'), 'mvp-desktop-rtsp');
  assert.equal(changeFromArchiveDir('.gitkeep'), null);
});

test('change arquivada precisa de linha no registro', () => {
  const registro = '| Change | Issue |\n| --- | --- |\n| `mvp-desktop-rtsp` | #1 |\n';
  assert.deepEqual(archivedWithoutRegistry(['2026-11-24-mvp-desktop-rtsp'], registro), []);
  assert.deepEqual(archivedWithoutRegistry(['2026-12-01-app-android'], registro), ['app-android']);
});

test('menção fora de tabela não conta como registro', () => {
  const registro = 'Texto citando app-android.\n| Change |\n| --- |\n';
  assert.deepEqual(archivedWithoutRegistry(['2026-12-01-app-android'], registro), ['app-android']);
});
