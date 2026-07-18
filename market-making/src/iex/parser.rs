//! IEX PCAP Parser + CSV Fallback
//!
//! Parses IEX historical PCAP files containing TOPS, DEEP, and trade messages.
//! Also supports a CSV fallback format for testing without PCAP files.
//!
//! IEX PCAP format:
//! - Standard PCAP global header (24 bytes)
//! - Per-packet: PCAP packet header (16 bytes) + Ethernet(14) + IP(20) + UDP(8) + IEX payload
//! - IEX payload: IEX header (40 bytes) + message bodies

use super::{BookSide, MarketEvent};
use std::fs::File;
use std::io::{BufRead, BufReader, Read};

// IEX message type codes
const MSG_TYPE_QUOTE: u8 = 0x51; // 'Q' - Quote update (TOPS)
const MSG_TYPE_TRADE: u8 = 0x54; // 'T' - Trade report
const MSG_TYPE_PRICE_LEVEL: u8 = 0x38; // '8' - Price level update (DEEP)

// PCAP magic numbers
const PCAP_MAGIC_NATIVE: u32 = 0xA1B2C3D4;
const PCAP_MAGIC_SWAPPED: u32 = 0xD4C3B2A1;

/// IEX PCAP file parser.
pub struct IexPcapParser {
    is_swapped: bool,
}

impl IexPcapParser {
    pub fn new() -> Self {
        Self { is_swapped: false }
    }

    /// Parse a PCAP file and return all market events.
    pub fn parse_file(&self, path: &str) -> Result<Vec<MarketEvent>, String> {
        let mut file =
            File::open(path).map_err(|e| format!("Failed to open PCAP file {}: {}", path, e))?;
        let mut data = Vec::new();
        file.read_to_end(&mut data)
            .map_err(|e| format!("Failed to read PCAP file: {}", e))?;

        self.parse_bytes(&data)
    }

    /// Parse raw PCAP bytes into market events.
    pub fn parse_bytes(&self, data: &[u8]) -> Result<Vec<MarketEvent>, String> {
        if data.len() < 24 {
            return Err("PCAP data too short for global header".to_string());
        }

        let magic = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        let is_swapped = match magic {
            PCAP_MAGIC_NATIVE => false,
            PCAP_MAGIC_SWAPPED => true,
            _ => return Err(format!("Invalid PCAP magic number: 0x{:08X}", magic)),
        };

        let mut events = Vec::new();
        let mut offset = 24; // Skip global header

        while offset + 16 <= data.len() {
            let pkt_header = &data[offset..offset + 16];
            let (ts_sec, ts_usec, incl_len) = if is_swapped {
                (
                    u32::from_be_bytes([pkt_header[0], pkt_header[1], pkt_header[2], pkt_header[3]]),
                    u32::from_be_bytes([pkt_header[4], pkt_header[5], pkt_header[6], pkt_header[7]]),
                    u32::from_be_bytes([pkt_header[8], pkt_header[9], pkt_header[10], pkt_header[11]]),
                )
            } else {
                (
                    u32::from_le_bytes([pkt_header[0], pkt_header[1], pkt_header[2], pkt_header[3]]),
                    u32::from_le_bytes([pkt_header[4], pkt_header[5], pkt_header[6], pkt_header[7]]),
                    u32::from_le_bytes([pkt_header[8], pkt_header[9], pkt_header[10], pkt_header[11]]),
                )
            };

            let timestamp_ns = (ts_sec as u64) * 1_000_000_000 + (ts_usec as u64) * 1_000;
            let pkt_data_start = offset + 16;
            let pkt_data_end = pkt_data_start + incl_len as usize;

            if pkt_data_end > data.len() {
                break;
            }

            let pkt_data = &data[pkt_data_start..pkt_data_end];

            // Skip Ethernet (14) + IP (20) + UDP (8) = 42 bytes to reach IEX payload
            if pkt_data.len() > 42 {
                let iex_payload = &pkt_data[42..];
                self.parse_iex_payload(iex_payload, timestamp_ns, &mut events);
            }

            offset = pkt_data_end;
        }

        Ok(events)
    }

    /// Parse IEX payload messages from a single packet.
    fn parse_iex_payload(&self, payload: &[u8], base_ts: u64, events: &mut Vec<MarketEvent>) {
        // IEX header is 40 bytes
        if payload.len() < 40 {
            return;
        }

        let msg_count = u16::from_le_bytes([payload[16], payload[17]]) as usize;
        let mut offset = 40;

        for _ in 0..msg_count {
            if offset + 2 > payload.len() {
                break;
            }

            let msg_type = payload[offset];
            let content_start = offset + 2;

            if content_start + 8 > payload.len() {
                break;
            }

            let symbol_bytes = &payload[content_start..content_start + 8];
            let symbol = String::from_utf8_lossy(symbol_bytes).trim().to_string();

            match msg_type {
                MSG_TYPE_QUOTE => {
                    if content_start + 32 <= payload.len() {
                        let bid_price = read_iex_price(&payload[content_start + 8..]);
                        let bid_size = u32::from_le_bytes([
                            payload[content_start + 16],
                            payload[content_start + 17],
                            payload[content_start + 18],
                            payload[content_start + 19],
                        ]) as f64;
                        let ask_price = read_iex_price(&payload[content_start + 20..]);
                        let ask_size = u32::from_le_bytes([
                            payload[content_start + 28],
                            payload[content_start + 29],
                            payload[content_start + 30],
                            payload[content_start + 31],
                        ]) as f64;

                        events.push(MarketEvent::QuoteUpdate {
                            symbol,
                            bid_price,
                            bid_size,
                            ask_price,
                            ask_size,
                            timestamp_ns: base_ts,
                        });
                    }
                }
                MSG_TYPE_TRADE => {
                    if content_start + 20 <= payload.len() {
                        let price = read_iex_price(&payload[content_start + 8..]);
                        let size = u32::from_le_bytes([
                            payload[content_start + 16],
                            payload[content_start + 17],
                            payload[content_start + 18],
                            payload[content_start + 19],
                        ]) as f64;

                        events.push(MarketEvent::Trade {
                            symbol,
                            price,
                            size,
                            timestamp_ns: base_ts,
                        });
                    }
                }
                MSG_TYPE_PRICE_LEVEL => {
                    if content_start + 21 <= payload.len() {
                        let side_byte = payload[content_start + 8];
                        let side = if side_byte == b'B' {
                            BookSide::Bid
                        } else {
                            BookSide::Ask
                        };
                        let price = read_iex_price(&payload[content_start + 9..]);
                        let size = u32::from_le_bytes([
                            payload[content_start + 17],
                            payload[content_start + 18],
                            payload[content_start + 19],
                            payload[content_start + 20],
                        ]) as f64;

                        events.push(MarketEvent::PriceLevelUpdate {
                            symbol,
                            side,
                            price,
                            size,
                            timestamp_ns: base_ts,
                        });
                    }
                }
                _ => {}
            }

            offset = content_start + 32;
            if offset >= payload.len() {
                break;
            }
        }
    }
}

impl Default for IexPcapParser {
    fn default() -> Self {
        Self::new()
    }
}

/// Read an IEX price field (4-byte signed integer scaled by 10000).
#[inline(always)]
fn read_iex_price(data: &[u8]) -> f64 {
    if data.len() < 4 {
        return 0.0;
    }
    let raw = i32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    raw as f64 / 10_000.0
}

/// CSV fallback parser for testing without PCAP files.
///
/// Expected CSV format:
/// ```text
/// timestamp_ns,symbol,event_type,bid_price,bid_size,ask_price,ask_size,trade_price,trade_size
/// ```
pub struct CsvParser;

impl CsvParser {
    /// Parse a CSV file into market events.
    pub fn parse_file(path: &str) -> Result<Vec<MarketEvent>, String> {
        let file =
            File::open(path).map_err(|e| format!("Failed to open CSV file {}: {}", path, e))?;
        let reader = BufReader::new(file);
        let mut events = Vec::new();

        for (line_num, line) in reader.lines().enumerate() {
            let line = line.map_err(|e| format!("Error reading line {}: {}", line_num, e))?;
            if line_num == 0 {
                continue; // Skip header
            }
            if line.trim().is_empty() {
                continue;
            }

            if let Some(event) = Self::parse_csv_line(&line) {
                events.push(event);
            }
        }

        Ok(events)
    }

    fn parse_csv_line(line: &str) -> Option<MarketEvent> {
        let fields: Vec<&str> = line.split(',').collect();
        if fields.len() < 3 {
            return None;
        }

        let timestamp_ns: u64 = fields[0].trim().parse().ok()?;
        let symbol = fields[1].trim().to_string();
        let event_type = fields[2].trim();

        match event_type {
            "Q" | "quote" => {
                if fields.len() >= 7 {
                    let bid_price: f64 = fields[3].trim().parse().ok()?;
                    let bid_size: f64 = fields[4].trim().parse().ok()?;
                    let ask_price: f64 = fields[5].trim().parse().ok()?;
                    let ask_size: f64 = fields[6].trim().parse().ok()?;
                    Some(MarketEvent::QuoteUpdate {
                        symbol,
                        bid_price,
                        bid_size,
                        ask_price,
                        ask_size,
                        timestamp_ns,
                    })
                } else {
                    None
                }
            }
            "T" | "trade" => {
                if fields.len() >= 9 {
                    let trade_price: f64 = fields[7].trim().parse().ok()?;
                    let trade_size: f64 = fields[8].trim().parse().ok()?;
                    Some(MarketEvent::Trade {
                        symbol,
                        price: trade_price,
                        size: trade_size,
                        timestamp_ns,
                    })
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

/// Load market events from a file, auto-detecting PCAP vs CSV format.
pub fn load_events(path: &str) -> Result<Vec<MarketEvent>, String> {
    if path.ends_with(".pcap") || path.ends_with(".pcap.gz") {
        let parser = IexPcapParser::new();
        parser.parse_file(path)
    } else if path.ends_with(".csv") {
        CsvParser::parse_file(path)
    } else {
        Err(format!("Unknown file format: {}", path))
    }
}

/// Generate synthetic market data for testing when no IEX data is available.
pub fn generate_synthetic_events(symbol: &str, n_events: usize) -> Vec<MarketEvent> {
    let mut events = Vec::with_capacity(n_events);
    let mut price = 150.0_f64;

    for i in 0..n_events {
        let ts = (i as u64) * 1_000_000; // 1ms apart
        let drift = ((i as f64) * 0.001).sin() * 0.05;
        price += drift;

        let spread = 0.02;
        let bid = price - spread / 2.0;
        let ask = price + spread / 2.0;
        let bid_sz = 100.0 + ((i as f64) * 0.7).sin().abs() * 400.0;
        let ask_sz = 100.0 + ((i as f64) * 0.5).cos().abs() * 400.0;

        events.push(MarketEvent::QuoteUpdate {
            symbol: symbol.to_string(),
            bid_price: bid,
            bid_size: bid_sz,
            ask_price: ask,
            ask_size: ask_sz,
            timestamp_ns: ts,
        });

        // Occasionally add a trade
        if i % 5 == 0 {
            let trade_price = if i % 10 == 0 { ask } else { bid };
            let trade_size = 50.0 + (i as f64 % 200.0);
            events.push(MarketEvent::Trade {
                symbol: symbol.to_string(),
                price: trade_price,
                size: trade_size,
                timestamp_ns: ts + 500,
            });
        }
    }

    events
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthetic_events_generated() {
        let events = generate_synthetic_events("AAPL", 100);
        assert!(!events.is_empty());
        assert!(events.len() >= 100);
    }

    #[test]
    fn csv_parse_quote_line() {
        let line = "1000,AAPL,Q,149.98,500,150.02,500,,";
        let event = CsvParser::parse_csv_line(line);
        assert!(event.is_some());
        if let Some(MarketEvent::QuoteUpdate { symbol, .. }) = event {
            assert_eq!(symbol, "AAPL");
        } else {
            panic!("Expected QuoteUpdate");
        }
    }

    #[test]
    fn csv_parse_trade_line() {
        let line = "1000,AAPL,T,,,,,150.00,100";
        let event = CsvParser::parse_csv_line(line);
        assert!(event.is_some());
        if let Some(MarketEvent::Trade { price, size, .. }) = event {
            assert!((price - 150.0).abs() < 1e-9);
            assert!((size - 100.0).abs() < 1e-9);
        } else {
            panic!("Expected Trade");
        }
    }
}