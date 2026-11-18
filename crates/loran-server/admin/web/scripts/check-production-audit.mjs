import { spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";

const routerAdvisory = "https://github.com/advisories/GHSA-qwww-vcr4-c8h2";
const approvedRouterVersion = "7.18.2";
const picomatchAdvisory = "https://github.com/advisories/GHSA-fc5q-39xc-vjpf";
const approvedBuildPicomatchVersion = "4.0.5";

const packageJson = JSON.parse(
  readFileSync(new URL("../package.json", import.meta.url), "utf8"),
);
const packageLock = JSON.parse(
  readFileSync(new URL("../package-lock.json", import.meta.url), "utf8"),
);
// Check the pnpm resolutions alongside the npm audit input so an exception
// cannot migrate to a different build dependency version.
const pnpmLock = readFileSync(
  new URL("../pnpm-lock.yaml", import.meta.url),
  "utf8",
);
const appSource = readFileSync(
  new URL("../src/App.tsx", import.meta.url),
  "utf8",
);

// The React Router exception applies to a static SPA using BrowserRouter.
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

// The reviewed picomatch version is a build dependency for Vite's source scan.
// Vite also rejects picomatch modules in the emitted Admin UI bundle.
const pnpmPicomatchVersions = new Set(
  Array.from(pnpmLock.matchAll(/^ {2}picomatch@([^:\s]+):/gm), ([, version]) => version),
);
const npmPicomatch = Object.entries(packageLock.packages ?? {}).filter(
  ([path]) => /(?:^|\/)node_modules\/picomatch$/.test(path),
);
const reviewedBuildPicomatch =
  pnpmPicomatchVersions.size === 1 &&
  pnpmPicomatchVersions.has(approvedBuildPicomatchVersion) &&
  npmPicomatch.length > 0 &&
  npmPicomatch.every(
    ([, entry]) => entry.dev === true && entry.version === approvedBuildPicomatchVersion,
  );

let buildAdvisoryAccepted = false;
const unapproved = [];
for (const [packageName, vulnerability] of Object.entries(
  report.vulnerabilities,
)) {
  if (!["high", "critical"].includes(vulnerability.severity)) {
    continue;
  }

  const roots = advisoryRoots(packageName);
  const buildOnly =
    reviewedBuildPicomatch &&
    vulnerability.severity === "high" &&
    Array.isArray(vulnerability.nodes) &&
    vulnerability.nodes.length > 0 &&
    vulnerability.nodes.every((path) => packageLock.packages[path]?.dev === true);
  const hasUnapprovedRoot = roots.some(
    ({ packageName: origin, url }) =>
      url !== routerAdvisory &&
      !(buildOnly && origin === "picomatch" && url === picomatchAdvisory),
  );
  if (roots.length === 0 || hasUnapprovedRoot) {
    unapproved.push({ packageName, severity: vulnerability.severity, roots });
  } else if (roots.some(({ url }) => url === picomatchAdvisory)) {
    buildAdvisoryAccepted = true;
  }
}

if (unapproved.length > 0) {
  console.error("Unapproved high/critical npm dependency advisories:");
  console.error(JSON.stringify(unapproved, null, 2));
  process.exit(1);
}

if (buildAdvisoryAccepted) {
  console.warn(
    `Build-only advisory accepted for picomatch ${approvedBuildPicomatchVersion}: ${picomatchAdvisory}`,
  );
}
console.log("Dependency audit passed: no unapproved high/critical advisories.");
