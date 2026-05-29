#!/usr/bin/env tsx
/**
 * 深度检查 is_member_skipped 条件
 */

process.env.WS_URL ??= 'ws://202.140.140.202:9944';

import { connectApi, disconnectApi } from '../framework/api.js';
import { codecToJson, readObjectField, coerceNumber } from '../framework/codec.js';
import { asBigInt } from '../framework/units.js';

const ACCOUNT = 'X4WMbyCMgCpMJzwg1cdWQuPRRfQiu8ifrJmfLdurviJcTXW94';
const ENTITY_ID = 100000;

const ALL_ACCOUNTS = [
  'X4WMbyCMgCpMJzwg1cdWQuPRRfQiu8ifrJmfLdurviJcTXW94', // pos 0 (TARGET)
  'X4XLb5Z7RMNAgqhBcm1v2QTcufnb13FnQBN3TirHqHXrX7pHr', // pos 1
  'X4Z7fwhyLgXK7xgFNx3wSqFxMYh1YQH9b5bXKaVNuTdmyXh',   // pos 2
  'X4V886aGfWwp5wJA5mwExnL9A6dnxSRyKTq71D2Beht8NK7',     // pos 3
];

function shortAddr(addr: string): string {
  if (!addr || addr.length < 16) return addr;
  return `${addr.slice(0, 8)}...${addr.slice(-6)}`;
}

async function main(): Promise<void> {
  const api = await connectApi();

  try {
    const mb = (api.query as any).entityMember;
    const cc = (api.query as any).commissionCore;
    const sl = (api.query as any).commissionSingleLine;

    console.log('=== is_member_skipped 深度检查 ===\n');

    // 获取完整账户地址 (通过 segments)
    const seg0 = codecToJson<string[]>(await sl.singleLineSegments(ENTITY_ID, 0)) ?? [];
    console.log(`Segment 0 完整地址:\n`);
    seg0.forEach((addr, i) => console.log(`  [${i}] ${addr}`));

    console.log('\n--- 逐个检查 is_member_skipped 条件 ---\n');

    for (const addr of seg0) {
      console.log(`\n账户: ${shortAddr(addr)}`);

      // 1. MemberProfiles — 判断 is_activated, is_member_active
      let profile: any = null;
      try {
        const raw = await mb.memberProfiles(ENTITY_ID, addr);
        if (raw && !(raw as any).isNone) {
          profile = codecToJson((raw as any).unwrap ? (raw as any).unwrap() : raw);
        }
      } catch(e) { console.log(`  memberProfiles 查询失败: ${e}`); }
      console.log(`  memberProfiles: ${JSON.stringify(profile)}`);

      // 2. Banned
      let isBanned = false;
      try {
        const raw = await mb.bannedMembers(ENTITY_ID, addr);
        isBanned = raw && !(raw as any).isNone;
      } catch {}
      console.log(`  bannedMembers: ${isBanned}`);

      // 3. MemberStatus / is_activated
      // 尝试多种可能的 storage name
      const possibleStorages = [
        'memberStatus', 'memberActivated', 'activatedMembers',
        'memberRegistrations', 'registrations',
      ];
      for (const s of possibleStorages) {
        if ((mb as any)[s]) {
          try {
            const raw = await (mb as any)[s](ENTITY_ID, addr);
            const val = codecToJson(raw);
            console.log(`  mb.${s}: ${JSON.stringify(val)}`);
          } catch(e) {
            console.log(`  mb.${s}: 查询失败 ${e}`);
          }
        }
      }

      // 4. 直接检查 RemovedMembers
      const removed = codecToJson<boolean>(await sl.removedMembers(ENTITY_ID, addr)) ?? false;
      console.log(`  SL removedMembers: ${removed}`);

      // 5. 尝试 level
      let levelId: any = null;
      try {
        const raw = await mb.memberLevels(ENTITY_ID, addr);
        levelId = codecToJson(raw);
      } catch {}
      console.log(`  memberLevels: ${JSON.stringify(levelId)}`);

      // 6. member pallet 的所有 storage keys 列出
    }

    // 看看 member pallet 有哪些 storage
    console.log('\n--- entityMember pallet storage 列表 ---\n');
    const mbKeys = Object.keys(mb).filter(k => typeof (mb as any)[k] === 'function');
    // 只列出类似 storage 的
    const storageKeys = mbKeys.filter(k => k[0] === k[0].toLowerCase() && !k.startsWith('_'));
    console.log(storageKeys.sort().join('\n'));

    // 尝试获取 member pallet 中所有已注册成员
    console.log('\n--- 检查 MemberRegistrations / Membership ---\n');

    // 尝试 membership 系列
    const membershipStorages = ['memberships', 'membershipInfo', 'memberOf', 'members'];
    for (const s of membershipStorages) {
      if ((mb as any)[s]) {
        try {
          for (const addr of seg0) {
            const raw = await (mb as any)[s](ENTITY_ID, addr);
            const val = codecToJson(raw);
            if (val) {
              console.log(`  mb.${s}(${ENTITY_ID}, ${shortAddr(addr)}): ${JSON.stringify(val)}`);
            }
          }
        } catch(e) {
          console.log(`  mb.${s}: ${e}`);
        }
      }
    }

    // 检查 commission engine 中的 MemberProvider trait 实现
    // 在 runtime/src/configs/mod.rs 看看 MemberProvider 是什么
    console.log('\n--- SingleLine pallet storage 列表 ---\n');
    const slKeys = Object.keys(sl).filter(k => typeof (sl as any)[k] === 'function');
    const slStorageKeys = slKeys.filter(k => k[0] === k[0].toLowerCase() && !k.startsWith('_'));
    console.log(slStorageKeys.sort().join('\n'));

    // 检查 commission core 所有 storage keys
    console.log('\n--- commissionCore pallet storage 列表 ---\n');
    const ccKeys = Object.keys(cc).filter(k => typeof (cc as any)[k] === 'function');
    const ccStorageKeys = ccKeys.filter(k => k[0] === k[0].toLowerCase() && !k.startsWith('_'));
    console.log(ccStorageKeys.sort().join('\n'));

  } finally {
    await disconnectApi(api);
  }
}

main().catch((err) => {
  console.error('Error:', err);
  process.exit(1);
});
