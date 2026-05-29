#!/usr/bin/env tsx

import path from 'node:path';
import { connectApi, disconnectApi } from './framework/api.js';
import { runSuites } from './framework/runner.js';
import { TestSuite } from './framework/types.js';
import { ALL_SUITES, DEFAULT_SUITES, SUITE_MAP } from './suites/index.js';

interface CliSelection {
  listOnly: boolean;
  suites: TestSuite[];
  label: string;
}

/**
 * 解析命令行参数，决定列出套件还是执行指定测试套件。
 */
function parseArgs(argv: string[]): CliSelection {
  if (argv.includes('--list')) {
    return { listOnly: true, suites: ALL_SUITES, label: '列表 / list' };
  }

  const suiteIndex = argv.indexOf('--suite');
  if (suiteIndex === -1) {
    return { listOnly: false, suites: DEFAULT_SUITES, label: '默认 / default' };
  }

  const requested = argv.slice(suiteIndex + 1).filter((arg) => !arg.startsWith('--'));
  if (requested.length === 0) {
    throw new Error(`--suite 后缺少套件 ID / Missing suite ids after --suite. Available: ${ALL_SUITES.map((suite) => suite.id).join(', ')}`);
  }

  const suites = requested.map((id) => {
    const suite = SUITE_MAP.get(id);
    if (!suite) {
      throw new Error(`未知套件 / Unknown suite: ${id}. Available: ${ALL_SUITES.map((item) => item.id).join(', ')}`);
    }
    return suite;
  });

  return { listOnly: false, suites, label: requested.join(', ') };
}

/**
 * 作为 E2E 总入口，连接链、加载测试账户并执行选中的测试套件。
 */
async function main(): Promise<void> {
  const selection = parseArgs(process.argv.slice(2));
  const traceBootstrap = process.env.E2E_TRACE_BOOTSTRAP === '1';

  if (selection.listOnly) {
    for (const suite of ALL_SUITES) {
      console.log(`${suite.id.padEnd(20)} ${suite.title} / ${suite.title} — ${suite.description}`);
    }
    return;
  }

  console.log(`执行测试套件 / Running suites: ${selection.label}`);

  if (traceBootstrap) {
    console.log('[引导 / bootstrap] 正在连接 API / connecting api');
  }
  const api = await connectApi();
  try {
    if (traceBootstrap) {
      console.log('[引导 / bootstrap] API 已连接 / api connected');
      console.log('[引导 / bootstrap] 正在加载测试账户 / loading actors');
    }
    const { getDevActors, getSelectedActorsFilePath } = await import('./framework/accounts.js');
    const actors = await getDevActors();
    if (traceBootstrap) {
      const actorsFile = getSelectedActorsFilePath();
      if (actorsFile) {
        console.log(`[引导 / bootstrap] 测试账户文件 / actors file: ${path.basename(actorsFile)}`);
      }
    }
    if (traceBootstrap) {
      console.log('[引导 / bootstrap] 测试账户已就绪 / actors ready');
    }
    const allPassed = await runSuites(api, actors, selection.suites);
    process.exitCode = allPassed ? 0 : 1;
  } finally {
    await disconnectApi(api);
  }
}

main().catch((error) => {
  console.error(`E2E 运行失败 / E2E runner failed: ${error instanceof Error ? error.message : String(error)}`);
  process.exitCode = 1;
});
