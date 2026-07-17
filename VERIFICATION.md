# Contract Verification

This document provides the SHA256 hashes of the deployed Soroban smart contract
bytecode for the StepFi protocol on Stellar **testnet**. Use these hashes to
verify that the on-chain bytecode matches the source published in this
repository.

The hashes below are the **on-chain wasm hashes** of the live contracts
(deployer `GCOYDYSEHRCFWGXUCMPSQ3ODEY2LGMBSVKKCOFH4NRIK4DEEDSETH7BF`). They were
last verified against the ledger and against a reproducible build of `main` on
**2026-07-17**. See [`contracts/deployed-testnet.json`](./contracts/deployed-testnet.json)
for the full deployment record.

> ⚠️ A second, unrelated deployment made on 2026-06-23 from an unrecognized key
> (`GDL63O...Q4LH`) is **not** part of StepFi and its bytecode matches no source
> in this repository. Those contract IDs are recorded as `orphanedDeployment` in
> `deployed-testnet.json` and must not be used. The IDs below are the only valid
> ones.

## Contract Hashes (Stellar Testnet)

| Contract | Contract ID | SHA256 Hash (on-chain wasm) |
| :--- | :--- | :--- |
| **Creditline** | `CAQDHYG3TALPNXG466SZUMJEPOI7VYV732LPFF3GHE4ASPBCNMIQBS3X` | `392ad2562e8836a2836695bb4ed32973bde100b243b5f695ddf2698464541c9e` |
| **Reputation** | `CC3BO57ZRJGA63QJBIBSOMI25Z3X2I5CYTARYRAUXUAILX6L3OWBL5SB` | `548ad3c1e0bca85a7adccb883879ed02e6bf93970d8af06ac8506d487a115da4` |
| **Liquidity Pool** | `CACKE7ML2BTOAGQTAAW5NEARHCFX4PXXKGEO6GMU6NHFBVYQFZRJS2BT` | `5cccbfb7dd2a723110fb299c84636b25d7a437c07092a943b5e4d76f141adf38` |
| **Vendor Registry** | `CCZ6T6NYCDNI26VGTPXKKWQDR7JCIZZ24LCEG4MMYHZJAG6BPWIVAU2L` | `b973d07391e2cd4834370ba596873a3cd34dc7e39c7f059eaca321f597d9ada2` |
| **Parameters** | `CCAE72SKYX55C5L56DBEFIMFVXRUIJY6JYLBREHEWRFNOW7AX5NBIJ5B` | `25cd88d4a48b706a59d7eae5c45a592b844048dc3428c24f3a8c7a420057b785` |

## How to Verify

### Method A — against the live chain (authoritative, toolchain-independent)

This downloads the exact bytecode currently running on-chain and hashes it. No
build required, so it cannot be affected by local toolchain differences.

```bash
# Requires the Stellar CLI: https://github.com/stellar/stellar-cli
for ID in \
  CAQDHYG3TALPNXG466SZUMJEPOI7VYV732LPFF3GHE4ASPBCNMIQBS3X \
  CC3BO57ZRJGA63QJBIBSOMI25Z3X2I5CYTARYRAUXUAILX6L3OWBL5SB \
  CACKE7ML2BTOAGQTAAW5NEARHCFX4PXXKGEO6GMU6NHFBVYQFZRJS2BT \
  CCZ6T6NYCDNI26VGTPXKKWQDR7JCIZZ24LCEG4MMYHZJAG6BPWIVAU2L \
  CCAE72SKYX55C5L56DBEFIMFVXRUIJY6JYLBREHEWRFNOW7AX5NBIJ5B ; do
  stellar contract fetch --network testnet --id "$ID" --out-file "$ID.wasm"
  sha256sum "$ID.wasm"
done
```

Compare each hash with the table above.

### Method B — reproducible build from source

```bash
git clone https://github.com/StepFi-app/StepFi-Contracts.git
cd StepFi-Contracts
stellar contract build
stellar contract optimize --wasm target/**/release/<contract>.wasm
sha256sum target/**/release/*.optimized.wasm
```

The **deployed bytecode is the optimized wasm** (`.optimized.wasm`), not the raw
build output — hash the optimized files when comparing. Note that the four
original contracts (reputation, liquidity pool, vendor registry, parameters)
were built and deployed with the `wasm32-unknown-unknown` target, and creditline
with the newer `wasm32v1-none` target; a byte-identical hash depends on using the
same target triple and pinned toolchain (`Cargo.lock` is committed). If Method B
diverges due to toolchain drift, Method A is authoritative.
