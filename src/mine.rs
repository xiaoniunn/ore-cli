use std::{sync::Arc,sync::Mutex,time::Instant}; // 导入必要的模块，用于并发和计时

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
    // 为每个线程分配工作
    let progress_bar = Arc::new(spinner::new_progress_bar()); // 创建进度条
    progress_bar.set_message("Mining..."); // 设置消息为"挖矿中..."
	
  // 使用Arc和Mutex来共享退出状态
    let found_solution = Arc::new(Mutex::new(false)); // 共享的退出状态
    let handles: Vec<_> = (0..threads)
        .map(|i| {
            std::thread::spawn({
                let proof = proof.clone(); // 克隆证明
                let progress_bar = progress_bar.clone(); // 克隆进度条
				
				let found_solution = found_solution.clone(); // 克隆共享状态

				
                let mut memory = equix::SolverMemory::new(); // 创建新的求解器内存
                move || {
                    let mut nonce = u64::MAX.saturating_div(threads).saturating_mul(i); // 计算初始nonce
                    let mut best_nonce = nonce; // 初始化最佳nonce
                    let mut best_difficulty = 0; // 初始化最佳难度
                    let mut best_hash = Hash::default(); // 初始化最佳哈希
                    loop {
						
						// 检查共享状态
						if *found_solution.lock().unwrap() {
							break; // 如果找到了解，则退出循环
						}
						
						 // 创建哈希
                        if let Ok(hx) = drillx::hash_with_memory(
                            &mut memory,
                            &proof.challenge,
                            &nonce.to_le_bytes(), // 使用nonce生成哈希
                        ) {
                            let difficulty = hx.difficulty(); // 获取哈希的难度
							if best_difficulty >10 {
								println!("{:?}",best_difficulty);
							}
							
							if difficulty > min_difficulty {
								best_difficulty=difficulty;
								best_nonce = nonce;
								best_hash = hx; 
								
								let mut solution_found = found_solution.lock().unwrap();
								*solution_found = true;
								println!("mini Success , Thread {} found a difficulty: {:?}", i, best_difficulty);
								return (best_nonce, best_difficulty, best_hash);
							} 
							
                        }
						
                        nonce += 1; // 增加nonce
                    }
					(best_nonce, best_difficulty, best_hash) 
                }
            })
        })
        .collect();


	//println!("{:?}","线程结束");
    // 等待线程完成并返回最佳nonce
    let mut best_nonce = 0; // 初始化最佳nonce
    let mut best_difficulty = 0; // 初始化最佳难度
    let mut best_hash = Hash::default(); // 初始化最佳哈希
    for h in handles {
        if let Ok((nonce, difficulty, hash)) = h.join() {
            if difficulty > best_difficulty {
                best_difficulty = difficulty; // 更新最佳难度
                best_nonce = nonce; // 更新最佳nonce
                best_hash = hash; // 更新最佳哈希
            }
        }
    }

    // 更新日志
    progress_bar.finish_with_message(format!(
        "Best hash: {} (difficulty: {})", // 打印最佳哈希和难度
        bs58::encode(best_hash.h).into_string(),
        best_difficulty
    ));

    Solution::new(best_hash.d, best_nonce.to_le_bytes()) // 返回新的解决方案
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
