// EN: Mnemonic backup quiz — pick correct words at random positions (nexus-com-dapp flow).
// CN: 助记词备份测验——在随机位置选择正确单词（对齐 nexus-com-dapp）。

export interface MnemonicVerifyQuiz {
  indices: number[];
  candidateWords: string[][];
}

function shuffle<T>(arr: T[]): T[] {
  const out = [...arr];
  for (let i = out.length - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1));
    [out[i], out[j]] = [out[j]!, out[i]!];
  }
  return out;
}

// EN: Build 3-position word-pick quiz with 1 correct + 5 decoys per row.
// CN: 生成 3 个位置的选词测验，每行 1 个正确词 + 5 个干扰项。
export function buildMnemonicVerifyQuiz(mnemonic: string): MnemonicVerifyQuiz {
  const words = mnemonic.trim().split(/\s+/);
  const indices: number[] = [];
  while (indices.length < 3) {
    const idx = Math.floor(Math.random() * words.length);
    if (!indices.includes(idx)) indices.push(idx);
  }
  indices.sort((a, b) => a - b);

  const candidateWords = indices.map((correctIdx) => {
    const correctWord = words[correctIdx]!;
    const others = words.filter((_, i) => i !== correctIdx);
    const decoys = shuffle(others).slice(0, 5);
    return shuffle([correctWord, ...decoys]);
  });

  return { indices, candidateWords };
}

// EN: Check user selections against mnemonic words at quiz indices.
// CN: 校验用户在测验位置上选择的单词是否正确。
export function verifyMnemonicSelections(
  mnemonic: string,
  indices: number[],
  selections: string[],
): boolean {
  const words = mnemonic.trim().split(/\s+/);
  return indices.every((idx, i) => selections[i] === words[idx]);
}
