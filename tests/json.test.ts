import { formatJson } from "@/utils/json";
import { expect, test } from "vitest";

test("formats and compacts JSON values", () => {
  expect(formatJson('{"a":[1,true]}')).toBe('{\n  "a": [\n    1,\n    true\n  ]\n}');
  expect(formatJson(' { "a" : 1 } ', true)).toBe('{"a":1}');
  expect(formatJson("null")).toBe("null");
});

test("rejects invalid JSON", () => {
  expect(() => formatJson("{")).toThrow(SyntaxError);
  expect(() => formatJson("")).toThrow(SyntaxError);
});
