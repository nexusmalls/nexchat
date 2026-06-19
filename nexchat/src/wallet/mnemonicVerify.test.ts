import { describe, expect, it } from "vitest";
import {
  buildMnemonicVerifyQuiz,
  verifyMnemonicSelections,
} from "@/wallet/mnemonicVerify";

const MNEMONIC =
  "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

describe("mnemonicVerify", () => {
  it("buildMnemonicVerifyQuiz returns 3 sorted indices", () => {
    const quiz = buildMnemonicVerifyQuiz(MNEMONIC);
    expect(quiz.indices).toHaveLength(3);
    expect(quiz.candidateWords).toHaveLength(3);
    for (let i = 1; i < quiz.indices.length; i++) {
      expect(quiz.indices[i]!).toBeGreaterThan(quiz.indices[i - 1]!);
    }
    for (const row of quiz.candidateWords) {
      expect(row).toHaveLength(6);
    }
  });

  it("verifyMnemonicSelections checks correct words", () => {
    const words = MNEMONIC.split(" ");
    const indices = [2, 5, 11];
    const selections = indices.map((i) => words[i]!);
    expect(verifyMnemonicSelections(MNEMONIC, indices, selections)).toBe(true);
    expect(verifyMnemonicSelections(MNEMONIC, indices, ["x", "y", "z"])).toBe(false);
  });
});
