import { pushRecent } from "@/utils/sshSessions";
import { focusExistingSession, sshEndpoint, tabsAfterClose } from "@/utils/sshTabs";
import { expect, test } from "vitest";

const tabs = [{ key: "a" }, { key: "b" }, { key: "c" }];

test("closing the active tab focuses the tab that takes its place", () => {
  expect(tabsAfterClose(tabs, "b", (tab) => tab.key === "b")).toEqual({
    tabs: [{ key: "a" }, { key: "c" }],
    activeKey: "c",
  });
});

test("closing the last tab focuses the previous one", () => {
  expect(tabsAfterClose(tabs, "c", (tab) => tab.key === "c").activeKey).toBe("b");
});

test("closing a background tab leaves the current session selected", () => {
  const result = tabsAfterClose(tabs, "a", (tab) => tab.key === "c");
  expect(result.tabs.map((tab) => tab.key)).toEqual(["a", "b"]);
  expect(result.activeKey).toBe("a");
});

test("closing the other tabs keeps the chosen session", () => {
  const result = tabsAfterClose(tabs, "a", (tab) => tab.key !== "b", "b");
  expect(result.tabs).toEqual([{ key: "b" }]);
  expect(result.activeKey).toBe("b");
});

test("closing tabs to the right leaves the chosen session and those before it", () => {
  const result = tabsAfterClose(tabs, "b", (_tab, index) => index > 1);
  expect(result.tabs.map((tab) => tab.key)).toEqual(["a", "b"]);
  expect(result.activeKey).toBe("b");
});

test("closing every tab leaves no session selected", () => {
  expect(tabsAfterClose(tabs, "b", () => true)).toEqual({ tabs: [], activeKey: "" });
});

test("recent hosts stay newest first and drop duplicates", () => {
  expect(pushRecent([2, 3], 1)).toEqual([1, 2, 3]);
  expect(pushRecent([1, 2, 3], 2)).toEqual([2, 1, 3]);
  expect(pushRecent([1, 2, 3, 4, 5, 6, 7, 8], 9)).toEqual([9, 1, 2, 3, 4, 5, 6, 7]);
});

test("an open host is focused, and a closed one is reconnected", () => {
  const open = [
    { key: "a", hostId: 1, phase: "ready" as const },
    { key: "b", hostId: 2, phase: "closed" as const },
  ];
  expect(focusExistingSession(open, 1)).toEqual({ key: "a", reconnect: false });
  expect(focusExistingSession(open, 2)).toEqual({ key: "b", reconnect: true });
  expect(focusExistingSession([{ key: "c", hostId: 4, phase: "idle" }], 4)).toEqual({ key: "c", reconnect: true });
  expect(focusExistingSession(open, 3)).toBeNull();
});

test("a connection string omits port 22 and brackets an ipv6 address", () => {
  expect(sshEndpoint({ username: "root", host: "10.0.0.8", port: 22 })).toBe("root@10.0.0.8");
  expect(sshEndpoint({ username: "root", host: "10.0.0.8", port: 2222 })).toBe("root@10.0.0.8:2222");
  expect(sshEndpoint({ username: "root", host: "2001:db8::1", port: 22 })).toBe("root@[2001:db8::1]");
  expect(sshEndpoint({ username: "root", host: "[2001:db8::1]", port: 2222 })).toBe("root@[2001:db8::1]:2222");
});
