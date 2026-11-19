// Disposable loopback AFP with Authority, Signer and the archive verifier enabled.
// Intercepts archive verification only to compare its payload; App creation and
// signed Profile readback use the real AFP.
import assert from "node:assert/strict";
import { createHash, randomUUID } from "node:crypto";
import { readFile } from "node:fs/promises";
import { createRequire } from "node:module";

const base = new URL(process.env.LORAN_ADMIN_URL);
assert(["localhost", "127.0.0.1", "[::1]"].includes(base.hostname), "Use a disposable loopback node");
const url = (path) => new URL(path, base).href;
const { chromium } = createRequire(import.meta.url)(process.env.PLAYWRIGHT_MODULE || "playwright");
const browser = await chromium.launch({ executablePath: process.env.CHROMIUM_EXECUTABLE });
const page = await browser.newPage({ ignoreHTTPSErrors: process.env.LORAN_ADMIN_LOCAL_TLS === "1" });
page.setDefaultTimeout(12000);
const button = (name) => page.getByRole("button", { name, exact: true });
const field = (name) => page.getByLabel(name, { exact: true });
const fixture = (name) => ({
  href: "https://packages.example.org/" + name + ".actr",
  sha256: createHash("sha256").update(name).digest("hex"),
  type: "publisher.example/" + name + "@1.0.0",
});
const selectionOf = ({ primary_package, front_package, packages }) => ({ primary_package, front_package, packages });

try {
  await page.goto(url("/admin/login"));
  await field("Password").fill((await readFile(process.env.LORAN_ADMIN_PASSWORD_FILE, "utf8")).trim());
  await page.locator('button[type="submit"]').click();
  await page.waitForFunction(() => Boolean(localStorage.getItem("loran_admin_token")));
  const token = await page.evaluate(() => localStorage.getItem("loran_admin_token"));
  const apiGet = (path) => page.request.get(url(path), { headers: { authorization: "Bearer " + token } });
  const caps = await (await apiGet("/admin/api/capabilities")).json();
  assert(caps.provider_id && caps.package_verifier_available);
  const realm = "packages-e2e-" + randomUUID().slice(0, 8) + "@" + caps.provider_id;

  await page.goto(url("/admin/apps"));
  await button("Create app").click();
  await field("App ID").fill(realm);
  for (let i = 0; i < 3; i++) await button("Add Service package").click();
  await button("Remove Service package service-4").click();
  assert.equal(await field("service package service-4 href").count(), 0);

  const original = {
    primary_package: "service", front_package: "front",
    packages: {
      service: fixture("main"),
      "service-2": fixture("extra-one"),
      "service-3": fixture("extra-two"),
      front: fixture("front"),
    },
  };
  for (const [ref, pkg] of Object.entries(original.packages)) {
    const prefix = ref === "service" || ref === "front" ? ref : "service package " + ref;
    for (const [key, value] of Object.entries(pkg)) await field(prefix + " " + key).fill(value);
  }

  const toggle = field("Edit complete package selection");
  const json = page.getByLabel(/Package selection JSON/);
  await toggle.check();
  assert.deepEqual(JSON.parse(await json.inputValue()), original);
  const changed = {
    primary_package: "runtime", front_package: "interface",
    packages: {
      runtime: original.packages.service,
      "service-2": { ...original.packages["service-2"], href: "https://packages.example.org/extra-one-new.actr" },
      "service-3": { ...original.packages["service-3"], dependencies: { parent: "runtime" } },
      interface: { ...original.packages.front, dependencies: { service: "runtime", helper: "service-2" } },
    },
  };
  const invalid = JSON.stringify(changed) + "{";
  await json.fill(invalid);
  await toggle.click();
  assert(await toggle.isChecked());
  assert.equal(await json.inputValue(), invalid);
  assert(await page.getByRole("alert").filter({ hasText: "not valid JSON" }).isVisible());
  assert(await button("Verify package archives").isDisabled());
  assert(await button("Next").isDisabled());

  const missingRef = { ...changed, primary_package: "unknown" };
  await json.fill(JSON.stringify(missingRef));
  await toggle.click();
  assert(await toggle.isChecked());
  assert(await page.getByRole("alert").filter({ hasText: "distinct package entries" }).isVisible());

  await json.fill(JSON.stringify(changed, null, 2));
  await toggle.uncheck();
  assert.equal(await field("service type").inputValue(), changed.packages.runtime.type);
  assert.equal(await field("front type").inputValue(), changed.packages.interface.type);
  assert.equal(await field("service package service-2 href").inputValue(), changed.packages["service-2"].href);
  assert(await button("Remove Service package service-2").isDisabled());
  changed.packages["service-3"].href = "https://packages.example.org/extra-two-updated.actr";
  await field("service package service-3 href").fill(changed.packages["service-3"].href);
  await toggle.check();
  assert.deepEqual(JSON.parse(await json.inputValue()), changed);
  await toggle.uncheck();
  console.log("PASS form ↔ JSON preserves four packages, primary/Front references and dependencies");

  const checks = [];
  await page.route(url("/admin/api/apps/check-selection"), (route) => {
    checks.push(route.request().postDataJSON());
    return route.fulfill({ json: { verified: true, count: Object.keys(checks.at(-1).packages).length } });
  });
  const verify = async () => {
    await button("Verify package archives").click();
    await page.getByRole("status").filter({ hasText: "4 package archives verified" }).waitFor();
  };
  await verify();
  await toggle.check();
  await verify();
  await toggle.uncheck();
  const created = page.waitForResponse((r) => r.url() === url("/admin/api/realms") && r.request().method() === "POST");
  await button("Next").click();
  const response = await created;
  assert.equal(response.status(), 201, "Real AFP must create the App");
  assert.deepEqual(checks, [changed, changed]);
  assert.deepEqual(selectionOf(response.request().postDataJSON()), changed);
  const profileResponse = await apiGet("/admin/api/realms/" + encodeURIComponent(realm) + "/profile");
  assert.equal(profileResponse.status(), 200);
  const { profile } = await profileResponse.json();
  assert(profile.signature, "Realm Profile is signed");
  assert.deepEqual(selectionOf(profile), changed);
  await button("Keep suspended for now").click();
  console.log("PASS both verification modes and Next send the same selection; signed Profile retains it");
} finally {
  await browser.close();
}
