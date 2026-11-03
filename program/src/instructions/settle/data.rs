use pinocchio::error::ProgramError;

use crate::{errors::EscrowProgramError, require_len, traits::InstructionData};

/// Basis points in 100 %.
pub const BPS_DENOMINATOR: u16 = 10_000;

/// Instruction data for Settle (PactKeeper)
///
/// # Layout
/// * `payout_bps` (u16, LE) - Share of the deposit for the beneficiary, 0..=10000
///   (10000 = everything to the contractor, 0 = everything back to the client)
pub struct SettleData {
    pub payout_bps: u16,
}

impl<'a> TryFrom<&'a [u8]> for SettleData {
    type Error = ProgramError;

    #[inline(always)]
    fn try_from(data: &'a [u8]) -> Result<Self, Self::Error> {
        require_len!(data, Self::LEN);

        let payout_bps = u16::from_le_bytes([data[0], data[1]]);
        if payout_bps > BPS_DENOMINATOR {
            return Err(EscrowProgramError::InvalidPayoutBps.into());
        }
        Ok(Self { payout_bps })
    }
}

impl<'a> InstructionData<'a> for SettleData {
    const LEN: usize = 2;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_settle_data_valid() {
        assert_eq!(SettleData::try_from(&7500u16.to_le_bytes()[..]).unwrap().payout_bps, 7500);
        assert_eq!(SettleData::try_from(&0u16.to_le_bytes()[..]).unwrap().payout_bps, 0);
        assert_eq!(SettleData::try_from(&10000u16.to_le_bytes()[..]).unwrap().payout_bps, 10000);
    }

    #[test]
    fn test_settle_data_over_100_percent() {
        let r = SettleData::try_from(&10001u16.to_le_bytes()[..]);
        assert_eq!(r.err(), Some(EscrowProgramError::InvalidPayoutBps.into()));
    }

    #[test]
    fn test_settle_data_too_short() {
        assert!(matches!(SettleData::try_from(&[1u8][..]), Err(ProgramError::InvalidInstructionData)));
    }
}
