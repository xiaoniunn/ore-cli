# 低费率 + 指定难度挖

A command line interface for ORE cryptocurrency mining.

## Install

To install the CLI, use [cargo](https://doc.rust-lang.org/cargo/getting-started/installation.html):

```sh
cargo install ore-cli
```

## Build

To build the codebase from scratch, checkout the repo and use cargo to build:

```sh
 cargo build --release
```

## Help

You can use the `-h` flag on any command to pull up a help menu with documentation:

```sh
ore -h
```

--nandu 20 即可挖20难度以上的

如:
ore mine --threads 18 --priority-fee 610000 --nandu 20


## 开发不易  

## 捐赠sol地址： FyzVe531Atmptw94swtse48wV26mXyE285CeXTe5JyKf


注：引用了 https://github.com/a3165458/ore-cli 仓库的低费率版本  在此基础上魔改


## linux windows教程

可以自己使用命令  cargo build --release 编译为ore.exe程序，使用很简单

1.下载编译后的 ore.exe
2.创建1.json文件存放秘钥的，放在同文件夹

然后执行挖矿命令
ore.exe mine --threads 192 --rpc https://node.onekey.so/sol --keypair 1.json --priority-fee 130000 --nandu 20


## linux 使用教程

git clone https://github.com/xiaoniunn/ore-cli

cd ore-cli

cargo build --release

cd target/release

然后把json文件复制到这里面 

然后执行挖矿命令
./ore mine --threads 192 --rpc https://node.onekey.so/sol --keypair ./1.json --priority-fee 130000 --nandu 20