import * as veilid from 'veilid-wasm';
import loadVeilidWasm, { veilidClient, VeilidAPIError, VeilidRoutingContext } from 'veilid-wasm';
import { getVeilidCoreStartupConfig, veildCoreInitConfig } from './veilid-config';
import { veilidEventEmitter } from './veilid-event-emitter';
import { v4 as uuidV4 } from 'uuid';

let loadVeilidWasmPromise: Promise<veilid.InitOutput>;
let isVeilidWasmLoaded = false;
export async function initVeilid() {
  if (isVeilidWasmLoaded) {
    return;
  }
  if (!loadVeilidWasmPromise) {
    console.log('loading veilid-wasm...');
    loadVeilidWasmPromise = loadVeilidWasm();
  }
  await loadVeilidWasmPromise;
  isVeilidWasmLoaded = true;
  console.log('veilid-wasm loaded!');
}

let IS_VEILID_RUNNING = false;

export function isVeilidRunning() {
  return IS_VEILID_RUNNING;
}

let startVeilidPromise: Promise<typeof veilidClient> | undefined;

/**
 * Idempotent: React's StrictMode runs effects twice in development, and starting a
 * second core would fail with an AlreadyInitialized error.
 */
export function startVeilid() {
  startVeilidPromise ??= startVeilidInner();
  return startVeilidPromise;
}

async function startVeilidInner() {
  const config = getVeilidCoreStartupConfig({
    namespace: `example-bench-${uuidV4()}`,
    password: 'singleton',
  });

  console.log('starting veilid core...');
  await veilidClient.initializeCore(veildCoreInitConfig);

  await veilidClient.startupCore(async (data) => {
    veilidEventEmitter.emit(data.kind, data);
  }, config);

  IS_VEILID_RUNNING = true;
  console.log('veilid core started!');
  console.log('Veilid Version', veilidClient.versionString());

  console.log('attaching to veilid network');
  await veilidClient.attach();

  // attach() only starts attaching. DHT operations are gated on the PublicInternet
  // routing domain being ready, which the default condition waits for.
  await veilidClient.waitForAttachmentCondition();
  console.log('veilid network ready!');

  return veilidClient;
}

veilidEventEmitter.on('Log', (data) => {
  switch (data?.logLevel) {
    case 'Warn':
      console.warn(data.message);
      break;
    case 'Info':
      console.info(data.message);
      break;
    case 'Debug':
      console.log(data.message);
      break;
    default:
      console.log(data.message);
      break;
  }
});

veilidEventEmitter.on('RouteChange', (routeChange) => {
  console.log('ROUTE CHANGE', routeChange);
});

veilidEventEmitter.on('ValueChange', (valueChange) => {
  console.log('VALUE CHANGE', valueChange);
});

export async function stopVeilid() {
  if (ROUTING_CONTEXT_SINGLETON) {
    ROUTING_CONTEXT_SINGLETON.free();
    ROUTING_CONTEXT_SINGLETON = undefined;
  }
  if (isVeilidRunning()) {
    IS_VEILID_RUNNING = false;
    await veilidClient.detach();
    await veilidClient.shutdownCore();
  }
}

let ROUTING_CONTEXT_SINGLETON: VeilidRoutingContext | undefined;
export function getRoutingContext() {
  if (!ROUTING_CONTEXT_SINGLETON) {
    ROUTING_CONTEXT_SINGLETON = VeilidRoutingContext.create();
  }
  return ROUTING_CONTEXT_SINGLETON;
}

/**
 * Everything veilid throws is a VeilidAPIError, which extends Error and carries a
 * `kind` naming the variant. Subclasses such as VeilidAPIErrorTryAgain allow
 * `instanceof` for one specific failure.
 */
export function describeVeilidError(error: unknown): string {
  if (error instanceof VeilidAPIError) {
    return `${error.kind}: ${error.message}`;
  }
  return error instanceof Error ? error.message : String(error);
}
