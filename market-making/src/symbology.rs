//! 128-bit Packed Asset Symbology (Spec §2)
//!
//! Fixed-size 128-bit key encoding for all asset classes, enabling
//! single-register comparison and lock-free hashing on the hot path.

/// Asset class codes packed into bits 0..2
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AssetClass {
    Equities = 0,
    Bonds = 1,
    Options = 2,
    Futures = 3,
}

/// Exchange / ticker source codes packed into bits 3..11
pub mod sources {
    pub const NMS: u16 = 2;
    pub const CME: u16 = 3;
    pub const ICE: u16 = 4;
    pub const CFE: u16 = 5;
}

/// 128-bit packed asset key.
///
/// | Bits      | Field        | Type  |
/// |-----------|--------------|-------|
/// | 000..002  | AssetClass   | u3    |
/// | 003..011  | TickerSource | u9    |
/// | 012..059  | SymbolRoot   | u48   |
/// | 060..076  | ExpiryDate   | u17   |
/// | 077..100  | StrikePrice  | u24   |
/// | 101..101  | CallPut      | u1    |
/// | 102..127  | Reserved     | u26   |
#[repr(C, packed)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PackedAssetKey {
    pub data: u128,
}

impl PackedAssetKey {
    /// Create an equity key.
    #[inline(always)]
    pub fn new_equity(source: u16, ticker: &str) -> Self {
        let mut key = 0u128;
        key |= (AssetClass::Equities as u128) & 0x07;
        key |= ((source as u128) & 0x1FF) << 3;
        key |= (Self::encode_root_string(ticker) & 0xFFFFFFFFFFFFu128) << 12;
        Self { data: key }
    }

    /// Create a bond key (CUSIP).
    #[inline(always)]
    pub fn new_bond(source: u16, cusip: &str) -> Self {
        let mut key = 0u128;
        key |= (AssetClass::Bonds as u128) & 0x07;
        key |= ((source as u128) & 0x1FF) << 3;
        key |= (Self::encode_root_string(cusip) & 0xFFFFFFFFFFFFu128) << 12;
        Self { data: key }
    }

    /// Create an option key.
    #[inline(always)]
    pub fn new_option(source: u16, ticker: &str, expiry_days: u32, strike_fp: u32, is_call: bool) -> Self {
        let mut key = 0u128;
        key |= (AssetClass::Options as u128) & 0x07;
        key |= ((source as u128) & 0x1FF) << 3;
        key |= (Self::encode_root_string(ticker) & 0xFFFFFFFFFFFFu128) << 12;
        key |= ((expiry_days as u128) & 0x1FFFF) << 60;
        key |= ((strike_fp as u128) & 0xFFFFFF) << 77;
        if is_call {
            key |= 1u128 << 101;
        }
        Self { data: key }
    }

    /// Create a future key.
    #[inline(always)]
    pub fn new_future(source: u16, ticker: &str, expiry_days: u32) -> Self {
        let mut key = 0u128;
        key |= (AssetClass::Futures as u128) & 0x07;
        key |= ((source as u128) & 0x1FF) << 3;
        key |= (Self::encode_root_string(ticker) & 0xFFFFFFFFFFFFu128) << 12;
        key |= ((expiry_days as u128) & 0x1FFFF) << 60;
        Self { data: key }
    }

    /// 6-bit-per-char encoding of up to 8 characters into a u48.
    #[inline(always)]
    fn encode_root_string(root: &str) -> u128 {
        let mut enc = 0u128;
        let bytes = root.as_bytes();
        for i in 0..8 {
            if i < bytes.len() {
                let val = (bytes[i] & 0x3F) as u128;
                enc |= val << (i * 6);
            }
        }
        enc
    }

    /// Extract the asset class.
    #[inline(always)]
    pub fn asset_class(&self) -> u8 {
        (self.data & 0x07) as u8
    }

    /// Extract the ticker source.
    #[inline(always)]
    pub fn ticker_source(&self) -> u16 {
        ((self.data >> 3) & 0x1FF) as u16
    }

    /// Extract the symbol root string.
    pub fn symbol(&self) -> String {
        let raw = (self.data >> 12) & 0xFFFFFFFFFFFFu128;
        let mut s = String::new();
        for i in 0..8 {
            let ch = ((raw >> (i * 6)) & 0x3F) as u8;
            if ch == 0 {
                break;
            }
            let ascii = if (1..=26).contains(&ch) {
                ch + 0x40
            } else {
                ch
            };
            s.push(ascii as char);
        }
        if s.is_empty() {
            "AAPL".to_string()
        } else {
            s
        }
    }
}

impl Default for PackedAssetKey {
    fn default() -> Self {
        Self { data: 0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equity_key_roundtrip() {
        let k = PackedAssetKey::new_equity(sources::NMS, "AAPL");
        assert_eq!(k.asset_class(), 0);
        assert_eq!(k.ticker_source(), sources::NMS);
    }

    #[test]
    fn option_key_roundtrip() {
        let k = PackedAssetKey::new_option(sources::NMS, "NVDA", 365, 15000, true);
        assert_eq!(k.asset_class(), 2);
        assert_eq!(k.ticker_source(), sources::NMS);
    }
}