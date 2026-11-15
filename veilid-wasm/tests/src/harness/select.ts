import { ALL_SUITE_NAMES } from '../suites/registry.js';
import { ALL_CONTEXTS, type ContextName } from './types.js';

// `process` is replaced by the page bundle but not in workers; the worker gets its selection
// over postMessage instead, so this is only ever read page-side.
const env: Record<string, string | undefined> =
  typeof process !== 'undefined' && process.env ? process.env : {};

function parseList(value: string | undefined): string[] | undefined {
  const items = (value ?? '')
    .split(',')
    .map((s) => s.trim())
    .filter((s) => s.length > 0);
  return items.length > 0 ? items : undefined;
}

// Suites selected with --tests, defaulting to all of them.
export function selectedSuiteNames(): string[] {
  const names = parseList(env.VEILID_TEST_SUITES) ?? ALL_SUITE_NAMES;
  const unknown = names.filter((n) => !ALL_SUITE_NAMES.includes(n));
  if (unknown.length > 0) {
    throw new Error(
      `unknown test suite(s): ${unknown.join(', ')}. Available: ${ALL_SUITE_NAMES.join(', ')}`,
    );
  }
  return names;
}

// Case-name substring selected with --case; unset runs every case.
export function selectedCaseFilter(): string | undefined {
  const text = (env.VEILID_TEST_CASE ?? '').trim();
  return text.length > 0 ? text : undefined;
}

// Context selected with --context, defaulting to the page. The runner script invokes wdio once
// per context, so a single run only ever has one.
export function selectedContext(): ContextName {
  const name = (env.VEILID_TEST_CONTEXT ?? 'window') as ContextName;
  if (!ALL_CONTEXTS.includes(name)) {
    throw new Error(`unknown context '${name}'. Available: ${ALL_CONTEXTS.join(', ')}`);
  }
  return name;
}
