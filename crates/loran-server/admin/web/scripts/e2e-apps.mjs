// Run against a disposable local Loran with Admin, Issuer and Signer enabled.
// LORAN_ADMIN_URL, LORAN_ADMIN_PASSWORD_FILE are required. PLAYWRIGHT_MODULE
// can point to an existing Playwright installation. Only test-owned apps change.
import assert from "node:assert/strict";
import { createHash, generateKeyPairSync, randomBytes, randomUUID, sign } from "node:crypto";
import { readFile, writeFile } from "node:fs/promises";
import { createRequire } from "node:module";
import dgram from "node:dgram";

const base = new URL(process.env.LORAN_ADMIN_URL);
assert(["127.0.0.1", "localhost", "[::1]"].includes(base.hostname), "Use a disposable loopback node");
const { chromium } = createRequire(import.meta.url)(process.env.PLAYWRIGHT_MODULE || "playwright");
const browser = await chromium.launch({ headless: true, executablePath: process.env.CHROMIUM_EXECUTABLE });
const page = await browser.newPage({
  ignoreHTTPSErrors: process.env.LORAN_ADMIN_LOCAL_TLS === "1",
  viewport: { width: 1280, height: 900 },
});
page.setDefaultTimeout(10000);
const passed = [];
const pageErrors = [];
page.on("pageerror", (error) => pageErrors.push(error.name));
let bearer;
const url = (path) => new URL(path, base).href;
const endpoint = (id, action = "") => "/admin/api/realms/" + encodeURIComponent(id) + action;
const button = (name, root = page) => root.getByRole("button", { name, exact: true });
async function step(name, task) {
  await task();
  passed.push(name);
  console.log("PASS " + name);
}
async function http(path, body, admin = false, expected = 200, method = body ? "POST" : "GET") {
  const result = await fetch(url(path), {
    method,
    headers: {
      ...(body && { "content-type": "application/json" }),
      ...(admin && { authorization: "Bearer " + bearer }),
    },
    ...(body && { body: JSON.stringify(body) }),
  });
  const json = await result.json().catch(() => ({}));
  if (expected === "reject")
    assert(result.status >= 400 && result.status < 500, path + " returned " + result.status);
  else
    assert.equal(
      result.status,
      expected,
      path +
        " returned " +
        result.status +
        ": " +
        String(json.error || json.message || "").replace(/ri_[\w-]+/g, "[invite]"),
    );
  return json;
}
const now = () => new Date().toISOString();
const future = (seconds) => new Date(Date.now() + seconds * 1000).toISOString();
const fixture = (ref) => ({
  href: "https://packages.example.org/" + ref + ".actr",
  sha256: createHash("sha256").update(ref).digest("hex"),
  type: "publisher.example/" + ref + "@1.0.0",
});
const selection = {
  primary_package: "core",
  front_package: "ui",
  packages: {
    core: fixture("core"),
    ui: { ...fixture("ui"), dependencies: { backend: "core" } },
    extra: fixture("extra"),
  },
};
const device = () => {
  const pair = generateKeyPairSync("ed25519");
  const pub = pair.publicKey.export({ format: "der", type: "spki" }).subarray(-32);
  return {
    privateKey: pair.privateKey,
    pub: pub.toString("base64"),
    id: createHash("sha256").update(pub).digest("hex").slice(0, 16),
  };
};
let realm,
  provider,
  appName,
  ownerInvite,
  ownerId,
  viewerId,
  editorId,
  viewerInvite,
  editorInvite,
  ownerDeviceInvite;
const owner = device(),
  viewer = device(),
  editor = device(),
  secondOwner = device();
async function membership(client, op, fields = {}, expected = 200, id = realm) {
  const envelope = {
    op,
    realm: id,
    provider_id: provider,
    nonce: randomUUID(),
    issued_at: now(),
    expires_at: future(90),
    ...fields,
  };
  const canonical = Object.fromEntries(
    Object.keys(envelope)
      .sort()
      .map((k) => [k, envelope[k]]),
  );
  const signature = sign(null, Buffer.from(JSON.stringify(canonical)), client.privateKey);
  return http(
    "/issuer/membership",
    { ...envelope, signature: { algorithm: "Ed25519", key_id: client.id, value: signature.toString("base64") } },
    false,
    expected,
  );
}
function join(client, invite, expected = 200) {
  assert(invite.startsWith("ri_"), "Missing invitation");
  const content = JSON.parse(Buffer.from(invite.slice(3), "base64url").toString());
  return membership(
    client,
    "join",
    { invite: content.secret, public_key: client.pub, label: "E2E device" },
    expected,
  );
}
async function confirm(name) {
  await button(name, page.locator(".fixed").last()).click();
}
async function issue(purpose) {
  await page.getByLabel("Invite for").selectOption(purpose);
  await button("Generate invite").click();
  const code = page.getByLabel("Invitation code");
  await code.waitFor();
  const value = await code.inputValue();
  const receiptId = await page.evaluate((id) => sessionStorage.getItem("loran-admin:invite:" + id), realm);
  await button("Done").click();
  await code.waitFor({ state: "hidden" });
  await http(endpoint(realm, "/operations/" + receiptId + "/reveal"), {}, true, 409);
  return value;
}
async function reloadDetails() {
  await page.reload();
  await page.getByRole("heading", { name: "Invitations", exact: true }).waitFor();
}
async function uiAction(trigger, confirmation) {
  await trigger.click();
  const response = page.waitForResponse(
    (r) =>
      r.request().method() === "POST" &&
      (r.url().includes("member-actions") || r.url().includes("owner-invite") || r.url().includes("/retire")),
  );
  await confirm(confirmation);
  assert((await response).ok(), "UI action failed: " + confirmation);
}
async function stun(port) {
  const socket = dgram.createSocket("udp4");
  const request = Buffer.alloc(20);
  request.writeUInt16BE(1);
  request.writeUInt32BE(0x2112a442, 4);
  randomBytes(12).copy(request, 8);
  try {
    await new Promise((resolve, reject) => {
      const timeout = setTimeout(() => reject(Error("STUN timed out")), 3000);
      socket.once("error", (e) => {
        clearTimeout(timeout);
        reject(e);
      });
      socket.once("message", (packet) => {
        clearTimeout(timeout);
        try {
          assert.equal(packet.readUInt16BE(0), 0x0101);
          assert(packet.subarray(8, 20).equals(request.subarray(8, 20)));
          resolve();
        } catch (e) {
          reject(e);
        }
      });
      socket.send(request, port, "127.0.0.1");
    });
  } finally {
    socket.close();
  }
}

try {
  await step("Browser login, protected API and legacy Apps link", async () => {
    await http("/admin/api/capabilities", null, false, 401);
    await page.goto(url("/admin/realms"));
    await page
      .locator('input[type="password"]')
      .fill((await readFile(process.env.LORAN_ADMIN_PASSWORD_FILE, "utf8")).trim());
    await page.locator('button[type="submit"]').click();
    await page.waitForFunction(() => Boolean(localStorage.getItem("loran_admin_token")));
    bearer = await page.evaluate(() => localStorage.getItem("loran_admin_token"));
    await page.goto(url("/admin/realms"));
    await page.waitForURL("**/admin/apps");
    const capabilities = await http("/admin/api/capabilities", null, true);
    assert(capabilities.realm_writes_enabled);
    for (let i = 0; i < 60; i++) {
      const descriptor = await fetch(url("/.well-known/actr"));
      if (descriptor.ok && (await descriptor.json()).realm_signing_keys?.keys?.length) break;
      await new Promise((resolve) => setTimeout(resolve, 500));
    }
    provider = capabilities.provider_id;
    realm = "admin-e2e-" + randomUUID().slice(0, 8) + "@" + provider;
    appName = "Local acceptance " + realm.split("@")[0];
  });
  await step("Create suspended App with a signed three-package profile, then resume", async () => {
    await button("Create app").click();
    await page.getByPlaceholder("realm-id@example.com").fill(realm);
    await page
      .locator("form")
      .filter({ has: page.getByPlaceholder("realm-id@example.com") })
      .locator("input[type=text]")
      .nth(1)
      .fill(appName);
    await page.getByLabel("Edit complete package selection").check();
    await page.getByLabel(/Package selection JSON/).fill(JSON.stringify(selection));
    const creation = page.waitForResponse(
      (r) => r.request().method() === "POST" && new URL(r.url()).pathname === "/admin/api/realms",
    );
    await button("Next").click();
    const creationResponse = await creation;
    if (!creationResponse.ok()) {
      const body = await creationResponse.json();
      throw Error(
        "Create app: " + (body.error_message || body.error || body.message || creationResponse.status()),
      );
    }
    const first = page.locator("code").filter({ hasText: /^ri_/ }).first();
    await first.waitFor();
    ownerInvite = await first.innerText();
    const before = await http(endpoint(realm), null, true);
    assert.equal(before.realm.enabled, false);
    await page.getByRole("button", { name: /saved the invite, resume app/ }).click();
    await button(appName).click();
    await page.getByRole("heading", { name: appName, exact: true }).waitFor();
    assert.equal(decodeURIComponent(new URL(page.url()).pathname), "/admin/apps/" + realm);
    const { profile } = await http(endpoint(realm, "/profile"), null, true);
    assert.equal(profile.realm.name, realm);
    assert.equal(profile.primary_package, "core");
    assert.equal(profile.front_package, "ui");
    assert.deepEqual(Object.keys(profile.packages).sort(), ["core", "extra", "ui"]);
    assert(profile.signature);
  });
  await step("Owner redeems first invite and explicitly authorizes the Admin device", async () => {
    await http(endpoint(realm, "/management-client"), { invite: ownerInvite }, true, "reject");
    ownerId = (await join(owner, ownerInvite)).member_id;
    const wrong = await membership(owner, "create_invite", { role: "editor", invite_expires_at: future(3600) });
    await http(endpoint(realm, "/management-client"), { invite: wrong.invite }, true, "reject");
    await membership(owner, "revoke_invite", { invite_id: wrong.invite_id });
    const grant = await membership(owner, "create_invite", {
      member_id: ownerId,
      invite_expires_at: future(3600),
    });
    await page.getByLabel("Owner device invite").fill(grant.invite);
    const attachment = page.waitForResponse(
      (r) => r.request().method() === "POST" && r.url().endsWith("/management-client"),
    );
    await button("Authorize Admin").click();
    const granted = await attachment;
    if (!granted.ok()) throw Error("Authorize Admin: " + (await granted.json()).error);
    await page.getByText("Authorized by the current Owner", { exact: true }).waitFor();
    const current = await http(endpoint(realm, "/management-client"), null, true);
    assert(current.connected && current.key_id !== owner.id);
  });
  await step("Owner-signed rename and Front upgrade keep the same App ID and complete selection", async () => {
    const current = (await http(endpoint(realm, "/profile"), null, true)).profile;
    await page.getByText("Edit app profile", { exact: true }).click();
    const nextName = appName + " updated";
    await page.getByLabel("App display name").fill(nextName);
    const updated = structuredClone(selection);
    updated.packages.ui.type = "publisher.example/ui@1.1.0";
    updated.packages.ui.href = "https://packages.example.org/ui-1.1.0.actr";
    updated.packages.ui.sha256 = "9".repeat(64);
    await page.getByLabel(/Complete package selection/).fill(JSON.stringify(updated));
    await button("Save signed profile").click();
    await page.getByRole("heading", { name: nextName, exact: true }).waitFor();
    const profile = (await http(endpoint(realm, "/profile"), null, true)).profile;
    assert.equal(profile.realm.name, realm);
    assert(profile.sequence > current.sequence);
    assert.deepEqual(profile.packages, updated.packages);
  });
  await step("Suspension blocks redemption and a resumed app accepts the same invitation", async () => {
    const open = await issue("viewer"),
      appClient = device();
    await button("Suspend app").click();
    await button("Resume app").waitFor();
    await join(appClient, open, "reject");
    await button("Resume app").click();
    await button("Suspend app").waitFor();
    const joined = await join(appClient, open);
    assert.equal(joined.role, "viewer");
    await membership(owner, "remove_member", { member_id: joined.member_id });
  });
  await step("A second app with the same packages has independent memberships and invitations", async () => {
    const other = realm.replace("admin-e2e-", "isolated-");
    const created = await http("/admin/api/realms", { realm: other, enabled: true, ...selection }, true, 201);
    assert.equal(created.realm.realm, other);
    await http(endpoint(realm, "/management-client"), { invite: created.owner_invite }, true, "reject");
    const status = await http(endpoint(other, "/management-client"), null, true);
    assert.equal(status.connected, false);
    await membership(owner, "list_members", {}, "reject", other);
    await http(endpoint(other, "/retire"), {}, true);
  });
  await step("Viewer, Editor and additional Owner device redeem three distinct invitations", async () => {
    viewerInvite = await issue("viewer");
    const v = await join(viewer, viewerInvite);
    viewerId = v.member_id;
    assert.equal(v.role, "viewer");
    assert.equal((await join(viewer, viewerInvite)).member_id, viewerId);
    await join(device(), viewerInvite, "reject");
    editorInvite = await issue("editor");
    const e = await join(editor, editorInvite);
    editorId = e.member_id;
    assert.equal(e.role, "editor");
    ownerDeviceInvite = await issue("owner_device");
    const o = await join(secondOwner, ownerDeviceInvite);
    assert.equal(o.member_id, ownerId);
    assert.equal(o.role, "owner");
    await membership(owner, "list_members");
    await membership(secondOwner, "list_members");
    await membership(viewer, "create_invite", { role: "editor", invite_expires_at: future(3600) }, "reject");
  });
  await step("Retry preserves an invite, conflicting retries fail, revocation prevents redemption", async () => {
    const id = randomUUID(),
      action = { request_id: id, action: "invite_member", role: "viewer", days: 1 };
    const first = await http(endpoint(realm, "/member-actions"), action, true);
    const retry = await http(endpoint(realm, "/member-actions"), action, true);
    assert(first.invite === retry.invite, "A retry minted another invitation");
    await http(endpoint(realm, "/member-actions"), { ...action, role: "editor" }, true, 409);
    await reloadDetails();
    const row = page.locator("li").filter({ hasText: first.invite_id });
    await uiAction(button("Revoke invitation", row), "Revoke invitation");
    await join(device(), first.invite, "reject");
    await http(endpoint(realm, "/operations/" + id + "/reveal"), {}, true, 409);
  });
  await step("Governance changes roles, revokes a device and removes a member", async () => {
    await reloadDetails();
    const member = page.getByText(viewerId, { exact: true }).locator("../..");
    await uiAction(button("Make Editor", member), "Change role");
    await member.getByRole("button", { name: "Make Viewer" }).waitFor();
    const clientRow = page.getByText(secondOwner.id, { exact: true }).locator("../..");
    await uiAction(button("Revoke device", clientRow), "Revoke device");
    await membership(secondOwner, "list_members", {}, "reject");
    await uiAction(button("Remove member", member), "Remove member");
    await membership(viewer, "list_members", {}, "reject");
  });
  await step(
    "Ownership transfer invalidates Admin privileges; recovery replaces Owner devices upon redemption",
    async () => {
      await reloadDetails();
      const member = page.getByText(editorId, { exact: true }).locator("../..");
      await uiAction(button("Transfer ownership", member), "Transfer ownership");
      await page.getByLabel("Owner device invite").waitFor();
      await http(
        endpoint(realm, "/member-actions"),
        { request_id: randomUUID(), action: "invite_member", role: "viewer", days: 1 },
        true,
        409,
      );
      await uiAction(button("Recover Owner"), "Recover Owner");
      const recovery = await page.locator(".fixed code").innerText();
      await membership(editor, "list_members");
      assert.equal((await join(device(), recovery)).role, "owner");
      await membership(editor, "list_members", {}, "reject");
      await button("Close", page.locator(".fixed")).click();
    },
  );
  await step("Retire app permanently and read back its profile", async () => {
    await uiAction(button("Retire app"), "Retire app");
    const current = await http(endpoint(realm), null, true);
    assert.equal(current.realm.status, "retired");
    assert.equal((await http(endpoint(realm, "/profile"), null, true)).profile.realm.status, "retired");
  });
  await step("Dashboard, architecture, WSS Relay, actual network listeners and UDP STUN", async () => {
    await page.goto(url("/admin"));
    await page.getByRole("link", { name: "WSS Relay service details", exact: true }).click();
    await page.getByRole("heading", { name: "WSS Relay", exact: true }).waitFor();
    const relay = await http("/admin/api/services/relay", null, true);
    assert(relay.enabled && relay.status.is_healthy);
    await page.getByText(relay.config.public_url, { exact: true }).waitFor();
    await page.getByRole("link", { name: "Architecture", exact: true }).click();
    await page.locator('a[href="/admin/services/relay"]').last().waitFor();
    await page.getByRole("link", { name: "Network", exact: true }).click();
    await button("Run checks").click();
    const { services } = await http("/admin/api/node/services", null, true);
    const signer = services.find((s) => s.type === 5);
    assert.equal(signer.port, Number(new URL(signer.url).port));
    const probe = await http("/admin/api/network/probe/" + signer.port + "?service_type=5", null, true);
    assert(probe.reachable);
    assert.equal(probe.target, new URL(signer.url).host);
    const ice = services.find((s) => s.type === 1);
    await stun(ice.port);
    assert.deepEqual(pageErrors, []);
  });
  if (process.env.LORAN_ADMIN_PACKAGE_SELECTION_FILE) {
    await step("Verify real signed Service and Front archives; reject a mismatched digest", async () => {
      const real = JSON.parse(await readFile(process.env.LORAN_ADMIN_PACKAGE_SELECTION_FILE, "utf8"));
      await page.goto(url("/admin/apps"));
      await button("Create app").click();
      await page.getByLabel("Edit complete package selection").check();
      await page.getByLabel(/Package selection JSON/).fill(JSON.stringify(real));
      await button("Verify package archives").click();
      await page
        .getByRole("status")
        .filter({ hasText: /package archives verified/ })
        .waitFor({ timeout: 95000 });
      real.packages[real.front_package].sha256 = "0".repeat(64);
      await http("/admin/api/apps/check-selection", real, true, 422);
    });
  }
  await step("Metrics distinguish empty samples and read failures in the browser", async () => {
    let unavailable = false;
    await page.route("**/admin/api/metrics/timeseries?**", (route) =>
      route.fulfill({
        status: unavailable ? 503 : 200,
        contentType: "application/json",
        body: JSON.stringify(unavailable ? { error: "unavailable" } : { samples: [] }),
      }),
    );
    await page.goto(url("/admin/services/relay"));
    await page.getByText("No samples in this period.", { exact: true }).waitFor();
    unavailable = true;
    await page.reload();
    await page
      .getByText(/metrics.*unavailable|unable to load metrics/i)
      .first()
      .waitFor();
    assert.deepEqual(pageErrors, []);
  });
} finally {
  await browser.close();
  if (process.env.LORAN_ADMIN_E2E_REPORT) {
    await writeFile(
      process.env.LORAN_ADMIN_E2E_REPORT,
      JSON.stringify({ app: realm, passed, finished_at: now() }, null, 2) + "\n",
    );
  }
}
