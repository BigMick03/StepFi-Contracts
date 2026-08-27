#![no_std]

mod access;
mod errors;
mod events;
mod safe_math;
mod storage;
mod types;

pub use errors::ParametersError;
pub use types::{
    default_parameters, MultisigConfig, Proposal, ProposalAction, ProtocolParameters,
};

use soroban_sdk::{contract, contractimpl, panic_with_error, Address, BytesN, Env, Vec};

const PROPOSAL_TTL_SECONDS: u64 = 604_800;

#[contract]
pub struct ParametersContract;

#[contractimpl]
impl ParametersContract {
    pub fn initialize(env: Env, admin: Address, params: ProtocolParameters) {
        if storage::has_admin(&env) {
            panic_with_error!(&env, ParametersError::AlreadyInitialized);
        }

        Self::validate_parameters(&env, &params);
        admin.require_auth();

        storage::set_admin(&env, &admin);
        storage::set_parameters(&env, &params);
        events::emit_parameters_updated(&env, &admin, &params);
    }

    pub fn initialize_defaults(env: Env, admin: Address) {
        Self::initialize(env, admin, default_parameters());
    }

    /// Step 1 of 2 for configuring the multisig (admin only). Stages a pending
    /// configuration and emits a prominent event; the config only becomes
    /// active after the admin confirms with a second, separate signature via
    /// `confirm_multisig`. This prevents a single admin key from silently
    /// swapping the signer set in one transaction.
    pub fn configure_multisig(env: Env, admin: Address, signers: Vec<Address>, threshold: u32) {
        admin.require_auth();
        access::require_admin(&env, &admin);

        if storage::has_multisig(&env) {
            panic_with_error!(&env, ParametersError::MultisigAlreadyConfigured);
        }
        if storage::has_pending_multisig(&env) {
            panic_with_error!(&env, ParametersError::MultisigPendingExists);
        }

        let config = MultisigConfig { signers, threshold };
        Self::validate_multisig_config(&env, &config);

        Self::enter_non_reentrant(&env);
        storage::set_pending_multisig(&env, &config);
        events::emit_multisig_pending(&env, &admin, &config);
        Self::exit_non_reentrant(&env);
    }

    /// Step 2 of 2 for configuring the multisig (admin only). Requires a fresh
    /// admin signature (separate transaction from `configure_multisig`) and
    /// activates the pending configuration.
    pub fn confirm_multisig(env: Env, admin: Address) {
        admin.require_auth();
        access::require_admin(&env, &admin);

        if storage::has_multisig(&env) {
            panic_with_error!(&env, ParametersError::MultisigAlreadyConfigured);
        }
        let config = storage::get_pending_multisig(&env)
            .unwrap_or_else(|err| panic_with_error!(&env, err));
        Self::validate_multisig_config(&env, &config);

        Self::enter_non_reentrant(&env);
        storage::set_multisig(&env, &config);
        storage::clear_pending_multisig(&env);
        events::emit_multisig_configured(&env, config.threshold, config.signers.len());
        Self::exit_non_reentrant(&env);
    }

    pub fn get_multisig(env: Env) -> Result<MultisigConfig, ParametersError> {
        storage::get_multisig(&env)
    }

    /// Cancels a staged (pending) multisig configuration without activating it
    /// (admin only). Lets the admin back out of a mistakenly staged request
    /// instead of being forced to confirm it; a new request can then be staged.
    pub fn cancel_pending_multisig(env: Env, admin: Address) {
        admin.require_auth();
        access::require_admin(&env, &admin);

        if !storage::has_pending_multisig(&env) {
            panic_with_error!(&env, ParametersError::MultisigNotPending);
        }
        Self::enter_non_reentrant(&env);
        storage::clear_pending_multisig(&env);
        events::emit_multisig_pending_cancelled(&env, &admin);
        Self::exit_non_reentrant(&env);
    }

    /// Migration helper (admin only): removes every stored proposal without
    /// decoding it and empties the in-flight index. Required after an upgrade
    /// that changes the `Proposal` XDR layout — pre-upgrade in-flight
    /// proposals would fail to decode on read, so they must be cleared before
    /// the multisig is used again, then re-proposed. Safe to call at any time;
    /// it only deletes proposal records, never the multisig or parameters.
    pub fn clear_proposals(env: Env, admin: Address) {
        admin.require_auth();
        access::require_admin(&env, &admin);

        let count = storage::get_proposal_count(&env);
        for id in 0..count {
            storage::remove_proposal(&env, id);
        }
        storage::set_active_proposals(&env, &Vec::new(&env));
    }

    pub fn propose(env: Env, proposer: Address, action: ProposalAction) -> u64 {
        proposer.require_auth();
        access::require_signer(&env, &proposer);

        match &action {
            ProposalAction::UpdateParameters(p) => Self::validate_parameters(&env, p),
            ProposalAction::UpdateSigners(c) => Self::validate_multisig_config(&env, c),
            _ => {}
        }

        let now = env.ledger().timestamp();
        let expires_at = now
            .checked_add(PROPOSAL_TTL_SECONDS)
            .unwrap_or_else(|| panic_with_error!(&env, ParametersError::Overflow));

        // Snapshot the current eligible signer set at proposal time. Every
        // approver must later be validated against this snapshot AND the
        // current membership, so signers removed (or added) afterwards can
        // neither approve nor have their approval counted.
        let config = storage::get_multisig(&env).unwrap_or_else(|err| panic_with_error!(&env, err));
        let eligible_signers = config.signers;

        let id = storage::next_proposal_id(&env);
        let mut approvals = Vec::new(&env);
        approvals.push_back(proposer.clone());

        let proposal = Proposal {
            id,
            action,
            proposer: proposer.clone(),
            approvals,
            eligible_signers,
            created_at: now,
            expires_at,
            executed: false,
            invalidated: false,
        };
        storage::set_proposal(&env, &proposal);
        // Track the proposal in the in-flight index so the signer-change
        // invalidation scan stays bounded to active proposals (see
        // `invalidate_signer_proposals`).
        let mut active = storage::get_active_proposals(&env);
        active.push_back(id);
        storage::set_active_proposals(&env, &active);
        events::emit_proposal_created(&env, id, &proposer);
        id
    }

    pub fn approve(env: Env, signer: Address, proposal_id: u64) {
        signer.require_auth();
        access::require_signer(&env, &signer);

        let mut proposal =
            storage::get_proposal(&env, proposal_id).unwrap_or_else(|err| panic_with_error!(&env, err));

        if proposal.invalidated {
            panic_with_error!(&env, ParametersError::ProposalInvalidated);
        }
        if proposal.executed {
            panic_with_error!(&env, ParametersError::ProposalAlreadyExecuted);
        }
        if env.ledger().timestamp() > proposal.expires_at {
            panic_with_error!(&env, ParametersError::ProposalExpired);
        }
        if proposal.approvals.contains(&signer) {
            panic_with_error!(&env, ParametersError::DuplicateSignature);
        }
        // The signer is a current member (checked above via require_signer);
        // they must ALSO have been eligible when the proposal was created.
        // This rejects signers added to the committee after the proposal.
        if !proposal.eligible_signers.contains(&signer) {
            panic_with_error!(&env, ParametersError::NotEligibleSigner);
        }

        proposal.approvals.push_back(signer.clone());
        storage::set_proposal(&env, &proposal);
        events::emit_proposal_approved(&env, proposal_id, &signer, proposal.approvals.len());
    }

    /// Execute a proposal once it has collected at least `threshold` approvals.
    /// Permissionless — the collected approvals are the authorization.
    pub fn execute(env: Env, proposal_id: u64) {
        let mut proposal =
            storage::get_proposal(&env, proposal_id).unwrap_or_else(|err| panic_with_error!(&env, err));

        if proposal.invalidated {
            panic_with_error!(&env, ParametersError::ProposalInvalidated);
        }
        if proposal.executed {
            panic_with_error!(&env, ParametersError::ProposalAlreadyExecuted);
        }
        if env.ledger().timestamp() > proposal.expires_at {
            panic_with_error!(&env, ParametersError::ProposalExpired);
        }

        let config = storage::get_multisig(&env).unwrap_or_else(|err| panic_with_error!(&env, err));

        // Every approver must have been eligible at proposal time AND still be
        // a current member. A single stale approver invalidates the proposal,
        // so a signer removed after approving can never have their approval
        // counted toward execution.
        for i in 0..proposal.approvals.len() {
            let approver = proposal.approvals.get_unchecked(i);
            if !proposal.eligible_signers.contains(&approver) || !config.signers.contains(&approver) {
                panic_with_error!(&env, ParametersError::StaleApproval);
            }
        }

        // Signer-set changes require a strictly higher quorum (threshold + 1,
        // capped at unanimity) so signers cannot cheapen their own gate or
        // admit colluders with the old threshold.
        let required_quorum = match &proposal.action {
            ProposalAction::UpdateSigners(_) => Self::elevated_quorum(&config),
            _ => config.threshold,
        };
        if proposal.approvals.len() < required_quorum {
            match &proposal.action {
                ProposalAction::UpdateSigners(_) => {
                    panic_with_error!(&env, ParametersError::ElevatedQuorumNotMet);
                }
                _ => panic_with_error!(&env, ParametersError::ThresholdNotMet),
            }
        }

        Self::enter_non_reentrant(&env);
        // Persist `executed = true` BEFORE dispatching: the signer-change
        // invalidation scan that runs inside `do_update_signers` reads
        // proposals from storage and would otherwise mark this very proposal
        // invalidated (emitting a spurious PROPINVL for an executed proposal).
        proposal.executed = true;
        storage::set_proposal(&env, &proposal);
        match proposal.action.clone() {
            ProposalAction::UpdateParameters(p) => Self::do_update_parameters(&env, &p),
            ProposalAction::SetAdmin(a) => Self::do_set_admin(&env, &a),
            ProposalAction::Upgrade(h) => Self::do_upgrade(&env, h),
            ProposalAction::UpdateSigners(c) => Self::do_update_signers(&env, &c),
        }
        // No longer in flight once executed.
        storage::remove_active_proposal(&env, proposal_id);
        events::emit_proposal_executed(&env, proposal_id);
        Self::exit_non_reentrant(&env);
    }

    pub fn get_proposal(env: Env, proposal_id: u64) -> Result<Proposal, ParametersError> {
        storage::get_proposal(&env, proposal_id)
    }


    pub fn get_admin(env: Env) -> Result<Address, ParametersError> {
        storage::get_admin(&env)
    }

    pub fn get_version(env: Env) -> u32 {
        storage::get_version(&env).unwrap_or_else(|err| panic_with_error!(&env, err))
    }

    pub fn get_parameters(env: Env) -> Result<ProtocolParameters, ParametersError> {
        storage::get_parameters(&env)
    }


    fn do_update_parameters(env: &Env, params: &ProtocolParameters) {
        Self::validate_parameters(env, params);
        let admin = storage::get_admin(env).unwrap_or_else(|err| panic_with_error!(env, err));
        storage::set_parameters(env, params);
        events::emit_parameters_updated(env, &admin, params);
    }

    fn do_set_admin(env: &Env, new_admin: &Address) {
        let old_admin = storage::get_admin(env).unwrap_or_else(|err| panic_with_error!(env, err));
        storage::set_admin(env, new_admin);
        events::emit_admin_updated(env, &old_admin, new_admin);
    }

    fn do_upgrade(env: &Env, new_wasm_hash: BytesN<32>) {
        let old = storage::get_version(env).unwrap_or(1u32);
        let new = old.checked_add(1).unwrap_or(old);
        storage::set_version(env, new);
        env.deployer().update_current_contract_wasm(new_wasm_hash);
        events::emit_contract_upgraded(env, old, new);
    }

    fn do_update_signers(env: &Env, config: &MultisigConfig) {
        Self::validate_multisig_config(env, config);
        storage::set_multisig(env, config);
        events::emit_multisig_configured(env, config.threshold, config.signers.len());
        // The signer set changed: clear/re-validate in-flight proposals that
        // target the signer set, so a stale proposal cannot install a further
        // signer-set change under the old membership.
        Self::invalidate_signer_proposals(env);
    }

    /// Mark every in-flight proposal whose action targets the signer set as
    /// invalidated. Called whenever the signer set changes. Non-signer-targeting
    /// proposals are left alone — they re-validate themselves at execute time
    /// via the eligible-signers snapshot.
    ///
    /// Scans only the in-flight index (proposals created since the last
    /// signer-set change), never the full proposal history, so the cost stays
    /// bounded as the contract accumulates proposals over time. The index is
    /// pruned in the same pass: executed, invalidated, expired, and missing
    /// proposals are dropped.
    fn invalidate_signer_proposals(env: &Env) {
        let now = env.ledger().timestamp();
        let active = storage::get_active_proposals(env);
        let mut remaining = Vec::new(env);
        for i in 0..active.len() {
            let id = active.get_unchecked(i);
            let Ok(mut proposal) = storage::get_proposal(env, id) else {
                continue; // missing — drop from the index
            };
            if proposal.executed || proposal.invalidated || now > proposal.expires_at {
                continue; // no longer in flight — drop from the index
            }
            if matches!(&proposal.action, ProposalAction::UpdateSigners(_)) {
                proposal.invalidated = true;
                storage::set_proposal(env, &proposal);
                events::emit_proposal_invalidated(env, id);
            } else {
                remaining.push_back(id);
            }
        }
        storage::set_active_proposals(env, &remaining);
    }

    /// Quorum required to change the signer set: the current threshold + 1,
    /// capped at full unanimity of the current signer set so it is always
    /// achievable (e.g. 2-of-3 → 3, 3-of-3 → 3, 4-of-7 → 5).
    fn elevated_quorum(config: &MultisigConfig) -> u32 {
        let n = config.signers.len();
        let threshold_plus_one = config.threshold.checked_add(1).unwrap_or(n);
        threshold_plus_one.min(n)
    }

    fn validate_parameters(env: &Env, params: &ProtocolParameters) {
        if params.min_guarantee_percent <= 0
            || params.min_guarantee_percent > 100
            || params.large_loan_threshold <= 0
        {
            panic_with_error!(env, ParametersError::InvalidParameters);
        }
    }

    fn validate_multisig_config(env: &Env, config: &MultisigConfig) {
        let n = config.signers.len();
        if config.threshold < 2 || config.threshold > n {
            panic_with_error!(env, ParametersError::InvalidThreshold);
        }
        for i in 0..n {
            let a = config.signers.get_unchecked(i);
            for j in (i + 1)..n {
                if a == config.signers.get_unchecked(j) {
                    panic_with_error!(env, ParametersError::DuplicateSigner);
                }
            }
        }
    }

    fn enter_non_reentrant(env: &Env) {
        if storage::is_reentrancy_locked(env).unwrap_or_else(|err| panic_with_error!(env, err)) {
            panic_with_error!(env, ParametersError::ReentrancyDetected);
        }
        storage::set_reentrancy_locked(env, true);
    }

    fn exit_non_reentrant(env: &Env) {
        storage::set_reentrancy_locked(env, false);
    }
}

#[cfg(test)]
mod tests;
