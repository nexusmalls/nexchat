# Futarchy Module

The futarchy module provides a straightforward, "no bells and whistles"
implementation of the
[futarchy governance system](https://docs.zeitgeist.pm/docs/learn/futarchy).

## Overview

The futarchy module is essentially an oracle based governance system: When a
proposal is submitted, an oracle is specified which evaluates whether the
proposal should be executed. The type of the oracle is configured using the
associated type `Oracle`, which must implement `FutarchyOracle`.

The typical oracle implementation for futarchy is the `DecisionMarketOracle`
implementation exposed by the neo-swaps module, which allows making decisions
based on prices in prediction markets. A `DecisionMarketOracle` is defined by
providing a pool ID and two outcomes, the _positive_ and _negative_ outcome. The
oracle evaluates positively (meaning that it will allow the proposal to pass) if
and only if the positive outcome is more valuable than the negative outcome over
a period of time for a certain absolute and relative threshold determined by a
`DecisionMarketOracleScoreboard`.

The Nexus governance flow is the following:

- Root or the configured Nexus Technical Committee threshold calls
  `submit_proposal`; Nexus does not add `pallet-democracy` for Futarchy.
- Root 或配置的 Nexus Technical Committee 门槛调用 `submit_proposal`；Nexus
  不为 Futarchy 新增 `pallet-democracy`。
- Wait until the `duration` specified in `submit_proposal` has passed. The
  oracle will be automatically evaluated and will either schedule
  `proposal.call` at `proposal.when` where `proposal` is the proposal specified
  in `submit_proposal`.

### Terminology

- _Call_: Refers to an on-chain extrinsic call.
- _Oracle_: A means of making a decision about a proposal. At any block, an
  oracle evaluates to `true` (proposal is accepted) or `false` (proposal is
  rejected).
- _Proposal_: Consists of a call, an oracle and a time of execution. If and only
  if the proposal is accepted, the call is scheduled for the specified time of
  execution.
