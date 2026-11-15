import { VeilidTableDB } from 'veilid-wasm';
import { textEncoder, textDecoder } from '../utils/marshalling-utils.js';
import type { Suite, TestCase } from '../harness/types.js';

const TABLE_NAME = 'some-table';
const TABLE_COLS = 1;

export const tableSuite: Suite = {
  name: 'table',
  requiresNetwork: false,
  cases: (): TestCase[] => {
    // Shared across the cases below, which run in order like the describe blocks they replace.
    let table: VeilidTableDB;
    const key = 'test-key with unicode 🚀';
    const value = 'test value with unicode 🚀';

    return [
      {
        name: 'should open and close a table',
        run: async ({ expect }) => {
          const t = new VeilidTableDB(TABLE_NAME, TABLE_COLS);
          await t.openTable();
          expect((await t.getKeys(0)).length).toBe(0);
        },
      },
      {
        name: 'should have no keys',
        run: async ({ expect }) => {
          table = new VeilidTableDB(TABLE_NAME, TABLE_COLS);
          await table.openTable();
          expect((await table.getKeys(0)).length).toBe(0);
        },
      },
      {
        name: 'should store value',
        run: async () => {
          await table.store(0, textEncoder.encode(key), textEncoder.encode(value));
        },
      },
      {
        name: 'should load value',
        run: async ({ expect }) => {
          const storedValue = await table.load(0, textEncoder.encode(key));
          expect(storedValue).toBeDefined();
          expect(textDecoder.decode(storedValue)).toBe(value);
        },
      },
      {
        name: 'should have key in list of keys',
        run: async ({ expect }) => {
          const keys = await table.getKeys(0);
          expect(keys.map((k: Uint8Array) => textDecoder.decode(k))).toEqual([key]);
        },
      },
      {
        // IndexedDB withholds a delete while any connection is open, so open handles must yield
        name: 'should delete a table idempotently',
        run: async ({ expect }) => {
          const name = 'delete-me-table';
          const t = new VeilidTableDB(name, TABLE_COLS);
          await t.openTable();
          await t.store(0, textEncoder.encode('k'), textEncoder.encode('v'));

          // true = a delete happened, false = nothing to do
          expect(await t.deleteTable()).toBe(true);
          expect(await t.deleteTable()).toBe(false);
        },
      },
      {
        name: 'should rename a table, carrying its contents',
        run: async ({ expect }) => {
          const oldName = 'rename-me-table';
          const newName = 'renamed-table';

          const t = new VeilidTableDB(oldName, TABLE_COLS);
          await t.openTable();
          await t.store(0, textEncoder.encode('k'), textEncoder.encode('v'));

          await t.renameTable(newName);

          // the handle now points at the new name and the contents followed
          const stored = await t.load(0, textEncoder.encode('k'));
          expect(stored).toBeDefined();
          expect(textDecoder.decode(stored)).toBe('v');

          // the old name is gone
          const old = new VeilidTableDB(oldName, TABLE_COLS);
          expect(await old.deleteTable()).toBe(false);

          expect(await t.deleteTable()).toBe(true);
        },
      },
      {
        name: 'should fail to rename onto an existing table',
        run: async ({ expect }) => {
          const a = new VeilidTableDB('rename-conflict-a', TABLE_COLS);
          await a.openTable();
          const b = new VeilidTableDB('rename-conflict-b', TABLE_COLS);
          await b.openTable();

          await expect(() => a.renameTable('rename-conflict-b')).toThrow();

          expect(await a.deleteTable()).toBe(true);
          expect(await b.deleteTable()).toBe(true);
        },
      },
      {
        name: 'should commit a transaction',
        run: async ({ expect }) => {
          const transaction = await table.createTransaction();

          const txKey = 'tranaction-key🔥';
          const first = 'first🅱';
          const second = 'second✔';
          const third = 'third📢';

          await transaction.store(0, textEncoder.encode(txKey), textEncoder.encode(first));
          await transaction.store(0, textEncoder.encode(txKey), textEncoder.encode(second));
          await transaction.store(0, textEncoder.encode(txKey), textEncoder.encode(third));
          await transaction.commit();

          const storedValue = await table.load(0, textEncoder.encode(txKey));
          expect(storedValue).toBeDefined();
          expect(textDecoder.decode(storedValue)).toBe(third);
        },
      },
    ];
  },
};
