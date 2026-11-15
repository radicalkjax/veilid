import path from 'node:path';
import { config as baseConfig } from './wdio.conf.js';

// The extension suite needs its own capability, running the bundle built by `npm run build:ext`.
// Current-stable Chrome refuses automated extension installs (verified on 150: --load-extension
// is ignored and chromedriver's CRX `extensions` capability never registers, even for a 1.6KB
// extension), so pin a Chrome-for-Testing build that still honors it. wdio downloads it.
const EXTENSION_CHROME_VERSION = process.env.EXT_CHROME_VERSION ?? '126';
const extensionDir = path.resolve(process.cwd(), 'extensionDist');

export const config: typeof baseConfig = {
  ...baseConfig,
  specs: ['./src/ContextRunner.test.ts'],
  exclude: [],
  capabilities: [
    {
      browserName: 'chrome',
      browserVersion: EXTENSION_CHROME_VERSION,
      'goog:chromeOptions': {
        args: [
          `--load-extension=${extensionDir}`,
          `--disable-extensions-except=${extensionDir}`,
          '--disable-features=DisableLoadExtensionCommandLineSwitch',
          // Extensions do not load in old headless mode.
          ...(process.env.WDIO_HEADLESS ? ['--headless=new', '--disable-gpu'] : []),
        ],
      },
      'goog:loggingPrefs': { browser: 'OFF' },
      'wdio:enforceWebDriverClassic': true,
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
    } as any,
  ],
};
