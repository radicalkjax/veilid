import { VeilidWASMConfig, veilidClient } from 'veilid-wasm';

// `process` is undefined in worker/service-worker bundles; fall back to an empty env there.
const env: Record<string, string | undefined> =
  typeof process !== 'undefined' && process.env ? process.env : {};

export const LOG_LEVEL =
  env.LOG_LEVEL == 'info' || env.INFO == '1' || env.INFO == 'true'
    ? 'Info'
    : env.LOG_LEVEL == 'debug' || env.DEBUG == '1' || env.DEBUG == 'true'
      ? 'Debug'
      : 'Off';
export const DEBUG = env.LOG_LEVEL == 'debug' || env.DEBUG == '1' || env.DEBUG == 'true';
export const STRESS = env.STRESS == '1' || env.STRESS == 'true';

export const veilidCoreInitConfig: VeilidWASMConfig = {
  logging: {
    api: {
      enabled: LOG_LEVEL === 'Off',
      level: 'Info',
    },
    performance: {
      enabled: LOG_LEVEL !== 'Off',
      level: LOG_LEVEL,
      console: {
        enabled: true,
      },
      directives:
        LOG_LEVEL == 'Debug'
          ? [
              'veilid_api=debug',
              'dht=debug',
              'fanout=debug',
              'network_result=debug' /*, "rpc_message=debug"*/,
            ]
          : [''],
    },
  },
};

export const veilidCoreStartupConfig = () => {
  // console.log("starting config")
  const defaultConfig = veilidClient.defaultConfig();
  defaultConfig.programName = 'veilid-wasm-test';
  if (env.NETWORK_KEY) {
    defaultConfig.network.networkKeyPassword = env.NETWORK_KEY;
  }
  if (env.BOOTSTRAP_KEYS) {
    defaultConfig.network.routingTable.bootstrapKeys = env.BOOTSTRAP_KEYS.split(',');
  }
  if (env.BOOTSTRAP) {
    defaultConfig.network.routingTable.bootstrap = env.BOOTSTRAP.split(',');
  }
  // Ensure we are starting from scratch
  defaultConfig.tableStore.delete = true;
  defaultConfig.protectedStore.delete = true;
  defaultConfig.blockStore.delete = true;

  // Tests should not participate in heavy server operations
  defaultConfig.capabilities.disable = [
    veilidClient.VEILID_CAPABILITY_DHT,
    veilidClient.VEILID_CAPABILITY_ROUTE,
  ];
  // console.log("ending config")

  return defaultConfig;
};
