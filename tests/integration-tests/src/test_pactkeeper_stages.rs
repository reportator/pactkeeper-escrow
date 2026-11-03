//! PactKeeper end-to-end scenarios for staged payments (no new instructions needed):
//! every stage is its own deposit receipt, so the oracle releases, splits or refunds stages independently.
//!
//! Receipt seed of a stage = keypair from sha256(contract text hash ‖ stage index): deterministic for the
//! server, unique per stage, and a stage cannot be funded twice (the receipt PDA already exists).
use crate::{
    fixtures::{SetArbiterTimeoutFixture, WithdrawSetup},
    utils::{
        assert_escrow_error, find_allowed_mint_pda, find_extensions_pda, find_receipt_pda, EscrowError, TestContext,
        TestInstruction,
    },
};
use escrow_program_client::instructions::{DepositBuilder, SetBeneficiaryBuilder, SettleBuilder, WithdrawBuilder};
use solana_sdk::{
    hash::hashv,
    instruction::AccountMeta,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
};

const DAY: u64 = 24 * 3600;
/// sha256 of the agreed contract text (any fixed 32 bytes for the test).
const CONTRACT_HASH: [u8; 32] = [42u8; 32];

fn stage_seed(contract_hash: &[u8; 32], stage: u8) -> Keypair {
    Keypair::new_from_array(hashv(&[contract_hash, &[stage]]).to_bytes())
}

/// Escrow of one deal: arbiter (oracle), beneficiary (contractor) and its token account. The fixture's own
/// deposit is not used by these scenarios (stage deposits get their own receipts).
struct DealEscrow {
    s: WithdrawSetup,
    arbiter: Keypair,
    beneficiary: Pubkey,
    beneficiary_ta: Pubkey,
}

fn deal_escrow(ctx: &mut TestContext, timeout: Option<u64>) -> DealEscrow {
    let s = WithdrawSetup::builder(ctx).arbiter().build();
    let arbiter = s.arbiter.as_ref().unwrap().insecure_clone();
    let beneficiary = Keypair::new().pubkey();
    let (extensions, extensions_bump) = find_extensions_pda(&s.escrow_pda);
    let ix = SetBeneficiaryBuilder::new()
        .payer(ctx.payer.pubkey())
        .admin(s.admin.pubkey())
        .beneficiary(beneficiary)
        .escrow(s.escrow_pda)
        .extensions(extensions)
        .extensions_bump(extensions_bump)
        .instruction();
    TestInstruction { instruction: ix, signers: vec![s.admin.insecure_clone()], name: "SetBeneficiary" }
        .send_expect_success(ctx);
    if let Some(t) = timeout {
        SetArbiterTimeoutFixture::build_with_escrow(ctx, s.escrow_pda, s.admin.insecure_clone(), t)
            .send_expect_success(ctx);
    }
    let beneficiary_ta = ctx.create_token_account(&beneficiary, &s.mint.pubkey());
    DealEscrow { s, arbiter, beneficiary, beneficiary_ta }
}

/// Client deposits one stage: the instruction and the stage receipt.
fn fund_stage_ix(ctx: &TestContext, d: &DealEscrow, stage: u8, amount: u64) -> (TestInstruction, Pubkey) {
    let s = &d.s;
    let seed = stage_seed(&CONTRACT_HASH, stage);
    let (allowed_mint, _) = find_allowed_mint_pda(&s.escrow_pda, &s.mint.pubkey());
    let (receipt, bump) = find_receipt_pda(&s.escrow_pda, &s.depositor.pubkey(), &s.mint.pubkey(), &seed.pubkey());
    let ix = DepositBuilder::new()
        .payer(ctx.payer.pubkey())
        .depositor(s.depositor.pubkey())
        .escrow(s.escrow_pda)
        .allowed_mint(allowed_mint)
        .receipt_seed(seed.pubkey())
        .receipt(receipt)
        .vault(s.vault)
        .depositor_token_account(s.depositor_token_account)
        .mint(s.mint.pubkey())
        .token_program(s.token_program)
        .extensions(s.extensions_pda)
        .bump(bump)
        .amount(amount)
        .instruction();
    (TestInstruction { instruction: ix, signers: vec![s.depositor.insecure_clone(), seed], name: "Deposit" }, receipt)
}

fn fund_stage(ctx: &mut TestContext, d: &DealEscrow, stage: u8, amount: u64) -> Pubkey {
    let (ix, receipt) = fund_stage_ix(ctx, d, stage, amount);
    ix.send_expect_success(ctx);
    receipt
}

fn settle(d: &DealEscrow, receipt: Pubkey, bps: u16) -> TestInstruction {
    let s = &d.s;
    let ix = SettleBuilder::new()
        .arbiter(d.arbiter.pubkey())
        .escrow(s.escrow_pda)
        .extensions(s.extensions_pda)
        .receipt(receipt)
        .vault(s.vault)
        .depositor(s.depositor.pubkey())
        .depositor_token_account(s.depositor_token_account)
        .beneficiary(d.beneficiary)
        .beneficiary_token_account(d.beneficiary_ta)
        .mint(s.mint.pubkey())
        .token_program(s.token_program)
        .payout_bps(bps)
        .instruction();
    TestInstruction { instruction: ix, signers: vec![d.arbiter.insecure_clone()], name: "Settle" }
}

/// Refund of one receipt; `with_arbiter` = co-signed by the oracle (before the timeout it must be).
fn refund(ctx: &TestContext, d: &DealEscrow, receipt: Pubkey, with_arbiter: bool) -> TestInstruction {
    let s = &d.s;
    let mut b = WithdrawBuilder::new();
    b.rent_recipient(ctx.payer.pubkey())
        .withdrawer(s.depositor.pubkey())
        .escrow(s.escrow_pda)
        .extensions(s.extensions_pda)
        .receipt(receipt)
        .vault(s.vault)
        .withdrawer_token_account(s.depositor_token_account)
        .mint(s.mint.pubkey())
        .token_program(s.token_program);
    let mut signers = vec![s.depositor.insecure_clone()];
    if with_arbiter {
        b.add_remaining_account(AccountMeta::new_readonly(d.arbiter.pubkey(), true));
        signers.push(d.arbiter.insecure_clone());
    }
    TestInstruction { instruction: b.instruction(), signers, name: "Withdraw" }
}

#[test]
fn test_stages_funded_upfront_release_split_refund() {
    // 3 stages funded at once: stage 1 approved (100 %), stage 2 settled by agreement (60 %),
    // stage 3 cancelled (refund). Every token ends up exactly where the deal says.
    let mut ctx = TestContext::new();
    let d = deal_escrow(&mut ctx, None);
    let client_before = ctx.get_token_balance(&d.s.depositor_token_account);
    let vault_before = ctx.get_token_balance(&d.s.vault);
    let (a1, a2, a3) = (300_000u64, 500_000u64, 200_000u64);

    let r1 = fund_stage(&mut ctx, &d, 0, a1);
    let r2 = fund_stage(&mut ctx, &d, 1, a2);
    let r3 = fund_stage(&mut ctx, &d, 2, a3);
    assert_eq!(ctx.get_token_balance(&d.s.vault), vault_before + a1 + a2 + a3);

    settle(&d, r1, 10_000).send_expect_success(&mut ctx);
    assert_eq!(ctx.get_token_balance(&d.beneficiary_ta), a1);

    settle(&d, r2, 6_000).send_expect_success(&mut ctx);
    assert_eq!(ctx.get_token_balance(&d.beneficiary_ta), a1 + a2 * 6 / 10);

    refund(&ctx, &d, r3, true).send_expect_success(&mut ctx);

    let paid = a1 + a2 * 6 / 10;
    assert_eq!(ctx.get_token_balance(&d.beneficiary_ta), paid);
    assert_eq!(ctx.get_token_balance(&d.s.depositor_token_account), client_before - paid);
    assert_eq!(ctx.get_token_balance(&d.s.vault), vault_before, "only the fixture's own deposit is left");
    for r in [r1, r2, r3] {
        assert!(ctx.get_account(&r).is_none(), "every stage receipt is closed");
    }
}

#[test]
fn test_stage_cannot_be_funded_twice() {
    let mut ctx = TestContext::new();
    let d = deal_escrow(&mut ctx, None);
    fund_stage(&mut ctx, &d, 0, 100_000);
    ctx.warp_to_slot(2); // new blockhash, so this is not rejected as a duplicate transaction
                         // Same stage seed → the receipt PDA already exists, so the stage cannot be funded twice.
    let (ix, _) = fund_stage_ix(&ctx, &d, 0, 100_000);
    ix.send_expect_error(&mut ctx);
}

#[test]
fn test_stage_seeds_are_deterministic_and_distinct() {
    assert_eq!(stage_seed(&CONTRACT_HASH, 1).pubkey(), stage_seed(&CONTRACT_HASH, 1).pubkey());
    assert_ne!(stage_seed(&CONTRACT_HASH, 0).pubkey(), stage_seed(&CONTRACT_HASH, 1).pubkey());
    assert_ne!(stage_seed(&CONTRACT_HASH, 0).pubkey(), stage_seed(&[7u8; 32], 0).pubkey());
}

#[test]
fn test_stages_funded_one_by_one() {
    // Client funds the next stage only when the previous one is approved.
    let mut ctx = TestContext::new();
    let d = deal_escrow(&mut ctx, None);
    let r1 = fund_stage(&mut ctx, &d, 0, 400_000);
    settle(&d, r1, 10_000).send_expect_success(&mut ctx);
    let r2 = fund_stage(&mut ctx, &d, 1, 600_000);
    settle(&d, r2, 10_000).send_expect_success(&mut ctx);
    assert_eq!(ctx.get_token_balance(&d.beneficiary_ta), 1_000_000);
}

#[test]
fn test_settled_stage_cannot_be_refunded_or_settled_again() {
    let mut ctx = TestContext::new();
    let d = deal_escrow(&mut ctx, None);
    let r1 = fund_stage(&mut ctx, &d, 0, 300_000);
    settle(&d, r1, 10_000).send_expect_success(&mut ctx);
    ctx.warp_to_slot(2);
    settle(&d, r1, 10_000).send_expect_error(&mut ctx);
    refund(&ctx, &d, r1, true).send_expect_error(&mut ctx);
    assert_eq!(ctx.get_token_balance(&d.beneficiary_ta), 300_000);
}

#[test]
fn test_client_cannot_refund_a_stage_alone_before_timeout() {
    let mut ctx = TestContext::new();
    let d = deal_escrow(&mut ctx, Some(30 * DAY));
    let r1 = fund_stage(&mut ctx, &d, 0, 300_000);
    let error = refund(&ctx, &d, r1, false).send_expect_error(&mut ctx);
    assert_escrow_error(error, EscrowError::InvalidArbiter);
}

#[test]
fn test_oracle_gone_client_recovers_unreleased_stages_after_timeout() {
    // Stage 1 was released before the oracle disappeared; stages 2 and 3 come back to the client after the timeout.
    let mut ctx = TestContext::new();
    let d = deal_escrow(&mut ctx, Some(30 * DAY));
    let client_before = ctx.get_token_balance(&d.s.depositor_token_account);
    let r1 = fund_stage(&mut ctx, &d, 0, 300_000);
    let r2 = fund_stage(&mut ctx, &d, 1, 500_000);
    let r3 = fund_stage(&mut ctx, &d, 2, 200_000);
    settle(&d, r1, 10_000).send_expect_success(&mut ctx);

    let now = ctx.get_current_timestamp();
    ctx.warp_to_timestamp(now + 30 * DAY as i64 + 1);
    refund(&ctx, &d, r2, false).send_expect_success(&mut ctx);
    refund(&ctx, &d, r3, false).send_expect_success(&mut ctx);

    assert_eq!(ctx.get_token_balance(&d.beneficiary_ta), 300_000, "released stage stays with the contractor");
    assert_eq!(ctx.get_token_balance(&d.s.depositor_token_account), client_before - 300_000);
}

#[test]
fn test_oracle_can_still_settle_after_timeout_until_client_withdraws() {
    let mut ctx = TestContext::new();
    let d = deal_escrow(&mut ctx, Some(DAY));
    let r1 = fund_stage(&mut ctx, &d, 0, 300_000);
    let now = ctx.get_current_timestamp();
    ctx.warp_to_timestamp(now + 2 * DAY as i64);
    settle(&d, r1, 5_000).send_expect_success(&mut ctx);
    assert_eq!(ctx.get_token_balance(&d.beneficiary_ta), 150_000);
}

#[test]
fn test_partial_payout_rounding_dust_returns_to_client() {
    let mut ctx = TestContext::new();
    let d = deal_escrow(&mut ctx, None);
    let client_before = ctx.get_token_balance(&d.s.depositor_token_account);
    let r = fund_stage(&mut ctx, &d, 0, 999_999);
    settle(&d, r, 3_333).send_expect_success(&mut ctx);
    let payout = 999_999u64 * 3_333 / 10_000; // 333_299 (rounded down)
    assert_eq!(ctx.get_token_balance(&d.beneficiary_ta), payout);
    assert_eq!(ctx.get_token_balance(&d.s.depositor_token_account), client_before - payout);
}

#[test]
fn test_minimal_and_almost_full_shares() {
    let mut ctx = TestContext::new();
    let d = deal_escrow(&mut ctx, None);
    let r1 = fund_stage(&mut ctx, &d, 0, 1_000_000);
    let r2 = fund_stage(&mut ctx, &d, 1, 1_000_000);
    settle(&d, r1, 1).send_expect_success(&mut ctx); // 0.01 %
    settle(&d, r2, 9_999).send_expect_success(&mut ctx); // 99.99 %
    assert_eq!(ctx.get_token_balance(&d.beneficiary_ta), 100 + 999_900);
}
