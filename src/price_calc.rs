use alloy::{
    primitives::{Address, U256},
    signers::k256::elliptic_curve::rand_core::le,
};
use eyre::{OptionExt, Result, eyre};
use std::collections::HashMap;
use tracing::{debug, info, warn};

#[derive(Debug, Clone)]
pub struct PoolInfo {
    pub address: Address,
    pub token0: Address,
    pub token1: Address,
    pub reserve0: U256,
    pub reserve1: U256,
    pub fee: u32,         // should be in basis points like 30 which is 0.3%
    pub protocol: String, // uniswap v2 or v3 for example
}

#[derive(Debug, Clone)]
pub struct TokenInfo {
    pub address: Address,
    pub symbol: String,
    pub decimals: u8,
}

#[derive(Debug)]
pub struct PriceCalculator {
    pub pools: HashMap<Address, PoolInfo>,
    pub tokens: HashMap<Address, TokenInfo>,
    pub token_pairs_to_pool: HashMap<(Address, Address), Vec<Address>>,
}

impl PriceCalculator {
    pub fn new() -> Self {
        Self {
            pools: HashMap::new(),
            tokens: HashMap::new(),
            token_pairs_to_pool: HashMap::new(),
        }
    }

    pub fn register_pool(
        &mut self,
        address: Address,
        token0: Address,
        token1: Address,
        reserve0: U256,
        reserve1: U256,
        fee: u32,
        protocol: String,
    ) -> Result<()> {
        if !self.tokens.contains_key(&token0) {
            return Err(eyre!("Token0 is not registered: {}", token0));
        }

        if !self.tokens.contains_key(&token1) {
            return Err(eyre!("Token1 is not registered: {}", token1));
        }

        let pool_info = PoolInfo {
            address,
            token0,
            token1,
            reserve0,
            reserve1,
            fee,
            protocol: protocol.clone(),
        };

        self.pools.insert(address, pool_info);

        self.token_pairs_to_pool
            .entry((token0, token1))
            .or_insert_with(Vec::new)
            .push(address);

        self.token_pairs_to_pool
            .entry((token1, token0))
            .or_insert_with(Vec::new)
            .push(address);
        info!(
            pool_address = %address,
            token0 = %token0,
            token1 = %token1,
            protocol = %protocol,
            "Registered pool"
        );
        Ok(())
    }

    pub fn register_token(&mut self, address: Address, symbol: String, decimals: u8) -> Result<()> {
        self.tokens.insert(
            address,
            TokenInfo {
                address,
                symbol: symbol.clone(),
                decimals,
            },
        );
        info!(
            token_address = %address,
            symbol = %symbol,
            decimals = decimals,
            "Registered token"
        );
        Ok(())
    }
    /// gets the spot price of token b in terms of token a
    /// returns how many token b you get for 1 amount of token a
    pub fn get_spot_price(&self, token_a: &Address, token_b: &Address) -> Result<f64> {
        let pool_address = match self.token_pairs_to_pool.get(&(*token_a, *token_b)) {
            Some(pool) => pool,
            None => {
                return Err(eyre::eyre!(
                    "No pool found for token pair: {}, {}",
                    token_a,
                    token_b
                ));
            }
        };

        if pool_address.is_empty() {
            return Err(eyre::eyre!(
                "No active pools for token pair: {}, {}",
                token_a,
                token_b
            ));
        }

        //TODO: change this to get the pool with the best price later
        let pool_address = &pool_address[0];
        let pool = match self.pools.get(&pool_address.clone()) {
            Some(p) => p,
            None => return Err(eyre::eyre!("Pool data not found: {}", pool_address)),
        };

        let token_a_info = self
            .tokens
            .get(token_a)
            .ok_or_eyre(format!("Token Info not found for {}", token_a))
            .unwrap();
        let token_b_info = self
            .tokens
            .get(token_b)
            .ok_or_eyre(format!("Token Info not found for {}", token_a))
            .unwrap();

        // Calculate price based on which token is token0 and which is token1
        let (reserve_a, reserve_b, decimal_a, decimal_b) = if pool.token0 == *token_a {
            (
                pool.reserve0,
                pool.reserve1,
                token_a_info.decimals,
                token_b_info.decimals,
            )
        } else {
            (
                pool.reserve1,
                pool.reserve0,
                token_b_info.decimals,
                token_a_info.decimals,
            )
        };

        let reserve_a_normalized = to_float(reserve_a, decimal_a)?;
        let reserve_b_normalized = to_float(reserve_b, decimal_b)?;

        let spot_price = reserve_b_normalized / reserve_a_normalized;

        let fee_factor = 1.0 - (pool.fee as f64 / 10000.0);
        let price_after_fee = spot_price * fee_factor;
        Ok(price_after_fee)
    }

    /// calculates the output amount for a given input (considers price impact as well)
    pub fn calculate_output_amount(
        &self,
        input_token: &Address,
        output_token: &Address,
        input_amount: U256,
    ) -> Result<U256> {
        let pool_addresses = match self.token_pairs_to_pool.get(&(*input_token, *output_token)) {
            Some(addresses) => addresses,
            None => {
                return Err(eyre::eyre!(
                    "No pool found for token pair: {}, {}",
                    input_token,
                    output_token
                ));
            }
        };

        if pool_addresses.is_empty() {
            return Err(eyre::eyre!(
                "No active pools for token pair: {}, {}",
                input_token,
                output_token
            ));
        }

        //TODO: Change this to factor in the pool with the best price
        let pool_address = &pool_addresses[0];
        let pool = match self.pools.get(pool_address) {
            Some(p) => p,
            None => return Err(eyre::eyre!("Pool data not found: {}", pool_address)),
        };

        let (input_resever, output_reseve) = if pool.token0 == *input_token {
            (pool.reserve0, pool.reserve1)
        } else {
            (pool.reserve1, pool.reserve0)
        };

        // calculating using x*y = k formula
        // which for uniswap v2 should be -> output : y*dx / (x+ dy*(1-fee))
        let fee_denom = 10000 - pool.fee;
        let input_amount_with_fee = input_amount.checked_mul(U256::from(fee_denom)).unwrap();
        let numer = output_reseve.checked_mul(input_amount_with_fee).unwrap();
        let denom = input_resever
            .checked_mul(U256::from(10000))
            .unwrap()
            .checked_add(input_amount_with_fee)
            .unwrap();
        let output_amount = numer / denom;

        Ok(output_amount)
    }

    pub fn checking_arbitrage_opps(
        &self,
        token_a: &Address,
        token_b: &Address,
        input_amount: U256,
    ) -> Result<(bool, f64)> {
        //must have two pools at least
        let pools = match self.token_pairs_to_pool.get(&(*token_a, *token_b)) {
            Some(p) => p,
            None => return Err(eyre::eyre!("No pools for token pair")),
        };

        if pools.len() < 2 {
            return Ok((false, 0.0));
        }

        let mut best_buy_pool = None;
        let mut best_sell_pool = None;
        let mut best_buy_output = U256::ZERO;
        let mut best_sell_output = U256::ZERO;

        for pool_addr in pools {
            let output = self.calculate_output_amount(token_a, token_b, input_amount)?;

            if best_buy_pool.is_none() || output > best_buy_output {
                best_buy_pool = Some(pool_addr);
                best_buy_output = output;
            }

            let rev_output = self.calculate_output_amount(token_b, token_a, input_amount)?;

            if best_sell_pool.is_none() || rev_output > best_sell_output {
                best_sell_pool = Some(pool_addr);
                best_sell_output = rev_output;
            }
        }

        let profit = if best_sell_output > input_amount {
            let token_a_info = self.tokens.get(token_a).unwrap();
            let profit_amounts = best_sell_output - input_amount;
            let profit_normalized = to_float(profit_amounts, token_a_info.decimals)?;
            let input_normalized = to_float(input_amount, token_a_info.decimals)?;
            let profit_percentage = (profit_normalized / input_normalized) * 100.0;
            (true, profit_percentage)
        } else {
            (false, 0.0)
        };
        Ok(profit)
    }
    /// used to update a pool reserves when a mint/burn/swap event occurs
    pub fn update_pool_reserves(
        &mut self,
        pool_address: Address,
        pool_reserve0: U256,
        pool_reserve1: U256,
    ) -> Result<()> {
        match self.pools.get_mut(&pool_address) {
            Some(pool) => {
                debug!(
                    pool_address = %pool_address,
                    old_reserve0 = %pool.reserve0,
                    old_reserve1 = %pool.reserve1,
                    new_reserve0 = %pool_reserve0,
                    new_reserve1 = %pool_reserve1,
                    "Updating pool reserves"
                );

                pool.reserve0 = pool_reserve0;
                pool.reserve1 = pool_reserve1;
                Ok(())
            }
            None => {
                warn!(pool_address = %pool_address, "Pool not found");
                Err(eyre::eyre!("Pool not found: {}", pool_address))
            }
        }
    }
}

fn to_float(value: U256, decimals: u8) -> Result<f64> {
    let value_string = value.to_string();
    let value_f64 = value_string.parse::<f64>()?;

    let decimal_factor = 10.0_f64.powi(decimals as i32);
    Ok(value_f64 / decimal_factor)
}
