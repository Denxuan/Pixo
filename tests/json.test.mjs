import { test } from "node:test";
import assert from "node:assert/strict";
import { formatJson } from "../src/utils/json.ts";

test("formats and compacts JSON values", () => {
  assert.equal(formatJson('{"a":[1,true]}'), '{\n  "a": [\n    1,\n    true\n  ]\n}');
  assert.equal(formatJson(' { "a" : 1 } ', true), '{"a":1}');
  assert.equal(formatJson("null"), "null");
});

test("rejects invalid JSON", () => {
  assert.throws(() => formatJson("{"), SyntaxError);
  assert.throws(() => formatJson(""), SyntaxError);
});
