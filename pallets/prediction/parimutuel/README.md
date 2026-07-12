# Parimutuel Pallet / 彩池制 Pallet

The parimutuel pallet implements a straightforward "losers pay winners" market
maker for categorical prediction markets.
彩池制 pallet 为分类预测市场实现一种直接的“输家支付赢家”做市机制。

## Overview / 概览

Any participant can bet any amount while the market is active. The collateral
enters a shared pot, and the participant receives shares representing their
stake. After resolution, the pot is distributed proportionally among holders
of shares for the winning outcome.
市场活跃期间，任何参与者都可下注任意金额。抵押资产进入共享资金池，参与者获得代表其
份额的代币。市场结算后，资金池按比例分配给持有获胜结果份额的参与者。

Selling shares is not supported. Parimutuel scoring is restricted to categorical
markets; scalar markets are rejected. If nobody bet on the winning outcome,
participants may refund their original bets minus external fees.
本机制不支持卖出份额，且仅适用于分类市场；标量市场会被拒绝。若无人押中获胜结果，
参与者可取回扣除外部费用后的原始下注。

## Terminology / 术语

- _Collateral / 抵押资产_: The market base asset backing the pot.
  支撑资金池的市场基础资产。
- _External fees / 外部费用_: Fees distributed to configured recipients before
  shares are minted. 在铸造份额前分配给配置接收方的费用。
- _Pot / 资金池_: The pallet-derived account holding wagered collateral.
  保存下注抵押资产的 pallet 派生账户。

## Nexus Phase 1 boundary / Nexus 第一阶段边界

The pallet consumes existing market records and does not admit collateral
independently. Its mock permits native collateral and only the explicitly
whitelisted USDX foreign-asset fixture. Production foreign-collateral existence,
mirror validity, and pause/freeze checks remain at the market-creation boundary.
本 pallet 仅消费既有市场记录，不独立准入抵押资产。其 mock 允许原生抵押资产，并且仅
允许显式白名单中的 USDX 外部资产测试项。生产环境中的外部抵押资产存在性、镜像有效性
及暂停/冻结检查仍由市场创建边界负责。

The existential deposit of parimutuel shares must be at least the collateral
existential deposit so the pot cannot be dusted.
彩池份额的生存存款必须不低于抵押资产的生存存款，以避免资金池账户被清除。
