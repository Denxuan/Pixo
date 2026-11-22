import { createOutputAck } from "@/utils/sshOutput";
import { expect, test, vi } from "vitest";

test("acks wait until a session exists and then send the parsed total", async () => {
  vi.useFakeTimers();
  const sent: Array<[number, number]> = [];
  let sessionId = 0;
  const ack = createOutputAck({
    sessionId: () => sessionId,
    send: (id, bytes) => {
      sent.push([id, bytes]);
    },
  });
  ack.note(10);
  ack.note(5);
  await vi.advanceTimersByTimeAsync(16);
  expect(sent).toEqual([]);
  sessionId = 7;
  await vi.advanceTimersByTimeAsync(16);
  expect(sent).toEqual([[7, 15]]);
  ack.close();
  vi.useRealTimers();
});

test("a failed ack is retried and closing drops anything still waiting", async () => {
  vi.useFakeTimers();
  const sent: number[] = [];
  let fail = true;
  const ack = createOutputAck({
    sessionId: () => 3,
    send: (id, bytes) => {
      sent.push(bytes);
      if (fail) return Promise.reject(new Error("again"));
      expect(id).toBe(3);
    },
  });
  ack.note(8);
  await vi.advanceTimersByTimeAsync(16);
  expect(sent).toEqual([8]);
  fail = false;
  await vi.advanceTimersByTimeAsync(16);
  expect(sent).toEqual([8, 8]);
  ack.note(4);
  ack.close();
  await vi.advanceTimersByTimeAsync(32);
  expect(sent).toEqual([8, 8]);
  vi.useRealTimers();
});
