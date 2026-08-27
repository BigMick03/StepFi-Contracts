use crate::{
    default_parameters, MultisigConfig, ParametersContract, ParametersContractClient,
    ParametersError, ProposalAction, ProtocolParameters,
};
use soroban_sdk::{
    testutils::{Address as _, Events, Ledger},
    vec, Address, Env, IntoVal, Vec,
};

const SEVEN_DAYS: u64 = 604_800;

/// Host error value produced when a contract function panics with
/// `panic_with_error!(ParametersError::X)`, for asserting on `try_*` clients.
fn contract_error(code: u32) -> soroban_sdk::Error {
    soroban_sdk::Error::from_contract_error(code)
}

fn setup() -> (Env, ParametersContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(ParametersContract, ());
    let client = ParametersContractClient::new(&env, &contract_id);
    let client: ParametersContractClient<'static> = unsafe { core::mem::transmute(client) };
    let admin = Address::generate(&env);

    (env, client, admin)
}

fn setup_multisig() -> (
    Env,
    ParametersContractClient<'static>,
    Address,
    Address,
    Address,
    Address,
) {
    let (env, client, admin) = setup();
    client.initialize_defaults(&admin);

    let s1 = Address::generate(&env);
    let s2 = Address::generate(&env);
    let s3 = Address::generate(&env);
    let signers: Vec<Address> = vec![&env, s1.clone(), s2.clone(), s3.clone()];
    client.configure_multisig(&admin, &signers, &2u32);
    client.confirm_multisig(&admin);

    (env, client, admin, s1, s2, s3)
}

#[test]
fn test_initialize_defaults() {
    let (_env, client, admin) = setup();
    client.initialize_defaults(&admin);

    assert_eq!(client.get_admin(), admin);
    assert_eq!(client.get_parameters(), default_parameters());
}

#[test]
fn test_get_admin_before_initialize_returns_typed_error() {
    let (_env, client, _admin) = setup();

    assert_eq!(
        client.try_get_admin(),
        Err(Ok(ParametersError::NotInitialized))
    );
}

#[test]
fn test_get_parameters_before_initialize_returns_typed_error() {
    let (_env, client, _admin) = setup();

    assert_eq!(
        client.try_get_parameters(),
        Err(Ok(ParametersError::NotInitialized))
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_invalid_parameters_rejected() {
    let (_env, client, admin) = setup();

    let params = ProtocolParameters {
        min_guarantee_percent: 0,
        ..default_parameters()
    };

    client.initialize(&admin, &params);
}

#[test]
fn test_configure_multisig_stores_committee() {
    let (env, client, admin, s1, s2, s3) = setup_multisig();
    let _ = (admin, env);

    let config = client.get_multisig();
    assert_eq!(config.threshold, 2);
    assert_eq!(config.signers.len(), 3);
    assert!(config.signers.contains(&s1));
    assert!(config.signers.contains(&s2));
    assert!(config.signers.contains(&s3));
}

#[test]
#[should_panic(expected = "Error(Contract, #11)")] // InvalidThreshold
fn test_configure_multisig_rejects_threshold_below_two() {
    let (env, client, admin) = setup();
    client.initialize_defaults(&admin);

    let s1 = Address::generate(&env);
    let s2 = Address::generate(&env);
    let signers: Vec<Address> = vec![&env, s1, s2];
    client.configure_multisig(&admin, &signers, &1u32);
}

#[test]
#[should_panic(expected = "Error(Contract, #11)")] // InvalidThreshold
fn test_configure_multisig_rejects_threshold_above_signer_count() {
    let (env, client, admin) = setup();
    client.initialize_defaults(&admin);

    let s1 = Address::generate(&env);
    let s2 = Address::generate(&env);
    let signers: Vec<Address> = vec![&env, s1, s2];
    client.configure_multisig(&admin, &signers, &3u32);
}

#[test]
#[should_panic(expected = "Error(Contract, #12)")] // DuplicateSigner
fn test_configure_multisig_rejects_duplicate_signers() {
    let (env, client, admin) = setup();
    client.initialize_defaults(&admin);

    let s1 = Address::generate(&env);
    let signers: Vec<Address> = vec![&env, s1.clone(), s1];
    client.configure_multisig(&admin, &signers, &2u32);
}

#[test]
#[should_panic(expected = "Error(Contract, #8)")] // MultisigAlreadyConfigured
fn test_configure_multisig_only_once() {
    let (env, client, admin, s1, s2, s3) = setup_multisig();
    let signers: Vec<Address> = vec![&env, s1, s2, s3];
    client.configure_multisig(&admin, &signers, &2u32);
}

#[test]
#[should_panic(expected = "Error(Contract, #10)")] // NotSigner
fn test_propose_rejects_non_signer() {
    let (env, client, _admin, _s1, _s2, _s3) = setup_multisig();
    let intruder = Address::generate(&env);
    client.propose(&intruder, &ProposalAction::SetAdmin(intruder.clone()));
}

#[test]
fn test_update_parameters_two_of_three_workflow() {
    let (_env, client, _admin, s1, s2, _s3) = setup_multisig();

    let params = ProtocolParameters {
        min_guarantee_percent: 30,
        min_reputation_threshold: 70,
        full_repayment_reward: 12,
        default_penalty: 25,
        large_loan_threshold: 7_500,
        large_loan_default_penalty: 40,
        base_interest_bps: 900,
        grace_period_seconds: 86_400,
        upgrade_delay_seconds: 86_400,
    };

    let id = client.propose(&s1, &ProposalAction::UpdateParameters(params.clone()));
    // Proposer counts as first approval; one more reaches the 2-of-3 threshold.
    client.approve(&s2, &id);
    client.execute(&id);

    assert_eq!(client.get_parameters(), params);
    assert!(client.get_proposal(&id).executed);
}

#[test]
#[should_panic(expected = "Error(Contract, #17)")] // ThresholdNotMet
fn test_execute_before_threshold_met_is_rejected() {
    let (_env, client, _admin, s1, _s2, _s3) = setup_multisig();

    let id = client.propose(&s1, &ProposalAction::UpdateParameters(default_parameters()));
    client.execute(&id);
}

#[test]
#[should_panic(expected = "Error(Contract, #16)")] // DuplicateSignature
fn test_duplicate_signature_rejected() {
    let (_env, client, _admin, s1, _s2, _s3) = setup_multisig();

    let id = client.propose(&s1, &ProposalAction::UpdateParameters(default_parameters()));
    client.approve(&s1, &id);
}

#[test]
#[should_panic(expected = "Error(Contract, #15)")] // ProposalAlreadyExecuted
fn test_cannot_execute_twice() {
    let (_env, client, _admin, s1, s2, _s3) = setup_multisig();

    let id = client.propose(&s1, &ProposalAction::UpdateParameters(default_parameters()));
    client.approve(&s2, &id);
    client.execute(&id);
    client.execute(&id);
}

#[test]
#[should_panic(expected = "Error(Contract, #14)")]
fn test_proposal_expires_after_seven_days() {
    let (env, client, _admin, s1, s2, _s3) = setup_multisig();

    let id = client.propose(&s1, &ProposalAction::UpdateParameters(default_parameters()));

    env.ledger().set_timestamp(SEVEN_DAYS + 1);
    client.approve(&s2, &id);
}

#[test]
#[should_panic(expected = "Error(Contract, #14)")]
fn test_expired_proposal_cannot_execute() {
    let (env, client, _admin, s1, s2, _s3) = setup_multisig();

    let id = client.propose(&s1, &ProposalAction::UpdateParameters(default_parameters()));
    client.approve(&s2, &id);

    env.ledger().set_timestamp(SEVEN_DAYS + 1);
    client.execute(&id);
}

#[test]
fn test_set_admin_via_proposal() {
    let (env, client, _admin, s1, s2, _s3) = setup_multisig();
    let new_admin = Address::generate(&env);

    let id = client.propose(&s1, &ProposalAction::SetAdmin(new_admin.clone()));
    client.approve(&s2, &id);
    client.execute(&id);

    assert_eq!(client.get_admin(), new_admin);
}

#[test]
fn test_update_signers_via_proposal() {
    let (env, client, _admin, s1, s2, s3) = setup_multisig();

    let n1 = Address::generate(&env);
    let n2 = Address::generate(&env);
    let new_config = MultisigConfig {
        signers: vec![&env, n1.clone(), n2.clone()],
        threshold: 2,
    };

    // Signer-set changes need elevated quorum: 2-of-3 → 3 approvals
    // (threshold + 1, capped at unanimity).
    let id = client.propose(&s1, &ProposalAction::UpdateSigners(new_config));
    client.approve(&s2, &id);
    client.approve(&s3, &id);
    client.execute(&id);

    let config = client.get_multisig();
    assert_eq!(config.signers.len(), 2);
    assert!(config.signers.contains(&n1));
    assert!(config.signers.contains(&n2));
    // Old signers are no longer part of the committee.
    assert!(!config.signers.contains(&s1));
}

#[test]
fn test_upgrade_via_proposal_increments_version() {
    let (env, client, _admin, s1, s2, _s3) = setup_multisig();
    assert_eq!(client.get_version(), 1u32);

    let wasm_hash = env.deployer().upload_contract_wasm(soroban_sdk::Bytes::from_slice(
        &env,
        include_bytes!("../../../contracts/test-fixtures/contract.wasm"),
    ));

    let id = client.propose(&s1, &ProposalAction::Upgrade(wasm_hash));
    client.approve(&s2, &id);
    client.execute(&id);

    let events: soroban_sdk::Vec<(Address, soroban_sdk::Vec<soroban_sdk::Val>, soroban_sdk::Val)> =
        env.events().all();
    let mut found = false;
    for e in events.iter() {
        let topic: soroban_sdk::Symbol = e.1.get_unchecked(0).into_val(&env);
        if topic == soroban_sdk::Symbol::new(&env, "CONTRACTUPGRADED") {
            found = true;
            break;
        }
    }
    assert!(found, "CONTRACTUPGRADED event not found");
}

#[test]
fn test_three_of_three_with_full_committee_approval() {
    let (env, client, admin) = setup();
    client.initialize_defaults(&admin);

    let s1 = Address::generate(&env);
    let s2 = Address::generate(&env);
    let s3 = Address::generate(&env);
    client.configure_multisig(
        &admin,
        &vec![&env, s1.clone(), s2.clone(), s3.clone()],
        &3u32,
    );
    client.confirm_multisig(&admin);

    let id = client.propose(&s1, &ProposalAction::UpdateParameters(default_parameters()));
    client.approve(&s2, &id);
    assert!(client.try_execute(&id).is_err());

    client.approve(&s3, &id);
    client.execute(&id);
    assert!(client.get_proposal(&id).executed);
}

// ─── stale-approval exploit (pre-fix: removed signer's approval counted) ─────

#[test]
fn test_stale_approval_never_counts_after_signer_removed() {
    // Exploit reproduction (end-to-end):
    //   1. A proposes an UpdateParameters proposal and B approves it.
    //   2. A second proposal removes B from the signer set and executes.
    //   3. Executing the first proposal would count B's now-stale approval
    //      toward the threshold.
    // Pre-fix: step 3 succeeded (B's approval counted → parameters changed).
    // Post-fix: execute() validates every approver against the current
    // membership and panics StaleApproval (#21), so parameters stay unchanged.
    let (env, client, _admin, s1, s2, s3) = setup_multisig();

    let new_params = ProtocolParameters {
        base_interest_bps: 900,
        ..default_parameters()
    };
    let params_proposal =
        client.propose(&s1, &ProposalAction::UpdateParameters(new_params.clone()));
    client.approve(&s2, &params_proposal);

    // Remove B (s2): signers become [A, C]. Elevated quorum (2-of-3 → 3)
    // reached with A (proposer) + B (stepping down) + C.
    let new_config = MultisigConfig {
        signers: vec![&env, s1.clone(), s3.clone()],
        threshold: 2,
    };
    let signers_proposal = client.propose(&s1, &ProposalAction::UpdateSigners(new_config));
    client.approve(&s2, &signers_proposal);
    client.approve(&s3, &signers_proposal);
    client.execute(&signers_proposal);

    assert!(!client.get_multisig().signers.contains(&s2));

    // B's approval must no longer count: the params proposal cannot execute.
    let res = client.try_execute(&params_proposal);
    assert_eq!(res, Err(Ok(contract_error(21)))); // StaleApproval
    // Parameters unchanged.
    assert_eq!(client.get_parameters(), default_parameters());
}

#[test]
fn test_removed_signer_approval_not_counted_even_after_unrelated_change() {
    // Same exploit shape as above but the remaining approver (A) is still a
    // member: with only 1 valid approval < threshold 2, execution still fails.
    let (env, client, _admin, s1, s2, s3) = setup_multisig();

    let params_proposal = client.propose(
        &s1,
        &ProposalAction::UpdateParameters(default_parameters()),
    );
    client.approve(&s2, &params_proposal);

    // Remove B: [A, C]
    let new_config = MultisigConfig {
        signers: vec![&env, s1.clone(), s3.clone()],
        threshold: 2,
    };
    let signers_proposal = client.propose(&s1, &ProposalAction::UpdateSigners(new_config));
    client.approve(&s2, &signers_proposal);
    client.approve(&s3, &signers_proposal);
    client.execute(&signers_proposal);

    let res = client.try_execute(&params_proposal);
    assert_eq!(res, Err(Ok(contract_error(21)))); // StaleApproval
}

// ─── elevated quorum for signer-set changes ──────────────────────────────────

#[test]
fn test_self_serving_threshold_reduction_rejected() {
    // 2-of-3 must not be able to cheapen itself to 2-of-2 with only 2
    // approvals: signer-set changes need elevated quorum (3 here).
    let (env, client, _admin, s1, s2, _s3) = setup_multisig();

    let weak_config = MultisigConfig {
        signers: vec![&env, s1.clone(), s2.clone()],
        threshold: 2,
    };
    let id = client.propose(&s1, &ProposalAction::UpdateSigners(weak_config));
    client.approve(&s2, &id);

    let res = client.try_execute(&id);
    assert_eq!(res, Err(Ok(contract_error(22)))); // ElevatedQuorumNotMet

    // The gate is unchanged — the weakened config was NOT installed.
    let config = client.get_multisig();
    assert_eq!(config.threshold, 2);
    assert_eq!(config.signers.len(), 3);
}

#[test]
fn test_update_signers_reaches_unanimity_succeeds() {
    // With the full elevated quorum (unanimity for 2-of-3), a signer-set
    // change still succeeds.
    let (env, client, _admin, s1, s2, s3) = setup_multisig();

    let new_config = MultisigConfig {
        signers: vec![&env, s1.clone(), s2.clone()],
        threshold: 2,
    };
    let id = client.propose(&s1, &ProposalAction::UpdateSigners(new_config));
    client.approve(&s2, &id);
    client.approve(&s3, &id);
    client.execute(&id);

    let config = client.get_multisig();
    assert_eq!(config.threshold, 2);
    assert_eq!(config.signers.len(), 2);
    assert!(config.signers.contains(&s1));
    assert!(config.signers.contains(&s2));
    assert!(!config.signers.contains(&s3));
}

#[test]
fn test_admitting_colluder_requires_unanimity() {
    // Two signers cannot admit a colluder: [A,B,C] → [A,B,attacker] needs 3
    // approvals, so a 2-signer cabal fails.
    let (env, client, _admin, s1, s2, _s3) = setup_multisig();
    let attacker = Address::generate(&env);

    let rigged_config = MultisigConfig {
        signers: vec![&env, s1.clone(), s2.clone(), attacker.clone()],
        threshold: 2,
    };
    let id = client.propose(&s1, &ProposalAction::UpdateSigners(rigged_config));
    client.approve(&s2, &id);

    let res = client.try_execute(&id);
    assert_eq!(res, Err(Ok(contract_error(22)))); // ElevatedQuorumNotMet
    assert!(!client.get_multisig().signers.contains(&attacker));
}

// ─── two-step configure_multisig (single admin key cannot silently swap) ────

#[test]
fn test_configure_multisig_two_step_requires_confirmation() {
    let (env, client, admin) = setup();
    client.initialize_defaults(&admin);

    let s1 = Address::generate(&env);
    let s2 = Address::generate(&env);
    let signers: Vec<Address> = vec![&env, s1.clone(), s2.clone()];

    // Step 1: request. The multisig is NOT active yet.
    client.configure_multisig(&admin, &signers, &2u32);
    assert_eq!(
        client.try_get_multisig(),
        Err(Ok(ParametersError::MultisigNotConfigured))
    );
    // Proposals are not possible while the multisig is unconfirmed.
    let id = client.try_propose(&s1, &ProposalAction::UpdateParameters(default_parameters()));
    assert_eq!(id, Err(Ok(contract_error(9)))); // MultisigNotConfigured

    // Step 2: confirm with a second admin signature.
    client.confirm_multisig(&admin);
    let config = client.get_multisig();
    assert_eq!(config.threshold, 2);
    assert_eq!(config.signers.len(), 2);
}

#[test]
#[should_panic(expected = "Error(Contract, #18)")] // MultisigPendingExists
fn test_configure_multisig_second_request_rejected_while_pending() {
    let (env, client, admin) = setup();
    client.initialize_defaults(&admin);

    let s1 = Address::generate(&env);
    let s2 = Address::generate(&env);
    let signers: Vec<Address> = vec![&env, s1.clone(), s2.clone()];
    client.configure_multisig(&admin, &signers, &2u32);
    // A second request while the first is pending is rejected.
    client.configure_multisig(&admin, &signers, &2u32);
}

#[test]
#[should_panic(expected = "Error(Contract, #19)")] // MultisigNotPending
fn test_confirm_multisig_without_pending_fails() {
    let (_env, client, admin) = setup();
    client.initialize_defaults(&admin);
    client.confirm_multisig(&admin);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")] // NotAdmin
fn test_configure_multisig_by_non_admin_fails() {
    let (env, client, admin) = setup();
    client.initialize_defaults(&admin);
    let intruder = Address::generate(&env);

    let s1 = Address::generate(&env);
    let s2 = Address::generate(&env);
    let signers: Vec<Address> = vec![&env, s1, s2];
    client.configure_multisig(&intruder, &signers, &2u32);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")] // NotAdmin
fn test_confirm_multisig_by_non_admin_fails() {
    let (env, client, admin) = setup();
    client.initialize_defaults(&admin);
    let intruder = Address::generate(&env);

    let s1 = Address::generate(&env);
    let s2 = Address::generate(&env);
    let signers: Vec<Address> = vec![&env, s1, s2];
    client.configure_multisig(&admin, &signers, &2u32);
    client.confirm_multisig(&intruder);
}

// ─── in-flight signer-targeting proposals are cleared on signer-set change ──

#[test]
fn test_signers_change_invalidates_inflight_signer_proposals() {
    let (env, client, _admin, s1, s2, s3) = setup_multisig();

    // Two competing signer-set proposals are in flight.
    let p1 = client.propose(
        &s1,
        &ProposalAction::UpdateSigners(MultisigConfig {
            signers: vec![&env, s1.clone(), s2.clone()],
            threshold: 2,
        }),
    );
    let p2 = client.propose(
        &s2,
        &ProposalAction::UpdateSigners(MultisigConfig {
            signers: vec![&env, s2.clone(), s3.clone()],
            threshold: 2,
        }),
    );

    // Execute p2 (elevated quorum: 3 approvals).
    client.approve(&s1, &p2);
    client.approve(&s3, &p2);
    client.execute(&p2);

    // p1 targeted the signer set and must now be invalidated.
    assert!(client.get_proposal(&p1).invalidated);
    assert_eq!(
        client.try_execute(&p1),
        Err(Ok(contract_error(20))) // ProposalInvalidated
    );
    assert_eq!(
        client.try_approve(&s3, &p1),
        Err(Ok(contract_error(20))) // ProposalInvalidated
    );
}

#[test]
fn test_non_signer_targeting_proposal_survives_unrelated_signer_change() {
    // A parameters proposal whose approvers are all still members stays
    // executable after an unrelated signer-set change (only stale approvals
    // die).
    let (env, client, _admin, s1, s2, s3) = setup_multisig();

    let params_proposal = client.propose(
        &s1,
        &ProposalAction::UpdateParameters(default_parameters()),
    );
    client.approve(&s2, &params_proposal);

    // Rotate out C, add D: [A, B, D]
    let d = Address::generate(&env);
    let new_config = MultisigConfig {
        signers: vec![&env, s1.clone(), s2.clone(), d.clone()],
        threshold: 2,
    };
    let signers_proposal = client.propose(&s1, &ProposalAction::UpdateSigners(new_config));
    client.approve(&s2, &signers_proposal);
    client.approve(&s3, &signers_proposal);
    client.execute(&signers_proposal);

    // A and B are still members: the params proposal executes normally.
    client.execute(&params_proposal);
    assert!(client.get_proposal(&params_proposal).executed);
}

#[test]
fn test_full_signer_rotation_invalidates_old_proposal() {
    let (env, client, _admin, s1, s2, s3) = setup_multisig();

    let params_proposal =
        client.propose(&s1, &ProposalAction::UpdateParameters(default_parameters()));

    // Rotate the entire committee out.
    let d1 = Address::generate(&env);
    let d2 = Address::generate(&env);
    let d3 = Address::generate(&env);
    let new_config = MultisigConfig {
        signers: vec![&env, d1, d2, d3],
        threshold: 2,
    };
    let signers_proposal = client.propose(&s1, &ProposalAction::UpdateSigners(new_config));
    client.approve(&s2, &signers_proposal);
    client.approve(&s3, &signers_proposal);
    client.execute(&signers_proposal);

    // The old proposal's only approver (A) is no longer a member.
    let res = client.try_execute(&params_proposal);
    assert_eq!(res, Err(Ok(contract_error(21)))); // StaleApproval
    assert!(!client.get_proposal(&params_proposal).executed);
}

// ─── approve() eligibility against the snapshot ──────────────────────────────

#[test]
fn test_approve_by_added_signer_on_old_proposal_rejected() {
    let (env, client, _admin, s1, s2, s3) = setup_multisig();

    let proposal = client.propose(
        &s1,
        &ProposalAction::UpdateParameters(default_parameters()),
    );

    // Add a new signer D.
    let d = Address::generate(&env);
    let new_config = MultisigConfig {
        signers: vec![&env, s1.clone(), s2.clone(), s3.clone(), d.clone()],
        threshold: 2,
    };
    let signers_proposal = client.propose(&s1, &ProposalAction::UpdateSigners(new_config));
    client.approve(&s2, &signers_proposal);
    client.approve(&s3, &signers_proposal);
    client.execute(&signers_proposal);

    // D is a current signer but was NOT eligible when the old proposal was
    // created — approving must be rejected.
    assert_eq!(
        client.try_approve(&d, &proposal),
        Err(Ok(contract_error(23))) // NotEligibleSigner
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #10)")] // NotSigner
fn test_approve_by_removed_signer_rejected() {
    let (env, client, _admin, s1, s2, s3) = setup_multisig();

    let proposal = client.propose(
        &s1,
        &ProposalAction::UpdateParameters(default_parameters()),
    );

    // Remove B: [A, C]
    let new_config = MultisigConfig {
        signers: vec![&env, s1.clone(), s3.clone()],
        threshold: 2,
    };
    let signers_proposal = client.propose(&s1, &ProposalAction::UpdateSigners(new_config));
    client.approve(&s2, &signers_proposal);
    client.approve(&s3, &signers_proposal);
    client.execute(&signers_proposal);

    // B is no longer a signer at all → rejected by current-membership check.
    client.approve(&s2, &proposal);
}

// ─── events ──────────────────────────────────────────────────────────────────

#[test]
fn test_configure_multisig_emits_pending_and_confirm_events() {
    let (env, client, admin) = setup();
    client.initialize_defaults(&admin);

    let s1 = Address::generate(&env);
    let s2 = Address::generate(&env);
    let signers: Vec<Address> = vec![&env, s1, s2];

    // Step 1 emits the prominent pending event.
    client.configure_multisig(&admin, &signers, &2u32);
    let events: soroban_sdk::Vec<(Address, soroban_sdk::Vec<soroban_sdk::Val>, soroban_sdk::Val)> =
        env.events().all();
    let mut saw_pending = false;
    for e in events.iter() {
        let topic: soroban_sdk::Symbol = e.1.get_unchecked(0).into_val(&env);
        if topic == soroban_sdk::symbol_short!("MSIGPEND") {
            saw_pending = true;
        }
    }
    assert!(saw_pending, "MSIGPEND event not found");

    // Step 2 emits the existing confirm event.
    client.confirm_multisig(&admin);
    let events: soroban_sdk::Vec<(Address, soroban_sdk::Vec<soroban_sdk::Val>, soroban_sdk::Val)> =
        env.events().all();
    let mut saw_confirm = false;
    for e in events.iter() {
        let topic: soroban_sdk::Symbol = e.1.get_unchecked(0).into_val(&env);
        if topic == soroban_sdk::symbol_short!("MSCONFIG") {
            saw_confirm = true;
        }
    }
    assert!(saw_confirm, "MSCONFIG event not found");
}

// ─── signer-set expansion still needs elevated quorum (issue test H) ───────

#[test]
fn test_signer_set_expansion_requires_elevated_quorum() {
    // [A, B, C] → [A, B, C, D]: expanding the committee must NOT be cheaper
    // than any other signer-set change — 2 approvals are insufficient, 3 are
    // required (threshold + 1 capped at unanimity).
    let (env, client, _admin, s1, s2, s3) = setup_multisig();

    let d = Address::generate(&env);
    let expanded = MultisigConfig {
        signers: vec![&env, s1.clone(), s2.clone(), s3.clone(), d.clone()],
        threshold: 2,
    };
    let id = client.propose(&s1, &ProposalAction::UpdateSigners(expanded.clone()));
    client.approve(&s2, &id);

    // Two approvals are below the elevated quorum of 3.
    assert_eq!(client.try_execute(&id), Err(Ok(contract_error(22))));
    assert_eq!(client.get_multisig().signers.len(), 3);

    // The third approval reaches it and the expansion installs.
    client.approve(&s3, &id);
    client.execute(&id);
    assert_eq!(client.get_multisig().signers.len(), 4);
    assert!(client.get_multisig().signers.contains(&d));
}

// ─── fully approved but not yet executed signer proposals are invalidated ───

#[test]
fn test_fully_approved_signer_proposal_invalidated_by_signers_change() {
    // A competing signer-set proposal is FULLY approved (elevated quorum
    // reached) but not yet executed when another signer-set change goes
    // through. The old proposal must not stay executable afterwards.
    let (env, client, _admin, s1, s2, s3) = setup_multisig();

    let stale = client.propose(
        &s1,
        &ProposalAction::UpdateSigners(MultisigConfig {
            signers: vec![&env, s1.clone(), s2.clone()],
            threshold: 2,
        }),
    );
    // Fully approved: elevated quorum (3) reached, but NOT executed yet.
    client.approve(&s2, &stale);
    client.approve(&s3, &stale);

    let winner = client.propose(
        &s2,
        &ProposalAction::UpdateSigners(MultisigConfig {
            signers: vec![&env, s2.clone(), s3.clone()],
            threshold: 2,
        }),
    );
    client.approve(&s1, &winner);
    client.approve(&s3, &winner);
    client.execute(&winner);

    // The fully-approved-but-unexecuted proposal is invalidated, not
    // executable under the new membership.
    assert!(client.get_proposal(&stale).invalidated);
    assert_eq!(client.try_execute(&stale), Err(Ok(contract_error(20))));
}

// ─── the executing signer proposal is never self-invalidated ────────────────

#[test]
fn test_executing_signer_proposal_is_not_self_invalidated() {
    // Regression: the invalidation scan must skip the very proposal being
    // executed — it must not emit a spurious PROPINVL for it or leave it
    // flagged invalidated.
    let (env, client, _admin, s1, s2, s3) = setup_multisig();

    let _stale = client.propose(
        &s1,
        &ProposalAction::UpdateSigners(MultisigConfig {
            signers: vec![&env, s1.clone(), s2.clone()],
            threshold: 2,
        }),
    );
    let winner = client.propose(
        &s2,
        &ProposalAction::UpdateSigners(MultisigConfig {
            signers: vec![&env, s2.clone(), s3.clone()],
            threshold: 2,
        }),
    );
    client.approve(&s1, &winner);
    client.approve(&s3, &winner);
    client.execute(&winner);

    let winner_proposal = client.get_proposal(&winner);
    assert!(winner_proposal.executed);
    assert!(!winner_proposal.invalidated);

    // No PROPINVL event references the executed proposal.
    let events: soroban_sdk::Vec<(Address, soroban_sdk::Vec<soroban_sdk::Val>, soroban_sdk::Val)> =
        env.events().all();
    for e in events.iter() {
        let topic: soroban_sdk::Symbol = e.1.get_unchecked(0).into_val(&env);
        if topic == soroban_sdk::symbol_short!("PROPINVL") {
            let id: u64 = e.2.clone().into_val(&env);
            assert_ne!(id, winner, "PROPINVL must not reference the executed proposal");
        }
    }
}

// ─── cancellation of a staged multisig configuration ────────────────────────

#[test]
fn test_cancel_pending_multisig_allows_restaging() {
    let (env, client, admin) = setup();
    client.initialize_defaults(&admin);

    let s1 = Address::generate(&env);
    let s2 = Address::generate(&env);
    let signers: Vec<Address> = vec![&env, s1.clone(), s2.clone()];

    // Stage a request, then cancel it before confirming.
    client.configure_multisig(&admin, &signers, &2u32);
    client.cancel_pending_multisig(&admin);

    // Nothing was activated.
    assert_eq!(
        client.try_get_multisig(),
        Err(Ok(ParametersError::MultisigNotConfigured))
    );

    // A corrected request can now be staged and confirmed.
    let signers2: Vec<Address> = vec![&env, s1.clone(), s2.clone(), Address::generate(&env)];
    client.configure_multisig(&admin, &signers2, &2u32);
    client.confirm_multisig(&admin);
    assert_eq!(client.get_multisig().signers.len(), 3);
}

#[test]
#[should_panic(expected = "Error(Contract, #19)")] // MultisigNotPending
fn test_cancel_pending_multisig_without_pending_fails() {
    let (_env, client, admin) = setup();
    client.initialize_defaults(&admin);
    client.cancel_pending_multisig(&admin);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")] // NotAdmin
fn test_cancel_pending_multisig_by_non_admin_fails() {
    let (env, client, admin) = setup();
    client.initialize_defaults(&admin);
    let intruder = Address::generate(&env);

    let s1 = Address::generate(&env);
    let s2 = Address::generate(&env);
    let signers: Vec<Address> = vec![&env, s1, s2];
    client.configure_multisig(&admin, &signers, &2u32);
    client.cancel_pending_multisig(&intruder);
}

// ─── migration helper: clear undecodable pre-upgrade proposals ──────────────

#[test]
fn test_clear_proposals_migration_helper() {
    let (_env, client, _admin, s1, s2, _s3) = setup_multisig();

    let p1 = client.propose(
        &s1,
        &ProposalAction::UpdateParameters(default_parameters()),
    );
    let p2 = client.propose(&s1, &ProposalAction::UpdateParameters(default_parameters()));
    client.approve(&s2, &p2);

    // Admin clears all stored proposals (migration path for a Proposal XDR
    // layout change).
    let admin = client.get_admin();
    client.clear_proposals(&admin);

    // All records are gone, the in-flight index is empty, and new proposals
    // still work.
    assert_eq!(client.try_get_proposal(&p1), Err(Ok(ParametersError::ProposalNotFound)));
    assert_eq!(client.try_get_proposal(&p2), Err(Ok(ParametersError::ProposalNotFound)));
    assert_eq!(client.try_execute(&p1), Err(Ok(contract_error(13)))); // ProposalNotFound

    let p3 = client.propose(
        &s1,
        &ProposalAction::UpdateParameters(default_parameters()),
    );
    client.approve(&s2, &p3);
    client.execute(&p3);
    assert!(client.get_proposal(&p3).executed);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")] // NotAdmin
fn test_clear_proposals_by_non_admin_fails() {
    let (env, client, _admin, _s1, _s2, _s3) = setup_multisig();
    let intruder = Address::generate(&env);
    client.clear_proposals(&intruder);
}

#[test]
fn test_signers_change_emits_proposal_invalidated_event() {
    let (env, client, _admin, s1, s2, s3) = setup_multisig();

    let stale = client.propose(
        &s1,
        &ProposalAction::UpdateSigners(MultisigConfig {
            signers: vec![&env, s1.clone(), s2.clone()],
            threshold: 2,
        }),
    );
    let winner = client.propose(
        &s2,
        &ProposalAction::UpdateSigners(MultisigConfig {
            signers: vec![&env, s2.clone(), s3.clone()],
            threshold: 2,
        }),
    );
    client.approve(&s1, &winner);
    client.approve(&s3, &winner);
    client.execute(&winner);

    let events: soroban_sdk::Vec<(Address, soroban_sdk::Vec<soroban_sdk::Val>, soroban_sdk::Val)> =
        env.events().all();
    let mut saw = false;
    for e in events.iter() {
        let topic: soroban_sdk::Symbol = e.1.get_unchecked(0).into_val(&env);
        if topic == soroban_sdk::symbol_short!("PROPINVL") {
            let id: u64 = e.2.clone().into_val(&env);
            if id == stale {
                saw = true;
            }
        }
    }
    assert!(saw, "PROPINVL event for stale proposal not found");
}
