// Companion to e2e-apps.mjs. Uses a disposable superv-managed node and the
// repository's package_runtime_fixture, whose HTTPS descriptor transport reads
// the operator-supplied fixture file. The signing and admission paths are real.
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { randomUUID } from "node:crypto";
import { mkdtemp, readFile, rename, rm, writeFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import { join } from "node:path";
const { chromium } = createRequire(import.meta.url)(process.env.PLAYWRIGHT_MODULE || "playwright");
const temp = await mkdtemp(join(tmpdir(), "loran-admin-domain-e2e-"));
const password = (await readFile(process.env.LORAN_ADMIN_PASSWORD_FILE, "utf8")).trim();
const browser = await chromium.launch({ headless: true, executablePath: process.env.CHROMIUM_EXECUTABLE });
const page = await browser.newPage({ ignoreHTTPSErrors: process.env.LORAN_ADMIN_LOCAL_TLS === "1" });
const passed = [];
let host, auth;
async function login(raw) {
  host = new URL(raw);
  assert(["localhost", "127.0.0.1", "[::1]"].includes(host.hostname));
  await page.goto(new URL("/admin/apps", host).href);
  await page.getByLabel("Password", { exact: true }).fill(password);
  await page.locator('button[type="submit"]').click();
  await page.waitForFunction(() => Boolean(localStorage.getItem("loran_admin_token")));
  auth = await page.evaluate(() => localStorage.getItem("loran_admin_token"));
  await page.goto(new URL("/admin/apps", host).href);
}
async function request(path, body, status, method = body ? "POST" : "GET") {
  const response = await fetch(new URL("/admin/api/" + path, host), {
    method,
    headers: { "content-type": "application/json", authorization: "Bearer " + auth },
    ...(body && { body: JSON.stringify(body) }),
  });
  const json = await response.json();
  assert.equal(response.status, status, path + " " + (json.error_message || json.error || ""));
  return json;
}
function cli(...args) {
  return JSON.parse(
    execFileSync(process.env.LORAN_ADMIN_ACTR_BIN, ["realm", ...args], { encoding: "utf8", cwd: temp }),
  );
}
async function step(name, fn) {
  await fn();
  passed.push(name);
  console.log("PASS " + name);
}
try {
  const selection = JSON.parse(await readFile(process.env.LORAN_ADMIN_PACKAGE_SELECTION_FILE, "utf8"));
  await step("superv-managed node exposes read-only Apps and rejects every Admin mutation", async () => {
    await login(process.env.LORAN_ADMIN_READONLY_URL);
    const caps = await request("capabilities", null, 200);
    assert(caps.superv_managed && !caps.realm_writes_enabled);
    assert(await page.getByRole("button", { name: "Create app" }).isDisabled());
    const relay = await request("services/relay", null, 200);
    if (!relay.enabled) {
      await page.goto(new URL("/admin/services/relay", host).href);
      await page.getByText("Disabled", { exact: true }).waitFor();
      if (relay.config.public_url) await page.getByText(relay.config.public_url, { exact: true }).waitFor();
    }
    const id = "readonly-" + randomUUID().slice(0, 8) + "@localhost",
      route = "realms/" + encodeURIComponent(id);
    for (const [path, body, method] of [
      ["realms", { realm: id, ...selection }],
      [route, { enabled: false }, "PUT"],
      [route + "/owner-invite", {}],
      [route + "/retire", {}],
      [route + "/management-client", { invite: "unused" }],
      [route + "/member-actions", { request_id: randomUUID(), action: "invite_member", role: "viewer", days: 1 }],
      [route + "/operations/" + randomUUID() + "/reveal", {}],
      [route + "/operations/" + randomUUID() + "/reveal", null, "DELETE"],
      [route + "/signed-management", {}],
      ["apps/check-selection", selection],
    ])
      await request(path, body, 403, method);
    assert(!(await request("realms", null, 200)).realms.find((app) => app.realm === id));
  });
  await login(process.env.LORAN_ADMIN_DOMAIN_FIXTURE_URL);
  const domain = "tenant.realm-e2e.test",
    realm = "signed-" + randomUUID().slice(0, 8) + "@" + domain;
  const route = "realms/" + encodeURIComponent(realm),
    docs = process.env.LORAN_ADMIN_DOMAIN_DOCUMENTS;
  const key = join(temp, "holder.json"),
    packages = join(temp, "selection.json"),
    draft = join(temp, "draft.json"),
    profile = join(temp, "profile.json");
  await writeFile(packages, JSON.stringify(selection));
  const keys = cli("keygen", "--key-id", "admin-e2e-holder", "--output", key);
  const descriptor = {
    version: 1,
    authority: "actr://" + domain,
    realm_signing_keys: keys,
    services: [{ rel: "realms", href: "https://" + domain + "/realms", protocol: "actr-realm-profile/1" }],
  };
  const published = JSON.parse(await readFile(docs, "utf8"));
  const descriptorUrl = "https://" + domain + "/.well-known/actr";
  // Rerunning needs the same holder. A different fixture identity would trip the runtime pin.
  if (published[descriptorUrl]) throw Error("Use a fresh external-domain fixture (database and descriptors)");
  published[descriptorUrl] = descriptor;
  async function publish() {
    await writeFile(docs + ".new", JSON.stringify(published));
    await rename(docs + ".new", docs);
  }
  await publish();
  const signed = cli(
    "create-request",
    "--key",
    key,
    "--realm",
    realm,
    "--provider",
    "localhost",
    "--selection",
    packages,
  );
  await step(
    "External domain holder submits a signed request in the UI and saves the returned draft",
    async () => {
      await request(route + "/signed-management", { ...signed, nonce: signed.nonce + "-tampered" }, 403);
      await page.getByText("External domain · signed requests", { exact: true }).click();
      await page.getByLabel("External App ID").fill(realm);
      await page.getByLabel("Signed request JSON").fill(JSON.stringify(signed));
      await page.getByRole("button", { name: "Submit signed request" }).click();
      const output = page.getByLabel(/Domain response/);
      await output.waitFor();
      const pending = JSON.parse(await output.inputValue());
      assert.equal(pending.status, "pending");
      assert.equal(pending.owner_invite, undefined);
      await writeFile(draft, JSON.stringify(pending));
      await request(route + "/signed-management", signed, 400);
    },
  );
  await step("Holder-signed profile activates the external app, recovers its Owner and retires it", async () => {
    const activation = cli("sign-profile", "--key", key, "--selection", packages, draft);
    const publishUrl = "https://" + domain + "/realms/" + realm.split("@")[0];
    published[publishUrl] = activation.profile;
    await publish();
    const success = await request(route + "/signed-management", activation, 200);
    assert.equal(success.status, "active");
    assert(success.owner_invite.startsWith("ri_"));
    const view = await request(route + "/profile", null, 200);
    assert.equal(view.profile.realm.name, realm);
    await writeFile(profile, JSON.stringify(view.profile));
    const recover = cli("reissue-request", "--key", key, "--realm", realm, "--provider", "localhost");
    const recovery = await request(route + "/signed-management", recover, 200);
    assert(recovery.owner_invite.startsWith("ri_"));
    const retire = cli("retire", "--key", key, profile);
    published[publishUrl] = retire.profile;
    await publish();
    assert.equal((await request(route + "/signed-management", retire, 200)).status, "retired");
  });
} finally {
  await browser.close();
  await rm(temp, { recursive: true, force: true });
  if (process.env.LORAN_ADMIN_AUTHORITY_REPORT)
    await writeFile(
      process.env.LORAN_ADMIN_AUTHORITY_REPORT,
      JSON.stringify({ passed, finished_at: new Date().toISOString() }, null, 2) + "\n",
    );
}
