use soroban_sdk::{symbol_short, Address, Env, Symbol};

use crate::types::{MultisigConfig, ProtocolParameters};

const PARAMS_UPDATED: Symbol = symbol_short!("PARMUPDT");
const ADMIN_UPDATED: Symbol = symbol_short!("PARMADMN");
const MS_CONFIGURED: Symbol = symbol_short!("MSCONFIG");
const MSIG_PENDING: Symbol = symbol_short!("MSIGPEND");
const MSIG_CANCELLED: Symbol = symbol_short!("MSIGCNCL");
const PROP_CREATED: Symbol = symbol_short!("PROPNEW");
const PROP_APPROVED: Symbol = symbol_short!("PROPAPPR");
const PROP_EXECUTED: Symbol = symbol_short!("PROPEXEC");
const PROP_INVALIDATED: Symbol = symbol_short!("PROPINVL");

pub fn emit_parameters_updated(env: &Env, admin: &Address, params: &ProtocolParameters) {
    env.events().publish(
        (PARAMS_UPDATED, admin),
        (
            params.min_guarantee_percent,
            params.min_reputation_threshold,
            params.full_repayment_reward,
            params.default_penalty,
            params.large_loan_threshold,
            params.large_loan_default_penalty,
            params.base_interest_bps,
        ),
    );
}

pub fn emit_admin_updated(env: &Env, old_admin: &Address, new_admin: &Address) {
    env.events()
        .publish((ADMIN_UPDATED, old_admin), new_admin.clone());
}

pub fn emit_contract_upgraded(env: &Env, old_version: u32, new_version: u32) {
    env.events().publish(
        (soroban_sdk::Symbol::new(env, "CONTRACTUPGRADED"),),
        (old_version, new_version, env.ledger().timestamp()),
    );
}

pub fn emit_multisig_configured(env: &Env, threshold: u32, num_signers: u32) {
    env.events()
        .publish((MS_CONFIGURED,), (threshold, num_signers));
}

/// Prominent event emitted when the admin *requests* a multisig configuration
/// (two-step: request → confirm). Carries the full signer list so the pending
/// change is observable on-chain before it becomes active.
pub fn emit_multisig_pending(env: &Env, admin: &Address, config: &MultisigConfig) {
    env.events().publish(
        (MSIG_PENDING, admin),
        (config.threshold, config.signers.clone()),
    );
}

/// Emitted when an in-flight proposal that targets the signer set is
/// invalidated because the signer set changed.
pub fn emit_proposal_invalidated(env: &Env, id: u64) {
    env.events().publish((PROP_INVALIDATED,), id);
}

/// Emitted when the admin cancels a staged (pending) multisig configuration
/// without activating it.
pub fn emit_multisig_pending_cancelled(env: &Env, admin: &Address) {
    env.events().publish((MSIG_CANCELLED, admin), ());
}

pub fn emit_proposal_created(env: &Env, id: u64, proposer: &Address) {
    env.events().publish((PROP_CREATED, proposer), id);
}

pub fn emit_proposal_approved(env: &Env, id: u64, signer: &Address, approvals: u32) {
    env.events().publish((PROP_APPROVED, signer), (id, approvals));
}

pub fn emit_proposal_executed(env: &Env, id: u64) {
    env.events().publish((PROP_EXECUTED,), id);
}
