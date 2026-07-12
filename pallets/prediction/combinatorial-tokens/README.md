# Combinatorial Tokens Module

The combinatorial-tokens module implements modern Zeitgeist's method of
creating and destroying outcome tokens.

组合代币模块实现 Zeitgeist 现代化的结果代币创建与销毁机制。Nexus Phase 1
保留上游业务语义，但不接入生产 runtime。

## Overview

In a categorical or scalar prediction market, one unit of a complete set (i.e. one unit of each outcome token of the market) always redeems for one unit of collateral.

In a Yes/No market, for instance, holding `x` units of Yes and `x` units of No means that, when the market resolves, you will always receive `x` units of collateral. In a scalar market, on the other hand, `x` units of Long and `x` units of Short will always redeem to a total of `x` units of collateral, as well.

This means that buying and selling collateral for complete sets should be allowed. For example, `x` units of collateral should fetch `x` units of complete set, and vice versa. Buying complete sets can be thought of as splitting collateral into outcome tokens, while selling complete sets can be thought of as merging outcome tokens back into collateral.

The combinatorial-tokens module generalizes this approach to not only allow splitting and merging into collateral, but also splitting and merging into outcome tokens of multiple different markets. This allows us to create outcome tokens that combine multiple events. They are called _combinatorial tokens_.

For example, splitting an `A` token from one categorical market using another categorical market with two outcomes `X` and `Y` yields `A & X` and `A & Y` tokens. They represent the event that `A` and `X` (resp. `Y`) occur. Splitting a Yes token from a binary market using a scalar market will give `Yes & Long` and `Yes & Short` tokens. They represent Long/Short tokens contingent on `Yes` occurring.

In addition to splitting and merging, combinatorial tokens can be redeemed if one of the markets involved in creating them has been resolved. For example, if the `XY` market above resolves to `X`, then every unit of `X & A` redeems for a unit of `A` and `Y & A` is worthless. If the scalar market above resolves so that `Long` is valued at `.4` and `Short` at `.6`, then every unit of `Yes & Long` redeems for `.4` units of `Yes` and every unit of `Yes & Short` redeems for `.6`.

An important distinction which we've so far neglected to make is the distinction between an abstract _collection_ like `X & A` or `Yes & Short` and a concrete _position_, which is a collection together with a collateral token against which it is valued. Collections are purely abstract and used in the implementation. Positions are actual tokens on the chain.

Collections and positions are identified using their IDs. When using the standard combinatorial ID Manager, this ID is a 256 bit value. The position ID of a certain token can be calculated using the collection ID and the collateral.

### Terminology

- _Combinatorial token_: Any instance of `zeitgeist_primitives::Asset::CombinatorialToken`.
- _Complete set (of a prediction market)_: An abstract set containing every outcome of a particular prediction market. One unit of a complete set is one unit of each outcome token from the market in question. After the market resolves, a complete set always redeems for exactly one unit of collateral.
- _Merge_: The process of exchanging multiple tokens for a single token of equal value.
- _Split_: The process of exchanging a token for more complicated tokens of equal value.

### Combinatorial ID Manager

Calculating 

alt_bn128

combinatorial tokens, as [defined by
Gnosis](https://gnosis-conditional-tokens.readthedocs.io/en/latest/developer-guide.html#) in Substrate.

## 中文概述

在分类或标量预测市场中，一套完整结果代币最终总能兑回一单位抵押资产。
本模块把“抵押资产与完整套装之间拆分/合并”的能力推广到多个市场：某个市场的头寸
可按另一市场的结果继续拆分，生成同时依赖多个事件的组合代币。

模块支持反向合并，并在相关市场完成裁决后按 payout vector 赎回组合头寸。“集合”
表示抽象事件组合，“头寸”则是集合与具体抵押资产的组合；链上实际持有的是头寸。
标准 ID 管理器使用 256 位标识符，并依据父集合、市场、结果集合和抵押资产确定集合 ID
及头寸 ID。

Phase 1 mock 永久不包含 parachain、XCM 或 ORML asset-registry 路径。组合代币 pallet
只读取已经存在的市场抵押资产，不负责新市场的抵押准入。生产接线前，Phase 2
必须在市场创建边界执行实时资产存在性、镜像有效性和暂停/冻结状态校验；在此之前，
mock 市场仅可使用共享静态策略允许的 native 资产或 USDX fixture `900_000`。
