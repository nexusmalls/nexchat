# Orderbook Pallet / 订单簿 Pallet

A pallet of an on-chain order book, which allows to exchange the market's base
asset for outcome assets and vice versa.
链上订单簿 pallet，用于在预测市场基础资产与结果资产之间进行双向交换。

## Overview

The order book can be set as a market's scoring rule. It allows to place,
partially or fully fill and remove orders.
订单簿可作为市场的计分规则，支持挂单、部分或全部成交以及撤单。

## Terminology

- `maker_partial_fill`: The partial amount of what the maker wants to get filled.
  maker 希望成交的部分数量。
- `maker_fill`: The amount of what the maker wants to get filled.
  maker 希望成交的数量。
- `taker_fill`: The amount of what the taker wants to fill.
  taker 希望成交的数量。
- `maker_asset`: The asset that the maker wants to sell.
  maker 希望出售的资产。
- `maker_amount`: The amount of the asset that the maker wants to sell.
  maker 希望出售的资产数量。
- `taker_asset`: The asset that the taker needs to have to buy the maker's
  asset.
  taker 为购买 maker 资产而必须持有的资产。
- `taker_amount`: The amount of the asset that the taker needs to have to buy
  the maker's asset.
  taker 为购买 maker 资产而必须持有的资产数量。

## Notes

- Orders must always bid or ask for the corresponding market's base asset.
  订单的买入或卖出一侧必须是对应市场的基础资产。
- External fees are always paid in the market's base asset after the order is
  filled. In particular, the recipient of the collateral pays the fee. The
  implementation, however, arranges the transfers slightly differently for
  convenience:
  外部费用始终在订单成交后以市场基础资产支付，抵押资产接收方承担费用；
  为便于实现，实际转账顺序略有不同：
  - If the order is an ask (maker sells outcome tokens), then the external fees
    are taken (not charged!) from the taker before the order is executed. The
    taker still receives the full amount of outcome tokens, but the maker
    receives only an adjusted amount.
    对于 ask（maker 卖出结果代币），外部费用在执行订单前从 taker 的付款中扣除；
    taker 仍收到全部结果代币，而 maker 收到扣费后的基础资产。
  - If the order is a bid (maker buys outcome tokens), then the external fees
    are charged from the taker after the transaction is executed. In particular,
    the maker still receives the full amount of outcome tokens.
    对于 bid（maker 买入结果代币），外部费用在执行交易后向 taker 收取；
    maker 仍收到全部结果代币。

The pallet consumes market records and does not admit collateral independently.
The Nexus mock accepts native collateral and only the explicitly whitelisted USDX
foreign-asset fixture; production collateral validation remains a market-creation
responsibility.
本 pallet 仅消费既有市场记录，不独立准入抵押资产。Nexus mock 接受原生抵押资产，
并仅允许显式白名单中的 USDX 外部资产测试项；生产环境的抵押资产校验仍由市场创建边界负责。

## Interface

### Dispatches

#### Public Dispatches

- `remove_order`: Allows a user to remove their order from the order book.
  允许用户撤销自己的订单。
- `fill_order`: Used to fill an order either partially or completely.
  用于部分或全部成交订单。
- `place_order`: Places a new order into the order book.
  在订单簿中创建新订单。
