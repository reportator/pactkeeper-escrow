use pinocchio::{account::AccountView, error::ProgramError, Address, ProgramResult};
use pinocchio_token_2022::instructions::TransferChecked;

use crate::{
    errors::EscrowProgramError,
    events::SettleEvent,
    instructions::{Settle, BPS_DENOMINATOR},
    state::{
        get_extensions_from_account, validate_extensions_pda, ArbiterData, BeneficiaryData, Escrow, ExtensionType,
        HookData, HookPoint, Receipt, TimelockData,
    },
    traits::{AccountDeserialize, EventSerialize, ExtensionData},
    utils::{close_pda_account, emit_event, get_mint_decimals},
};

/// Share of `amount` for the beneficiary, rounded down (the remainder, incl. dust, returns to the depositor).
#[inline(always)]
pub fn payout_amount(amount: u64, payout_bps: u16) -> u64 {
    ((amount as u128) * (payout_bps as u128) / (BPS_DENOMINATOR as u128)) as u64
}

/// Processes the Settle instruction (PactKeeper).
///
/// The arbiter splits one receipt: `payout_bps` / 10000 to the beneficiary fixed on the escrow,
/// the rest back to the depositor. The receipt is closed and its rent returned to the depositor
/// (no separate rent recipient: the arbiter cannot route the rent elsewhere).
/// Refund (everything back) stays the regular Withdraw, co-signed by the arbiter.
pub fn process_settle(program_id: &Address, accounts: &[AccountView], instruction_data: &[u8]) -> ProgramResult {
    let ix = Settle::try_from((instruction_data, accounts))?;

    // Validate escrow PDA
    {
        let escrow_data = ix.accounts.escrow.try_borrow()?;
        let _escrow = Escrow::from_account(&escrow_data, ix.accounts.escrow, program_id)?;
    }

    // Read and validate receipt: it belongs to this escrow and to the given depositor
    let (amount, receipt_seed, receipt_mint, deposited_at) = {
        let receipt_data = ix.accounts.receipt.try_borrow()?;
        let receipt = Receipt::from_account(&receipt_data, ix.accounts.receipt, program_id)?;
        receipt.validate_depositor(ix.accounts.escrow.address(), ix.accounts.depositor.address())?;
        (receipt.amount, receipt.receipt_seed, receipt.mint, receipt.deposited_at)
    };

    // Same mint as deposited (prevents cross-mint payouts)
    if receipt_mint != *ix.accounts.mint.address() {
        return Err(ProgramError::InvalidAccountData);
    }

    // Extensions: arbiter and beneficiary are mandatory for Settle
    validate_extensions_pda(ix.accounts.escrow, ix.accounts.extensions, program_id)?;
    let exts = get_extensions_from_account(
        ix.accounts.extensions,
        &[ExtensionType::Timelock, ExtensionType::Hook, ExtensionType::Arbiter, ExtensionType::Beneficiary],
    )?;

    let (Some(arbiter_bytes), Some(beneficiary_bytes)) = (exts[2].as_ref(), exts[3].as_ref()) else {
        return Err(EscrowProgramError::SettleNotConfigured.into());
    };
    let arbiter = ArbiterData::from_bytes(arbiter_bytes)?;
    if ix.accounts.arbiter.address() != &arbiter.arbiter || !ix.accounts.arbiter.is_signer() {
        return Err(EscrowProgramError::InvalidArbiter.into());
    }
    BeneficiaryData::from_bytes(beneficiary_bytes)?.validate(ix.accounts.beneficiary)?;

    if let Some(ref timelock_bytes) = exts[0] {
        TimelockData::from_bytes(timelock_bytes)?.validate(deposited_at)?;
    }

    let hook_data = exts[1].as_ref().map(|b| HookData::from_bytes(b)).transpose()?;
    let hook_accounts = [ix.accounts.escrow, ix.accounts.depositor, ix.accounts.mint, ix.accounts.receipt];
    if let Some(ref hook) = hook_data {
        hook.invoke(HookPoint::PreWithdraw, ix.accounts.remaining_accounts, &hook_accounts)?;
    }

    // Split and transfer, escrow PDA signs
    let payout = payout_amount(amount, ix.data.payout_bps);
    let refund = amount - payout;
    let decimals = get_mint_decimals(ix.accounts.mint)?;
    {
        let escrow_data = ix.accounts.escrow.try_borrow()?;
        let escrow = Escrow::from_bytes(&escrow_data)?;
        escrow.with_signer(|signers| {
            if payout > 0 {
                TransferChecked {
                    from: ix.accounts.vault,
                    mint: ix.accounts.mint,
                    to: ix.accounts.beneficiary_token_account,
                    authority: ix.accounts.escrow,
                    amount: payout,
                    decimals,
                    token_program: ix.accounts.token_program.address(),
                }
                .invoke_signed(signers)?;
            }
            if refund > 0 {
                TransferChecked {
                    from: ix.accounts.vault,
                    mint: ix.accounts.mint,
                    to: ix.accounts.depositor_token_account,
                    authority: ix.accounts.escrow,
                    amount: refund,
                    decimals,
                    token_program: ix.accounts.token_program.address(),
                }
                .invoke_signed(signers)?;
            }
            Ok::<(), ProgramError>(())
        })?;
    }

    if let Some(ref hook) = hook_data {
        hook.invoke(HookPoint::PostWithdraw, ix.accounts.remaining_accounts, &hook_accounts)?;
    }

    // The receipt rent goes back to the client who paid it
    close_pda_account(ix.accounts.receipt, ix.accounts.depositor)?;

    let event = SettleEvent::new(
        *ix.accounts.escrow.address(),
        *ix.accounts.depositor.address(),
        *ix.accounts.beneficiary.address(),
        receipt_mint,
        receipt_seed,
        amount,
        ix.data.payout_bps,
        payout,
    );
    emit_event(program_id, ix.accounts.event_authority, ix.accounts.escrow_program, &event.to_bytes())?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_payout_amount() {
        assert_eq!(payout_amount(1000, 10000), 1000);
        assert_eq!(payout_amount(1000, 0), 0);
        assert_eq!(payout_amount(1000, 7500), 750);
        assert_eq!(payout_amount(999, 3333), 332); // rounded down, dust goes back to the depositor
        assert_eq!(payout_amount(u64::MAX, 10000), u64::MAX); // no overflow
    }
}
