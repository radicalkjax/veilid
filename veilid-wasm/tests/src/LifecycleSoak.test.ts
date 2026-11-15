// Measures, without asserting, what a node in a page lives through while the page is hidden.
// Runs instead of the suites when LIFECYCLE_SOAK_MINUTES is set (see wdio.conf.ts); read the
// SOAK and node log lines it prints.
import { browser } from '@wdio/globals';
import init, { veilidClient, type VeilidWASMConfig } from 'veilid-wasm';
import { veilidCoreStartupConfig } from './utils/veilid-config.js';
import { getLogTimestamp, waitForMs } from './utils/wait-utils.js';

const env: Record<string, string | undefined> =
  typeof process !== 'undefined' && process.env ? process.env : {};
const MINUTES = Number(env.LIFECYCLE_SOAK_MINUTES ?? '0');
const ATTACH_TIMEOUT_MS = 120_000;
const REPORT_INTERVAL_MS = 10_000;

// The facilities that report stalls, throttling, purges, readiness and saves
const soakInitConfig: VeilidWASMConfig = {
  logging: {
    api: {
      enabled: true,
      level: 'Info',
      directives: ['attach=debug', 'net=debug', 'rtab=debug', 'stor=debug'],
    },
    performance: {
      enabled: false,
      level: 'Off',
    },
  },
};

function report(line: string): void {
  console.log(`${getLogTimestamp()}: SOAK ${line}`);
}

// WebDriver has to stay on this page, which the runner watches: cover it with a tab it opens
// itself, or where that leaves it visible, minimize the window. Returns how to show it again.
async function hidePage(): Promise<() => Promise<void>> {
  const tab = window.open('about:blank', '_blank');
  await waitForMs(1_000);
  if (document.hidden) {
    report('hidden under a tab');
    return () => Promise.resolve(tab?.close());
  }
  tab?.close();
  await browser.minimizeWindow();
  await waitForMs(1_000);
  report(`minimized: hidden=${document.hidden}`);
  return async () => {
    await browser.maximizeWindow();
  };
}

await init();

describe('lifecycle soak', () => {
  it(`records a page hidden for ${MINUTES} minutes`, async () => {
    await veilidClient.initializeCore(soakInitConfig);
    await veilidClient.startupCore((update) => {
      if (update.kind === 'Log') {
        console.log(`${getLogTimestamp()}: ${update.message}`);
      } else if (update.kind === 'Attachment') {
        report(`attachment ${update.state} public=${update.publicInternetReady}`);
      }
    }, veilidCoreStartupConfig());
    await veilidClient.attach();
    await veilidClient.waitForAttachmentCondition(undefined, ATTACH_TIMEOUT_MS);
    report(`attached; hiding for ${MINUTES} minutes`);

    const showPage = await hidePage();
    const hiddenAt = Date.now();
    while (Date.now() - hiddenAt < MINUTES * 60_000) {
      await waitForMs(REPORT_INTERVAL_MS);
      report(`hidden=${document.hidden} elapsed=${Math.round((Date.now() - hiddenAt) / 1000)}s`);
    }
    await showPage();
    await waitForMs(1_000);
    report(`shown hidden=${document.hidden}`);

    const shownAt = Date.now();
    const state = await veilidClient.getState();
    report(`on show: ${state.attachment.state} public=${state.attachment.publicInternetReady}`);
    const ready = await veilidClient.waitForAttachmentCondition(undefined, ATTACH_TIMEOUT_MS);
    report(`ready ${Date.now() - shownAt}ms after show: ${ready.state}`);
    report((await veilidClient.debug('lifecycle')).replace(/\n/g, ' | '));

    await veilidClient.detach();
    await veilidClient.shutdownCore();
  });
});
