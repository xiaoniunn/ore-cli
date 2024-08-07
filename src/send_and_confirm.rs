use std::time::Duration; // 导入用于时间处理的Duration模块

use colored::*; // 导入colored库，用于控制台输出的颜色和格式化
use solana_client::{
    client_error::{ClientError, ClientErrorKind, Result as ClientResult}, // 导入Solana客户端错误处理
    rpc_config::RpcSendTransactionConfig, // 导入RPC发送交易配置
};
use solana_program::{
    instruction::Instruction, // 导入指令模块
    native_token::{lamports_to_sol, sol_to_lamports}, // 导入Solana的单位转换功能（LAMPORTS和SOL）
};
use solana_rpc_client::spinner; // 导入spinner，用于控制台中的进度指示
use solana_sdk::{
    commitment_config::CommitmentLevel, // 导入承诺级别配置
    compute_budget::ComputeBudgetInstruction, // 导入计算预算指令
    signature::{Signature, Signer}, // 导入签名相关功能
    transaction::Transaction, // 导入交易相关功能
};
use solana_transaction_status::{TransactionConfirmationStatus, UiTransactionEncoding}; // 导入交易状态和编码

use crate::Miner; // 从当前crate导入Miner结构体

const MIN_SOL_BALANCE: f64 = 0.005; // 定义最低SOL余额常量

const RPC_RETRIES: usize = 0; // 定义RPC重试次数
const _SIMULATION_RETRIES: usize = 4; // 定义模拟重试次数
const GATEWAY_RETRIES: usize = 150; // 定义网关重试次数
const CONFIRM_RETRIES: usize = 1; // 定义确认重试次数

const CONFIRM_DELAY: u64 = 0; // 定义确认延迟（毫秒）
const GATEWAY_DELAY: u64 = 300; // 定义网关延迟（毫秒）

pub enum ComputeBudget {
    Dynamic, // 动态计算预算
    Fixed(u32), // 固定计算预算
}


impl Miner {
    // 异步方法：发送并确认交易
    pub async fn send_and_confirm(
        &self,
        ixs: &[Instruction], // 交易指令数组
        compute_budget: ComputeBudget, // 计算预算类型
        skip_confirm: bool, // 是否跳过确认
    ) -> ClientResult<Signature> { // 返回结果类型
        let progress_bar = spinner::new_progress_bar(); // 创建进度条
        let signer = self.signer(); // 获取签名者
        let client = self.rpc_client.clone(); // 克隆RPC客户端

        // 如果余额为零，则返回错误
        if let Ok(balance) = client.get_balance(&signer.pubkey()).await {
            if balance <= sol_to_lamports(MIN_SOL_BALANCE) { // 检查余额是否小于最低要求
                panic!( // 抛出错误
                    "{} Insufficient balance: {} SOL\nPlease top up with at least {} SOL",
                    "ERROR".bold().red(), // 格式化错误消息
                    lamports_to_sol(balance), // 转换余额为SOL
                    MIN_SOL_BALANCE // 显示最低余额要求
                );
            }
        }

        // 设置计算单位
        let mut final_ixs = vec![]; // 初始化最终指令数组
        match compute_budget { // 根据计算预算类型进行处理
            ComputeBudget::Dynamic => {
                // TODO: 模拟（待实现）
                final_ixs.push(ComputeBudgetInstruction::set_compute_unit_limit(1_400_000)) // 设置动态计算单位限制
            }
            ComputeBudget::Fixed(cus) => {
                final_ixs.push(ComputeBudgetInstruction::set_compute_unit_limit(cus)) // 设置固定计算单位限制
            }
        }
        final_ixs.push(ComputeBudgetInstruction::set_compute_unit_price( // 设置计算单位价格
            self.priority_fee, // 使用优先费用
        ));
        final_ixs.extend_from_slice(ixs); // 将指令数组添加到最终指令数组

        // 构建交易
        let send_cfg = RpcSendTransactionConfig { // 设置发送交易配置
            skip_preflight: true, // 跳过预检查
            preflight_commitment: Some(CommitmentLevel::Confirmed), // 预检查承诺级别
            encoding: Some(UiTransactionEncoding::Base64), // 设置编码格式
            max_retries: Some(RPC_RETRIES), // 设置最大重试次数
            min_context_slot: None, // 最小上下文槽
        };
        let mut tx = Transaction::new_with_payer(&final_ixs, Some(&signer.pubkey())); // 创建新的交易并设置付款者

        // 签名交易
        let (hash, _slot) = client
            .get_latest_blockhash_with_commitment(self.rpc_client.commitment()) // 获取最新区块哈希
            .await
            .unwrap(); // 解包结果
        tx.sign(&[&signer], hash); // 签名交易

        // 提交交易
        let mut attempts = 0; // 初始化尝试次数
        loop {
            progress_bar.set_message(format!("Submitting transaction... (attempt {})", attempts)); // 更新进度条消息
            match client.send_transaction_with_config(&tx, send_cfg).await { // 提交交易
                Ok(sig) => { // 如果提交成功
                    // 跳过确认
                    if skip_confirm {
                        progress_bar.finish_with_message(format!("Sent: {}", sig)); // 完成进度条并显示消息
                        return Ok(sig); // 返回签名
                    }

                    // 确认交易是否成功
                    for _ in 0..CONFIRM_RETRIES { // 根据确认重试次数进行循环
                        std::thread::sleep(Duration::from_millis(CONFIRM_DELAY)); // 延迟确认
                        match client.get_signature_statuses(&[sig]).await { // 获取签名状态
                            Ok(signature_statuses) => { // 如果获取成功
                                for status in signature_statuses.value { // 遍历状态
                                    if let Some(status) = status { // 如果状态存在
                                        if let Some(err) = status.err { // 如果存在错误
                                            progress_bar.finish_with_message(format!( // 完成进度条并显示错误消息
                                                "{}: {}",
                                                "ERROR".bold().red(),
                                                err
                                            ));
                                            return Err(ClientError { // 返回错误
                                                request: None,
                                                kind: ClientErrorKind::Custom(err.to_string()), // 自定义错误
                                            });
                                        }
                                        if let Some(confirmation) = status.confirmation_status { // 如果存在确认状态
                                            match confirmation { // 根据确认状态处理
                                                TransactionConfirmationStatus::Processed => {}
                                                TransactionConfirmationStatus::Confirmed
                                                | TransactionConfirmationStatus::Finalized => {
                                                    progress_bar.finish_with_message(format!( // 完成进度条并显示成功消息
                                                        "{} {}",
                                                        "OK".bold().green(),
                                                        sig
                                                    ));
                                                    return Ok(sig); // 返回签名
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            // 处理确认错误
                            Err(err) => {
                                progress_bar.set_message(format!( // 更新进度条消息
                                    "{}: {}",
                                    "ERROR".bold().red(),
                                    err.kind().to_string()
                                ));
                            }
                        }
                    }
                }

                // 处理提交错误
                Err(err) => {
                    progress_bar.set_message(format!( // 更新进度条消息
                        "{}: {}",
                        "ERROR".bold().red(),
                        err.kind().to_string()
                    ));
                }
            }

            // 重试
            std::thread::sleep(Duration::from_millis(GATEWAY_DELAY)); // 延迟重试
            attempts += 1; // 增加尝试次数
            if attempts > GATEWAY_RETRIES { // 如果超过最大重试次数
                progress_bar.finish_with_message(format!("{}: Max retries", "ERROR".bold().red())); // 完成进度条并显示错误消息
                return Err(ClientError { // 返回错误
                    request: None,
                    kind: ClientErrorKind::Custom("Max retries".into()), // 自定义最大重试错误
                });
            }
        }
    }

    // TODO: 模拟交易（待实现）
    fn _simulate(&self) {
        // 模拟交易
        // let mut sim_attempts = 0; // 初始化模拟尝试次数
        // 'simulate: loop {
        //     let sim_res = client // 获取模拟结果
        //         .simulate_transaction_with_config(
        //             &tx, // 传入交易
        //             RpcSimulateTransactionConfig { // 设置模拟配置
        //                 sig_verify: false,
        //                 replace_recent_blockhash: true,
        //                 commitment: Some(self.rpc_client.commitment()),
        //                 encoding: Some(UiTransactionEncoding::Base64),
        //                 accounts: None,
        //                 min_context_slot: Some(slot),
        //                 inner_instructions: false,
        //             },
        //         )
        //         .await;
        //     match sim_res { // 处理模拟结果
        //         Ok(sim_res) => {
        //             if let Some(err) = sim_res.value.err { // 如果存在错误
        //                 println!("Simulaton error: {:?}", err); // 打印错误
        //                 sim_attempts += 1; // 增加尝试次数
        //             } else if let Some(units_consumed) = sim_res.value.units_consumed { // 如果消耗的单位存在
        //                 if dynamic_cus { // 如果是动态计算预算
        //                     println!("Dynamic CUs: {:?}", units_consumed); // 打印消耗的单位
        //                     let cu_budget_ix = ComputeBudgetInstruction::set_compute_unit_limit(
        //                         units_consumed as u32 + 1000, // 设置计算单位限制
        //                     );
        //                     let cu_price_ix = // 设置计算单位价格
        //                         ComputeBudgetInstruction::set_compute_unit_price(self.priority_fee);
        //                     let mut final_ixs = vec![]; // 初始化最终指令数组
        //                     final_ixs.extend_from_slice(&[cu_budget_ix, cu_price_ix]); // 添加计算单位指令
        //                     final_ixs.extend_from_slice(ixs); // 添加其他指令
        //                     tx = Transaction::new_with_payer(&final_ixs, Some(&signer.pubkey())); // 创建新的交易
        //                 }
        //                 break 'simulate; // 退出模拟循环
        //             }
        //         }
        //         Err(err) => { // 处理模拟错误
        //             println!("Simulaton error: {:?}", err); // 打印错误
        //             sim_attempts += 1; // 增加尝试次数
        //         }
        //     }

        //     // 如果模拟失败则中止
        //     if sim_attempts.gt(&SIMULATION_RETRIES) { // 如果超过最大模拟重试次数
        //         return Err(ClientError { // 返回错误
        //             request: None,
        //             kind: ClientErrorKind::Custom("Simulation failed".into()), // 自定义模拟失败错误
        //         });
        //     }
        // }
    }
}