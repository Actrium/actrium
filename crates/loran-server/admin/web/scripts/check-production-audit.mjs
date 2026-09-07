import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";

const approvedAdvisory =
  "https://github.com/advisories/GHSA-qwww-vcr4-c8h2";
const approvedRouterVersion = "7.18.2";

const packageJson = JSON.parse(
  readFileSync(new URL("../package.json", import.meta.url), "utf8"),
);
const packageLock = JSON.parse(
  readFileSync(new URL("../package-lock.json", import.meta.url), "utf8"),
);
// CI installs dependencies from pnpm-lock.yaml, not package-lock.json. Read the
// pnpm lockfile too so the gate audits the versions that are actually built and
// shipped, and so drift between the two lockfiles fails the gate closed.
const pnpmLock = readFileSync(
  new URL("../pnpm-lock.yaml", import.meta.url),
  "utf8",
);
const appSource = readFileSync(
  new URL("../src/App.tsx", import.meta.url),
  "utf8",
);

// The only temporary exception is an upstream advisory for React Router's
// server-side RSC mode. Actrix Admin is a static Vite SPA. Fail closed if the
// dependency version or that architectural invariant changes.
const allDependencies = {
  ...packageJson.dependencies,
  ...packageJson.devDependencies,
};
const hasRscTooling = Object.keys(allDependencies).some(
  (name) => name.startsWith("@react-router/") || name === "react-server-dom-webpack",
);
const usesStaticBrowserRouter =
  appSource.includes("BrowserRouter") &&
  !appSource.includes("createBrowserRouter") &&
  !appSource.includes("RouterProvider");
const routerVersion =
  packageLock.packages?.["node_modules/react-router"]?.version;

// Extract the react-router version resolved in pnpm-lock.yaml. The `packages:`
// section keys entries as `  react-router@<version>:` (two-space indent). The
// leading boundary keeps this from matching `react-router-dom@...`.
const pnpmRouterMatch = pnpmLock.match(
  /^ {2}react-router@(\d+\.\d+\.\d+)\b/m,
);
const pnpmRouterVersion = pnpmRouterMatch?.[1];

if (
  hasRscTooling ||
  !usesStaticBrowserRouter ||
  routerVersion !== approvedRouterVersion ||
  pnpmRouterVersion !== approvedRouterVersion
) {
  console.error(
    "React Router audit exception is no longer valid: expected the reviewed " +
      `static BrowserRouter profile at react-router ${approvedRouterVersion} ` +
      `in both lockfiles (package-lock.json: ${routerVersion ?? "missing"}, ` +
      `pnpm-lock.yaml: ${pnpmRouterVersion ?? "missing"}).`,
  );
  process.exit(1);
}

const result = spawnSync(
  "npm",
  ["audit", "--package-lock-only", "--json"],
  {
    encoding: "utf8",
    env: {
      ...process.env,
      npm_config_registry: "https://registry.npmjs.org",
    },
    maxBuffer: 16 * 1024 * 1024,
  },
);

if (result.error || !result.stdout) {
  console.error(result.error ?? result.stderr ?? "npm audit returned no report");
  process.exit(1);
}

let report;
try {
  report = JSON.parse(result.stdout);
} catch {
  console.error("npm audit did not return valid JSON");
  console.error(result.stderr);
  process.exit(1);
}

if (report.error || !report.vulnerabilities) {
  console.error("npm audit failed before producing a vulnerability report");
  console.error(JSON.stringify(report.error ?? report, null, 2));
  process.exit(1);
}

function advisoryRoots(packageName, seen = new Set()) {
  if (seen.has(packageName)) {
    return [];
  }
  seen.add(packageName);

  const vulnerability = report.vulnerabilities[packageName];
  if (!vulnerability || !Array.isArray(vulnerability.via)) {
    return [{ packageName, url: null, title: "unresolved audit dependency" }];
  }

  return vulnerability.via.flatMap((via) =>
    typeof via === "string"
      ? advisoryRoots(via, new Set(seen))
      : [
          {
            packageName,
            url: via.url ?? null,
            title: via.title ?? "unnamed advisory",
          },
        ],
  );
}

const unapproved = [];
for (const [packageName, vulnerability] of Object.entries(
  report.vulnerabilities,
)) {
  if (!["high", "critical"].includes(vulnerability.severity)) {
    continue;
  }

  const roots = advisoryRoots(packageName);
  if (
    roots.length === 0 ||
    roots.some((advisory) => advisory.url !== approvedAdvisory)
  ) {
    unapproved.push({ packageName, severity: vulnerability.severity, roots });
  }
}

if (unapproved.length > 0) {
  console.error("Unapproved high/critical npm dependency advisories:");
  console.error(JSON.stringify(unapproved, null, 2));
  process.exit(1);
}

console.log(
  "Dependency audit passed: no unapproved high/critical advisories; " +
    "the reviewed React Router RSC-only exception remains architecture-gated.",
);
