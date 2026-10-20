# Tortuga self-test

Tortuga testing itself with Tortuga (R-MOCK-7). Each project here is an API "designed" in Tortuga:
its requests carry example responses (R-MOCK-1) whose `match` conditions only hold when Tortuga sent
exactly the right request: headers, auth, query, body. The workflows then run against the project's
mock (R-MOCK-5) and test what came back. A request sent wrong gets the fallback example (a 4xx or
5xx) or no match at all, and the run fails. Every response is also checked against its examples
(`--check-examples`, R-MOCK-8), so a mock answer that drifts from its own design fails too.

```sh
make selftest                                              # every workflow, as CI does
tortuga-cli run selftest/sending "User lifecycle" --mock --check-examples \
  --workspace selftest/tortuga-selftest.tortuga-workspace.toml   # one workflow
tortuga-cli mock selftest/sending                          # serve one project's mock

tortuga-cli mock selftest/contract-built --port 4011 &     # the drifted "real" API
tortuga-cli run selftest/contract "Order lifecycle" --env built --check-examples \
  --workspace selftest/tortuga-selftest.tortuga-workspace.toml   # fails, listing the drift
```

`crates/tortuga-cli/tests/selftest.rs` runs every workflow of every project in the workspace
file, so `cargo test`, `make verify` and CI run them too. The workspace adds the `X-Client` header
from its `client` variable to every request (R-SET-2, R-WS-8), and every example checks for it.

| Project | Covers |
|---|---|
| `sending/` | Servers (R-SRV-1, R-SRV-3), project and folder default headers and auth (R-COL-4, R-COL-11), Basic and Bearer auth (R-HTTP-5), query params with one switched off (R-HTTP-2), path variables (R-HTTP-10), JSON and form bodies with their default Content-Type (R-HTTP-4), variables (R-VAR-1), captures from JSON, headers and status (R-TEST-4), workflows passing captures on (R-FLOW-1 to R-FLOW-3), picking an example by header and `delay_ms` (R-MOCK-3). |
| `scripts/` | Pre-request scripts setting variables and headers (R-TEST-2), Jest- and Chai-style tests (R-TEST-1, R-TEST-3, R-TEST-8), XML bodies and `xpath:` captures (R-HTTP-19), example conditions on an XML body (R-MOCK-3). |
| `data-driven/` | A workflow run once per row of a CSV file, with non-ASCII values (R-FLOW-4), query conditions (R-MOCK-3). |
| `contract/` | The contract check (R-MOCK-8): an orders API as designed. Its workflow passes against its own mock; the test also runs it with `--env built` against the mock of `contract-built/`, an implementation that drifted (a field renamed, types changed, `200` instead of `201`), and expects exactly those differences. `contract-built/` is not in the workspace: it only plays the real API. |

Opened in the app (Workspace → Open…), the same projects can be sent by hand against
"Start mock server" (R-MOCK-2, R-MOCK-4).
