# Styx Module / Styx 模块

A module for burning native chain tokens in order to gain entry into a registry
for off-chain use.

该模块通过销毁链原生代币，获得进入链下用途注册表的资格。

## Overview / 概览

The pallet lets the signer burn native tokens, and lets governance update the
price. In the Zeitgeist ecosystem this grants the ability to claim the avatar of
the signer.

本 pallet 允许签名者销毁原生代币，并允许治理更新价格。在 Zeitgeist
生态中，这会授予签名者申领其头像的资格。

## Interface / 接口

### Dispatches / 调度调用

#### Public Dispatches / 公开调用

- `cross` - Burns native chain tokens to cross, granting the ability to claim
  your Zeitgeist avatar.
- `cross` —— 销毁原生代币以完成跨越，并获得申领 Zeitgeist 头像的资格。

#### Admin Dispatches / 管理调用

The administrative dispatches are used to perform admin functions on chain:

管理调用用于执行链上管理功能：

- `set_burn_amount` - Sets the new burn price for the cross. Intended to be
  called by governance.
- `set_burn_amount` —— 设置新的跨越销毁价格，预期由治理调用。

The origins from which the admin functions are called (`SetBurnAmountOrigin`)
are mainly minimum vote proportions from council.

管理函数的调用来源（`SetBurnAmountOrigin`）主要是委员会的最低投票比例。
