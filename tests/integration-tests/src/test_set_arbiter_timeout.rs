//! PactKeeper: SetArbiterTimeout — if the arbiter (oracle) has not acted on a deposit for `timeout` seconds,
//! the depositor may withdraw it (full refund) without the arbiter's signature.
use crate::{
    fixtures::{
        CreateEscrowFixture, RemoveExtensionFixture, SetArbiterTimeoutFixture, SetImmutableFixture, WithdrawSetup,
    },
    utils::{
        assert_escrow_error, assert_instruction_error, extensions_utils::find_extension, find_escrow_pda,
        test_empty_data, test_missing_signer, test_not_writable, test_truncated_data, test_wrong_account,
        test_wrong_current_program, test_wrong_system_program, EscrowError, InstructionTestFixture, TestContext,
        TestInstruction, RANDOM_PUBKEY,
    },
};
use escrow_program_client::instructions::WithdrawBuilder;
use solana_sdk::{
    instruction::{AccountMeta, InstructionError},
    signature::Signer,
};

const EXTENSION_TYPE_ARBITER_TIMEOUT: u16 = 5;
const EXTENSION_TYPE_ARBITER: u16 = 3;
const DAY: u64 = 24 * 3600;

fn timeout_of(ctx: &TestContext, setup_extensions: &solana_sdk::pubkey::Pubkey) -> Option<u64> {
    let account = ctx.get_account(setup_extensions)?;
    find_extension(&account.data, EXTENSION_TYPE_ARBITER_TIMEOUT)
        .map(|d| u64::from_le_bytes(d[0..8].try_into().unwrap()))
}

/// Withdraw by the depositor alone: no arbiter in remaining accounts.
fn withdraw_without_arbiter(ctx: &TestContext, s: &WithdrawSetup) -> TestInstruction {
    let instruction = WithdrawBuilder::new()
        .rent_recipient(ctx.payer.pubkey())
        .withdrawer(s.depositor.pubkey())
        .escrow(s.escrow_pda)
        .extensions(s.extensions_pda)
        .receipt(s.receipt_pda)
        .vault(s.vault)
        .withdrawer_token_account(s.depositor_token_account)
        .mint(s.mint.pubkey())
        .token_program(s.token_program)
        .instruction();
    TestInstruction { instruction, signers: vec![s.depositor.insecure_clone()], name: "Withdraw" }
}

// ---------- generic account / data checks ----------

#[test]
fn test_set_arbiter_timeout_missing_admin_signer() {
    let mut ctx = TestContext::new();
    test_missing_signer::<SetArbiterTimeoutFixture>(&mut ctx, 1, 0);
}

#[test]
fn test_set_arbiter_timeout_extensions_not_writable() {
    let mut ctx = TestContext::new();
    test_not_writable::<SetArbiterTimeoutFixture>(&mut ctx, 3);
}

#[test]
fn test_set_arbiter_timeout_wrong_system_program() {
    let mut ctx = TestContext::new();
    test_wrong_system_program::<SetArbiterTimeoutFixture>(&mut ctx);
}

#[test]
fn test_set_arbiter_timeout_wrong_escrow_program() {
    let mut ctx = TestContext::new();
    test_wrong_current_program::<SetArbiterTimeoutFixture>(&mut ctx);
}

#[test]
fn test_set_arbiter_timeout_invalid_event_authority() {
    let mut ctx = TestContext::new();
    test_wrong_account::<SetArbiterTimeoutFixture>(&mut ctx, 5, InstructionError::Custom(2));
}

#[test]
fn test_set_arbiter_timeout_empty_data() {
    let mut ctx = TestContext::new();
    test_empty_data::<SetArbiterTimeoutFixture>(&mut ctx);
}

#[test]
fn test_set_arbiter_timeout_truncated_data() {
    let mut ctx = TestContext::new();
    test_truncated_data::<SetArbiterTimeoutFixture>(&mut ctx);
}

#[test]
fn test_set_arbiter_timeout_escrow_not_owned_by_program() {
    let mut ctx = TestContext::new();
    let error =
        SetArbiterTimeoutFixture::build_valid(&mut ctx).with_account_at(2, RANDOM_PUBKEY).send_expect_error(&mut ctx);
    assert_instruction_error(error, InstructionError::InvalidAccountOwner);
}

// ---------- setting the timeout ----------

fn new_escrow(ctx: &mut TestContext) -> (solana_sdk::pubkey::Pubkey, solana_sdk::signature::Keypair) {
    let escrow_ix = CreateEscrowFixture::build_valid(ctx);
    let admin = escrow_ix.signers[0].insecure_clone();
    let escrow_seed = escrow_ix.signers[1].pubkey();
    escrow_ix.send_expect_success(ctx);
    (find_escrow_pda(&escrow_seed).0, admin)
}

#[test]
fn test_set_arbiter_timeout_success_and_update() {
    let mut ctx = TestContext::new();
    let (escrow, admin) = new_escrow(&mut ctx);
    let extensions = crate::utils::find_extensions_pda(&escrow).0;
    SetArbiterTimeoutFixture::build_with_escrow(&mut ctx, escrow, admin.insecure_clone(), 30 * DAY)
        .send_expect_success(&mut ctx);
    assert_eq!(timeout_of(&ctx, &extensions), Some(30 * DAY));
    // Updating replaces the value (no duplicate extension).
    SetArbiterTimeoutFixture::build_with_escrow(&mut ctx, escrow, admin, 45 * DAY).send_expect_success(&mut ctx);
    assert_eq!(timeout_of(&ctx, &extensions), Some(45 * DAY));
}

#[test]
fn test_set_arbiter_timeout_wrong_admin() {
    let mut ctx = TestContext::new();
    let (escrow, _admin) = new_escrow(&mut ctx);
    let wrong = ctx.create_funded_keypair();
    let error = SetArbiterTimeoutFixture::build_with_escrow(&mut ctx, escrow, wrong, DAY).send_expect_error(&mut ctx);
    assert_instruction_error(error, InstructionError::Custom(1));
}

#[test]
fn test_set_arbiter_timeout_zero_rejected() {
    let mut ctx = TestContext::new();
    let (escrow, admin) = new_escrow(&mut ctx);
    let error = SetArbiterTimeoutFixture::build_with_escrow(&mut ctx, escrow, admin, 0).send_expect_error(&mut ctx);
    assert_instruction_error(error, InstructionError::InvalidInstructionData);
}

#[test]
fn test_set_arbiter_timeout_over_i64_rejected() {
    let mut ctx = TestContext::new();
    let (escrow, admin) = new_escrow(&mut ctx);
    let error = SetArbiterTimeoutFixture::build_with_escrow(&mut ctx, escrow, admin, i64::MAX as u64 + 1)
        .send_expect_error(&mut ctx);
    assert_instruction_error(error, InstructionError::InvalidInstructionData);
}

#[test]
fn test_set_arbiter_timeout_frozen_after_immutable() {
    let mut ctx = TestContext::new();
    let (escrow, admin) = new_escrow(&mut ctx);
    SetImmutableFixture::build_with_escrow(&mut ctx, escrow, admin.insecure_clone()).send_expect_success(&mut ctx);
    let error = SetArbiterTimeoutFixture::build_with_escrow(&mut ctx, escrow, admin, DAY).send_expect_error(&mut ctx);
    assert_escrow_error(error, EscrowError::EscrowImmutable);
}

// ---------- effect on Withdraw ----------

#[test]
fn test_withdraw_before_timeout_still_needs_arbiter() {
    let mut ctx = TestContext::new();
    let s = WithdrawSetup::builder(&mut ctx).arbiter().arbiter_timeout(30 * DAY).build();
    let now = ctx.get_current_timestamp();
    ctx.warp_to_timestamp(now + 30 * DAY as i64 - 1);
    let error = withdraw_without_arbiter(&ctx, &s).send_expect_error(&mut ctx);
    assert_escrow_error(error, EscrowError::InvalidArbiter);
    assert!(ctx.get_account(&s.receipt_pda).is_some(), "funds stay in escrow");
}

#[test]
fn test_withdraw_after_timeout_without_arbiter_refunds_everything() {
    let mut ctx = TestContext::new();
    let s = WithdrawSetup::builder(&mut ctx).arbiter().arbiter_timeout(30 * DAY).build();
    let before = ctx.get_token_balance(&s.depositor_token_account);
    let vault_before = ctx.get_token_balance(&s.vault);
    let now = ctx.get_current_timestamp();
    ctx.warp_to_timestamp(now + 30 * DAY as i64); // exactly at the deadline
    withdraw_without_arbiter(&ctx, &s).send_expect_success(&mut ctx);
    assert_eq!(ctx.get_token_balance(&s.depositor_token_account), before + crate::fixtures::DEFAULT_DEPOSIT_AMOUNT);
    assert_eq!(ctx.get_token_balance(&s.vault), vault_before - crate::fixtures::DEFAULT_DEPOSIT_AMOUNT);
    assert!(ctx.get_account(&s.receipt_pda).is_none(), "receipt closed");
}

#[test]
fn test_withdraw_after_timeout_with_arbiter_account_still_works() {
    // The server may keep passing the arbiter: after the timeout it is skipped, not required.
    let mut ctx = TestContext::new();
    let s = WithdrawSetup::builder(&mut ctx).arbiter().arbiter_timeout(DAY).build();
    let now = ctx.get_current_timestamp();
    ctx.warp_to_timestamp(now + DAY as i64 + 1);
    s.build_instruction(&ctx).send_expect_success(&mut ctx);
    assert!(ctx.get_account(&s.receipt_pda).is_none());
}

#[test]
fn test_withdraw_after_timeout_arbiter_passed_but_not_signing_is_ignored() {
    let mut ctx = TestContext::new();
    let s = WithdrawSetup::builder(&mut ctx).arbiter().arbiter_timeout(DAY).build();
    let arbiter = s.arbiter.as_ref().unwrap().pubkey();
    let now = ctx.get_current_timestamp();
    ctx.warp_to_timestamp(now + DAY as i64 + 1);
    let mut ix = withdraw_without_arbiter(&ctx, &s);
    ix.instruction.accounts.push(AccountMeta::new_readonly(arbiter, false));
    ix.send_expect_success(&mut ctx);
    assert!(ctx.get_account(&s.receipt_pda).is_none());
}

#[test]
fn test_withdraw_before_timeout_with_arbiter_works_as_before() {
    let mut ctx = TestContext::new();
    let s = WithdrawSetup::builder(&mut ctx).arbiter().arbiter_timeout(30 * DAY).build();
    s.build_instruction(&ctx).send_expect_success(&mut ctx);
    assert!(ctx.get_account(&s.receipt_pda).is_none());
}

#[test]
fn test_timeout_without_arbiter_extension_changes_nothing() {
    // No arbiter configured: Withdraw never needed one; the timeout is simply unused.
    let mut ctx = TestContext::new();
    let s = WithdrawSetup::builder(&mut ctx).arbiter_timeout(DAY).build();
    withdraw_without_arbiter(&ctx, &s).send_expect_success(&mut ctx);
}

#[test]
fn test_timelock_still_applies_after_arbiter_timeout() {
    // A timelock longer than the arbiter timeout keeps the funds locked until it expires.
    let mut ctx = TestContext::new();
    let s = WithdrawSetup::builder(&mut ctx).arbiter().arbiter_timeout(DAY).timelock(3 * DAY).build();
    let now = ctx.get_current_timestamp();
    ctx.warp_to_timestamp(now + 2 * DAY as i64);
    let error = withdraw_without_arbiter(&ctx, &s).send_expect_error(&mut ctx);
    assert_escrow_error(error, EscrowError::TimelockNotExpired);
    ctx.warp_to_timestamp(now + 3 * DAY as i64 + 1);
    ctx.warp_to_slot(2); // new blockhash: the same transaction failed above
    withdraw_without_arbiter(&ctx, &s).send_expect_success(&mut ctx);
}

#[test]
fn test_removing_timeout_restores_arbiter_requirement() {
    let mut ctx = TestContext::new();
    let s = WithdrawSetup::builder(&mut ctx).arbiter().arbiter_timeout(DAY).build();
    RemoveExtensionFixture::build_with_escrow(
        &mut ctx,
        s.escrow_pda,
        s.admin.insecure_clone(),
        EXTENSION_TYPE_ARBITER_TIMEOUT,
    )
    .send_expect_success(&mut ctx);
    assert_eq!(timeout_of(&ctx, &s.extensions_pda), None);
    assert!(find_extension(&ctx.get_account(&s.extensions_pda).unwrap().data, EXTENSION_TYPE_ARBITER).is_some());
    let now = ctx.get_current_timestamp();
    ctx.warp_to_timestamp(now + 10 * DAY as i64);
    let error = withdraw_without_arbiter(&ctx, &s).send_expect_error(&mut ctx);
    assert_escrow_error(error, EscrowError::InvalidArbiter);
}

#[test]
fn test_timeout_counts_from_each_deposit() {
    // The deadline is per receipt (deposited_at), so a later deposit gets a later deadline.
    let mut ctx = TestContext::new();
    let now = ctx.get_current_timestamp();
    ctx.warp_to_timestamp(now + 5 * DAY as i64); // the escrow is configured, then time passes before the deposit
    let s = WithdrawSetup::builder(&mut ctx).arbiter().arbiter_timeout(DAY).build();
    let deposited = ctx.get_current_timestamp();
    ctx.warp_to_timestamp(deposited + DAY as i64 - 10);
    let error = withdraw_without_arbiter(&ctx, &s).send_expect_error(&mut ctx);
    assert_escrow_error(error, EscrowError::InvalidArbiter);
}
