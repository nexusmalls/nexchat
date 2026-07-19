# Styx Module / Styx 模块

A module for burning native chain tokens in order to gain entry into a registry
for off-chain use.

该模块通过销毁链原生代币，获得进入链下用途注册表的资格。

## Overview / 概览

The Nexus port lets the signer burn native NEX through `pallet-balances`, and
lets governance update the price. The successful crossing is recorded for
off-chain registry use.

Nexus 移植版通过 `pallet-balances` 销毁签名者的原生 NEX，并允许治理更新价格；
成功跨越会写入链上记录，供链下注册表使用。

## Interface / 接口

### Dispatches / 调度调用

#### Public Dispatches / 公开调用

- `cross` - Burns native NEX and records the account in the registry.
- `cross` —— 销毁原生 NEX，并将账户写入注册表。

#### Admin Dispatches / 管理调用

The administrative dispatches are used to perform admin functions on chain:

管理调用用于执行链上管理功能：

- `set_burn_amount` - Sets the new burn price for the cross. Intended to be
  called by governance.
- `set_burn_amount` —— 设置新的跨越销毁价格，预期由治理调用。

`SetBurnAmountOrigin` is runtime-configurable. Nexus maps it to Root or the
Treasury Council threshold during production runtime wiring.

`SetBurnAmountOrigin` 可由 runtime 配置；Nexus 在生产 runtime 接线时将其映射到
Root 或 Treasury Council 门槛。
