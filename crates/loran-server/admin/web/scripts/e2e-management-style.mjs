// Runs against a disposable AFP and test-owned Publisher records.
import assert from "node:assert/strict";
import { randomUUID } from "node:crypto";
import { readFile } from "node:fs/promises";
import { createRequire } from "node:module";

const base = new URL(process.env.LORAN_ADMIN_URL);
assert(["127.0.0.1", "localhost", "[::1]"].includes(base.hostname));
const fixture = JSON.parse(await readFile(process.env.LORAN_STYLE_PUBLISHERS, "utf8"));
assert.match(fixture.publisher, /^review-style-[a-f0-9]+\.test$/);
assert.match(fixture.pending, /^review-style-pending-[a-f0-9]+\.test$/);
assert(Number.isSafeInteger(fixture.id));
const selection = JSON.parse(await readFile(process.env.LORAN_STYLE_SELECTION, "utf8"));
const { chromium } = createRequire(import.meta.url)(process.env.PLAYWRIGHT_MODULE || "playwright");
const browser = await chromium.launch({ executablePath: process.env.CHROMIUM_EXECUTABLE });
const page = await browser.newPage({ ignoreHTTPSErrors: process.env.LORAN_ADMIN_LOCAL_TLS === "1", viewport: { width: 1280, height: 900 } });
page.setDefaultTimeout(12000);
const url = (path) => new URL(path, base).href;
const button = (name, region = page) => region.getByRole("button", { name, exact: true });
const heading = (name) => page.getByRole("heading", { name, exact: true, level: 1 });
const table = (name) => page.getByRole("table", { name, exact: true });
const statusText = (row, name) => row.getByText(name, { exact: true });
const appList = "/admin/apps";
const registry = "/admin/publishers";
const registryApi = "/admin/api/publishers/admin/";
const keyUrl = "/registry/publishers/" + encodeURIComponent(fixture.publisher) + "/verifying-key";
const publisherRow = table("Publishers").getByRole("row").filter({ has: button(fixture.publisher) });
const publisherToggle = button(fixture.publisher, publisherRow);
const external = button("External domain · signed requests");
const keyRow = (key) => table("Key history").getByRole("row").filter({ hasText: key });
const consoleErrors = [];
const writes = [];
let nativeDialogs = 0;
let token;
page.on("pageerror", error => consoleErrors.push(error.name));
page.on("dialog", async dialog => { nativeDialogs++; await dialog.dismiss(); });
page.on("request", req => {
  if (req.url().includes(registryApi) && ["POST", "DELETE"].includes(req.method())) writes.push(req.url());
});

const pass = (message) => console.log("PASS " + message);
async function api(path, method = "GET", body, authenticated = true) {
  return page.request.fetch(url(path), {
    method,
    headers: authenticated ? { Authorization: "Bearer " + token } : {},
    ...(body === undefined ? {} : { data: body }),
  });
}
async function checkKey(status, key = "") {
  const response = await api(keyUrl + (key ? "?key_id=" + encodeURIComponent(key) : ""), "GET", undefined, false);
  if (status === "unavailable") assert(response.status() >= 400 && response.status() < 500);
  else assert.equal(response.status(), status);
}
async function style(locator, properties) {
  await locator.waitFor();
  return locator.evaluate((node, props) => {
    const computed = getComputedStyle(node);
    return Object.fromEntries(props.map(prop => [prop, computed.getPropertyValue(prop)]));
  }, properties);
}
const headingStyle = ["font-size", "font-weight", "line-height", "color"];
const thStyle = ["font-size", "font-weight", "padding-top", "padding-left", "text-transform", "color"];
const badgeStyle = ["font-size", "font-weight", "padding-left", "border-radius", "background-color", "color"];
const buttonStyle = ["font-size", "font-weight", "padding-left", "min-height", "border-radius", "background-color", "color"];
async function checkNarrowTable(name) {
  const region = page.getByRole("region", { name: name + " table", exact: true });
  const metrics = await region.evaluate(node => ({
    viewport: innerWidth,
    page: document.documentElement.scrollWidth,
    container: node.clientWidth,
    scroll: node.scrollWidth,
    x: node.getBoundingClientRect().right,
  }));
  assert(metrics.page <= metrics.viewport);
  assert(metrics.x <= metrics.viewport);
  assert(metrics.scroll > metrics.container);
  assert(await region.evaluate(node => { node.scrollLeft = node.scrollWidth; return node.scrollLeft > 0; }));
  await region.evaluate(node => { node.scrollLeft = 0; });
}
async function confirmAction(trigger, title, label, consequence) {
  const before = writes.length;
  await trigger.click();
  const dialog = page.getByRole("dialog", { name: title, exact: true });
  await dialog.waitFor();
  assert.match(await dialog.innerText(), consequence);
  const rect = await dialog.boundingBox();
  assert(rect.x >= 0 && rect.x + rect.width <= 390 && rect.y >= 0 && rect.y + rect.height <= 844);
  await button("Cancel", dialog).click();
  await dialog.waitFor({ state: "hidden" });
  assert.equal(writes.length, before, title + " must not send a request on cancel");

  const mutation = page.waitForResponse(response =>
    response.url().includes(registryApi) && ["POST", "DELETE"].includes(response.request().method()),
  );
  await trigger.click();
  await button(label, dialog).click();
  assert.equal((await mutation).status(), 200);
  await dialog.waitFor({ state: "hidden" });
  assert.equal(writes.length, before + 1, title + " must submit once");
}

try {
  await page.goto(url("/admin/login"));
  await page.getByLabel("Password", { exact: true }).fill((await readFile(process.env.LORAN_ADMIN_PASSWORD_FILE, "utf8")).trim());
  await page.locator('button[type="submit"]').click();
  await page.waitForFunction(() => Boolean(localStorage.getItem("loran_admin_token")));
  token = await page.evaluate(() => localStorage.getItem("loran_admin_token"));
  const appName = "Review style App";
  const realm = "review-style-" + randomUUID().slice(0, 8) + "@localhost";
  assert.equal((await api("/admin/api/realms", "POST", {
    realm, enabled: true, display_name: appName, ...selection,
  })).status(), 201);
  const unauthorized = await api(registryApi + fixture.id + "/suspend", "POST", undefined, false);
  assert.equal(unauthorized.status(), 401);
  await checkKey(200);
  pass("Registry mutations still require an Admin session");

  await page.goto(url(appList));
  const appRow = table("Apps").getByRole("row").filter({ has: page.getByText(realm, { exact: true }) });
  const listTitle = heading("Apps");
  const appsHeadingStyle = await style(listTitle, headingStyle);
  const appsHeaderStyle = await style(table("Apps").getByRole("columnheader").first(), thStyle);
  const appTitleX = (await listTitle.boundingBox()).x;
  const appsBadgeStyle = await style(statusText(appRow, "Active"), badgeStyle);
  const appsRowHeight = (await appRow.boundingBox()).height;
  const primary = await style(button("Create app"), buttonStyle);

  assert.equal(await external.getAttribute("aria-expanded"), "false");
  await external.press("Enter");
  assert.equal(await external.getAttribute("aria-expanded"), "true");
  const request = page.getByLabel("Signed request JSON", { exact: true });
  await request.fill(JSON.stringify({ realm: "example@style.test", signature: {} }));
  await external.press("Space");
  await request.waitFor({ state: "hidden" });
  await external.press("Enter");
  assert.match(await request.inputValue(), /example@style\.test/);
  await page.getByLabel("External App ID", { exact: true }).fill("example@style.test");
  const secondary = await style(button("Submit signed request"), buttonStyle);
  const invalidSigned = page.waitForResponse(response =>
    response.url().endsWith("/signed-management") && response.request().method() === "POST",
  );
  await button("Submit signed request").click();
  assert.equal((await invalidSigned).status(), 400);
  await page.getByRole("alert").filter({ hasText: "Invalid signed Realm request" }).waitFor();
  pass("Apps disclosure supports keyboard and retains the signed request workflow");

  await button(appName, appRow).click();
  await heading(appName).waitFor();
  const danger = await style(button("Retire app"), buttonStyle);
  await page.getByRole("link", { name: "All apps", exact: true }).click();
  await heading("Apps").waitFor();
  await page.goto(url(registry));
  assert.deepEqual(await style(heading("Publisher Registry"), headingStyle), appsHeadingStyle);
  assert.equal((await heading("Publisher Registry").boundingBox()).x, appTitleX);
  assert.deepEqual(await style(table("Publishers").getByRole("columnheader").first(), thStyle), appsHeaderStyle);
  assert.deepEqual(await style(statusText(publisherRow, "Active"), badgeStyle), appsBadgeStyle);
  assert(Math.abs((await publisherRow.boundingBox()).height - appsRowHeight) <= 1, "Table row heights differ by more than one CSS pixel");
  assert.deepEqual(await style(button("Suspend", publisherRow), buttonStyle), secondary);
  assert.deepEqual(await style(button("Delete", publisherRow), buttonStyle), danger);
  const pendingRow = table("Publishers").getByRole("row").filter({ has: button(fixture.pending) });
  await statusText(pendingRow, "Pending").waitFor();
  assert.equal(await button("Suspend", pendingRow).count(), 0);
  assert.equal(await button("Revoke key", pendingRow).count(), 0);
  pass("Desktop heading, margin, table header, row height, badges and action styles match");

  assert.equal(await publisherToggle.getAttribute("aria-expanded"), "false");
  await publisherToggle.press("Enter");
  await keyRow(fixture.retired).waitFor();
  assert.equal(await publisherToggle.getAttribute("aria-expanded"), "true");
  assert(await publisherRow.locator("xpath=following-sibling::tr[1]").getByRole("table", { name: "Key history" }).isVisible());
  await button("Close key history").click();
  await keyRow(fixture.retired).waitFor({ state: "hidden" });
  assert.equal(await publisherToggle.getAttribute("aria-expanded"), "false");
  pass("Publisher history opens under its own row and supports keyboard and close action");

  await page.setViewportSize({ width: 390, height: 844 });
  await button("Collapse sidebar menu").click();
  await page.goto(url(appList));
  await appRow.waitFor();
  await checkNarrowTable("Apps");
  await button("Create app").click();
  await page.getByRole("heading", { name: "Create app", exact: true }).waitFor();
  await button("Cancel").click();
  await external.click();
  await page.getByLabel("Signed request JSON", { exact: true }).waitFor();
  assert.equal(await request.evaluate(node => node.getBoundingClientRect().right <= innerWidth), true);
  await page.goto(url(registry));
  await publisherRow.waitFor();
  await checkNarrowTable("Publishers");
  await publisherToggle.click();
  await keyRow(fixture.retired).waitFor();
  const historyHeading = page.getByRole("heading", { name: "Key History — " + fixture.publisher, exact: true });
  const viewport = await page.getByRole("region", { name: "Publishers table", exact: true }).boundingBox();
  const history = await historyHeading.boundingBox();
  assert(history.x >= viewport.x && history.x + history.width <= viewport.x + viewport.width);
  assert(await historyHeading.evaluate(node => node.scrollWidth === node.clientWidth));
  await checkNarrowTable("Key history");
  if (process.env.LORAN_STYLE_PREVIEW) {
    await table("Publishers").locator("../..").screenshot({ path: process.env.LORAN_STYLE_PREVIEW, animations: "disabled" });
  }
  pass("Narrow views keep overflow inside the tables and retain reachable controls");

  await confirmAction(button("Suspend", publisherRow), "Suspend publisher", "Suspend", /Key lookup stops/);
  await statusText(publisherRow, "Suspended").waitFor();
  assert.equal(await button("Revoke key", publisherRow).count(), 0);
  await checkKey("unavailable");
  assert.deepEqual(await style(button("Reinstate", publisherRow), buttonStyle), primary);
  await button("Reinstate", publisherRow).click();
  await statusText(publisherRow, "Active").waitFor();
  await checkKey(200);
  pass("Suspend confirms in-app and Reinstate restores lookup and the active badge");

  await checkKey(200, fixture.retired);
  await confirmAction(button("Revoke", keyRow(fixture.retired)), "Revoke historical key", "Revoke", /signed with this key will fail verification/);
  await statusText(keyRow(fixture.retired), "Revoked").waitFor();
  await checkKey("unavailable", fixture.retired);
  await checkKey(200);
  await checkKey(200, fixture.otherRetired);
  pass("Historical key confirmation revokes only the selected historical key");

  await checkKey(200, fixture.next);
  await confirmAction(button("Revoke key", publisherRow), "Revoke current key", "Revoke key", /staged replacement key will also be revoked/);
  await statusText(keyRow(fixture.current), "Revoked").waitFor();
  await statusText(keyRow(fixture.next), "Revoked").waitFor();
  assert.equal(await button("Revoke key", publisherRow).count(), 0);
  await checkKey("unavailable", fixture.current);
  await checkKey("unavailable", fixture.next);
  pass("Current-key confirmation revokes live and staged keys and refreshes history");

  await confirmAction(button("Delete", publisherRow), "Delete publisher", "Delete", /Permanently delete/);
  await publisherRow.waitFor({ state: "hidden" });
  await checkKey(404);
  assert.equal(nativeDialogs, 0);
  assert.deepEqual(consoleErrors, []);
  pass("Delete uses the site dialog and removes the Publisher through the real API");
} finally {
  await browser.close();
}
