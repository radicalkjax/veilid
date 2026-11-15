import init from 'veilid-wasm';
import { driveContext } from './harness/drive.js';
import { selectedCaseFilter, selectedContext, selectedSuiteNames } from './harness/select.js';
import { suitesByName } from './suites/registry.js';
import { caseSelected, type CaseResult } from './harness/types.js';

await init();

// One run = one context. The whole selection executes once, in that context, and each case is
// then reported as its own mocha test so the spec reporter shows what actually ran where.
const context = selectedContext();
const suiteNames = selectedSuiteNames();
const caseFilter = selectedCaseFilter();
const suites = suitesByName(suiteNames);

let resultsPromise: Promise<CaseResult[]>;
let results: CaseResult[] | undefined;
let runError: unknown;

async function ensureRun(): Promise<CaseResult[]> {
  if (!results && !runError) {
    try {
      results = await resultsPromise;
    } catch (e) {
      runError = e;
    }
  }
  if (runError) {
    throw runError instanceof Error ? runError : new Error(JSON.stringify(runError));
  }
  return results!;
}

describe(`veilid-wasm [${context}]`, () => {
  before('run selected suites in context', () => {
    // Kick the run off once; each case below waits on it and reports its own outcome.
    resultsPromise = driveContext(context, suiteNames, caseFilter);
  });

  for (const suite of suites) {
    describe(suite.name, () => {
      for (const testCase of suite.cases().filter((c) => caseSelected(c, caseFilter))) {
        it(testCase.name, async function () {
          const all = await ensureRun();
          const result = all.find((r) => r.suite === suite.name && r.name === testCase.name);

          if (!result) {
            throw new Error(`no result reported for ${suite.name}: ${testCase.name}`);
          }
          if (result.skipped) {
            console.log(`SKIPPED [${context}] ${suite.name}: ${result.skipped}`);
            this.skip();
          }
          if (!result.ok) {
            throw new Error(result.error ?? 'case failed');
          }
        });
      }
    });
  }
});
