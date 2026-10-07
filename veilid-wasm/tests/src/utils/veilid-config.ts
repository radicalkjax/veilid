import { VeilidWASMConfig, veilidClient, } from 'veilid-wasm';

export const LOG_LEVEL = (process.env.LOG_LEVEL == "info" || process.env.INFO == "1" || process.env.INFO == "true") ? "Info" : (process.env.LOG_LEVEL == "debug" || process.env.DEBUG == "1" || process.env.DEBUG == "true") ? "Debug" : "Off";
export const DEBUG = process.env.LOG_LEVEL == "debug" || process.env.DEBUG == "1" || process.env.DEBUG == "true";
export const STRESS = process.env.STRESS == "1" || process.env.STRESS == "true";

export const veilidCoreInitConfig: VeilidWASMConfig = {
  logging: {
    api: {
      enabled: LOG_LEVEL === "Off",
      level: "Info",
    },
    performance: {
      enabled: LOG_LEVEL !== "Off",
      level: LOG_LEVEL,
      console: {
        enabled: true,
      },
      directives: LOG_LEVEL == "Debug" ? ["veilid_api=debug", "dht=debug", "fanout=debug", "network_result=debug" /*, "rpc_message=debug"*/] : [""],
    },
  },
};

export const veilidCoreStartupConfig = (() => {
  // console.log("starting config")
  const defaultConfig = veilidClient.defaultConfig();
  defaultConfig.programName = 'veilid-wasm-test';
  if (process.env.NETWORK_KEY) {
    defaultConfig.network.networkKeyPassword = process.env.NETWORK_KEY;
  }
  if (process.env.BOOTSTRAP_KEYS) {
    defaultConfig.network.routingTable.bootstrapKeys = process.env.BOOTSTRAP_KEYS.split(',')
  }
  if (process.env.BOOTSTRAP) {
    defaultConfig.network.routingTable.bootstrap = process.env.BOOTSTRAP.split(',');
  }
  // Ensure we are starting from scratch
  defaultConfig.tableStore.delete = true;
  defaultConfig.protectedStore.delete = true;
  defaultConfig.blockStore.delete = true;

  // Tests should not participate in heavy server operations
  defaultConfig.capabilities.disable = [veilidClient.VEILID_CAPABILITY_DHT, veilidClient.VEILID_CAPABILITY_ROUTE];
  // console.log("ending config")

  return defaultConfig;
})(); 
