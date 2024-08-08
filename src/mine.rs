use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};
use std::thread;

use colored::*;
use drillx::{
    equix::{self},
    Hash, Solution,
};
use ore_api::{
    consts::{BUS_ADDRESSES, BUS_COUNT, EPOCH_DURATION},
    state::{Config, Proof},
};
use rand::Rng;
use solana_program::pubkey::Pubkey;
use solana_rpc_client::spinner;
use solana_sdk::signer::Signer;

use crate::{
    args::MineArgs,
    send_and_confirm::ComputeBudget,
    utils::{amount_u64_to_string, get_clock, get_config, get_proof_with_authority, proof_pubkey},
    Miner,
};

impl Miner {
    pub async fn mine(&self, args: MineArgs) {
        // Register, if needed.
        let signer = self.signer();
        self.open().await;

        // Check num threads
        self.check_num_cores(args.threads);

        // Start mining loop
        loop {
            // Fetch proof
            let proof = get_proof_with_authority(&self.rpc_client, signer.pubkey()).await;
            println!(
                "\nStake balance: {} ORE",
                amount_u64_to_string(proof.balance)
            );
println!("{:?}", args);
            // Calc cutoff time
            let cutoff_time = self.get_cutoff(proof, args.buffer_time).await;

            // Run drillx
            let config = get_config(&self.rpc_client).await;
println!("{:?}", config);			
			
			
            let solution = Self::find_hash_par(
                proof,
                cutoff_time,
                args.threads,
                args.nandu as u32,
            )
            .await;

            // Submit most difficult hash
            let mut compute_budget = 500_000;
            let mut ixs = vec![ore_api::instruction::auth(proof_pubkey(signer.pubkey()))];
            if self.should_reset(config).await && rand::thread_rng().gen_range(0..100).eq(&0) {
                compute_budget += 100_000;
                ixs.push(ore_api::instruction::reset(signer.pubkey()));
            }
            ixs.push(ore_api::instruction::mine(
                signer.pubkey(),
                signer.pubkey(),
                find_bus(),
                solution,
            ));
            self.send_and_confirm(&ixs, ComputeBudget::Fixed(compute_budget), false)
                .await
                .ok();
        }
    }

        // 异步方法：并行查找哈希
async fn find_hash_par(
    proof: Proof,
    cutoff_time: u64,
    threads: u64,
    min_difficulty: u32,
) -> Solution {
    let progress_bar = Arc::new(spinner::new_progress_bar());
    progress_bar.set_message("Mining...");

    let found_solution = Arc::new(AtomicBool::new(false));
    let handles: Vec<_> = (0..threads)
        .map(|i| {
            let proof = proof.clone();
            let progress_bar = progress_bar.clone();
            let found_solution = found_solution.clone();

            let mut memory = equix::SolverMemory::new();
            thread::spawn(move || {
                let mut nonce = i; // 每个线程从不同的nonce开始
                let mut best_difficulty = 0;
                let mut best_hash = Hash::default();

                while !found_solution.load(Ordering::Relaxed) {
                    if let Ok(hx) = drillx::hash_with_memory(
                        &mut memory,
                        &proof.challenge,
                        &nonce.to_le_bytes(),
                    ) {
                        let difficulty = hx.difficulty();
                        if difficulty > min_difficulty {
                            found_solution.store(true, Ordering::Relaxed);
                            println!("Success! Thread {} found a difficulty: {:?}", i, difficulty);
                            return (nonce, difficulty, hx);
                        }
                    }
                    nonce += threads; // 递增nonce，确保每个线程处理不同的nonce
                }
                (0, 0, Hash::default())
            })
        })
        .collect();

    let mut best_nonce = 0;
    let mut best_difficulty = 0;
    let mut best_hash = Hash::default();
    for h in handles {
        if let Ok((nonce, difficulty, hash)) = h.join() {
            if difficulty > best_difficulty {
                best_difficulty = difficulty;
                best_nonce = nonce;
                best_hash = hash;
            }
        }
    }

    progress_bar.finish_with_message(format!(
        "Best hash: {} (difficulty: {})",
        bs58::encode(best_hash.h).into_string(),
        best_difficulty
    ));

    Solution::new(best_hash.d, best_nonce.to_le_bytes())
}

    pub fn check_num_cores(&self, threads: u64) {
        // Check num threads
        let num_cores = num_cpus::get() as u64;
        if threads.gt(&num_cores) {
            println!(
                "{} Number of threads ({}) exceeds available cores ({})",
                "WARNING".bold().yellow(),
                threads,
                num_cores
            );
        }
    }

    async fn should_reset(&self, config: Config) -> bool {
        let clock = get_clock(&self.rpc_client).await;
        config
            .last_reset_at
            .saturating_add(EPOCH_DURATION)
            .saturating_sub(5) // Buffer
            .le(&clock.unix_timestamp)
    }

    async fn get_cutoff(&self, proof: Proof, buffer_time: u64) -> u64 {
        let clock = get_clock(&self.rpc_client).await;
        proof
            .last_hash_at
            .saturating_add(60)
            .saturating_sub(buffer_time as i64)
            .saturating_sub(clock.unix_timestamp)
            .max(0) as u64
    }
}

// TODO Pick a better strategy (avoid draining bus)
fn find_bus() -> Pubkey {
    let i = rand::thread_rng().gen_range(0..BUS_COUNT);
    BUS_ADDRESSES[i]
}
