//! PactKeeper: SetBeneficiary + Settle (the arbiter splits a deposit between the contractor and the client).
//! Refund stays the regular Withdraw co-signed by the arbiter (see test_withdraw.rs).
use crate::{
    fixtures::{SetImmutableFixture, WithdrawSetup, DEFAULT_DEPOSIT_AMOUNT},
    utils::{
        assert_escrow_error, assert_instruction_error, find_extensions_pda, EscrowError, TestContext, TestInstruction,
    },
};
use escrow_program_client::instructions::{SetBeneficiaryBuilder, SettleBuilder};
use solana_sdk::{
    instruction::InstructionError,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
};
use spl_token_2022::ID as TOKEN_2022_PROGRAM_ID;

/// A funded deal: escrow with arbiter (oracle), beneficiary (contractor) with its token account,
/// and one deposit of DEFAULT_DEPOSIT_AMOUNT by the client.
struct Deal {
    setup: WithdrawSetup,
    arbiter: Keypair,
    beneficiary: Pubkey,
    beneficiary_token_account: Pubkey,
}

fn set_beneficiary_ix(ctx: &TestContext, escrow: Pubkey, admin: &Keypair, beneficiary: Pubkey) -> TestInstruction {
    let (extensions_pda, extensions_bump) = find_extensions_pda(&escrow);
    let instruction = SetBeneficiaryBuilder::new()
        .payer(ctx.payer.pubkey())
        .admin(admin.pubkey())
        .beneficiary(beneficiary)
        .escrow(escrow)
        .extensions(extensions_pda)
        .extensions_bump(extensions_bump)
        .instruction();
    TestInstruction { instruction, signers: vec![admin.insecure_clone()], name: "SetBeneficiary" }
}

fn deal(ctx: &mut TestContext, token_2022: bool, with_beneficiary: bool) -> Deal {
    let mut builder = WithdrawSetup::builder(ctx).arbiter();
    if token_2022 {
        builder = builder.token_2022();
    }
    let setup = builder.build();
    let arbiter = setup.arbiter.as_ref().unwrap().insecure_clone();
    let beneficiary = Keypair::new().pubkey();
    if with_beneficiary {
        set_beneficiary_ix(ctx, setup.escrow_pda, &setup.admin, beneficiary).send_expect_success(ctx);
    }
    let beneficiary_token_account = if setup.token_program == TOKEN_2022_PROGRAM_ID {
        ctx.create_token_2022_account(&beneficiary, &setup.mint.pubkey())
    } else {
        ctx.create_token_account(&beneficiary, &setup.mint.pubkey())
    };
    Deal { setup, arbiter, beneficiary, beneficiary_token_account }
}

fn settle_ix(
    d: &Deal,
    arbiter: &Keypair,
    beneficiary: Pubkey,
    beneficiary_ta: Pubkey,
    depositor: Pubkey,
    bps: u16,
) -> TestInstruction {
    let s = &d.setup;
    let instruction = SettleBuilder::new()
        .arbiter(arbiter.pubkey())
        .escrow(s.escrow_pda)
        .extensions(s.extensions_pda)
        .receipt(s.receipt_pda)
        .vault(s.vault)
        .depositor(depositor)
        .depositor_token_account(s.depositor_token_account)
        .beneficiary(beneficiary)
        .beneficiary_token_account(beneficiary_ta)
        .mint(s.mint.pubkey())
        .token_program(s.token_program)
        .payout_bps(bps)
        .instruction();
    TestInstruction { instruction, signers: vec![arbiter.insecure_clone()], name: "Settle" }
}

fn valid_settle(d: &Deal, bps: u16) -> TestInstruction {
    settle_ix(d, &d.arbiter, d.beneficiary, d.beneficiary_token_account, d.setup.depositor.pubkey(), bps)
}

/// Runs a settlement and checks every balance: beneficiary +payout, depositor +rest, vault −amount, receipt closed
/// and its rent back to the depositor.
fn assert_split(token_2022: bool, bps: u16, expected_payout: u64) {
    let mut ctx = TestContext::new();
    let d = deal(&mut ctx, token_2022, true);
    let s = &d.setup;
    let depositor_before = ctx.get_token_balance(&s.depositor_token_account);
    let vault_before = ctx.get_token_balance(&s.vault);
    let receipt_rent = ctx.get_account(&s.receipt_pda).unwrap().lamports;
    let depositor_lamports = ctx.get_account(&s.depositor.pubkey()).unwrap().lamports;

    valid_settle(&d, bps).send_expect_success(&mut ctx);

    assert_eq!(ctx.get_token_balance(&d.beneficiary_token_account), expected_payout);
    assert_eq!(
        ctx.get_token_balance(&s.depositor_token_account),
        depositor_before + DEFAULT_DEPOSIT_AMOUNT - expected_payout
    );
    assert_eq!(ctx.get_token_balance(&s.vault), vault_before - DEFAULT_DEPOSIT_AMOUNT);
    assert!(ctx.get_account(&s.receipt_pda).is_none(), "receipt must be closed");
    assert_eq!(ctx.get_account(&s.depositor.pubkey()).unwrap().lamports, depositor_lamports + receipt_rent);
}

// ---------- success ----------

#[test]
fn test_settle_full_payout_to_contractor() {
    assert_split(false, 10_000, DEFAULT_DEPOSIT_AMOUNT);
}

#[test]
fn test_settle_zero_payout_everything_back_to_client() {
    assert_split(false, 0, 0);
}

#[test]
fn test_settle_partial_payout_from_settlement() {
    assert_split(false, 7_500, DEFAULT_DEPOSIT_AMOUNT * 3 / 4);
}

#[test]
fn test_settle_rounds_down_for_the_contractor() {
    // 1_000_000 * 3333 / 10000 = 333_300 exactly; 1 bp of 1_000_000 = 100
    assert_split(false, 1, 100);
    assert_split(false, 3_333, 333_300);
}

#[test]
fn test_settle_token_2022() {
    assert_split(true, 6_000, DEFAULT_DEPOSIT_AMOUNT * 6 / 10);
}

#[test]
fn test_settle_twice_fails_receipt_closed() {
    let mut ctx = TestContext::new();
    let d = deal(&mut ctx, false, true);
    valid_settle(&d, 10_000).send_expect_success(&mut ctx);
    ctx.svm.expire_blockhash();
    let err = valid_settle(&d, 10_000).send_expect_error(&mut ctx);
    assert!(matches!(err, solana_sdk::transaction::TransactionError::InstructionError(_, _)), "got {err:?}");
}

// ---------- who may settle and where the money may go ----------

#[test]
fn test_settle_wrong_arbiter() {
    let mut ctx = TestContext::new();
    let d = deal(&mut ctx, false, true);
    let stranger = ctx.create_funded_keypair();
    let err = settle_ix(&d, &stranger, d.beneficiary, d.beneficiary_token_account, d.setup.depositor.pubkey(), 10_000)
        .send_expect_error(&mut ctx);
    assert_escrow_error(err, EscrowError::InvalidArbiter);
}

#[test]
fn test_settle_depositor_cannot_settle_himself() {
    let mut ctx = TestContext::new();
    let d = deal(&mut ctx, false, true);
    let depositor = d.setup.depositor.insecure_clone();
    let err = settle_ix(&d, &depositor, d.beneficiary, d.beneficiary_token_account, depositor.pubkey(), 0)
        .send_expect_error(&mut ctx);
    assert_escrow_error(err, EscrowError::InvalidArbiter);
}

#[test]
fn test_settle_arbiter_must_sign() {
    let mut ctx = TestContext::new();
    let d = deal(&mut ctx, false, true);
    let mut ix = valid_settle(&d, 10_000);
    ix.instruction.accounts[0].is_signer = false;
    ix.signers.clear();
    let err = ix.send_expect_error(&mut ctx);
    assert_instruction_error(err, InstructionError::MissingRequiredSignature);
}

#[test]
fn test_settle_cannot_redirect_to_another_beneficiary() {
    let mut ctx = TestContext::new();
    let d = deal(&mut ctx, false, true);
    let thief = Keypair::new().pubkey();
    let thief_ta = ctx.create_token_account(&thief, &d.setup.mint.pubkey());
    let err =
        settle_ix(&d, &d.arbiter, thief, thief_ta, d.setup.depositor.pubkey(), 10_000).send_expect_error(&mut ctx);
    assert_escrow_error(err, EscrowError::InvalidBeneficiary);
}

#[test]
fn test_settle_beneficiary_token_account_must_be_its_ata() {
    let mut ctx = TestContext::new();
    let d = deal(&mut ctx, false, true);
    let other = Keypair::new().pubkey();
    let other_ta = ctx.create_token_account(&other, &d.setup.mint.pubkey());
    let err = settle_ix(&d, &d.arbiter, d.beneficiary, other_ta, d.setup.depositor.pubkey(), 10_000)
        .send_expect_error(&mut ctx);
    assert!(matches!(err, solana_sdk::transaction::TransactionError::InstructionError(_, _)), "got {err:?}");
    assert_eq!(ctx.get_token_balance(&other_ta), 0);
}

#[test]
fn test_settle_depositor_must_match_receipt() {
    // The remainder and the rent can only go to the client who made the deposit
    let mut ctx = TestContext::new();
    let d = deal(&mut ctx, false, true);
    let other = ctx.create_funded_keypair().pubkey();
    let mut ix = settle_ix(&d, &d.arbiter, d.beneficiary, d.beneficiary_token_account, other, 0);
    let other_ta = ctx.create_token_account(&other, &d.setup.mint.pubkey());
    ix.instruction.accounts[6].pubkey = other_ta;
    let err = ix.send_expect_error(&mut ctx);
    assert_escrow_error(err, EscrowError::InvalidWithdrawer);
}

#[test]
fn test_settle_payout_over_100_percent() {
    let mut ctx = TestContext::new();
    let d = deal(&mut ctx, false, true);
    let err = valid_settle(&d, 10_001).send_expect_error(&mut ctx);
    assert_escrow_error(err, EscrowError::InvalidPayoutBps);
}

#[test]
fn test_settle_without_beneficiary_not_configured() {
    let mut ctx = TestContext::new();
    let d = deal(&mut ctx, false, false);
    let err = valid_settle(&d, 10_000).send_expect_error(&mut ctx);
    assert_escrow_error(err, EscrowError::SettleNotConfigured);
}

#[test]
fn test_refund_is_withdraw_with_arbiter() {
    // Cancelled deal: everything back to the client through the existing Withdraw + arbiter signature.
    let mut ctx = TestContext::new();
    let d = deal(&mut ctx, false, true);
    let before = ctx.get_token_balance(&d.setup.depositor_token_account);
    d.setup.build_instruction(&ctx).send_expect_success(&mut ctx);
    assert_eq!(ctx.get_token_balance(&d.setup.depositor_token_account), before + DEFAULT_DEPOSIT_AMOUNT);
    assert_eq!(ctx.get_token_balance(&d.beneficiary_token_account), 0);
}

// ---------- SetBeneficiary ----------

#[test]
fn test_set_beneficiary_only_admin() {
    let mut ctx = TestContext::new();
    let d = deal(&mut ctx, false, false);
    let not_admin = ctx.create_funded_keypair();
    let err = set_beneficiary_ix(&ctx, d.setup.escrow_pda, &not_admin, d.beneficiary).send_expect_error(&mut ctx);
    assert_escrow_error(err, EscrowError::InvalidAdmin);
}

#[test]
fn test_set_beneficiary_can_be_corrected_while_mutable() {
    let mut ctx = TestContext::new();
    let d = deal(&mut ctx, false, true);
    let corrected = Keypair::new().pubkey();
    let corrected_ta = ctx.create_token_account(&corrected, &d.setup.mint.pubkey());
    set_beneficiary_ix(&ctx, d.setup.escrow_pda, &d.setup.admin, corrected).send_expect_success(&mut ctx);
    settle_ix(&d, &d.arbiter, corrected, corrected_ta, d.setup.depositor.pubkey(), 10_000)
        .send_expect_success(&mut ctx);
    assert_eq!(ctx.get_token_balance(&corrected_ta), DEFAULT_DEPOSIT_AMOUNT);
}

#[test]
fn test_set_beneficiary_frozen_after_immutable() {
    let mut ctx = TestContext::new();
    let d = deal(&mut ctx, false, true);
    SetImmutableFixture::build_with_escrow(&mut ctx, d.setup.escrow_pda, d.setup.admin.insecure_clone())
        .send_expect_success(&mut ctx);
    let err = set_beneficiary_ix(&ctx, d.setup.escrow_pda, &d.setup.admin, Keypair::new().pubkey())
        .send_expect_error(&mut ctx);
    assert_escrow_error(err, EscrowError::EscrowImmutable);
    // Settling still works with the frozen beneficiary
    valid_settle(&d, 10_000).send_expect_success(&mut ctx);
}
