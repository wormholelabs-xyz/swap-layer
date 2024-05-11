import { Keypair, PublicKey } from "@solana/web3.js";
import * as wormholeSdk from "@certusone/wormhole-sdk";

export const CORE_BRIDGE_PID = new PublicKey("worm2ZoG2kUd4vFXhvjh93UUH596ayRfgQ2MgjNMTth");
export const GUARDIAN_KEY = "cfb12303a19cde580bb4dd771639b0d26bc68353645571a8cff516ab2ee113a0";

export const FEE_UPDATER_KEYPAIR = Keypair.fromSecretKey(
    Buffer.from(
        "rI0Zx3zKrtyTbkR6tjGflafgMUJFoVSOnPikC2FPl1dyHvGqDulylhs8RuGza/GcmplFUU/jqMXBxiPy2RhgMQ==",
        "base64",
    ),
);

export const USDT_MINT_ADDRESS = new PublicKey("Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB");

export const WHIRLPOOL_PROGRAM_ID = new PublicKey("whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc");
export const WHIRLPOOL_USDC_USDT = new PublicKey("4fuUiYxTQ6QCrdSq9ouBYcTM7bqSwYTSyLueGZLTy4T4");

export const REGISTERED_PEERS: { [k in wormholeSdk.ChainName]?: Array<number> } = {
    ethereum: Array.from(Buffer.alloc(32, "50", "hex")),
};
