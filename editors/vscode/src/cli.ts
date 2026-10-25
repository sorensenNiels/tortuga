// How the extension talks to `tortuga-cli` (R-VSC-1, R-VSC-2): the arguments it passes and
// what it reads back. No `vscode` import, so it is tested with plain `node --test`.

import { spawn } from "node:child_process";

/** `tortuga-cli list`: a project as the engine reads it. */
export interface Listing {
  name: string;
  items: Entry[];
  workflows: Named[];
  environments: Named[];
}

export type Entry =
  | { kind: "folder"; name: string; path: string; items: Entry[] }
  | { kind: "request"; name: string; path: string; method: string }
  | { kind: "broken"; path: string; error: string };

export interface Named {
  id: string;
  name: string;
}

/** `tortuga-cli run --report json`. */
export interface Report {
  workflow: string;
  passed: boolean;
  iterations: Iteration[];
}

export interface Iteration {
  row?: Record<string, string>;
  passed: boolean;
  skipped: number;
  ended?: string;
  steps: Step[];
}

export interface Step {
  name: string;
  request: string;
  status: number | null;
  elapsed_ms: number | null;
  tests: { name: string; passed: boolean; message?: string | null }[];
  errors: string[];
  failed: boolean;
}

/** The settings a run uses. Empty strings mean "not set". */
export interface RunOptions {
  environment: string;
  workspaceFile: string;
  mock: boolean;
}

export function listArgs(root: string, workspaceFile: string): string[] {
  return ["list", root, ...(workspaceFile ? ["--workspace", workspaceFile] : [])];
}

/** `target`: a workflow id, a folder or request path, or `undefined` for every request. */
export function runArgs(
  root: string,
  target: string | undefined,
  options: RunOptions,
  reportFile: string,
): string[] {
  const args = ["run", root];
  if (target !== undefined) {
    args.push(target);
  }
  if (options.environment) {
    args.push("--env", options.environment);
  }
  if (options.workspaceFile) {
    args.push("--workspace", options.workspaceFile);
  }
  if (options.mock) {
    args.push("--mock");
  }
  args.push("--report", "json", "--out", reportFile);
  return args;
}

/** What happened to one request in a run, over every iteration it ran in. */
export interface Outcome {
  passed: boolean;
  messages: string[];
  durationMs: number;
}

/**
 * The outcome of each request in `report`, by its path in the project. A request that
 * ran in several iterations fails if any of them failed; its messages say which row.
 */
export function outcomesByRequest(report: Report): Map<string, Outcome> {
  const outcomes = new Map<string, Outcome>();
  const several = report.iterations.length > 1;
  report.iterations.forEach((iteration, n) => {
    const prefix = several ? `${iterationLabel(iteration, n)}: ` : "";
    for (const step of iteration.steps) {
      const outcome = outcomes.get(step.request) ?? { passed: true, messages: [], durationMs: 0 };
      outcome.passed &&= !step.failed;
      outcome.durationMs += step.elapsed_ms ?? 0;
      outcome.messages.push(...stepMessages(step).map((m) => prefix + m));
      outcomes.set(step.request, outcome);
    }
  });
  return outcomes;
}

/** Why a whole run failed: one line per failed step, naming the step and its row. */
export function failures(report: Report): string[] {
  const several = report.iterations.length > 1;
  return report.iterations.flatMap((iteration, n) =>
    iteration.steps
      .filter((step) => step.failed)
      .map((step) => {
        const where = several ? `${iterationLabel(iteration, n)}, ` : "";
        const why = stepMessages(step);
        return `${where}${step.name}: ${why.length > 0 ? why.join("; ") : "failed"}`;
      }),
  );
}

/** The run's total time: the sum of its responses' times. */
export function duration(report: Report): number {
  return report.iterations
    .flatMap((i) => i.steps)
    .reduce((sum, step) => sum + (step.elapsed_ms ?? 0), 0);
}

function iterationLabel(iteration: Iteration, n: number): string {
  return iteration.row ? `Row ${n + 1}` : `Iteration ${n + 1}`;
}

function stepMessages(step: Step): string[] {
  const failedTests = step.tests
    .filter((t) => !t.passed)
    .map((t) => (t.message ? `${t.name}: ${t.message}` : t.name));
  return [...step.errors, ...failedTests];
}

/** The output of a finished `tortuga-cli` process. */
export interface Exit {
  code: number | null;
  stdout: string;
  stderr: string;
  /** Set when the program could not be started (e.g. not installed). */
  spawnError?: Error;
}

/**
 * Runs `program` with `args`, passing what it prints to `onOutput` as it comes.
 * `signal` stops it.
 */
export function execute(
  program: string,
  args: string[],
  onOutput: (text: string) => void = () => {},
  signal?: AbortSignal,
): Promise<Exit> {
  return new Promise((resolve) => {
    let stdout = "";
    let stderr = "";
    const child = spawn(program, args, { signal, windowsHide: true });
    child.stdout.setEncoding("utf8").on("data", (text: string) => {
      stdout += text;
      onOutput(text);
    });
    child.stderr.setEncoding("utf8").on("data", (text: string) => {
      stderr += text;
      onOutput(text);
    });
    child.on("error", (spawnError) => {
      // A stopped run also ends up here; "close" follows and reports it.
      if (!signal?.aborted) {
        resolve({ code: null, stdout, stderr, spawnError });
      }
    });
    child.on("close", (code) => resolve({ code, stdout, stderr }));
  });
}

/** A short reason a `tortuga-cli` call failed, for a notification or test message. */
export function exitMessage(program: string, exit: Exit): string {
  if (exit.spawnError) {
    const missing = (exit.spawnError as NodeJS.ErrnoException).code === "ENOENT";
    return missing
      ? `Could not find '${program}'. Install tortuga-cli or set "tortuga.cliPath".`
      : `Could not start '${program}': ${exit.spawnError.message}`;
  }
  const stderr = exit.stderr.trim().replace(/^tortuga-cli: /, "");
  // Errors in the command print the usage after a blank line; only the error matters.
  return stderr.split(/\r?\n\r?\n/)[0] || `tortuga-cli exited with ${exit.code}`;
}
