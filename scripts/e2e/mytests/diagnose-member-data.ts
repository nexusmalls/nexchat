#!/usr/bin/env tsx
/**
 * 检查 entityMembers 数据和 MemberProvider trait 实现
 */

process.env.WS_URL ??= 'ws://202.140.140.202:9944';

import { connectApi, disconnectApi } from '../framework/api.js';
import { codecToJson, codecToHuman, readObjectField, coerceNumber } from '../framework/codec.js';
import { asBigInt } from '../framework/units.js';

const ENTITY_ID = 100000;

const ALL_ACCOUNTS = [
  'X4WMbyCMgCpMJzwg1cdWQuPRRfQiu8ifrJmfLdurviJcTXW94',
  'X4XLb5Z7rDcjhQ6EajaLXXdPwGBDkBTES7FpALx1WHKrX7pHr',
  'X4Z7fwhyLddkWyM1kCbGoK3Mdt9hUkfdawSyA5hrsdYTdmyXh',
  'X4V886aGbFF11h8KaZZNRjGGgKhxVcVoEgjJW1J7E9Zht8NK7',
];

function shortAddr(addr: string): string {
  if (!addr || addr.length < 16) return addr;
  return `${addr.slice(0, 8)}...${addr.slice(-6)}`;
}

async function main(): Promise<void> {
  const api = await connectApi();

  try {
    const mb = (api.query as any).entityMember;

    console.log('=== entityMembers 数据检查 ===\n');

    for (const addr of ALL_ACCOUNTS) {
      console.log(`\n--- ${shortAddr(addr)} ---`);

      // entityMembers
      try {
        const raw = await mb.entityMembers(ENTITY_ID, addr);
        const json = codecToJson(raw);
        const human = codecToHuman(raw);
        console.log(`  entityMembers (json): ${JSON.stringify(json, null, 2)}`);
        console.log(`  entityMembers (human): ${JSON.stringify(human, null, 2)}`);
      } catch(e) {
        console.log(`  entityMembers 查询失败: ${e}`);
      }

      // memberLevelExpiry
      try {
        const raw = await mb.memberLevelExpiry(ENTITY_ID, addr);
        const json = codecToJson(raw);
        console.log(`  memberLevelExpiry: ${JSON.stringify(json)}`);
      } catch {}

      // memberOrderCount
      try {
        const raw = await mb.memberOrderCount(ENTITY_ID, addr);
        const json = codecToJson(raw);
        console.log(`  memberOrderCount: ${JSON.stringify(json)}`);
      } catch {}

      // directReferrals
      try {
        const raw = await mb.directReferrals(ENTITY_ID, addr);
        const json = codecToJson(raw);
        console.log(`  directReferrals: ${JSON.stringify(json)}`);
      } catch {}

      // pendingMembers
      try {
        const raw = await mb.pendingMembers(ENTITY_ID, addr);
        const json = codecToJson(raw);
        console.log(`  pendingMembers: ${JSON.stringify(json)}`);
      } catch {}
    }

    // entityMemberPolicy
    console.log('\n\n=== entityMemberPolicy ===');
    try {
      const raw = await mb.entityMemberPolicy(ENTITY_ID);
      console.log(`  ${JSON.stringify(codecToJson(raw), null, 2)}`);
    } catch(e) {
      console.log(`  查询失败: ${e}`);
    }

    // entityLevelSystems
    console.log('\n=== entityLevelSystems ===');
    try {
      const raw = await mb.entityLevelSystems(ENTITY_ID);
      console.log(`  ${JSON.stringify(codecToJson(raw), null, 2)}`);
    } catch(e) {
      console.log(`  查询失败: ${e}`);
    }

    // memberCount
    console.log('\n=== memberCount ===');
    try {
      const raw = await mb.memberCount(ENTITY_ID);
      console.log(`  ${JSON.stringify(codecToJson(raw))}`);
    } catch(e) {
      console.log(`  查询失败: ${e}`);
    }

    // levelMemberCount for levels 0-10
    console.log('\n=== levelMemberCount ===');
    for (let lvl = 0; lvl <= 10; lvl++) {
      try {
        const raw = await mb.levelMemberCount(ENTITY_ID, lvl);
        const count = coerceNumber(codecToJson(raw)) ?? 0;
        if (count > 0) console.log(`  Level ${lvl}: ${count}`);
      } catch {}
    }

  } finally {
    await disconnectApi(api);
  }
}

main().catch((err) => {
  console.error('Error:', err);
  process.exit(1);
});
