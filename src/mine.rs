use std::{sync::Arc,sync::Mutex,time::Instant}; // 导入必要的模块，用于并发和计时
use colored::*; // 导入colored，用于控制台输出格式化
use drillx::{equix::{self}, Hash, Solution}; // 导入drillx中的equix模块，及相关的Hash和Solution结构
use ore_api::{consts::{BUS_ADDRESSES, BUS_COUNT, EPOCH_DURATION}, state::{Config, Proof}}; // 导入常量和状态管理
use rand::Rng; // 导入随机数生成器特征
use solana_program::pubkey::Pubkey; // 导入Pubkey，用于Solana程序的地址表示
use solana_rpc_client::spinner; // 导入spinner，用于控制台中的进度指示
use solana_sdk::signer::Signer; // 导入Signer，用于加密操作

use crate::{ // 从当前crate导入必要的组件
    args::MineArgs, // 矿工操作的参数
    send_and_confirm::ComputeBudget, // 交易的计算预算管理
    utils::{amount_u64_to_string, get_clock, get_config, get_proof_with_authority, proof_pubkey}, // 工具函数
    Miner, // 导入Miner结构体
};

// Miner结构体的实现块
impl Miner {
    // 异步方法：矿工进行挖矿操作
    pub async fn mine(&self, args: MineArgs) {
        // 注册，如果需要的话。
        let signer = self.signer(); // 获取签名者
        self.open().await; // 打开矿工

        // 检查线程数量
        self.check_num_cores(args.threads); // 检查可用的线程数是否合适

        // 开始挖矿循环
        loop {
            // 获取证明
            let proof = get_proof_with_authority(&self.rpc_client, signer.pubkey()).await; // 获取带有权威的证明
            println!(
                "\nStake balance: {} ORE",
                amount_u64_to_string(proof.balance) // 打印当前的ORE余额
            );
			println!("{:?}", args);
			
            // 计算截止时间
            let cutoff_time = self.get_cutoff(proof, args.buffer_time).await; // 获取截止时间

            // 运行drillx
            let config = get_config(&self.rpc_client).await; // 获取当前配置
			
			//let min_difficulty_c = 14;
			println!("{:?}", config);
			
			
            let solution = Self::find_hash_par(
                proof,
                cutoff_time,
                args.threads,
                args.nandu as u32,
            )
            .await; // 并行查找哈希

			//println!("{:?}",solution);

            // 提交最难的哈希
            let mut compute_budget = 500_000; // 初始化计算预算
            let mut ixs = vec![ore_api::instruction::auth(proof_pubkey(signer.pubkey()))]; // 创建交易指令
            if self.should_reset(config).await && rand::thread_rng().gen_range(0..100).eq(&0) {
                compute_budget += 100_000; // 增加计算预算
                ixs.push(ore_api::instruction::reset(signer.pubkey())); // 添加重置指令
            }
            ixs.push(ore_api::instruction::mine(
                signer.pubkey(),
                signer.pubkey(),
                find_bus(), // 查找并返回bus地址
                solution, // 提交找到的解决方案
            ));
            self.send_and_confirm(&ixs, ComputeBudget::Fixed(compute_budget), false) // 发送并确认交易
                .await
                .ok(); // 忽略结果
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
							
                           // if difficulty.gt(&best_difficulty) { // 如果难度更高
                           //     best_nonce = nonce; // 更新最佳nonce
                           //     best_difficulty = difficulty; // 更新最佳难度
                            //    best_hash = hx; // 更新最佳哈希
                           // }
							
                        }
						
						/*
						
						if best_difficulty > min_difficulty {
							
							
							
							
							let mut solution_found = found_solution.lock().unwrap();
                            *solution_found = true;
                            println!("Thread {} found a solution: {:?}", i, best_difficulty);
                            break;
						} else if i == 0 { // 如果是第一个线程
							progress_bar.set_message(format!(
								"Mining... ({} sec remaining)", // 更新进度条消息
								0,
							));
						}
*/
                        // 增加nonce
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

    // 检查可用的核心数量
    pub fn check_num_cores(&self, threads: u64) {
        // 检查线程数
        let num_cores = num_cpus::get() as u64; // 获取可用核心数
        if threads.gt(&num_cores) { // 如果线程数超过可用核心数
            println!(
                "{} Number of threads ({}) exceeds available cores ({})", // 打印警告信息
                "WARNING".bold().yellow(),
                threads,
                num_cores
            );
        }
    }

    // 异步方法：检查是否需要重置
    async fn should_reset(&self, config: Config) -> bool {
        let clock = get_clock(&self.rpc_client).await; // 获取当前时钟
		
		
		
        config
            .last_reset_at
            .saturating_add(150) // 计算下次重置的时间
            .saturating_sub(5) // 设置缓冲
            .le(&clock.unix_timestamp) // 检查是否时间到了
    }

    // 异步方法：获取截止时间
    async fn get_cutoff(&self, proof: Proof, buffer_time: u64) -> u64 {
        let clock = get_clock(&self.rpc_client).await; // 获取当前时钟
        proof
            .last_hash_at
            .saturating_add(150) // 添加60秒的时间
            .saturating_sub(buffer_time as i64) // 减去缓冲时间
            .saturating_sub(clock.unix_timestamp) // 减去当前时间
            .max(0) as u64 // 确保不小于0
    }
}

// TODO: 选择更好的策略（避免耗尽bus）
fn find_bus() -> Pubkey {
    let i = rand::thread_rng().gen_range(0..BUS_COUNT); // 生成随机数，以选择bus
    BUS_ADDRESSES[i] // 返回对应的bus地址
}