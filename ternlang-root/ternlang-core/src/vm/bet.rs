use crate::trit::Trit;

#[derive(Debug, PartialEq, Eq)]
pub enum BetFault {
    InvalidState(u8), // The 0b00 state
}

pub fn pack_trits(trits: &[Trit]) -> Vec<u8> {
    let mut packed = Vec::with_capacity((trits.len() + 3) / 4);
    for chunk in trits.chunks(4) {
        let mut byte = 0u8;
        for (i, &trit) in chunk.iter().enumerate() {
            let bits = match trit {
                Trit::Reject => 0b01,
                Trit::Tend   => 0b11,
                Trit::Affirm => 0b10,
            };
            byte |= bits << (i * 2);
        }
        packed.push(byte);
    }
    packed
}

pub fn unpack_trits(bytes: &[u8], count: usize) -> Result<Vec<Trit>, BetFault> {
    let mut trits = Vec::with_capacity(count);
    for (byte_idx, &byte) in bytes.iter().enumerate() {
        for bit_idx in 0..4 {
            if trits.len() >= count {
                break;
            }
            let bits = (byte >> (bit_idx * 2)) & 0b11;
            let trit = match bits {
                0b01 => Trit::Reject,
                0b11 => Trit::Tend,
                0b10 => Trit::Affirm,
                0b00 => return Err(BetFault::InvalidState(byte_idx as u8)),
                _ => unreachable!(),
            };
            trits.push(trit);
        }
    }
    Ok(trits)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_packing_unpacking() {
        let trits = vec![Trit::Reject, Trit::Tend, Trit::Affirm, Trit::Reject, Trit::Tend];
        let packed = pack_trits(&trits);
        assert_eq!(packed.len(), 2);
        
        let unpacked = unpack_trits(&packed, trits.len()).unwrap();
        assert_eq!(trits, unpacked);
    }

    #[test]
    fn test_invalid_state() {
        let packed = vec![0b00]; // Invalid 0b00 state
        let result = unpack_trits(&packed, 1);
        assert_eq!(result, Err(BetFault::InvalidState(0)));
    }
}
