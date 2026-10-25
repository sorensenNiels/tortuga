// R-VSC-1, R-VSC-2: what the extension passes to tortuga-cli and reads back.

import * as assert from "node:assert/strict";
import { test } from "node:test";

import { Report, Step, duration, execute, exitMessage, failures, listArgs, outcomesByRequest, runArgs } from "../src/cli";

function step(request: string, failed: boolean, extra: Partial<Step> = {}): Step {
  return { name: request, request, status: 200, elapsed_ms: 10, tests: [], errors: [], failed, ...extra };
}

test("run passes the target, environment, workspace and mock, and always a JSON report", () => {
  const args = runArgs("/p ü😀", "users/get-user.toml", { environment: "Dev", workspaceFile: "/w.toml", mock: true }, "/tmp/r.json");
  assert.deepEqual(args, [
    "run", "/p ü😀", "users/get-user.toml", "--env", "Dev", "--workspace", "/w.toml", "--mock",
    "--report", "json", "--out", "/tmp/r.json",
  ]);
});

test("a run without a target or settings runs every request with no options", () => {
  const args = runArgs("/p", undefined, { environment: "", workspaceFile: "", mock: false }, "/r.json");
  assert.deepEqual(args, ["run", "/p", "--report", "json", "--out", "/r.json"]);
});

test("list passes the workspace file only when there is one", () => {
  assert.deepEqual(listArgs("/p", ""), ["list", "/p"]);
  assert.deepEqual(listArgs("/p", "/w.toml"), ["list", "/p", "--workspace", "/w.toml"]);
});

test("each request gets its own outcome, with failed tests and errors as messages", () => {
  const report: Report = {
    workflow: "Users",
    passed: false,
    iterations: [{
      passed: false,
      skipped: 0,
      steps: [
        step("a.toml", false),
        step("b.toml", true, {
          errors: ["Server \"API\" has no URL"],
          tests: [{ name: "status is 200", passed: false, message: "Expected 404 to be 200" }, { name: "ok", passed: true }],
        }),
      ],
    }],
  };

  const outcomes = outcomesByRequest(report);

  assert.deepEqual(outcomes.get("a.toml"), { passed: true, messages: [], durationMs: 10 });
  assert.deepEqual(outcomes.get("b.toml"), {
    passed: false,
    messages: ["Server \"API\" has no URL", "status is 200: Expected 404 to be 200"],
    durationMs: 10,
  });
  assert.deepEqual(failures(report), ["b.toml: Server \"API\" has no URL; status is 200: Expected 404 to be 200"]);
  assert.equal(duration(report), 20);
});

test("a request that fails in one data row fails, saying which row", () => {
  const report: Report = {
    workflow: "Greet",
    passed: false,
    iterations: [
      { row: { name: "Ann" }, passed: true, skipped: 0, steps: [step("greet.toml", false)] },
      { row: { name: "Bo" }, passed: false, skipped: 0, steps: [step("greet.toml", true, { tests: [{ name: "says hi", passed: false }] })] },
    ],
  };

  const outcome = outcomesByRequest(report).get("greet.toml");

  assert.equal(outcome?.passed, false);
  assert.deepEqual(outcome?.messages, ["Row 2: says hi"]);
  assert.deepEqual(failures(report), ["Row 2, greet.toml: says hi"]);
});

test("an empty run passes with nothing to report", () => {
  const report: Report = { workflow: "Empty", passed: true, iterations: [] };
  assert.equal(outcomesByRequest(report).size, 0);
  assert.deepEqual(failures(report), []);
  assert.equal(duration(report), 0);
});

test("a missing tortuga-cli says how to set it", async () => {
  const exit = await execute("tortuga-cli-that-does-not-exist", []);
  assert.match(exitMessage("tortuga-cli-that-does-not-exist", exit), /tortuga\.cliPath/);
});

test("an error from tortuga-cli shows without its prefix and the usage after it", () => {
  const message = exitMessage("tortuga-cli", {
    code: 2,
    stdout: "",
    stderr: "tortuga-cli: 'x' is not a workflow, folder or request of the project\n\nUsage: tortuga-cli run …",
  });
  assert.equal(message, "'x' is not a workflow, folder or request of the project");
});

test("the output of a program comes back as it prints it", async () => {
  const seen: string[] = [];
  const exit = await execute(process.execPath, ["-e", "console.log('ü😀'); process.exit(3)"], (t) => seen.push(t));
  assert.equal(exit.code, 3);
  assert.equal(exit.stdout.trim(), "ü😀");
  assert.equal(seen.join("").trim(), "ü😀");
});
