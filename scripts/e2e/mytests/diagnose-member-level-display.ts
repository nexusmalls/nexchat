#!/usr/bin/env tsx

process.env.WS_URL ??= 'ws://202.140.140.202:9944';

import { connectApi, disconnectApi } from '../framework/api.js';
import { codecToJson, codecToHuman, readObjectField, coerceNumber } from '../framework/codec.js';

const ENTITY_ID = 100000;
const ACCOUNT = 'X4XLb5Z7rDcjhQ6EajaLXXdPwGBDkBTES7FpALx1WHKrX7pHr';

function pretty(value: unknown): string {
  return JSON.stringify(value, null, 2);
}

function pickNumber(record: unknown, ...keys: string[]): number | undefined {
  for (const key of keys) {
    const val = readObjectField(record, key);
    const num = coerceNumber(val);
    if (num != null) return num;
  }
  return undefined;
}

async function main(): Promise<void> {
  const api = await connectApi();
  try {
    const mb = (api.query as any).entityMember;
    const call = (api.call as any) ?? {};
    const memberTeamApi = call.memberTeamApi;
    const commissionDashboardApi = call.commissionDashboardApi;

    console.log('=== Lv.5 / 5星 显示诊断 ===');
    console.log(`WS_URL: ${process.env.WS_URL}`);
    console.log(`entityId: ${ENTITY_ID}`);
    console.log(`account: ${ACCOUNT}`);

    const entityMemberRaw = await mb.entityMembers(ENTITY_ID, ACCOUNT);
    const entityMemberJson = codecToJson(entityMemberRaw);
    const entityMemberHuman = codecToHuman(entityMemberRaw);
    console.log('\n--- storage.entityMembers ---');
    console.log(pretty(entityMemberJson));
    console.log('\n--- storage.entityMembers (human) ---');
    console.log(pretty(entityMemberHuman));

    try {
      const raw = await mb.memberLevels(ENTITY_ID, ACCOUNT);
      console.log('\n--- storage.memberLevels ---');
      console.log(pretty(codecToJson(raw)));
    } catch {}

    try {
      const raw = await mb.memberLevelExpiry(ENTITY_ID, ACCOUNT);
      console.log('\n--- storage.memberLevelExpiry ---');
      console.log(pretty(codecToJson(raw)));
    } catch {}

    try {
      const raw = await mb.directReferrals(ENTITY_ID, ACCOUNT);
      console.log('\n--- storage.directReferrals ---');
      console.log(pretty(codecToJson(raw)));
    } catch {}

    try {
      const raw = await mb.entityLevelSystems(ENTITY_ID);
      console.log('\n--- storage.entityLevelSystems ---');
      console.log(pretty(codecToJson(raw)));
    } catch {}

    if (memberTeamApi?.getMemberInfo) {
      const info = codecToJson(await memberTeamApi.getMemberInfo(ENTITY_ID, ACCOUNT));
      console.log('\n--- runtimeApi.memberTeamApi.getMemberInfo ---');
      console.log(pretty(info));

      const level = pickNumber(info, 'level', 'memberLevel', 'currentLevel', 'effectiveLevel');
      const direct = pickNumber(info, 'directReferralCount', 'directCount', 'directReferrals', 'directMembers');
      const team = pickNumber(info, 'teamCount', 'teamMemberCount', 'teamSize', 'teamMembers');
      console.log('\nsummary.memberInfo');
      console.log(pretty({ level, direct, team }));
    } else {
      console.log('\nmemberTeamApi.getMemberInfo 不可用');
    }

    if (memberTeamApi?.getReferralTeam) {
      const teamRows = codecToJson(await memberTeamApi.getReferralTeam(ENTITY_ID, ACCOUNT, 10));
      console.log('\n--- runtimeApi.memberTeamApi.getReferralTeam(depth=10) ---');
      console.log(pretty(teamRows));
      console.log(`team rows: ${Array.isArray(teamRows) ? teamRows.length : 'n/a'}`);
    }

    if (commissionDashboardApi?.getDirectReferralInfo) {
      const directInfo = codecToJson(await commissionDashboardApi.getDirectReferralInfo(ENTITY_ID, ACCOUNT));
      console.log('\n--- runtimeApi.commissionDashboardApi.getDirectReferralInfo ---');
      console.log(pretty(directInfo));
    }

    if (commissionDashboardApi?.getDirectReferralDetails) {
      const directDetails = codecToJson(await commissionDashboardApi.getDirectReferralDetails(ENTITY_ID, ACCOUNT));
      console.log('\n--- runtimeApi.commissionDashboardApi.getDirectReferralDetails ---');
      console.log(pretty(directDetails));
      const directCount = pickNumber(directDetails, 'totalCount', 'count', 'directCount');
      console.log(`direct detail count: ${directCount ?? 'n/a'}`);
    }

    if (commissionDashboardApi?.getTeamPerformanceInfo) {
      const teamInfo = codecToJson(await commissionDashboardApi.getTeamPerformanceInfo(ENTITY_ID, ACCOUNT));
      console.log('\n--- runtimeApi.commissionDashboardApi.getTeamPerformanceInfo ---');
      console.log(pretty(teamInfo));
    }
  } finally {
    await disconnectApi(api);
  }
}

main().catch((err) => {
  console.error('Error:', err);
  process.exit(1);
});
