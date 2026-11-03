use alloc::vec::Vec;
use pinocchio::{account::AccountView, cpi::Seed, error::ProgramError, Address, ProgramResult};

use crate::{
    events::ArbiterTimeoutSetEvent,
    instructions::SetArbiterTimeout,
    state::{update_or_append_extension, ArbiterTimeoutData, Escrow, ExtensionType, ExtensionsPda},
    traits::{EventSerialize, ExtensionData, PdaSeeds},
    utils::emit_event,
};

/// Processes the SetArbiterTimeout instruction (PactKeeper).
///
/// Sets (or updates) how long after a deposit the arbiter must act; afterwards the depositor may
/// withdraw without the arbiter. Creates the extensions PDA if it doesn't exist.
pub fn process_set_arbiter_timeout(
    program_id: &Address,
    accounts: &[AccountView],
    instruction_data: &[u8],
) -> ProgramResult {
    let ix = SetArbiterTimeout::try_from((instruction_data, accounts))?;

    // Read escrow and validate
    let escrow_data = ix.accounts.escrow.try_borrow()?;
    let escrow = Escrow::from_account(&escrow_data, ix.accounts.escrow, program_id)?;
    escrow.validate_admin(ix.accounts.admin.address())?;
    escrow.require_mutable()?;

    // Validate extensions PDA
    let extensions_pda = ExtensionsPda::new(ix.accounts.escrow.address());
    extensions_pda.validate_pda(ix.accounts.extensions, program_id, ix.data.extensions_bump)?;

    // Build extension data
    let timeout_bytes = ArbiterTimeoutData::new(ix.data.timeout).to_bytes();

    // Get seeds and append/update extension
    let extensions_bump_seed = [ix.data.extensions_bump];
    let extensions_seeds: Vec<Seed> = extensions_pda.seeds_with_bump(&extensions_bump_seed);
    let extensions_seeds_array: [Seed; 3] = extensions_seeds.try_into().map_err(|_| ProgramError::InvalidArgument)?;

    update_or_append_extension(
        ix.accounts.payer,
        ix.accounts.extensions,
        program_id,
        ix.data.extensions_bump,
        ExtensionType::ArbiterTimeout,
        &timeout_bytes,
        extensions_seeds_array,
    )?;

    // Emit event
    let event = ArbiterTimeoutSetEvent::new(*ix.accounts.escrow.address(), ix.data.timeout);
    emit_event(program_id, ix.accounts.event_authority, ix.accounts.escrow_program, &event.to_bytes())?;

    Ok(())
}
