# Progress Tracker — StepFi-Contracts

Update this file after every completed contract change, fix, or architectural decision. Progress state must reflect the actual deployed and tested state — not the intended state.

---

## Current Phase

**Phase 1 — Contract Infrastructure & Core Fixes**

## Current Goal

`LoanType` and per-installment tracking are in. Next: per-loan grace period (Next Up #4), then vouching contract.

---

## Completed

### Parameters-Contract Multisig Hardening (stale approvals, threshold reduction, admin bypass)
- **Problem:** three flaws made the governance multisig "theater": (1) `configure_multisig()` required only the single admin key's auth, so one compromised key could silently install any signer set; (2) proposals stored only `approvals: Vec<Address>`, so a signer removed by an executed `UpdateSigners` kept their approval counted by `execute()` (len >= threshold only); (3) `execute()` validated against the current config with no escalation guard, so a 2-of-3 set could propose `UpdateSigners` to 2-of-2 (or admit a colluder) with only old-threshold approvals.
- **Eligible-signer snapshot:** `Proposal` now records `eligible_signers: Vec<Address>` (snapshot of the current signer set at propose time). `approve()` requires the signer to be a current member AND in the snapshot (rejects signers added after the proposal — `NotEligibleSigner = 23`); `execute()` validates every approver against the snapshot AND current membership and panics `StaleApproval = 21` if any approver was removed since — so removed signers' approvals are never counted (acceptance criterion 1).
- **Elevated quorum for signer-set changes:** `UpdateSigners` requires `threshold + 1` approvals, capped at full unanimity of the current signer set (2-of-3 → 3, 3-of-3 → 3, 4-of-7 → 5), so the quorum is always achievable. Cheapening to a weaker threshold or admitting a colluder now needs more than the old threshold (`ElevatedQuorumNotMet = 22`). Chosen over bare `threshold + 1` because that would brick 3-of-3 and up.
- **Two-step `configure_multisig`:** `configure_multisig(admin, signers, threshold)` now only *stages* a pending config and emits a prominent `MSIGPEND` event carrying the full signer list; `confirm_multisig(admin)` activates it with a second, separate admin signature and emits the existing `MSCONFIG` event. A single admin key can no longer atomically swap the signer set; the pending change is observable on-chain before activation. New errors `MultisigPendingExists = 18`, `MultisigNotPending = 19`.
- **In-flight invalidation:** when `do_update_signers` executes, every in-flight (not executed/invalidated) proposal whose action is `UpdateSigners` is marked `invalidated` and emits `PROPINVL`; invalidated proposals cannot be approved or executed (`ProposalInvalidated = 20`). Non-signer-targeting proposals are left alone — they re-validate at execute time via the snapshot, so e.g. a parameters proposal whose approvers are all still members still executes (only stale approvals die).
- **Tests (17 new, 37 total in parameters-contract, all green):** stale-approval exploit reproduced end-to-end (approve params proposal, remove the approver via `UpdateSigners`, then `execute` fails `StaleApproval` and params stay unchanged — pre-fix this executed), self-serving threshold reduction rejected (`ElevatedQuorumNotMet`), colluder admission rejected, unanimity path succeeds, two-step configure (pending not active, second request rejected, confirm without pending rejected, non-admin guards on both steps), in-flight signer proposal invalidation (approve+execute both blocked), unrelated signer change leaves valid proposals executable, full rotation kills old proposals, added-signer approval rejected (`NotEligibleSigner`), removed-signer approval rejected, and `MSIGPEND`/`MSCONFIG`/`PROPINVL` event assertions. Creditline integration test updated for the new `configure_multisig(admin, …)` + `confirm_multisig` flow.
- **Storage note:** `Proposal` gained two fields (`eligible_signers`, `invalidated`), which changes its persistent XDR layout. parameters-contract is deployed on testnet; old in-flight proposals (if any) would fail to decode, so deployment of this change requires an upgrade and re-proposal of any in-flight items (see Open Questions).
- **Verification:** `cargo test --workspace` → all 6 crates green (373 tests, 0 failed; parameters-contract 37); `cargo clippy -p parameters-contract -- -D warnings` clean for all changed code (only pre-existing dead-code lints in the untouched `safe_math.rs` remain, same as other contracts); `wasm32-unknown-unknown --release` build succeeds.
- **Files:** `contracts/parameters-contract/src/lib.rs`, `storage.rs`, `types.rs`, `errors.rs`, `events.rs`, `tests.rs`, plus `contracts/creditline-contract/src/tests.rs` (call-site update)

### Parameters Multisig Hardening — Audit Review Response (PR #95)
- **Reviewer gaps addressed:**
  - **Unbounded invalidation scan fixed.** `invalidate_signer_proposals()` previously iterated every proposal id ever created (`0..proposal_count`) on each signer-set change — the scan cost grows with proposal history and would eventually exceed the Soroban instruction budget. The contract now keeps an instance-storage in-flight index (`PROPSACT`) of proposal ids that are not yet executed/invalidated; the scan iterates only that index and prunes executed/invalidated/expired/missing ids in the same pass. `propose()` adds to the index, `execute()` removes from it, `clear_proposals()` empties it.
  - **Spurious self-invalidation fixed.** `execute()` now persists `executed = true` before dispatching the action, so the invalidation scan that runs inside `do_update_signers` skips the very proposal being executed instead of flagging it invalidated and emitting a misleading `PROPINVL` for it (previously overwritten silently). Regression test `test_executing_signer_proposal_is_not_self_invalidated` asserts no `PROPINVL` references the executed proposal.
  - **Two-step config cancellation added.** New admin-only `cancel_pending_multisig(admin)` (error `MultisigNotPending = 19` when nothing staged, new `MSIGCNCL` event) lets the admin back out of a mistakenly staged configuration instead of being forced to confirm it — completing the two-step flow's replacement/cancellation path requested by issue #84.
  - **Concrete migration sequence for the Proposal XDR layout break.** The `Proposal` struct change renders pre-upgrade in-flight proposals undecodable. New admin-only migration helper `clear_proposals(admin)` removes every stored proposal key *without decoding* (removal never decodes) and empties the in-flight index. Documented upgrade sequence: (1) `upgrade()` to the new WASM; (2) run `clear_proposals` once (admin); (3) re-propose any in-flight governance items; (4) resume multisig activity. Until step 2 runs, any read of an old-layout proposal would panic — so the sequence must complete before `approve`/`execute`/signer changes.
  - **Test coverage closed for issue test items H and J.** Added `test_signer_set_expansion_requires_elevated_quorum` (2-of-3 cannot expand to 4 signers with only 2 approvals) and `test_fully_approved_signer_proposal_invalidated_by_signers_change` (fully approved but not yet executed signer proposal is invalidated by a competing signer change).
- **Verification:** `cargo test -p parameters-contract` → 45 passed (37 previous + 8 new), 0 failed; `cargo test --workspace` → 381 passed, 0 failed; `cargo clippy -p parameters-contract --tests -- -D warnings` reports only the pre-existing `safe_math.rs` dead-code lints (unchanged).
- **Files:** `contracts/parameters-contract/src/lib.rs`, `storage.rs`, `events.rs`, `tests.rs`, `context/progress-tracker.md`

### Issue #58 — Principal-Interest-Fee Repayment Waterfall
- Added `RepaymentAllocation` struct and `apply_waterfall()` helper in `lib.rs` with correct priority: late fees → interest → service fee → principal
- Fixed `repay_loan()` to use the corrected waterfall order (was principal-first, now late-fees-first)
- Rewrote `repay_installment()` to: accrue late fees, apply waterfall, transfer tokens, call pool's `receive_repayment()`, return guarantee on full repayment, update reputation
- Each `*_outstanding` bucket decremented correctly per payment
- `remaining_balance == sum(all outstanding buckets)` invariant asserted in tests
- Added 8 new tests: waterfall order verification, bucket invariant for both repay_loan and repay_installment, partial/full bucket decrementation, full repayment via repay_installment, active debt tracking
- Updated `test_repay_loan_auto_accrues_late_fees` for new waterfall behavior (late fees paid first, not last)

### Issue #7 — Vendor Approval Flow
- Added `VendorStatus` enum (`Pending`, `Approved`, `Suspended`, `Rejected`) to `types.rs`
- Replaced `active: bool` with `status: VendorStatus` in `VendorInfo`
- `register_vendor()` now sets `status = VendorStatus::Pending` instead of immediately active
- Added `approve_vendor()` (admin-only, requires Pending → Approved)
- Added `suspend_vendor()` (admin-only, any status → Suspended)
- `is_active()` returns `true` only for `Approved` vendors — automatically prevents unapproved/suspended vendors from receiving loans in `creditline-contract`
- Legacy functions (`activate_vendor`, `deactivate_vendor`, `set_vendor_status`) updated to map to new enum
- New error: `VendorNotPending = 10` in `vendor-registry-contract`
- Updated `publish_vendor_status` event to emit `VendorStatus` instead of `bool`
- All vendor-registry tests updated; 7 new tests added (approval flow, non-pending rejection, suspension, re-approval, reentrancy guards for approve/suspend)
- Creditline tests updated to approve vendors after registration
- No changes needed to `creditline-contract/src/lib.rs` — `validate_vendor()` already uses `is_active()` which now checks for `Approved`

### Workspace Cleanup
- Removed dead code: `lp-contract` (superseded by `liquidity-pool-contract`)
- Removed empty placeholder: `adapter-trustless-contract`
- Updated `Cargo.toml` workspace members to reflect 5 active contracts
- Removed `[profile]` sections from individual contract `Cargo.toml` files (profiles belong in workspace root only)

### Renaming
- Renamed `merchant-registry-contract` → `vendor-registry-contract`
- Updated all Rust source references: `merchant_registry_contract` → `vendor_registry_contract`
- Updated all struct names: `MerchantRegistry*` → `VendorRegistry*`
- Updated `Cargo.toml` dependency paths in `creditline-contract`

### Critical Fixes
- Added TTL constants (`PERSISTENT_TTL_THRESHOLD`, `PERSISTENT_TTL_EXTEND_TO`) to `creditline-contract/src/storage.rs`
- Added `upgrade()` function to all 5 contracts: reputation, creditline, liquidity-pool, vendor-registry, parameters
- All 5 contracts build cleanly: `cargo build` passes with zero errors (3 minor unused constant warnings — acceptable)
 - Added numeric `VERSION` instance key, `get_version()` API, and `CONTRACTUPGRADED` event across contracts; added unit tests asserting admin gating and version bump on upgrade

### Deployment
- Created `scripts/deploy-testnet.sh` — full deployment script covering all 5 contracts in correct dependency order
- Script outputs contract IDs and saves to `.env.contracts`

### Documentation
- `README.md` fully rewritten as StepFi-Contracts 

### LoanType + Per-Installment Tracking (creditline-contract)
- Added `LoanType` enum (`Standard`, `LearnerInstallment`) to `types.rs`
- Added `paid: bool` and `paid_at: u64` to `RepaymentInstallment`
- Added `loan_type: LoanType` to `Loan`
- Threaded `loan_type` through `create_loan`, `request_loan`, `build_loan`
- New `repay_installment(borrower, loan_id, installment_index, amount) -> i128`: bounds-checks index, rejects already-paid slots, decrements `remaining_balance`, marks `paid`/`paid_at`, persists, emits `INSTPAID`
- New errors: `InvalidInstallmentIndex = 23`, `InstallmentAlreadyPaid = 24`
- New event: `INSTPAID` via `emit_installment_paid`
- All 93 existing tests updated and passing; 0 failing

### repay_installment Unit Tests
- Added `setup_loan_with_schedule` helper that creates a loan with N equal installments
- `test_repay_installment_happy_path`: pays installment 0, verifies `paid`/`paid_at`, balance decremented, second installment untouched
- `test_repay_installment_double_pay_rejected`: asserts `InstallmentAlreadyPaid` (#24) on second payment of same slot
- `test_repay_installment_out_of_bounds`: asserts `InvalidInstallmentIndex` (#23) for index >= schedule length
- `test_repay_installment_non_borrower_rejected`: asserts `UnauthorizedRepayer` (#14) when caller is not the borrower
- `test_repay_installment_zero_amount_rejected`: asserts `InvalidRepaymentAmount` (#13) for zero payment
- Total tests: 98 (93 existing + 5 new) — all passing

### Issue #6 — Typed Storage Errors
- Removed all `.expect(...)` and bare `.unwrap()` matches from `contracts/*/src/storage.rs`
- Converted storage getters/readers to typed `Result<T, ContractError>` paths while preserving intentional zero/false/default semantics
- Added TTL extension after persistent writes for creditline user indexes/active debt, liquidity-pool LP shares, and vendor-registry vendor/count records
- Added missing `NotInitialized` variants to creditline, parameters, and reputation errors without renumbering existing variants
- Added before-initialize regression coverage across all 5 active contracts using generated `try_*` clients
- Verified with `cargo check --offline`, `cargo build --offline`, `cargo test --offline`, and `cargo clippy --offline -- -D warnings` — 230 passed, 0 failed, 4 ignored

### Issue #4 — Mentor Vouching Contract
- Added `vouching-contract` workspace member with `vouch`, `revoke_vouch`, `get_vouches`, `set_mentor`, and initialization APIs
- Stored verified mentors and mentor/learner vouch records in persistent storage with TTL extension after every persistent write
- Added learner-to-mentor indexing so `get_vouches(learner)` avoids global scans
- Added `MENTORVOUCHED`, `VOUCHREVOKED`, and `MENTORVERIFIED` event helpers using short Soroban event symbols
- Added reputation `add_boost` and `remove_boost` updater-gated APIs for vouching cross-contract calls
- Added mock reputation cross-contract tests covering mentor verification, vouching, revocation, duplicate rejection, unverified mentor rejection, admin rejection, and event emission
- Added `get_version()` and `upgrade()` functions following the same pattern as all other contracts
- Added `CONTRACTUPGRADED` event emission on upgrade
- Added version and upgrade unit tests
- Removed unused `safe_math` functions (replaced with comment placeholder for future use)

### Issue #87 — Vouch expiry & boost-accounting clamp
- **Problem:** Vouches never expired on-chain (no `expire_vouch`, no enforcement), so a mentor's boost inflated a learner's reputation permanently. Secondary: `revoke_vouch()` subtracted the historical `boost_amount` while `vouch()` re-minted with the *current* config boost, and `remove_boost` could push a learner's score below their pre-vouch baseline when combined with penalties or a mid-life boost-config change.
- **Fix (vouching-contract, not yet deployed — `PENDING_DEPLOYMENT`, so the `VouchRecord` layout change is safe):**
  - Added `VOUCH_DURATION: u64 = 2_592_000` (30 days) in `types.rs`.
  - Added permissionless `expire_vouch(mentor, learner)` (no `require_auth`, mirrors creditline's `apply_late_fees`): validates `record.ts + VOUCH_DURATION < now` (else `VouchNotExpired = 13`), deactivates the record, and removes the boost (clamped). Idempotent — re-expiring an already-inactive record is a no-op.
  - `get_vouches()` now returns expired records with `active = false` regardless of stored state, so readers never see a stale active boost.
  - Added `baseline: u32` to `VouchRecord` (learner score before this vouch's boost) and a `get_reputation_score()` cross-contract read. New `remove_reputation_boost_clamped()` removes only `min(boost_amount, current_score - baseline)`, so removal can never drop the learner below their pre-vouch baseline even after penalties/config changes.
  - `revoke_vouch()` now uses the clamped removal instead of subtracting the raw `boost_amount`.
  - New event `VOUCHEXPIRED`; new error `VouchNotExpired = 13`.
- **Tests (`tests.rs`):** added `decrease_score` to the mock reputation contract and 6 new tests — permissionless expiry removes boost, expiry-before-TTL rejected (`VouchNotExpired`), idempotent expiry, revoke-after-expiry fails cleanly (`VouchNotActive`), `get_vouches` marks expired inactive without an explicit expire, and boost removal clamped to baseline (pre-existing reputation + penalty scenario).
- **Verification (initial):** `cargo test -p vouching-contract` → 24 passed, 0 failed.

### Issue #87 — Revision (automated audit follow-up)
- **Audit findings addressed:**
  - **Order-dependent drift on multiple overlapping vouches.** The initial per-record `baseline` (captured at each vouch's own time) made `remove_reputation_boost_clamped` non-additive: with two concurrent vouches across a boost-config change (boost 10 then 5), expiring the older/larger vouch first clamped the successor's removal to zero, permanently leaving a residual boost. Replaced per-record baseline with a **shared learner baseline** (score before ANY active vouch, captured on the first vouch and shared by all overlapping vouches) and an **aggregate `total_vouch_boost`** per learner. Removal is now `min(boost_amount, current - baseline)`, which is exact and order-independent.
  - **Missing required test:** added `test_boost_config_change_exact_older_larger_expired_first` and `test_boost_config_change_exact_newer_smaller_expired_first` covering issue acceptance criterion 2 (boost-config change between vouch and expiry keeps accounting exact), in both expiry orderings.
  - **`expire_vouch` idempotency / doc mismatch:** the TTL check previously preceded the active check, so calling within TTL on a revoked (inactive) record panicked `VouchNotExpired` instead of no-op. Reordered so an already-inactive record returns immediately (no-op) regardless of TTL; the TTL check only applies to still-active records. Added `test_expire_vouch_on_revoked_record_is_noop_within_ttl`.
  - **`storage.rs` left unchanged:** the aggregate baseline/total helpers now live in `storage.rs` (previously the TTL logic was only in `lib.rs`), satisfying the original files-to-touch note.
- **Storage changes:** `DataKey` gained `LearnerBaseline(Address)` and `LearnerTotalBoost(Address)`; `VouchRecord.baseline` field removed (no longer needed). `storage.rs` gained `get/set/clear_learner_baseline` and `get/set_total_vouch_boost`.
- **Verification (revised):** `cargo test -p vouching-contract` → 27 passed, 0 failed (3 new audit-follow-up tests).

---

## In Progress

### Issue #59 — Socialize Default Losses to Pool Share Price
- Added `absorb_loss(creditline, principal_shortfall)` entrypoint to `liquidity-pool-contract` restricted to the registered CreditLine
- Reduces both `locked_liquidity` and `total_liquidity` by the unrecovered principal, with independent caps to prevent negative accounting
- Added `LQLOSS` event (`emit_loss_absorbed`) to liquidity-pool events
- Updated `mark_defaulted()` to compute `principal_shortfall = principal_outstanding - guarantee_amount` and call `absorb_loss` after `receive_guarantee`
- Added 8 LP pool tests: basic absorption, share price drop, capping, partial repayment flow, unauthorized caller rejection, zero/negative amount rejection, event emission
- Added 4 creditline tests: absorb_loss called on default, zero-shortfall skip, partial repayment shortfall, end-to-end share price impact with real LP contract
- Updated MockLiquidityPool and MockLiquidityPoolEmpty with `absorb_loss` stub for test compatibility
- Fixed: `IntoVal` import moved before first usage in `test_mark_defaulted_loss_absorption_share_price_impact`

## Recently Fixed

### Security: Unauthorized `distribute_interest` / `accumulate_interest` (SC-17)
- **Problem:** `distribute_interest()` and `accumulate_interest()` were public mutating functions with no `require_auth()` and no caller restriction. Any funded account could call them with an arbitrary amount, draining the pool's token balance to treasury and merchant fund addresses and inflating the share price so the caller could redeem LP shares for more than deposited.
- **Fix:** Changed both function signatures to accept `creditline: Address` as the first parameter. Added `creditline.require_auth()` as the literal first line and `Self::require_creditline(&env, &creditline)` as the second, matching `receive_repayment()` exactly. Both functions now pull `interest_amount` tokens into the pool via `token_client.transfer()` before any accounting change. Updated doc comments to remove the admin edge-case mention.
- **Internal call site preserved:** `receive_repayment()` still calls `distribute_interest_internal()` directly — it has already pulled funds and validated the caller, so it must not go through the newly guarded public wrappers.
- **Pre-existing bug fixed:** `calculate_withdrawal()` now returns 0 when the pool has no shares, fixing two pre-existing test failures.
- **Files:** `contracts/liquidity-pool-contract/src/lib.rs`, `contracts/liquidity-pool-contract/src/tests.rs`
- **New tests:** 8 new tests (unauthorized caller rejection for both functions, token pull + distribution for both, receive_repayment no-regression, receive_repayment single-distribution regression)
- **Verification:** `cargo check`, `cargo test -p liquidity-pool-contract` (86 passed, 0 failed), `cargo clippy -p liquidity-pool-contract -- -D warnings` (0 warnings)

### Issue #7 — Follow-up: Missing `approve_vendor` in `RealIntegrationCtx::register_vendor`
- Discovered second `register_vendor` helper in `RealIntegrationCtx` (integration test struct, ~line 2390) that only called `register_vendor` without `approve_vendor`
- All integration tests using `RealIntegrationCtx` created loans with `Pending` vendors → `validate_vendor` → `is_active` returned `false` → `VendorNotActive` (#3)
- Added `self.vendor_registry.approve_vendor(&self.admin, vendor)` after registration in `RealIntegrationCtx::register_vendor`

---

## Next Up (In Order)

1. **Learner grace period** — Make `grace_period_seconds` per-loan (not just global via parameters)
2. **Reputation rules** — Update `creditline-contract` to call different reputation adjustments for `LoanType::LearnerInstallment`
3. **Testnet deployment** ✅ — All 5 contracts deployed and initialized (see Contract Deployment Status below); IDs in StepFi-API env config
4. **End-to-end validation** — Verify loan lifecycle on testnet via Stellar CLI

---

## Open Questions

- **Proposal layout change on deployed testnet** — ✅ Resolved: the hardened `Proposal` struct changed its persistent XDR layout, so the deployed parameters-contract must be upgraded and any pre-upgrade in-flight proposals cleared first via the new admin-only `clear_proposals()` migration helper (removes keys without decoding), then re-proposed. See the review-response entry above for the full sequence.
- What token is used for loans — native XLM or a USDC anchor? (Affects token contract address in `initialize()`)
- What is the correct `grace_period_seconds` for learner installment loans? (Longer than standard BNPL — possibly 7-14 days per installment)
- Should sponsor pool deposits go through `liquidity-pool-contract` or a new `sponsor-pool-contract`?

---

## Architecture Decisions

- **Elevated quorum for signer-set changes** — `threshold + 1` capped at unanimity of the *current* signer set, chosen over bare `threshold + 1` (which is unachievable for 3-of-3) and over flat unanimity (which is heavy for large committees). Documented in the multisig hardening entry above.
- **6 contracts, not 5** — `vouching-contract` added for mentor-based reputation boosting. `lp-contract` was dead code, removed. `liquidity-pool-contract` is the canonical LP implementation.
- **Vendor over Merchant** — Renamed to reflect StepFi's learning-focused domain.
- **TTL approach** — Using 60-day threshold / 120-day extension constants. Off-chain indexer is responsible for bumping TTL on active loan entries.
- **Upgrade pattern** — All contracts have `upgrade()` gated by admin `require_auth()`. Admin address is set at `initialize()` and transferable via `set_admin()`.
- **Loan sharding** — 32 shards (`loan_id % 32`) in creditline-contract to distribute persistent storage keys and avoid hot-key contention.
- **Reentrancy** — Boolean `LOCKED` flag in instance storage. Cheaper than mutex, sufficient for Soroban's single-threaded execution model.

---

## Contract Deployment Status

All 6 contracts are deployed, initialized, and active on Stellar testnet
(matches `README.md` and StepFi-Web `VERIFICATION.md`). These are the IDs
live clients (StepFi-Web `constants/config.ts`) point at:

| Contract | Testnet Deployed | Contract ID | Last Deployed |
|---|---|---|---|
| `reputation-contract` | ✅ Yes | `CC3BO57ZRJGA63QJBIBSOMI25Z3X2I5CYTARYRAUXUAILX6L3OWBL5SB` | 2026-05-11 |
| `parameters-contract` | ✅ Yes | `CCAE72SKYX55C5L56DBEFIMFVXRUIJY6JYLBREHEWRFNOW7AX5NBIJ5B` | 2026-05-11 |
| `vendor-registry-contract` | ✅ Yes | `CCZ6T6NYCDNI26VGTPXKKWQDR7JCIZZ24LCEG4MMYHZJAG6BPWIVAU2L` | 2026-05-11 |
| `liquidity-pool-contract` | ✅ Yes | `CACKE7ML2BTOAGQTAAW5NEARHCFX4PXXKGEO6GMU6NHFBVYQFZRJS2BT` | 2026-05-11 |
| `vouching-contract` | ⏳ Pending | `PENDING_DEPLOYMENT` | — |
| `creditline-contract` | ✅ Yes | `CAQDHYG3TALPNXG466SZUMJEPOI7VYV732LPFF3GHE4ASPBCNMIQBS3X` | 2026-05-12 (redeployed) |

Deployer: `GCOYDYSEHRCFWGXUCMPSQ3ODEY2LGMBSVKKCOFH4NRIK4DEEDSETH7BF`

> ✅ Resolved 2026-07-17: The 2026-05-11 set above (deployer `GCOYDYSE...H7BF`,
> = `stepfi-deployer` on the maintainer machine) is confirmed **live and correct**.
> A reproducible `stellar contract build` of current `main` (multi-sig admin
> included, commit `44a8c00`) produces bytecode whose SHA256 hashes match the
> on-chain wasm of all five contracts above exactly — the contracts were created
> in May and upgraded in place via their `upgrade()` functions as the source
> evolved. All clients (web, landing, docs, live API `.well-known/stellar.toml`)
> reference this set.
>
> The **second** deployment recorded on 2026-06-23 (deployer `GDL63O...Q4LH`) is
> identified as an **orphaned experimental deploy**: its key is not recognized on
> the maintainer machine, appears in no deploy script/env/shell-history, its
> account was funded by testnet Friendbot immediately before deploy (no memo), and
> its on-chain wasm matches no build of any branch in this repo. No client ever
> referenced it. It is now recorded under `orphanedDeployment` in
> `deployed-testnet.json` and marked DO NOT USE. Investigation into the origin of
> the `GDL63O...` key is **ongoing**.

> Update this table after running `scripts/deploy-testnet.sh`

---

## Session Notes

- Always run `cargo build` after any contract change before committing.
- Always run `cargo test` before marking any contract feature complete.
- Never modify storage key structures of a contract that has been deployed — it breaks existing data. Use a migration pattern or deploy a new contract.
- The `creditline-contract` depends on all other contracts — it must be initialized last.
- Do not add new workspace members to `Cargo.toml` without creating the full contract file structure first.
