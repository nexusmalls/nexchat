import { useMemo } from "react";
import { useAppStore } from "@/state/appStore";
import { mergeRosters } from "@/store/contactBook";

/// EN: Env demo roster merged with user-saved contacts for UI lists.
/// CN: 演示名册与用户通讯录合并，供 UI 列表使用。
export function useContactRoster() {
  const mentionRoster = useAppStore((s) => s.mentionRoster);
  const userContacts = useAppStore((s) => s.userContacts);
  const self = useAppStore((s) => s.account?.account);
  return useMemo(
    () => mergeRosters(mentionRoster, userContacts, self),
    [mentionRoster, userContacts, self],
  );
}
