# Global Disputes / 全局争议

A module for setting one out of multiple outcomes with the most locked native
tokens as the canonical outcome.

本模块通过比较多个结果上锁定的原生代币总量，将锁定量最高的结果确定为规范结果。

## Overview

This is the default process when a dispute mechanism (e. g. Court) fails to
resolve. In the zeitgeist ecosystem this grants the ability to lock native
tokens by voting on one of multiple outcomes to determine the canonical outcome
on which the market finally resolves.

当其他争议机制（例如 Court）无法裁决时，这是默认的后备流程。参与者可锁定原生代币
对多个结果之一投票，并以锁定量最高的结果作为市场最终结算的规范结果。

## Terminology

- `outcome_sum` - The actual amount of native tokens for one outcome, which is
  used to calculate the outcome with the most locked native tokens.
- `outcome_sum`——某个结果锁定的原生代币总量，用于确定锁定量最高的结果。

## Interface

### Dispatches

#### Public Dispatches

- `add_vote_outcome` - Add voting outcome to a global dispute in exchange for a
  constant fee. Errors if the voting outcome already exists or if the global
  dispute has not started or has already finished.
- `add_vote_outcome`——支付固定费用，为全局争议增加可投票结果。
- `vote_on_outcome` - Vote on existing voting outcomes by locking native tokens.
  Fails if the global dispute has not started or has already finished.
- `vote_on_outcome`——锁定原生代币，对已有结果投票。
- `unlock_vote_balance` - Return all locked native tokens in a global dispute.
  If the global dispute is not concluded yet the lock remains.
- `unlock_vote_balance`——返还已结束全局争议中的投票锁定资金。
- `purge_outcomes` - Purge all outcomes to allow the winning outcome owner(s) to
  get their reward. Fails if the global dispute is not concluded yet.
- `purge_outcomes`——清理结果，使获胜结果的所有者能够领取奖励。
- `reward_outcome_owner` - Reward the collected fees to the owner(s) of a voting
  outcome. Fails if not all outcomes are already purged.
- `reward_outcome_owner`——将收取的费用奖励给获胜结果的所有者。
- `refund_vote_fees` - Return all vote funds and fees, when a global dispute was
  destroyed.
- `refund_vote_fees`——全局争议销毁后退还投票资金和费用。

#### Private Pallet API

- `push_vote_outcome` - Add an initial voting outcome and vote on it with
  `initial_vote_balance`.
- `push_vote_outcome`——增加初始投票结果及其初始投票金额。
- `determine_voting_winner` - Determine the canonical voting outcome based on
  total locked tokens.
- `determine_voting_winner`——根据锁定代币总量确定规范结果。
- `does_exist` - Check if the global dispute does already exist.
- `does_exist`——检查全局争议是否存在。
- `is_active` - Check if the global dispute is active to get votes
  (`vote_on_outcome`) and allow the addition of new voting outcomes with
  `add_vote_outcome`.
- `is_active`——检查全局争议是否仍可投票和增加结果。
- `start_global_dispute` - Start a global dispute.
- `start_global_dispute`——启动全局争议。
- `destroy_global_dispute` - Allow the users to get their voting funds and fee
  payments back.
- `destroy_global_dispute`——销毁全局争议并允许参与者取回资金和费用。
