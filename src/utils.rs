//#[derive(Debug)]
//pub struct Erc20Transfer {
//    pub token_address: Address,
//    pub from_address: Address,
//    pub to_address: Address,
//    pub amount: U256,
//    pub amount_formatted: String,
//    pub token_symbol: &'static str, // Would usually come from token metadata
//    pub token_decimals: u8,         // Would usually come from token metadata
//}

use alloy::{
    hex,
    primitives::{Address, U256},
    rpc::types::Log,
};
use eyre::Result;

pub fn decode_transfer_event(log: &Log) -> Result<(Address, Address, U256)> {
    let transfer_event_signature =
        hex::decode("ddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef")
            .map_err(|e| format!("Failed to decode event signature: {}", e))
            .unwrap();

    let topic0 = log
        .topic0()
        .ok_or_else(|| "Missing topic0".to_string())
        .unwrap();
    if topic0.as_slice() != transfer_event_signature.as_slice() {
        return Err(eyre::eyre!("not a Transfer event"));
    }

    if log.topics().len() < 3 {
        return Err(eyre::eyre!("invalid transfer event"));
    }
    let from = Address::from_slice(&log.topics()[1].as_slice()[12..]);
    let to = Address::from_slice(&log.topics()[2].as_slice()[12..]);
    let amount = U256::from_be_slice(log.data().data.as_ref());
    Ok((from, to, amount))
}

pub fn format_transfer_data(
    token_address: &Address,
    from: &Address,
    to: &Address,
    amount: U256,
) -> String {
    // You can add token decimal handling here if needed
    // For USDT, divide by 10^6, for UNI or USDC, divide by 10^18, etc.

    let decimals = match token_address.to_string().to_lowercase().as_str() {
        "0xdac17f958d2ee523a2206206994597c13d831ec7" => 6, // USDT
        //"0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48" => 6, // USDC
        //"0x1f9840a85d5af5bf1d1762f925bdaddc4201f984" => 18, // UNI
        _ => 18, // Default to 18 decimals for most ERC-20 tokens
    };

    let decimal_factor = U256::from(10).pow(U256::from(decimals));
    let whole_units = amount / decimal_factor;
    let fractional = amount % decimal_factor;

    let token_symbol = match token_address.to_string().to_lowercase().as_str() {
        "0xdac17f958d2ee523a2206206994597c13d831ec7" => "USDT",
        "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48" => "USDC",
        "0x1f9840a85d5af5bf1d1762f925bdaddc4201f984" => "UNI",
        _ => "tokens",
    };

    let formatted_amount = format!("{}.{}", whole_units, fractional);

    format!(
        "Transfer: {} {} from {} to {}",
        formatted_amount,
        token_symbol,
        from.to_string(),
        to.to_string()
    )
}
