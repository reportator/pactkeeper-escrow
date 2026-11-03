use pinocchio::{account::AccountView, error::ProgramError};

use crate::{
    traits::InstructionAccounts,
    utils::{
        validate_associated_token_account, verify_current_program, verify_current_program_account,
        verify_event_authority, verify_owned_by, verify_readonly, verify_signer, verify_system_program,
        verify_token_program, verify_writable,
    },
};

/// Accounts for the Settle instruction (PactKeeper)
///
/// The arbiter (PactKeeper oracle) splits one deposit receipt between the beneficiary
/// (contractor) and the depositor (client) and closes the receipt.
///
/// # Account Layout
/// 0. `[signer]` arbiter - Must match the escrow's Arbiter extension
/// 1. `[]` escrow - Escrow PDA (signing authority for vault transfers)
/// 2. `[]` extensions - Extensions PDA (must hold Arbiter and Beneficiary)
/// 3. `[writable]` receipt - Deposit receipt to settle and close
/// 4. `[writable]` vault - Escrow's vault token account (source)
/// 5. `[writable]` depositor - Receipt depositor (client); receives the receipt rent
/// 6. `[writable]` depositor_token_account - Depositor's ATA (gets the remainder)
/// 7. `[]` beneficiary - Must match the escrow's Beneficiary extension
/// 8. `[writable]` beneficiary_token_account - Beneficiary's ATA (gets the payout)
/// 9. `[]` mint - Token mint
/// 10. `[]` token_program - SPL Token program
/// 11. `[]` system_program - System program
/// 12. `[]` event_authority - Event authority PDA
/// 13. `[]` escrow_program - Current program
///
/// # Remaining Accounts
/// If hook extension is set:
///   0. `[]` hook_program - The hook program to invoke (PreWithdraw / PostWithdraw)
///   1..M. `[]` extra accounts - Additional accounts to pass to the hook (all read-only)
pub struct SettleAccounts<'a> {
    pub arbiter: &'a AccountView,
    pub escrow: &'a AccountView,
    pub extensions: &'a AccountView,
    pub receipt: &'a AccountView,
    pub vault: &'a AccountView,
    pub depositor: &'a AccountView,
    pub depositor_token_account: &'a AccountView,
    pub beneficiary: &'a AccountView,
    pub beneficiary_token_account: &'a AccountView,
    pub mint: &'a AccountView,
    pub token_program: &'a AccountView,
    pub system_program: &'a AccountView,
    pub event_authority: &'a AccountView,
    pub escrow_program: &'a AccountView,
    pub remaining_accounts: &'a [AccountView],
}

impl<'a> TryFrom<&'a [AccountView]> for SettleAccounts<'a> {
    type Error = ProgramError;

    #[inline(always)]
    fn try_from(accounts: &'a [AccountView]) -> Result<Self, Self::Error> {
        let [arbiter, escrow, extensions, receipt, vault, depositor, depositor_token_account, beneficiary, beneficiary_token_account, mint, token_program, system_program, event_authority, escrow_program, remaining_accounts @ ..] =
            accounts
        else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };

        // 1. Validate signers
        verify_signer(arbiter, false)?;

        // 2. Validate writable
        verify_writable(receipt, true)?;
        verify_writable(vault, true)?;
        verify_writable(depositor, true)?;
        verify_writable(depositor_token_account, true)?;
        verify_writable(beneficiary_token_account, true)?;

        // 3. Validate readonly
        verify_readonly(escrow)?;
        verify_readonly(extensions)?;
        verify_readonly(beneficiary)?;
        verify_readonly(mint)?;

        // 4. Validate program IDs
        verify_token_program(token_program)?;
        verify_owned_by(mint, token_program.address())?;
        verify_system_program(system_program)?;
        verify_current_program(escrow_program)?;
        verify_event_authority(event_authority)?;

        // 5. Validate accounts owned by current program
        verify_current_program_account(escrow)?;
        verify_current_program_account(receipt)?;

        // 6. Validate ATAs: the tokens can only go to the depositor's and the beneficiary's own accounts
        validate_associated_token_account(vault, escrow.address(), mint, token_program)?;
        validate_associated_token_account(depositor_token_account, depositor.address(), mint, token_program)?;
        validate_associated_token_account(beneficiary_token_account, beneficiary.address(), mint, token_program)?;

        Ok(Self {
            arbiter,
            escrow,
            extensions,
            receipt,
            vault,
            depositor,
            depositor_token_account,
            beneficiary,
            beneficiary_token_account,
            mint,
            token_program,
            system_program,
            event_authority,
            escrow_program,
            remaining_accounts,
        })
    }
}

impl<'a> InstructionAccounts<'a> for SettleAccounts<'a> {}
