import type { ApiPromise } from '@polkadot/api';
import { ensureFundedActors, ensureNamedActorsFunded, readPreferredMarketPrice } from './bootstrap.js';
import { captureChainSnapshot } from './api.js';
import { DevActors, SuiteContext, TestSuite } from './types.js';

/**
 * 将未知错误对象统一转换为可打印的错误消息。
 */
function asErrorMessage(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }
  return String(error);
}

/**
 * 统一输出测试运行日志，并按配置决定输出到 stdout 还是 stderr。
 */
function emit(message: string): void {
  if (process.env.E2E_LOG_STDERR === '1') {
    console.error(message);
    return;
  }
  console.log(message);
}

/**
 * 按顺序执行测试套件，并汇总通过/失败结果。
 */
export async function runSuites(api: ApiPromise, actors: DevActors, suites: TestSuite[]): Promise<boolean> {
  const chain = await captureChainSnapshot(api);
  let allPassed = true;
  let passedSuites = 0;

  emit(`链信息 / Chain: ${chain.chain} (${chain.specName} v${chain.specVersion})`);

  for (const suite of suites) {
    let suitePassed = true;
    let stepCount = 0;

    emit(`\n• ${suite.title} [${suite.id}]`);
    emit(`  ${suite.description}`);

    const ctx: SuiteContext = {
      api,
      actors,
      chain,
      step: async (name, fn) => {
        const startedAt = Date.now();
        stepCount += 1;

        try {
          const result = await fn();
          emit(`  ✓ ${name} / ${name} (${Date.now() - startedAt}ms)`);
          return result;
        } catch (error) {
          suitePassed = false;
          emit(`  ✗ ${name} / ${name} (${Date.now() - startedAt}ms) — ${asErrorMessage(error)}`);
          throw error;
        }
      },
      note: (message) => emit(`  - ${message}`),
      ensureFunds: (minNex: number = 25_000) => ensureFundedActors(api, actors, minNex),
      ensureFundsFor: (actorNames: string[], minNex: number = 25_000) => ensureNamedActorsFunded(api, actors, actorNames, minNex),
      readMarketPrice: () => readPreferredMarketPrice(api),
    };

    try {
      await suite.run(ctx);
      if (suitePassed) {
        passedSuites += 1;
        emit(`  ✓ 套件通过 / suite passed (${stepCount} step${stepCount === 1 ? '' : 's'})`);
      } else {
        allPassed = false;
      }
    } catch (error) {
      allPassed = false;
      emit(`  ! 套件失败 / suite failed — ${asErrorMessage(error)}`);
    }
  }

  const failedSuites = suites.length - passedSuites;
  emit(`\n执行完成 / Completed ${suites.length} suite(s): ${passedSuites} passed, ${failedSuites} failed`);
  return allPassed;
}
