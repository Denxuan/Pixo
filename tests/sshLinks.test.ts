import { httpRanges, pathAt, pathAtColumn } from "@/utils/sshLinks";
import { shellSingleQuote } from "@/utils/sshQuote";
import { parseOpenHosts } from "@/utils/sshSessions";
import { expect, test } from "vitest";

test("http links drop trailing punctuation", () => {
  expect(httpRanges("see https://example.com/a), and http://10.0.0.8/x.")).toEqual([
    { start: 4, end: 25, text: "https://example.com/a" },
    { start: 32, end: 49, text: "http://10.0.0.8/x" },
  ]);
});

test("a path token is taken only when the column is inside it", () => {
  expect(pathAt("ls /var/log/syslog,", 4)).toBe("/var/log/syslog");
  expect(pathAt("ls /var/log/syslog", 0)).toBeNull();
  expect(pathAt("cd ~/src", 3)).toBe("~/src");
});

test("wide-character cells still find the path under the cursor", () => {
  expect(pathAtColumn(["目", "", " ", "/", "t", "m", "p"], 5)).toBe("/tmp");
});

test("shell quoting matches the remote insertion rules", () => {
  expect(shellSingleQuote("/tmp/a b")).toBe("'/tmp/a b'");
  expect(shellSingleQuote("o'brien")).toBe("'o'\\''brien'");
  expect(shellSingleQuote("a\nb")).toBeNull();
});

test("saved sessions keep host ids and drop anything else", () => {
  expect(parseOpenHosts("[2, 2, 0, \"x\", 9]")).toEqual([2, 2, 9]);
  expect(parseOpenHosts("nope")).toEqual([]);
  expect(parseOpenHosts(null)).toEqual([]);
});
