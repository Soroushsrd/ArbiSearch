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

pub fn decode_swap_event(log: &Log) -> Result<(Address, U256, U256, U256, U256, Address)> {
    let swap_event_signature =
        hex::decode("d78ad95fa46c994b6551d0da85fc275fe613ce37657fb8d5e3d130840159d822")
            .map_err(|e| format!("Failed to decode event signature: {}", e))
            .unwrap();

    let topic0 = log
        .topic0()
        .ok_or_else(|| "missing topic0".to_string())
        .unwrap();

    if topic0 != swap_event_signature.as_slice() {
        return Err(eyre::eyre!("not a Swap event"));
    }

    if log.topics().len() < 3 {
        return Err(eyre::eyre!("invalid Swap event"));
    }

    let sender = Address::from_slice(&log.topics()[1].as_slice()[12..]);
    let to = Address::from_slice(&log.topics()[2].as_slice()[12..]);

    let amount0_in = U256::from_be_slice(&log.data().data[0..32]);
    let amount1_in = U256::from_be_slice(&log.data().data[32..64]);
    let amount0_out = U256::from_be_slice(&log.data().data[64..96]);
    let amount1_out = U256::from_be_slice(&log.data().data[96..128]);

    Ok((sender, amount0_in, amount1_in, amount0_out, amount1_out, to))
}
pub fn format_swap_data(
    pool_address: &Address,
    sender: &Address,
    amount0_in: U256,
    amount1_in: U256,
    amount0_out: U256,
    amount1_out: U256,
    to: &Address,
) -> String {
    let (token0, token1, decimals0, decimals1) =
        match pool_address.to_string().to_lowercase().as_str() {
            // USDC/ETH pair
            "0xb4e16d0168e52d35cacd2c6185b44281ec28c9dc" => ("USDC", "ETH", 6, 18),
            // ETH/USDT pair
            "0x0d4a11d5eeaac28ec3f61d100daf4d40471f1852" => ("ETH", "USDT", 18, 6),
            _ => ("Token0", "Token1", 18, 18),
        };

    let decimal_factor0 = U256::from(10).pow(U256::from(decimals0));
    let decimal_factor1 = U256::from(10).pow(U256::from(decimals1));

    // Determine swap direction and format appropriately
    let swap_description = if amount0_in > U256::from(0) {
        // Swapping token0 for token1
        let amount_in = amount0_in;
        let amount_out = amount1_out;

        let whole_in = amount_in / decimal_factor0;
        let fractional_in = amount_in % decimal_factor0;
        let whole_out = amount_out / decimal_factor1;
        let fractional_out = amount_out % decimal_factor1;

        let formatted_amount_in = format!("{}.{}", whole_in, fractional_in);
        let formatted_amount_out = format!("{}.{}", whole_out, fractional_out);

        format!(
            "Swap: {} {} -> {} {} (from {} to {})",
            formatted_amount_in,
            token0,
            formatted_amount_out,
            token1,
            sender.to_string(),
            to.to_string()
        )
    } else {
        // Swapping token1 for token0
        let amount_in = amount1_in;
        let amount_out = amount0_out;

        let whole_in = amount_in / decimal_factor1;
        let fractional_in = amount_in % decimal_factor1;
        let whole_out = amount_out / decimal_factor0;
        let fractional_out = amount_out % decimal_factor0;

        let formatted_amount_in = format!("{}.{}", whole_in, fractional_in);
        let formatted_amount_out = format!("{}.{}", whole_out, fractional_out);

        format!(
            "Swap: {} {} -> {} {} (from {} to {})",
            formatted_amount_in,
            token1,
            formatted_amount_out,
            token0,
            sender.to_string(),
            to.to_string()
        )
    };

    swap_description
}

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
