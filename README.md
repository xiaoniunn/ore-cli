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