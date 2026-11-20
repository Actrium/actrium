// Run against a disposable AFP with real signed Echo packages and the readiness fixture.
import assert from "node:assert/strict";
import { createPublicKey, randomUUID, verify } from "node:crypto";
import { spawn, spawnSync } from "node:child_process";
import { closeSync, openSync } from "node:fs";
import { copyFile, mkdir, readFile, writeFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { join, resolve } from "node:path";

const root = resolve(process.env.LORAN_RELEASE_FIXTURES);
const base = new URL(process.env.LORAN_ADMIN_URL);
assert(["localhost", "127.0.0.1", "[::1]"].includes(base.hostname));
const url = (path) => new URL(path, base).href;
const load = async (file) => JSON.parse(await readFile(join(root, file), "utf8"));
const initial = await load("packages/selection-0.1.7.json");
const target = await load("packages/selection-0.1.8.json");
const publisher = await load("packages/publisher.json");
const publisherKey = publisher.keys[0];
const authority = await load("authority.json");
const { chromium } = createRequire(import.meta.url)(process.env.PLAYWRIGHT_MODULE || "playwright");
const browser = await chromium.launch({ executablePath: process.env.CHROMIUM_EXECUTABLE });
const page = await browser.newPage({ ignoreHTTPSErrors: process.env.LORAN_ADMIN_LOCAL_TLS === "1" });
page.setDefaultTimeout(25_000);
const button = (name) => page.getByRole("button", { name, exact: true });
const field = (name) => page.getByLabel(name, { exact: true });
const apiPath = (app, suffix = "") => "/admin/api/realms/" + encodeURIComponent(app) + suffix;
const tests = [];
let token;

function pass(name) {
  tests.push(name);
  console.log("PASS " + name);
}
async function http(path, method = "GET", body, status = 200) {
  const response = await fetch(url(path), {
    method,
    headers: { authorization: "Bearer " + token, "content-type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const result = await response.json().catch(() => ({}));
  assert.equal(response.status, status, method + " " + path + " failed");
  return result;
}
function canonical(value) {
  if (Array.isArray(value)) return value.map(canonical);
  if (value && typeof value === "object") {
    return Object.fromEntries(Object.keys(value).sort().map((key) => [key, canonical(value[key])]));
  }
  return value;
}
function signed(profile) {
  const { signature, ...document } = profile;
  const publicKey = authority.realm_signing_keys.keys.find((key) => key.id === signature.key_id);
  assert(publicKey);
  const key = createPublicKey({
    key: Buffer.concat([Buffer.from("302a300506032b6570032100", "hex"), Buffer.from(publicKey.public_key, "base64")]),
    format: "der",
    type: "spki",
  });
  assert(verify(null, Buffer.from(JSON.stringify(canonical(document))), key, Buffer.from(signature.value, "base64")));
  return profile;
}
const profileOf = async (app) => signed((await http(apiPath(app, "/profile"))).profile);
const selectionOf = ({ primary_package, front_package, packages }) => ({ primary_package, front_package, packages });

const cli = process.env.LORAN_ECHO_ACTR_BIN;
const icePort = Number(process.env.LORAN_ECHO_ICE_PORT || 25513);
assert(Number.isInteger(icePort) && icePort > 0 && icePort <= 65535);
function actr(args, input) {
  const result = spawnSync(cli, args, { input, encoding: "utf8", timeout: 30_000 });
  assert.equal(result.status, 0, "Echo fixture actr " + args.slice(0, 2).join(" ") + " failed");
  return JSON.parse(result.stdout);
}
function joinRealm(ownerInvite, key, label) {
  return actr(["realm", "join", "--issuer", url("/issuer"), "--provider", "localhost",
    "--output", key, "--label", label, "-"], ownerInvite);
}
const runtimes = [];
async function startEcho(app, version, key) {
  const run = join(root, app, "echo-" + version);
  await mkdir(run, { recursive: true });
  const config = join(run, "actr.toml");
  await writeFile(config, [
    "edition = 1", "[package]",
    "path = " + JSON.stringify(join(root, "packages/service-" + version + ".actr")),
    "[deployment]", "realm = " + JSON.stringify(app), "realm_client_key = " + JSON.stringify(key),
    "[signaling]", "url = " + JSON.stringify(url("/signaling/ws").replace("https:", "wss:")),
    "[issuer_endpoint]", "url = " + JSON.stringify(url("/issuer")),
    "[[trust]]", 'kind = "static"', "pubkey_b64 = " + JSON.stringify(publisherKey.public_key),
    "[discovery]", "visible = true",
    "[webrtc]", "stun_urls = " + JSON.stringify(["stun:127.0.0.1:" + icePort]), "turn_urls = " + JSON.stringify(["turn:127.0.0.1:" + icePort]),
    "[[acl.rules]]", 'permission = "allow"', 'realm = "self"',
    "type = " + JSON.stringify("publisher.afp-p1.test/echo-front@" + version),
  ].join("\n"), { mode: 0o600 });
  const log = join(run, "service.log");
  const fd = openSync(log, "a", 0o600);
  const child = spawn(cli, ["run", "--config", config, "--hyper-dir", join(run, "hyper")], {
    cwd: run, stdio: ["ignore", fd, fd], env: { ...process.env, RUST_LOG: "info" },
  });
  closeSync(fd);
  runtimes.push(child);
  const deadline = Date.now() + 15_000;
  while (Date.now() < deadline && child.exitCode === null) {
    if ((await readFile(log, "utf8")).includes("Running")) return;
    await new Promise((done) => setTimeout(done, 150));
  }
  throw Error("Echo Service " + version + " did not start");
}
async function configureProbe(app, probeKey, route) {
  const dir = join(root, "readiness", app);
  await mkdir(dir, { recursive: true });
  await writeFile(join(dir, "probe.json"), JSON.stringify({ route }), { mode: 0o600 });
  await writeFile(join(dir, "actr.toml"), [
    "edition = 1", "[deployment]", "realm = " + JSON.stringify(app),
    "realm_client_key = " + JSON.stringify(probeKey),
    "[signaling]", "url = " + JSON.stringify(url("/signaling/ws").replace("https:", "wss:")),
    "[issuer_endpoint]", "url = " + JSON.stringify(url("/issuer")),
    "[[trust]]", 'kind = "static"', "pubkey_b64 = " + JSON.stringify(publisherKey.public_key),
    "[hyper]", "data_dir = " + JSON.stringify(join(dir, "hyper")),
    "[discovery]", "visible = false",
    "[webrtc]", "stun_urls = " + JSON.stringify(["stun:127.0.0.1:" + icePort]), "turn_urls = " + JSON.stringify(["turn:127.0.0.1:" + icePort]),
    "[[acl.rules]]", 'permission = "allow"', 'realm = "self"', 'type = "publisher.afp-p1.test/echo@0.1.7"',
    "[[acl.rules]]", 'permission = "allow"', 'realm = "self"', 'type = "publisher.afp-p1.test/echo@0.1.8"',
  ].join("\n"), { mode: 0o600 });
  await writeFile(join(dir, "request.bin"), Buffer.from([10, 5, 114, 101, 97, 100, 121]), { mode: 0o600 });
}
async function createLocalApp(rebuild) {
  const name = "p1-" + (rebuild ? "rebuild-" : "fixed-") + randomUUID().slice(0, 8) + "@localhost";
  const localPath = rebuild ? "echo-front.actr" : join(root, "Front files", "echo-front.actr");
  if (!rebuild) {
    await mkdir(join(root, "Front files"), { recursive: true });
    await copyFile(join(root, "packages/front-0.1.7.actr"), localPath);
  }
  await page.goto(url("/admin/apps"));
  await button("Create app").click();
  await field("App ID").fill(name);
  for (const key of ["href", "sha256", "type"]) {
    await field("service " + key).fill(initial.packages.service[key]);
  }
  await field("Front source").selectOption("local");
  await field("Front signing key ID").fill(publisherKey.id);
  await field("front type").fill(initial.packages.front.type);
  if (rebuild) {
    await field("Allow rebuilds signed by this key").check();
    assert.equal(await field("front sha256").count(), 0);
  } else {
    await field("front sha256").fill(initial.packages.front.sha256);
    await field("front href").fill("/etc/echo-front.actr");
    await button("Next").click();
    await page.getByText("Enter a safe absolute .actr path or a file name.", { exact: true }).waitFor();
  }
  await field("front href").fill(localPath);
  const expected = structuredClone(initial);
  expected.packages.front.href = localPath;
  expected.packages.front.local_path = { signing_key_id: publisherKey.id, allow_rebuild: rebuild };
  if (rebuild) delete expected.packages.front.sha256;
  await field("Edit complete package selection").check();
  const json = page.getByLabel(/Package selection JSON/);
  assert.deepEqual(JSON.parse(await json.inputValue()), expected);
  await field("Edit complete package selection").uncheck();
  await button("Verify package archives").click();
  await page.getByRole("status").filter({ hasText: "1 package archives verified" }).waitFor();
  const created = page.waitForResponse((response) => response.url() === url("/admin/api/realms") &&
    response.request().method() === "POST");
  await button("Next").click();
  assert.equal((await created).status(), 201);
  await button("Keep suspended for now").click();
  const appProfile = await profileOf(name);
  assert.deepEqual(selectionOf(appProfile), expected);
  const publicProfile = signed(await http("/actr/realms/localhost/" + name.split("@")[0]));
  assert.deepEqual(publicProfile, appProfile);
  await page.goto(url("/admin/apps/" + encodeURIComponent(name)));
  await page.getByText(/^\s*front\s+\(Front\)/).click();
  await page.getByText(localPath, { exact: true }).waitFor();
  assert.equal(await page.getByRole("link", { name: localPath, exact: true }).count(), 0);
  pass(rebuild ? "Rebuildable local Front omits SHA-256 in the public signed Profile" :
    "Fixed local Front preserves the absolute path and SHA-256 in the public signed Profile");
}
function releaseBody(selection, baseline, rollback = false) {
  return {
    selection,
    operation: { operation_id: randomUUID().replace(/-/g, ""), expected_sequence: baseline.sequence, rollback },
  };
}
try {
  await page.goto(url("/admin/login"));
  await field("Password").fill((await readFile(process.env.LORAN_ADMIN_PASSWORD_FILE, "utf8")).trim());
  await page.locator('button[type="submit"]').click();
  await page.waitForFunction(() => Boolean(localStorage.getItem("loran_admin_token")));
  token = await page.evaluate(() => localStorage.getItem("loran_admin_token"));
  await createLocalApp(false);
  await createLocalApp(true);

  const app = "p1-cutover-" + randomUUID().slice(0, 8) + "@localhost";
  const created = await http("/admin/api/realms", "POST", {
    realm: app, enabled: true, display_name: "P1 Service cutover", ...initial,
  }, 201);
  const baseline = await profileOf(app);
  const originalKey = join(root, app + ".owner");
  const owner = joinRealm(created.owner_invite, originalKey, "Echo Service owner");
  const probeKey = join(root, app + ".probe");
  const probeInvite = actr(["realm", "invite", "--key", originalKey, "--role", "viewer", "--days", "1"]);
  joinRealm(probeInvite.invite, probeKey, "Independent readiness check");
  const activeRelease = apiPath(app, "/packages");
  const unchanged = await http(activeRelease, "PUT", releaseBody(initial, baseline));
  assert.equal(unchanged.changed, false);
  assert.equal(unchanged.sequence, baseline.sequence);
  await http(activeRelease, "PUT", releaseBody(target, baseline), 409);
  assert.deepEqual(await profileOf(app), baseline);
  pass("An Admin can submit an unchanged selection, but a release requires an authorized Owner device");

  const adminInvite = actr(["realm", "invite", "--key", originalKey, "--member", owner.member_id, "--days", "1"]);
  await http(apiPath(app, "/management-client"), "POST", { invite: adminInvite.invite });
  await startEcho(app, "0.1.7", originalKey);
  await http(apiPath(app, "/member-actions"), "POST", {
    action: "update_app", request_id: randomUUID(), selection: target,
  }, 400);
  assert.deepEqual(await profileOf(app), baseline);
  pass("Legacy profile edit cannot bypass the guarded release");

  const missing = await http(activeRelease, "PUT", releaseBody(target, baseline), 503);
  assert.match(missing.error, /not configured/);
  assert.deepEqual(await profileOf(app), baseline);
  pass("Missing readiness configuration preserves the current signed Profile");

  const route = "echo.v1.EchoService.Echo";
  await configureProbe(app, probeKey, route);
  const bad = structuredClone(target);
  bad.packages.service.sha256 = "0".repeat(64);
  await http(activeRelease, "PUT", releaseBody(bad, baseline), 503);
  assert.deepEqual(await profileOf(app), baseline);
  pass("Invalid target archive digest preserves the current signed Profile");

  await http(activeRelease, "PUT", releaseBody(target, baseline), 503);
  assert.deepEqual(await profileOf(app), baseline);
  pass("A valid target archive cannot publish before its Service is online");

  await startEcho(app, "0.1.8", originalKey);
  await page.goto(url("/admin/apps/" + encodeURIComponent(app)));
  await button("Release packages").click();
  await field("Packages JSON").fill(JSON.stringify(target.packages));
  const response = page.waitForResponse(
    (res) => res.url() === url(activeRelease) && res.request().method() === "PUT",
    { timeout: 310_000 },
  );
  await button("Update packages").click();
  const published = await response;
  assert.equal(published.status(), 200, (await published.json()).error ?? "Package release failed");
  const sent = published.request().postDataJSON();
  const released = await profileOf(app);
  assert.equal(released.sequence, baseline.sequence + 1);
  assert.deepEqual(selectionOf(released), target);
  assert.equal((await http(activeRelease, "PUT", sent)).sequence, released.sequence);
  await http(activeRelease, "PUT", releaseBody(initial, baseline), 409);
  pass("AFP UI verifies the target Service RPC and publishes exactly once with replay and sequence checks");

  await configureProbe(app, probeKey, "echo.v1.EchoService.Unavailable");
  await http(activeRelease, "PUT", releaseBody(initial, released, true), 503);
  assert.deepEqual(await profileOf(app), released);
  pass("A live previous Service with a failing readiness route cannot be restored");

  await configureProbe(app, probeKey, route);
  const rollback = await http(activeRelease, "PUT", releaseBody(initial, released, true));
  const restored = await profileOf(app);
  assert.equal(rollback.sequence, released.sequence + 1);
  assert.equal(restored.sequence, rollback.sequence);
  assert.deepEqual(selectionOf(restored), initial);
  pass("Rollback verifies the previous Service and restores its signed selection");

  const local = structuredClone(initial);
  local.packages.front.href = "echo-front.actr";
  local.packages.front.local_path = { signing_key_id: publisherKey.id, allow_rebuild: false };
  await configureProbe(app, probeKey, "echo.v1.EchoService.Unavailable");
  const frontRelease = await http(activeRelease, "PUT", releaseBody(local, restored));
  assert.equal(frontRelease.sequence, restored.sequence + 1);
  const localProfile = await profileOf(app);
  assert.deepEqual(selectionOf(localProfile), local);
  pass("A Front-only release retains the selected Service and verifies the local Publisher key");

  await configureProbe(app, probeKey, route);
  await page.goto(url("/admin/apps/" + encodeURIComponent(app)));
  await page.getByText("Edit app profile", { exact: true }).click();
  const name = field("App display name");
  const draftName = "Unsaved cutover name";
  await name.fill(draftName);
  await button("Release packages").click();
  const modal = page.getByRole("heading", { name: "Realm packages", exact: true }).locator("xpath=ancestor::section[1]");
  await field("Packages JSON").fill(JSON.stringify(target.packages));
  let failedRefreshes = 0;
  const failRefresh = async (request) => {
    if (request.request().method() === "GET") {
      failedRefreshes++;
      await request.fulfill({ status: 503, json: { error: "Package state temporarily unavailable" } });
    } else {
      await request.fallback();
    }
  };
  await page.route(url(activeRelease), failRefresh);
  let afterCutover;
  try {
    const result = page.waitForResponse(
      res => res.url() === url(activeRelease) && res.request().method() === "PUT",
      { timeout: 310_000 },
    );
    await button("Update packages").click();
    assert.equal((await result).status(), 200);
    afterCutover = await profileOf(app);
    assert.equal(afterCutover.sequence, localProfile.sequence + 1);
    assert.deepEqual(selectionOf(afterCutover), target);
    await modal.getByRole("status").filter({ hasText: "Published Profile " + afterCutover.sequence + "." }).waitFor();
    await modal.getByText(app + " · Profile " + afterCutover.sequence, { exact: true }).waitFor();
    await modal.getByRole("alert").filter({ hasText: "Package details may be out of date." }).waitFor();
    assert(failedRefreshes > 0);
    assert(await button("Update packages").isDisabled());
    assert(await button("Rollback").isDisabled());
    await page.getByText("Provider localhost · Sequence " + afterCutover.sequence, { exact: true }).waitFor();
    pass("A successful release and its sequence survive a failed package refresh, and stale edits are blocked");
  } finally {
    await page.unroute(url(activeRelease), failRefresh);
  }
  await modal.getByRole("button", { name: "Close", exact: true }).click();
  assert.equal(await name.inputValue(), draftName);
  await button("Save signed profile").click();
  await page.getByRole("heading", { name: draftName, exact: true, level: 1 }).waitFor();
  const renamed = await profileOf(app);
  assert.equal(renamed.realm.display_name, draftName);
  assert.equal(renamed.sequence, afterCutover.sequence + 1);
  assert(await button("Save signed profile").isDisabled());
  pass("Publishing packages preserves the name draft, and saving it updates the signed Profile");

  const updatedName = "Name from a different Admin request";
  await http(apiPath(app, "/member-actions"), "POST", {
    action: "update_app", display_name: updatedName, request_id: randomUUID(),
  });
  const latest = await profileOf(app);
  await http(activeRelease, "PUT", releaseBody(local, latest), 400);
  await http(activeRelease, "PUT", releaseBody(local, renamed, true), 409);
  const differentLocal = structuredClone(local);
  differentLocal.packages.front.href = "different-front.actr";
  await http(activeRelease, "PUT", releaseBody(differentLocal, latest, true), 409);
  assert.deepEqual(await profileOf(app), latest);
  pass("A normal Service release or a stale or mismatched rollback cannot restore a Local Front");

  await configureProbe(app, probeKey, "echo.v1.EchoService.Unavailable");
  await button("Release packages").click();
  const localRollback = page.waitForResponse(
    res => res.url() === url(activeRelease) && res.request().method() === "PUT",
    { timeout: 310_000 },
  );
  await button("Rollback").click();
  const received = await localRollback;
  assert.equal(received.status(), 200, (await received.json()).error ?? "Local Front rollback failed");
  const restoredLocal = await profileOf(app);
  assert.equal(restoredLocal.sequence, latest.sequence + 1);
  assert.deepEqual(selectionOf(restoredLocal), local);
  assert.equal(restoredLocal.realm.display_name, updatedName);
  await modal.getByRole("button", { name: "Close", exact: true }).click();
  await page.getByRole("heading", { name: updatedName, exact: true, level: 1 }).waitFor();
  assert.equal(await name.inputValue(), updatedName);
  pass("A recorded Local Front rollback restores the signed selection at a newer sequence");
  await writeFile(join(root, "logs", "package-release-results.json"), JSON.stringify({ tests }, null, 2));
} catch (error) {
  console.error(error);
  process.exitCode = 1;
} finally {
  await browser.close();
  for (const child of runtimes) child.kill("SIGTERM");
}
