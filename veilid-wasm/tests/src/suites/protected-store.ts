import { veilidClient } from 'veilid-wasm';
import { textEncoder, textDecoder } from '../utils/marshalling-utils.js';
import type { Suite, TestCase } from '../harness/types.js';

const KEY = '_ps_suite_key';
const KEY2 = '_ps_suite_key2';

// Keys veilid-core keeps for itself, which the API must neither list nor touch.
const RESERVED_KEYS = ['device_encryption_key', '_test_key'];

export const protectedStoreSuite: Suite = {
  name: 'pstore',
  requiresNetwork: false,
  cases: (): TestCase[] => [
    {
      name: 'should save, load and overwrite a secret',
      run: async ({ expect }) => {
        await veilidClient.removeUserSecret(KEY);

        expect(await veilidClient.saveUserSecret(KEY, textEncoder.encode('hello'))).toBe(false);
        expect(textDecoder.decode(await veilidClient.loadUserSecret(KEY))).toBe('hello');

        expect(await veilidClient.saveUserSecret(KEY, textEncoder.encode('world'))).toBe(true);
        expect(textDecoder.decode(await veilidClient.loadUserSecret(KEY))).toBe('world');
      },
    },
    {
      name: 'should roundtrip binary and empty values',
      run: async ({ expect }) => {
        const value = new Uint8Array(256).map((_, i) => i);
        await veilidClient.saveUserSecret(KEY, value);
        expect(await veilidClient.loadUserSecret(KEY)).toEqual(value);

        await veilidClient.saveUserSecret(KEY, new Uint8Array(0));
        expect(await veilidClient.loadUserSecret(KEY)).toEqual(new Uint8Array(0));
      },
    },
    {
      name: 'should report a missing secret as absent',
      run: async ({ expect }) => {
        expect(await veilidClient.loadUserSecret('_ps_never_stored')).toBeUndefined();
        expect(await veilidClient.removeUserSecret('_ps_never_stored')).toBe(false);
      },
    },
    {
      name: 'should list stored secrets and drop removed ones',
      run: async ({ expect }) => {
        await veilidClient.saveUserSecret(KEY, textEncoder.encode('a'));
        await veilidClient.saveUserSecret(KEY2, textEncoder.encode('b'));

        let keys = await veilidClient.listUserSecrets();
        expect(keys.includes(KEY)).toBe(true);
        expect(keys.includes(KEY2)).toBe(true);

        expect(await veilidClient.removeUserSecret(KEY)).toBe(true);
        expect(await veilidClient.removeUserSecret(KEY)).toBe(false);

        keys = await veilidClient.listUserSecrets();
        expect(keys.includes(KEY)).toBe(false);
        expect(keys.includes(KEY2)).toBe(true);
      },
    },
    {
      name: 'should never list keys reserved by veilid',
      run: async ({ expect }) => {
        await veilidClient.saveUserSecret(KEY, textEncoder.encode('a'));
        const keys = await veilidClient.listUserSecrets();
        for (const reserved of RESERVED_KEYS) {
          expect(keys.includes(reserved)).toBe(false);
        }
      },
    },
    {
      name: 'should reject access to keys reserved by veilid',
      run: async ({ expect }) => {
        for (const reserved of RESERVED_KEYS) {
          await expect(() => veilidClient.loadUserSecret(reserved)).toThrow();
          await expect(() =>
            veilidClient.saveUserSecret(reserved, textEncoder.encode('evil')),
          ).toThrow();
          await expect(() => veilidClient.removeUserSecret(reserved)).toThrow();
        }
      },
    },
    {
      name: 'should delete all app secrets',
      run: async ({ expect }) => {
        await veilidClient.saveUserSecret(KEY, textEncoder.encode('a'));
        await veilidClient.saveUserSecret(KEY2, textEncoder.encode('b'));

        await veilidClient.deleteAllUserSecrets();

        const keys = await veilidClient.listUserSecrets();
        expect(keys.includes(KEY)).toBe(false);
        expect(keys.includes(KEY2)).toBe(false);
      },
    },
  ],
};
