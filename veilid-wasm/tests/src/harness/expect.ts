// Minimal expect shim.
//
// Test bodies run in workers and service workers where `@wdio/globals` does not exist, so the
// suites use this instead. It implements only the matchers the suites actually use, with the
// same names, so bodies read the same in or out of the page.

function typeName(v: unknown): string {
  if (v === null) return 'null';
  if (Array.isArray(v)) return 'array';
  if (v instanceof Uint8Array) return 'Uint8Array';
  return typeof v;
}

function show(v: unknown): string {
  if (v instanceof Uint8Array) return `Uint8Array[${Array.from(v).join(',')}]`;
  if (typeof v === 'bigint') return `${v}n`;
  try {
    return JSON.stringify(v) ?? String(v);
  } catch {
    return String(v);
  }
}

function deepEqual(a: unknown, b: unknown): boolean {
  if (a === b) return true;
  if (typeof a === 'bigint' || typeof b === 'bigint') return String(a) === String(b);
  if (a === null || b === null || a === undefined || b === undefined) return false;

  if (a instanceof Uint8Array || b instanceof Uint8Array) {
    if (!(a instanceof Uint8Array) || !(b instanceof Uint8Array)) return false;
    return a.length === b.length && a.every((x, i) => x === b[i]);
  }
  if (Array.isArray(a) || Array.isArray(b)) {
    if (!Array.isArray(a) || !Array.isArray(b)) return false;
    return a.length === b.length && a.every((x, i) => deepEqual(x, b[i]));
  }
  if (typeof a === 'object' && typeof b === 'object') {
    const ao = a as Record<string, unknown>;
    const bo = b as Record<string, unknown>;
    // Ignore keys explicitly set to undefined, matching jest's toEqual.
    const ak = Object.keys(ao).filter((k) => ao[k] !== undefined);
    const bk = Object.keys(bo).filter((k) => bo[k] !== undefined);
    if (ak.length !== bk.length) return false;
    return ak.every((k) => deepEqual(ao[k], bo[k]));
  }
  return false;
}

function matchesObject(actual: unknown, expected: Record<string, unknown>): boolean {
  if (typeof actual !== 'object' || actual === null) return false;
  const ao = actual as Record<string, unknown>;
  return Object.keys(expected).every((k) => deepEqual(ao[k], expected[k]));
}

function getPath(obj: unknown, path: string): { found: boolean; value: unknown } {
  let cur: unknown = obj;
  for (const part of path.split('.')) {
    if (typeof cur !== 'object' || cur === null || !(part in cur)) {
      return { found: false, value: undefined };
    }
    cur = (cur as Record<string, unknown>)[part];
  }
  return { found: true, value: cur };
}

// Call a function and report whether it threw, awaiting it if it returns a promise.
function captureThrow(fn: unknown): boolean | Promise<boolean> {
  if (typeof fn !== 'function') {
    throw new Error(`expected a function to call, got ${typeName(fn)}`);
  }
  try {
    const r = (fn as () => unknown)();
    if (r && typeof (r as Promise<unknown>).then === 'function') {
      return (r as Promise<unknown>).then(
        () => false,
        () => true,
      );
    }
    return false;
  } catch {
    return true;
  }
}

export interface Matchers {
  toBe(expected: unknown): void;
  toEqual(expected: unknown): void;
  toStrictEqual(expected: unknown): void;
  toBeDefined(): void;
  toBeUndefined(): void;
  // instanceof wants a Function here; wasm-bindgen classes have private constructors, so no
  // construct signature accepts them
  // eslint-disable-next-line @typescript-eslint/no-unsafe-function-type
  toBeInstanceOf(ctor: Function): void;
  toBeGreaterThan(n: number): void;
  toHaveProperty(path: string, ...expectedValue: unknown[]): void;
  toMatchObject(expected: Record<string, unknown>): void;
  toThrow(): void | Promise<void>;
}

export interface Expectation extends Matchers {
  not: Matchers;
  // Assert against the rejection value of a promise.
  rejects: {
    toEqual(expected: unknown): Promise<void>;
    toMatchObject(expected: Record<string, unknown>): Promise<void>;
  };
}

function rejectionMatchers(actual: unknown) {
  const rejection = async (): Promise<unknown> => {
    if (!actual || typeof (actual as Promise<unknown>).then !== 'function') {
      throw new Error(`expected a promise to reject, got ${typeName(actual)}`);
    }
    try {
      await (actual as Promise<unknown>);
    } catch (e) {
      return e;
    }
    throw new Error('expected promise to reject, but it resolved');
  };

  return {
    toEqual: async (expected: unknown) => {
      const e = await rejection();
      if (!deepEqual(e, expected)) {
        throw new Error(`expected rejection ${show(e)} to equal ${show(expected)}`);
      }
    },
    toMatchObject: async (expected: Record<string, unknown>) => {
      const e = await rejection();
      if (!matchesObject(e, expected)) {
        throw new Error(`expected rejection ${show(e)} to match ${show(expected)}`);
      }
    },
  };
}

function build(actual: unknown, negated: boolean): Matchers {
  const check = (pass: boolean, message: string) => {
    if (pass === negated) {
      throw new Error(negated ? `expected NOT ${message}` : `expected ${message}`);
    }
  };

  return {
    toBe: (expected) =>
      check(Object.is(actual, expected), `${show(actual)} to be ${show(expected)}`),
    toEqual: (expected) =>
      check(deepEqual(actual, expected), `${show(actual)} to equal ${show(expected)}`),
    toStrictEqual: (expected) =>
      check(
        deepEqual(actual, expected) && typeName(actual) === typeName(expected),
        `${show(actual)} to strictly equal ${show(expected)}`,
      ),
    toBeDefined: () => check(actual !== undefined, `${show(actual)} to be defined`),
    toBeUndefined: () => check(actual === undefined, `${show(actual)} to be undefined`),
    toBeInstanceOf: (ctor) =>
      check(
        actual instanceof (ctor as new (...args: never[]) => unknown),
        `${show(actual)} to be instance of ${ctor.name}`,
      ),
    toBeGreaterThan: (n) =>
      check((actual as number) > n, `${show(actual)} to be greater than ${n}`),
    toHaveProperty: (path: string, ...expectedValue: unknown[]) => {
      const { found, value: got } = getPath(actual, path);
      if (expectedValue.length > 0) {
        check(
          found && deepEqual(got, expectedValue[0]),
          `property '${path}' to be ${show(expectedValue[0])}`,
        );
      } else {
        check(found, `to have property '${path}'`);
      }
    },
    toMatchObject: (expected) =>
      check(matchesObject(actual, expected), `${show(actual)} to match ${show(expected)}`),
    toThrow: () => {
      const threw = captureThrow(actual);
      if (typeof threw === 'boolean') {
        check(threw, 'function to throw');
        return;
      }
      return threw.then((t) => check(t, 'function to throw'));
    },
  };
}

export function expect(actual: unknown): Expectation {
  return {
    ...build(actual, false),
    not: build(actual, true),
    rejects: rejectionMatchers(actual),
  };
}
