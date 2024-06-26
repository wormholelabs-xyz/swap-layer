import { tryNativeToUint8Array } from "@certusone/wormhole-sdk";
import {
  GovernanceCommand,
  encodeGovernanceCommandsBatch,
} from "@xlabs/wh-swap-layer-ts-sdk";
import {
  deploySwapLayerImplementation,
  deploySwapLayerProxy,
} from "../helpers/deployments";
import {
  ChainInfo,
  getOperatingChains,
  getSwapLayer,
  getSwapLayerAddress,
  getWhConnectName,
  init,
  loadChains,
  loadFeeConfig,
  loadPrivateKey,
  loadSwapLayerConfiguration,
  writeOutputFiles,
} from "../helpers/env";
import {
  UniversalAddress,
  toNative,
} from "@wormhole-foundation/sdk-definitions";
import { Wormhole } from "@wormhole-foundation/connect-sdk";
import { EvmPlatform } from "@wormhole-foundation/connect-sdk-evm";

const processName = "configureSwapLayer";
init();
const chains = getOperatingChains();

async function run() {
  console.log(`Start ${processName}!`);

  for (const chain of chains) {
    console.log(`Updating configuration for chain ${chain.chainId}...`);

    const swapLayer = getSwapLayer(chain);
    const commands = createSwapLayerConfiguration(chain);
    const encodedCommands = encodeGovernanceCommandsBatch(commands);
    try {
      await swapLayer.batchGovernanceCommands(encodedCommands);
    } catch (e) {
      console.error(e);
      console.log("Failed to update configuration for chain " + chain.chainId);
      process.exit(1);
    }
  }
}

function createSwapLayerConfiguration(
  operatingChain: ChainInfo
): GovernanceCommand[] {
  const configurationOptions = loadSwapLayerConfiguration();
  const feeConfig = loadFeeConfig();
  const allChains = loadChains();
  const output: GovernanceCommand[] = [];

  if (configurationOptions.shouldUpdatePeers) {
    for (const currentChain of allChains) {
      if (operatingChain.chainId == currentChain.chainId) {
        continue; //don't register the same chain on itself
      }

      const network = getWhConnectName(); // Or "Testnet"
      const wh = new Wormhole(network, [EvmPlatform]);

      const swapLayerAddress = getSwapLayerAddress(currentChain);
      const universalShim: UniversalAddress = toNative<"Ethereum">(
        "Ethereum",
        swapLayerAddress
      ).toUniversalAddress();

      const feeParams = feeConfig.find(
        (config) => config.chainId == currentChain.chainId
      );
      if (!feeParams)
        throw new Error(
          "No fee config found for chain " + currentChain.chainId
        );

      console.log(
        "Creating registration for chain:",
        currentChain.chainName,
        "with address:",
        swapLayerAddress,
        "and fee params:",
        feeParams
      );

      const registerCommand: GovernanceCommand = {
        GovernanceCommand: "UpdatePeer",
        peer: {
          chain: currentChain.chainName,
          address: universalShim as any,
        },
        ...feeParams,
      };

      output.push(registerCommand);
    }
  }
  // if (configurationOptions.shouldUpdateAssistant) {
  //   //TODO pack update assistant command w/ newAssistantAddress
  // }
  // if (configurationOptions.shouldSweepTokens) {
  //   //TODO pack sweep tokens command
  // }
  // if (configurationOptions.shouldUpdateFeeRecipient) {
  //   //TODO pack update fee recipient command w/ newFeeRecipient
  // }

  return output;
}

run().then(() => console.log("Done!"));
