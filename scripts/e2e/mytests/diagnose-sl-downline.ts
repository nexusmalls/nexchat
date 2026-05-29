#!/usr/bin/env tsx
/**
 * diagnose-sl-downline.ts
 *
 * Diagnose why SingleLine DOWNLINE commission is 0 for entity 100000 / target account.
 *
 * Checks:
 *   1. Full SL queue listing (all segments) with removed/skipped status
 *   2. Buyer position in the SL queue
 *   3. Who is at positions BELOW the buyer (potential downline beneficiaries)
 *   4. Whether the target account is at a position below the buyer
 *   5. RemovedMembers entries for entity 100000
 *   6. SL enabled status
 *   7. Order #3 commission records (SingleLine outputs)
 */

process.env.WS_URL ??= 'ws://202.140.140.202:9944';

import { connectApi, disconnectApi } from '../framework/api.js';
import { codecToJson, readObjectField, coerceNumber } from '../framework/codec.js';
import { formatNex, asBigInt } from '../framework/units.js';

const TARGET_ACCOUNT = 'X4Z7fwhyLddkWyM1kCbGoK3Mdt9hUkfdawSyA5hrsdYTdmyXh';
const ENTITY_ID = 100000;
const ORDER_ID = 3;

// The buyer from order #3 (partial address from context)
const BUYER_PREFIX = 'X4V886aG';

function shortAddr(addr: string): string {
  if (!addr || addr.length < 16) return addr;
  return `${addr.slice(0, 8)}...${addr.slice(-6)}`;
}

function ln(char = '=', len = 90): string { return char.repeat(len); }
function header(text: string): void {
  console.log(`\n${ln('=')}`);
  console.log(`  ${text}`);
  console.log(ln('='));
}
function sub(text: string): void {
  console.log(`\n  ${ln('-', 78)}`);
  console.log(`  ${text}`);
  console.log(`  ${ln('-', 78)}`);
}

async function main(): Promise<void> {
  const api = await connectApi();

  try {
    const currentBlock = (await api.rpc.chain.getHeader()).number.toNumber();
    const cc = (api.query as any).commissionCore;
    const sl = (api.query as any).commissionSingleLine;
    const mb = (api.query as any).entityMember;
    const tx = (api.query as any).entityTransaction;

    header(`SingleLine Downline Commission Diagnosis -- Block #${currentBlock}`);
    console.log(`  Target Account: ${TARGET_ACCOUNT}`);
    console.log(`  Entity ID:      ${ENTITY_ID}`);
    console.log(`  Order ID:       ${ORDER_ID}`);

    // ================================================================
    // 0. Find order #3 and identify full buyer address
    // ================================================================
    sub('0. Order #3 Details');

    let buyerAddr = '';
    let orderInfo: any = null;
    try {
      const orderRaw = await tx.orders(ORDER_ID);
      if (orderRaw && !(orderRaw as any).isNone) {
        orderInfo = codecToJson<Record<string, unknown>>((orderRaw as any).unwrap());
        buyerAddr = String(readObjectField(orderInfo, 'buyer') ?? '');
        const seller = String(readObjectField(orderInfo, 'seller') ?? '');
        const payer = readObjectField(orderInfo, 'payer') as string | null;
        const status = String(readObjectField(orderInfo, 'status') ?? 'Unknown');
        const totalAmount = asBigInt(readObjectField(orderInfo, 'totalAmount', 'total_amount') ?? 0);
        const platformFee = asBigInt(readObjectField(orderInfo, 'platformFee', 'platform_fee') ?? 0);
        const entityId = coerceNumber(readObjectField(orderInfo, 'entityId', 'entity_id'));
        const paymentAsset = String(readObjectField(orderInfo, 'paymentAsset', 'payment_asset') ?? 'Native');
        const completedAt = readObjectField(orderInfo, 'completedAt', 'completed_at');

        console.log(`  Entity:        ${entityId}`);
        console.log(`  Buyer:         ${buyerAddr}`);
        console.log(`  Seller:        ${seller}`);
        console.log(`  Payer:         ${payer ?? '(none)'}`);
        console.log(`  Status:        ${status}`);
        console.log(`  Total Amount:  ${formatNex(totalAmount)}`);
        console.log(`  Platform Fee:  ${formatNex(platformFee)}`);
        console.log(`  Payment Asset: ${paymentAsset}`);
        console.log(`  Completed At:  ${completedAt != null ? `#${coerceNumber(completedAt)}` : '(not completed)'}`);

        if (!buyerAddr.startsWith(BUYER_PREFIX)) {
          console.log(`  [WARN] Buyer does not match expected prefix ${BUYER_PREFIX}!`);
        }
      } else {
        console.log(`  Order #${ORDER_ID} not found!`);
      }
    } catch (e) {
      console.log(`  Error fetching order: ${e}`);
    }

    // ================================================================
    // 1. Full SL Queue -- iterate all segments
    // ================================================================
    sub('1. SingleLine Queue -- All Members');

    let segCount = 0;
    try {
      segCount = coerceNumber(codecToJson(await sl.singleLineSegmentCount(ENTITY_ID))) ?? 0;
    } catch {}
    console.log(`  Segment Count: ${segCount}`);

    type QueueMember = {
      globalIndex: number;
      segId: number;
      localPos: number;
      addr: string;
      removed: boolean;
      isBanned: boolean;
      isActivated: boolean;
      isMemberActive: boolean;
    };

    const allMembers: QueueMember[] = [];
    let segSize = 1000; // will try to detect from storage

    for (let seg = 0; seg < segCount; seg++) {
      const segMembers = codecToJson<string[]>(await sl.singleLineSegments(ENTITY_ID, seg)) ?? [];
      if (seg === 0 && segMembers.length > 0) {
        // Detect segment size from first full segment or use known value
      }
      for (let pos = 0; pos < segMembers.length; pos++) {
        const addr = String(segMembers[pos]);
        const globalIndex = seg * 1000 + pos; // assuming MaxSingleLineLength=1000

        // Check RemovedMembers
        let removed = false;
        try {
          removed = codecToJson<boolean>(await sl.removedMembers(ENTITY_ID, addr)) ?? false;
        } catch {}

        allMembers.push({
          globalIndex,
          segId: seg,
          localPos: pos,
          addr,
          removed,
          isBanned: false,
          isActivated: true,
          isMemberActive: true,
        });
      }
    }

    console.log(`  Total Queue Length: ${allMembers.length} members across ${segCount} segment(s)\n`);

    // Try to get actual index from storage for each member, to verify
    console.log(`  ${'Idx'.padStart(5)} ${'Address'.padEnd(52)} ${'StoredIdx'.padStart(10)} ${'Removed'.padStart(8)} Notes`);
    console.log(`  ${'---'.padStart(5)} ${'-------'.padEnd(52)} ${'--------'.padStart(10)} ${'-------'.padStart(8)} -----`);

    for (const m of allMembers) {
      let storedIndex: number | null = null;
      try {
        const raw = codecToJson(await sl.singleLineIndex(ENTITY_ID, m.addr));
        storedIndex = coerceNumber(raw) ?? null;
      } catch {}

      let notes = '';
      if (m.addr === TARGET_ACCOUNT) notes += ' <<< TARGET';
      if (buyerAddr && m.addr === buyerAddr) notes += ' <<< BUYER(order#3)';
      if (m.removed) notes += ' [REMOVED]';
      if (storedIndex !== null && storedIndex !== m.globalIndex) {
        notes += ` [INDEX MISMATCH: stored=${storedIndex} vs seg-calc=${m.globalIndex}]`;
      }

      console.log(
        `  ${String(m.globalIndex).padStart(5)} ${m.addr.padEnd(52)} ${storedIndex != null ? String(storedIndex).padStart(10) : 'null'.padStart(10)} ${String(m.removed).padStart(8)}${notes}`,
      );
    }

    // ================================================================
    // 2. Buyer SL Index (from storage)
    // ================================================================
    sub('2. Buyer SL Queue Position (Order #3)');

    if (buyerAddr) {
      const buyerIndex = codecToJson(await sl.singleLineIndex(ENTITY_ID, buyerAddr));
      const buyerIndexNum = coerceNumber(buyerIndex);
      console.log(`  Buyer Address: ${buyerAddr}`);
      console.log(`  Buyer SL Index: ${buyerIndexNum !== undefined ? buyerIndexNum : 'NOT IN QUEUE (null)'}`);

      if (buyerIndexNum !== undefined) {
        // ================================================================
        // 3. Who is BELOW the buyer (buyer_index + 1, +2, ... up to 60 levels)
        // ================================================================
        sub('3. Positions BELOW Buyer (downline direction, up to 60 levels)');

        console.log(`  Buyer is at position #${buyerIndexNum}`);
        console.log(`  Downline direction means higher index values (buyer_index + 1, +2, ...).\n`);

        const maxDown = 60; // max_down from config
        console.log(`  ${'Dist'.padStart(5)} ${'Idx'.padStart(5)} ${'Address'.padEnd(52)} ${'Removed'.padStart(8)} ${'Is Target'.padStart(10)}`);
        console.log(`  ${'----'.padStart(5)} ${'---'.padStart(5)} ${'-------'.padEnd(52)} ${'-------'.padStart(8)} ${'---------'.padStart(10)}`);

        let targetFoundBelow = false;
        for (let dist = 1; dist <= maxDown; dist++) {
          const targetIdx = buyerIndexNum + dist;
          const member = allMembers.find(m => m.globalIndex === targetIdx);
          if (!member) {
            console.log(`  ${String(dist).padStart(5)} ${String(targetIdx).padStart(5)} (end of queue)`);
            break;
          }
          const isTarget = member.addr === TARGET_ACCOUNT;
          if (isTarget) targetFoundBelow = true;

          console.log(
            `  ${String(dist).padStart(5)} ${String(targetIdx).padStart(5)} ${member.addr.padEnd(52)} ${String(member.removed).padStart(8)} ${(isTarget ? 'YES' : '').padStart(10)}`,
          );
        }

        console.log();
        if (targetFoundBelow) {
          console.log(`  >> TARGET ACCOUNT IS below the buyer => would receive SingleLineDownline commission`);
        } else {
          console.log(`  >> TARGET ACCOUNT is NOT below the buyer => CANNOT receive downline commission from this order`);
        }

        // Also check upline direction
        sub('3b. Positions ABOVE Buyer (upline direction, up to 60 levels)');
        console.log(`  Upline direction means lower index values (buyer_index - 1, -2, ...).\n`);

        console.log(`  ${'Dist'.padStart(5)} ${'Idx'.padStart(5)} ${'Address'.padEnd(52)} ${'Removed'.padStart(8)} ${'Is Target'.padStart(10)}`);
        console.log(`  ${'----'.padStart(5)} ${'---'.padStart(5)} ${'-------'.padEnd(52)} ${'-------'.padStart(8)} ${'---------'.padStart(10)}`);

        let targetFoundAbove = false;
        const maxUp = 60;
        for (let dist = 1; dist <= maxUp; dist++) {
          if (buyerIndexNum < dist) break;
          const targetIdx = buyerIndexNum - dist;
          const member = allMembers.find(m => m.globalIndex === targetIdx);
          if (!member) {
            console.log(`  ${String(dist).padStart(5)} ${String(targetIdx).padStart(5)} (no member at this position)`);
            break;
          }
          const isTarget = member.addr === TARGET_ACCOUNT;
          if (isTarget) targetFoundAbove = true;

          console.log(
            `  ${String(dist).padStart(5)} ${String(targetIdx).padStart(5)} ${member.addr.padEnd(52)} ${String(member.removed).padStart(8)} ${(isTarget ? 'YES' : '').padStart(10)}`,
          );
        }

        console.log();
        if (targetFoundAbove) {
          console.log(`  >> TARGET ACCOUNT IS above the buyer => received SingleLineUpline commission (confirmed by the 2,200 NEX payout)`);
        } else {
          console.log(`  >> TARGET ACCOUNT is NOT above the buyer`);
        }
      }
    } else {
      console.log(`  Could not determine buyer address from order #${ORDER_ID}`);
    }

    // ================================================================
    // 4. Target account position analysis
    // ================================================================
    sub('4. Target Account Position Analysis');

    const targetIndex = codecToJson(await sl.singleLineIndex(ENTITY_ID, TARGET_ACCOUNT));
    const targetIndexNum = coerceNumber(targetIndex);
    console.log(`  Target: ${TARGET_ACCOUNT}`);
    console.log(`  Target SL Index: ${targetIndexNum !== undefined ? targetIndexNum : 'NOT IN QUEUE'}`);

    if (targetIndexNum !== undefined && buyerAddr) {
      const buyerIndexNum = coerceNumber(codecToJson(await sl.singleLineIndex(ENTITY_ID, buyerAddr)));
      if (buyerIndexNum !== undefined) {
        const distance = targetIndexNum - buyerIndexNum;
        console.log(`  Buyer SL Index:  ${buyerIndexNum}`);
        console.log(`  Distance (target - buyer): ${distance}`);
        console.log();

        if (distance < 0) {
          console.log(`  ==> Target is ABOVE the buyer (lower index).`);
          console.log(`      Target can receive UPLINE commission (SingleLineUpline) from buyer's order.`);
          console.log(`      Target CANNOT receive DOWNLINE commission (SingleLineDownline) from buyer's order.`);
          console.log();
          console.log(`  EXPLANATION:`);
          console.log(`      In the SingleLine model:`);
          console.log(`        - process_upline() walks BACKWARDS from buyer (lower indices) => gives Upline commission`);
          console.log(`        - process_downline() walks FORWARDS from buyer (higher indices) => gives Downline commission`);
          console.log(`      Since target (index=${targetIndexNum}) < buyer (index=${buyerIndexNum}),`);
          console.log(`      the target is in the UPLINE direction, not the downline direction.`);
          console.log(`      The target ONLY received Upline commission. Downline = 0 is CORRECT BEHAVIOR.`);
        } else if (distance > 0) {
          console.log(`  ==> Target is BELOW the buyer (higher index).`);
          console.log(`      Target should receive DOWNLINE commission.`);
          console.log(`      If downline = 0, something else is wrong.`);
        } else {
          console.log(`  ==> Target IS the buyer! Buyer does not receive commission from own order.`);
        }
      }
    }

    // ================================================================
    // 5. RemovedMembers entries for entity 100000
    // ================================================================
    sub('5. RemovedMembers Storage Check');

    // Check target account
    let targetRemoved = false;
    try {
      targetRemoved = codecToJson<boolean>(await sl.removedMembers(ENTITY_ID, TARGET_ACCOUNT)) ?? false;
    } catch {}
    console.log(`  Target RemovedMembers: ${targetRemoved}`);

    if (buyerAddr) {
      let buyerRemoved = false;
      try {
        buyerRemoved = codecToJson<boolean>(await sl.removedMembers(ENTITY_ID, buyerAddr)) ?? false;
      } catch {}
      console.log(`  Buyer  RemovedMembers: ${buyerRemoved}`);
    }

    // List all removed members
    console.log(`\n  All removed members in entity ${ENTITY_ID}:`);
    let removedCount = 0;
    for (const m of allMembers) {
      if (m.removed) {
        console.log(`    Index #${m.globalIndex}: ${m.addr}`);
        removedCount++;
      }
    }
    if (removedCount === 0) {
      console.log(`    (none)`);
    }

    // ================================================================
    // 6. SL Enabled Status + Config
    // ================================================================
    sub('6. SingleLine Enabled & Config');

    let slEnabled = true;
    try {
      slEnabled = codecToJson<boolean>(await sl.singleLineEnabled(ENTITY_ID)) ?? true;
    } catch {}
    console.log(`  SingleLineEnabled: ${slEnabled}`);

    let slConfig: any = null;
    try {
      const raw = await sl.singleLineConfigs(ENTITY_ID);
      if (raw && !(raw as any).isNone) {
        slConfig = codecToJson((raw as any).unwrap ? (raw as any).unwrap() : raw);
      }
    } catch {}

    if (slConfig) {
      const uplineRate = coerceNumber(readObjectField(slConfig, 'uplineRate', 'upline_rate')) ?? 0;
      const downlineRate = coerceNumber(readObjectField(slConfig, 'downlineRate', 'downline_rate')) ?? 0;
      const baseUp = coerceNumber(readObjectField(slConfig, 'baseUplineLevels', 'base_upline_levels')) ?? 0;
      const baseDown = coerceNumber(readObjectField(slConfig, 'baseDownlineLevels', 'base_downline_levels')) ?? 0;
      const maxUp = coerceNumber(readObjectField(slConfig, 'maxUplineLevels', 'max_upline_levels')) ?? 0;
      const maxDown = coerceNumber(readObjectField(slConfig, 'maxDownlineLevels', 'max_downline_levels')) ?? 0;
      const threshold = asBigInt(readObjectField(slConfig, 'levelIncrementThreshold', 'level_increment_threshold') ?? 0);

      console.log(`  Upline Rate:        ${uplineRate} bps (${(uplineRate / 100).toFixed(2)}%)`);
      console.log(`  Downline Rate:      ${downlineRate} bps (${(downlineRate / 100).toFixed(2)}%)`);
      console.log(`  Base Upline Levels: ${baseUp}`);
      console.log(`  Base Down Levels:   ${baseDown}`);
      console.log(`  Max Upline Levels:  ${maxUp}`);
      console.log(`  Max Down Levels:    ${maxDown}`);
      console.log(`  Level Inc. Thresh:  ${formatNex(threshold)}`);
    } else {
      console.log(`  [!!] SingleLine Config NOT FOUND for entity ${ENTITY_ID}`);
    }

    // Check CommissionCore enabled modes
    const commConfig = codecToJson<Record<string, unknown>>(
      await cc.commissionConfigs(ENTITY_ID),
    );
    const enabled = readObjectField(commConfig, 'enabled');
    console.log(`\n  CommissionCore enabled modes: ${JSON.stringify(enabled)}`);

    // ================================================================
    // 7. Order #3 Commission Records
    // ================================================================
    sub('7. Order #3 Commission Records');

    const recs = codecToJson<any[]>(await cc.orderCommissionRecords(ORDER_ID)) ?? [];
    console.log(`  Total records for order #${ORDER_ID}: ${recs.length}\n`);

    if (recs.length > 0) {
      console.log(`  ${'Type'.padEnd(24)} ${'Level'.padStart(6)} ${'Beneficiary'.padEnd(52)} ${'Amount'.padStart(22)} ${'Status'.padEnd(12)} Notes`);
      console.log(`  ${'----'.padEnd(24)} ${'-----'.padStart(6)} ${'-----------'.padEnd(52)} ${'------'.padStart(22)} ${'------'.padEnd(12)} -----`);

      let slUplineCount = 0;
      let slDownlineCount = 0;
      let slUplineTotal = 0n;
      let slDownlineTotal = 0n;

      for (const rec of recs) {
        const ctype = String(readObjectField(rec, 'commissionType', 'commission_type') ?? '');
        const beneficiary = String(readObjectField(rec, 'beneficiary') ?? '');
        const amount = asBigInt(readObjectField(rec, 'amount') ?? 0);
        const level = coerceNumber(readObjectField(rec, 'level')) ?? 0;
        const status = String(readObjectField(rec, 'status') ?? '');

        let notes = '';
        if (beneficiary === TARGET_ACCOUNT) notes += ' <<< TARGET';
        if (buyerAddr && beneficiary === buyerAddr) notes += ' <<< BUYER';

        if (ctype === 'SingleLineUpline') { slUplineCount++; slUplineTotal += amount; }
        if (ctype === 'SingleLineDownline') { slDownlineCount++; slDownlineTotal += amount; }

        console.log(
          `  ${ctype.padEnd(24)} ${(`L${level}`).padStart(6)} ${beneficiary.padEnd(52)} ${formatNex(amount).padStart(22)} ${status.padEnd(12)}${notes}`,
        );
      }

      console.log(`\n  SingleLine Summary for Order #${ORDER_ID}:`);
      console.log(`    Upline records:   ${slUplineCount} totaling ${formatNex(slUplineTotal)}`);
      console.log(`    Downline records: ${slDownlineCount} totaling ${formatNex(slDownlineTotal)}`);

      if (slDownlineCount === 0) {
        console.log(`\n  [!!] ZERO downline commission records for this order.`);
        console.log(`       This means process_downline() produced no outputs.`);

        if (buyerAddr) {
          const buyerIndexNum = coerceNumber(codecToJson(await sl.singleLineIndex(ENTITY_ID, buyerAddr)));
          if (buyerIndexNum !== undefined) {
            const totalLen = allMembers.length;
            console.log(`       Buyer index: ${buyerIndexNum}, Queue length: ${totalLen}`);
            if (buyerIndexNum >= totalLen - 1) {
              console.log(`       CAUSE: Buyer is at the LAST position in the queue (or beyond).`);
              console.log(`       There are NO members below the buyer to receive downline commission!`);
            } else {
              console.log(`       Buyer is NOT at the last position, so there ARE members below.`);
              console.log(`       Check if all below members are skipped (removed/banned/inactive).`);
            }
          }
        }
      }
    } else {
      console.log(`  No commission records found for order #${ORDER_ID}.`);
    }

    // Also check token commission records
    sub('7b. Order #3 TOKEN Commission Records');
    const tokenRecs = codecToJson<any[]>(await cc.orderTokenCommissionRecords(ORDER_ID)) ?? [];
    console.log(`  Total TOKEN records for order #${ORDER_ID}: ${tokenRecs.length}`);
    if (tokenRecs.length > 0) {
      for (const rec of tokenRecs) {
        const ctype = String(readObjectField(rec, 'commissionType', 'commission_type') ?? '');
        const beneficiary = String(readObjectField(rec, 'beneficiary') ?? '');
        const amount = asBigInt(readObjectField(rec, 'amount') ?? 0);
        const level = coerceNumber(readObjectField(rec, 'level')) ?? 0;
        let notes = '';
        if (beneficiary === TARGET_ACCOUNT) notes += ' <<< TARGET';
        if (ctype.includes('SingleLine')) {
          console.log(`    ${ctype} L${level} => ${shortAddr(beneficiary)} amount=${amount}${notes}`);
        }
      }
    }

    // ================================================================
    // 8. Check all orders for downline commission to target
    // ================================================================
    sub('8. Scan ALL Orders for Downline Commission to Target');

    const nextOrderIdRaw = await tx.nextOrderId();
    const nextOrderId = coerceNumber(codecToJson(nextOrderIdRaw)) ?? 0;
    console.log(`  Scanning orders 0..${nextOrderId - 1} for SingleLineDownline to target...\n`);

    let foundDownline = false;
    for (let oid = 0; oid < nextOrderId; oid++) {
      const orderRaw = await tx.orders(oid);
      if (!orderRaw || (orderRaw as any).isNone) continue;
      const order = codecToJson<Record<string, unknown>>((orderRaw as any).unwrap());
      const entityId = coerceNumber(readObjectField(order, 'entityId', 'entity_id'))!;
      if (entityId !== ENTITY_ID) continue;

      const oRecs = codecToJson<any[]>(await cc.orderCommissionRecords(oid)) ?? [];
      for (const rec of oRecs) {
        const ctype = String(readObjectField(rec, 'commissionType', 'commission_type') ?? '');
        const beneficiary = String(readObjectField(rec, 'beneficiary') ?? '');
        if (ctype === 'SingleLineDownline' && beneficiary === TARGET_ACCOUNT) {
          const amount = asBigInt(readObjectField(rec, 'amount') ?? 0);
          const level = coerceNumber(readObjectField(rec, 'level')) ?? 0;
          const buyer = String(readObjectField(order, 'buyer') ?? '');
          console.log(`  Found! Order #${oid}: buyer=${shortAddr(buyer)} L${level} amount=${formatNex(amount)}`);
          foundDownline = true;
        }
      }
    }

    if (!foundDownline) {
      console.log(`  No SingleLineDownline commission found for target across ALL orders.`);
    }

    // ================================================================
    // 9. For Target to get Downline commission, someone ABOVE must buy
    // ================================================================
    sub('9. Who Needs to Buy for Target to Get Downline Commission?');

    if (targetIndexNum !== undefined) {
      console.log(`  Target is at SL index #${targetIndexNum}.`);
      console.log(`  For Target to receive SingleLineDownline commission,`);
      console.log(`  a buyer at index < ${targetIndexNum} must place an order,`);
      console.log(`  AND (target_index - buyer_index) <= buyer's effective downline levels.\n`);

      // List members above the target
      const membersAbove = allMembers.filter(m => m.globalIndex < targetIndexNum!);
      console.log(`  Members above target (potential buyers whose orders give target downline commission):`);
      for (const m of membersAbove) {
        const dist = targetIndexNum - m.globalIndex;
        let notes = m.removed ? ' [REMOVED - would be skipped]' : '';
        console.log(`    Index #${m.globalIndex}: ${shortAddr(m.addr)} (distance=${dist})${notes}`);
      }

      if (membersAbove.length === 0) {
        console.log(`    (NONE - target is at position 0, no one is above)`);
        console.log(`\n  CONCLUSION: Target is at the top of the queue. No one can produce`);
        console.log(`  downline commission for the target because there are no members at lower indices.`);
      }

      // Check: did any of those above-target members have completed orders?
      console.log(`\n  Checking if any member above target placed a completed order:`);
      for (const m of membersAbove) {
        for (let oid = 0; oid < nextOrderId; oid++) {
          const orderRaw = await tx.orders(oid);
          if (!orderRaw || (orderRaw as any).isNone) continue;
          const order = codecToJson<Record<string, unknown>>((orderRaw as any).unwrap());
          const entityId = coerceNumber(readObjectField(order, 'entityId', 'entity_id'))!;
          if (entityId !== ENTITY_ID) continue;
          const buyer = String(readObjectField(order, 'buyer') ?? '');
          if (buyer === m.addr) {
            const status = String(readObjectField(order, 'status') ?? '');
            const totalAmount = asBigInt(readObjectField(order, 'totalAmount', 'total_amount') ?? 0);
            const dist = targetIndexNum - m.globalIndex;
            console.log(`    Index #${m.globalIndex} (${shortAddr(m.addr)}) bought order #${oid}: status=${status} amount=${formatNex(totalAmount)} distance-to-target=${dist}`);
          }
        }
      }
    }

    // ================================================================
    // SUMMARY
    // ================================================================
    header('DIAGNOSIS SUMMARY');

    console.log(`\n  Target Account:  ${TARGET_ACCOUNT}`);
    console.log(`  Target SL Index: ${targetIndexNum ?? 'N/A'}`);
    if (buyerAddr) {
      const buyerIndexNum = coerceNumber(codecToJson(await sl.singleLineIndex(ENTITY_ID, buyerAddr)));
      console.log(`  Buyer (order#3): ${buyerAddr}`);
      console.log(`  Buyer SL Index:  ${buyerIndexNum ?? 'N/A'}`);

      if (targetIndexNum !== undefined && buyerIndexNum !== undefined) {
        const diff = targetIndexNum - buyerIndexNum;
        console.log(`\n  Relative Position: target(${targetIndexNum}) - buyer(${buyerIndexNum}) = ${diff}`);

        if (diff < 0) {
          console.log(`\n  ROOT CAUSE: Target (index ${targetIndexNum}) is ABOVE the buyer (index ${buyerIndexNum}).`);
          console.log(`  In the SingleLine model:`);
          console.log(`    - Upline commission goes to members at LOWER indices than buyer`);
          console.log(`    - Downline commission goes to members at HIGHER indices than buyer`);
          console.log(`  Since target < buyer, target is in the UPLINE direction.`);
          console.log(`  Target correctly received 2,200 NEX as Upline commission.`);
          console.log(`  Target CANNOT receive Downline commission from this order because`);
          console.log(`  downline looks at indices ABOVE the buyer, and target is BELOW.`);
          console.log(`\n  For target to receive Downline commission, a member at an index`);
          console.log(`  LOWER than ${targetIndexNum} would need to place an order, with target`);
          console.log(`  being within that buyer's effective downline reach.`);
        } else if (diff > 0) {
          console.log(`\n  Target IS below the buyer. Downline = 0 suggests:`);
          console.log(`    - Target may be skipped (removed/banned/inactive), OR`);
          console.log(`    - Distance exceeds buyer's effective downline levels, OR`);
          console.log(`    - Commission budget was exhausted before reaching target`);
        } else {
          console.log(`\n  Target IS the buyer -- buyers don't receive commission from own orders.`);
        }
      }
    }

    console.log(`\n${ln('=')}\n`);

  } finally {
    await disconnectApi(api);
  }
}

main().catch((err) => {
  console.error('Error:', err);
  process.exit(1);
});
