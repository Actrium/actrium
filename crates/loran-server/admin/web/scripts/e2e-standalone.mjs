// Run against a disposable loopback AFP with authority and local Relay disabled,
// relay.public_url set, and no actr verifier. Uses the same browser variables as e2e-apps.
import assert from "node:assert/strict";
import { randomUUID } from "node:crypto";
import { readFile } from "node:fs/promises";
import { createRequire } from "node:module";

const base = new URL(process.env.LORAN_ADMIN_URL);
assert(["localhost", "127.0.0.1", "[::1]"].includes(base.hostname), "Use a disposable loopback node");
const { chromium } = createRequire(import.meta.url)(process.env.PLAYWRIGHT_MODULE || "playwright");
const browser = await chromium.launch({ executablePath: process.env.CHROMIUM_EXECUTABLE });
const page = await browser.newPage({ ignoreHTTPSErrors: process.env.LORAN_ADMIN_LOCAL_TLS === "1" });
page.setDefaultTimeout(12000);
const url = (path) => new URL(path, base).href;
const button = (name, root = page) => root.getByRole("button", { name, exact: true });
let auth;
const get = (path) => page.request.get(url(path), { headers: auth });
const packageFixture = (name, hash) => ({
  href: "https://packages.example.org/" + name + ".actr",
  sha256: hash.repeat(64),
  type: "publisher.example/" + name + "@1.0.0",
});

try {
  await page.goto(url("/admin/login"));
  await page.getByLabel("Password", { exact: true }).fill(
    (await readFile(process.env.LORAN_ADMIN_PASSWORD_FILE, "utf8")).trim(),
  );
  await page.locator('button[type="submit"]').click();
  await page.waitForFunction(() => Boolean(localStorage.getItem("loran_admin_token")));
  auth = { authorization: "Bearer " + await page.evaluate(() => localStorage.getItem("loran_admin_token")) };
  const caps = await (await get("/admin/api/capabilities")).json();
  assert(caps.realm_writes_enabled);
  assert.equal(caps.provider_id, null);
  assert.equal(caps.package_verifier_available, false);

  await page.goto(url("/admin/apps"));
  await button("Create app").click();
  const advanced = page.getByLabel("Edit complete package selection");
  await advanced.check();
  assert(await button("Verify package archives").isDisabled());
  await page.getByText(/Package verification is unavailable on this node/).waitFor();
  const verification = await page.request.post(url("/admin/api/apps/check-selection"), {
    headers: auth,
    data: {
      primary_package: "service", front_package: "front",
      packages: { service: packageFixture("service", "1"), front: packageFixture("front", "2") },
    },
  });
  assert.equal(verification.status(), 503);
  await advanced.uncheck();
  const app = "standalone-e2e-" + randomUUID().slice(0, 8) + "@localhost";
  await page.getByLabel("App ID", { exact: true }).fill(app);
  await button("Next").click();
  await button("I have saved the invite, resume app").click();
  console.log("PASS missing verifier disables only package verification");

  await button(app).click();
  await page.getByText("This app has no signed Realm Profile.", { exact: true }).waitFor();
  await button("Recover Owner").click();
  await button("Recover Owner", page.getByRole("dialog", { name: "Recover Owner" })).click();
  await page.locator(".fixed code").filter({ hasText: /^ri_/ }).waitFor();
  await button("Close", page.locator(".fixed")).click();
  await button("Retire app").click();
  await button("Retire app", page.getByRole("dialog", { name: "Retire app" })).click();
  await page.getByText("Status · Retired", { exact: true }).waitFor();
  const retired = await get("/admin/api/realms/" + encodeURIComponent(app));
  assert.equal((await retired.json()).realm.status, "retired");
  console.log("PASS unsigned App can recover Owner and retire from its details");

  const discovery = await (await get("/relay/config")).json();
  assert.equal(discovery.enabled, false);
  assert(discovery.url);
  await page.goto(url("/admin/services/relay"));
  const card = page.getByRole("region", { name: "Service access" });
  await card.getByText(discovery.url, { exact: true }).waitFor();
  await card.getByText("Local relay is disabled on this node.", { exact: true }).waitFor();
  assert.equal(await card.getByText("Local listener", { exact: true }).count(), 0);
  console.log("PASS disabled local Relay still shows the published remote endpoint");
} finally {
  await browser.close();
}
