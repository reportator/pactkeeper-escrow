use pinocchio::error::ProgramError;

use crate::{require_len, traits::InstructionData};

/// Instruction data for SetArbiterTimeout (PactKeeper)
///
/// # Layout
/// * `extensions_bump` (u8) - Bump for extensions PDA
/// * `timeout` (u64) - Seconds after a deposit after which the depositor may withdraw it without the arbiter.
///   Must be > 0 and fit in i64.
pub struct SetArbiterTimeoutData {
    pub extensions_bump: u8,
    pub timeout: u64,
}

impl<'a> TryFrom<&'a [u8]> for SetArbiterTimeoutData {
    type Error = ProgramError;

    #[inline(always)]
    fn try_from(data: &'a [u8]) -> Result<Self, Self::Error> {
        require_len!(data, Self::LEN);

        let timeout = u64::from_le_bytes(data[1..9].try_into().unwrap());
        // 0 would make the arbiter optional from the start; remove the arbiter instead if that is intended.
        if timeout == 0 || timeout > i64::MAX as u64 {
            return Err(ProgramError::InvalidInstructionData);
        }

        Ok(Self { extensions_bump: data[0], timeout })
    }
}

impl<'a> InstructionData<'a> for SetArbiterTimeoutData {
    const LEN: usize = 1 + 8;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bytes(bump: u8, timeout: u64) -> [u8; 9] {
        let mut data = [0u8; 9];
        data[0] = bump;
        data[1..9].copy_from_slice(&timeout.to_le_bytes());
        data
    }

    #[test]
    fn test_set_arbiter_timeout_data_valid() {
        let parsed = SetArbiterTimeoutData::try_from(&bytes(254, 86_400)[..]).unwrap();
        assert_eq!(parsed.extensions_bump, 254);
        assert_eq!(parsed.timeout, 86_400);
    }

    #[test]
    fn test_set_arbiter_timeout_data_zero_rejected() {
        assert!(matches!(SetArbiterTimeoutData::try_from(&bytes(1, 0)[..]), Err(ProgramError::InvalidInstructionData)));
    }

    #[test]
    fn test_set_arbiter_timeout_data_over_i64_rejected() {
        let r = SetArbiterTimeoutData::try_from(&bytes(1, i64::MAX as u64 + 1)[..]);
        assert!(matches!(r, Err(ProgramError::InvalidInstructionData)));
    }

    #[test]
    fn test_set_arbiter_timeout_data_too_short() {
        assert!(matches!(SetArbiterTimeoutData::try_from(&[1u8, 2][..]), Err(ProgramError::InvalidInstructionData)));
    }
}
