// Read-only browser check against a local AFP with all seven service pages.
// Set LORAN_ADMIN_URL, LORAN_ADMIN_CONFIG, LORAN_ADMIN_PASSWORD_FILE. Optionally
// set LORAN_TEST_REALM to compare against an existing public Realm Profile.
// LORAN_EXPECT_ICE_HOST can assert that an edge publishes a DNS name.
import assert from "node:assert/strict";
import { networkInterfaces } from "node:os";
import { lookup } from "node:dns/promises";
import { readFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { parse } from "smol-toml";

const base = new URL(process.env.LORAN_ADMIN_URL);
const resolved = await lookup(base.hostname, { all: true });
const localAddresses = Object.values(networkInterfaces()).flat().map(ip => ip.address);
assert(resolved.length && resolved.every(ip => ip.address.startsWith("127.") || localAddresses.includes(ip.address)), "Use a local AFP");
const cfg = parse(await readFile(process.env.LORAN_ADMIN_CONFIG, "utf8"));
const password = (await readFile(process.env.LORAN_ADMIN_PASSWORD_FILE, "utf8")).trim();
const { chromium } = createRequire(import.meta.url)(process.env.PLAYWRIGHT_MODULE || "playwright");
const browser = await chromium.launch({ executablePath: process.env.CHROMIUM_EXECUTABLE });
const page = await browser.newPage({ ignoreHTTPSErrors: true });
page.setDefaultTimeout(10000);
const problems = [];
page.on("pageerror", e => problems.push(e.name));
page.on("request", r => { if (r.url().includes("/network/probe/")) problems.push("unexpected network probe"); });

const hp = (host, port) => (host.includes(":") && !host.startsWith("[") ? "[" + host + "]" : host) + ":" + port;
const http = cfg.bind.http;
const ice = cfg.bind.ice;
const signer = cfg.bind.signer;
const tls = Boolean(http.cert && http.key);
const origin = (tls ? "https://" : "http://") + hp(http.domain_name, http.advertised_port ?? http.port);
const ws = origin.replace(/^http/, "ws");
const iceBind = hp(ice.ip, ice.port) + " / UDP";
const httpBind = hp(http.ip, http.port) + " / TCP";
const signerPort = signer?.port ?? Math.min(http.port + 1, 65535);
const mtls = cfg.services.signer.require_mtls && cfg.services.signer.mtls_cert && cfg.services.signer.mtls_key && cfg.services.signer.mtls_ca_cert;
const signerUrl = mtls ? "https://" + hp(signer?.domain_name ?? http.domain_name, signerPort) : "http://127.0.0.1:" + signerPort;
const relayUrl = cfg.relay?.public_url || ws + "/relay/ws";
const realm = process.env.LORAN_TEST_REALM;
let profile;
const card = page.getByRole("region", { name: "Service access" });
const publication = card.getByRole("region", { name: "Published ICE endpoints" });
const iceConfigUrl = new URL("/actr/ice-config", base).href;
const iceUrls = (urls, service) => urls.filter(url =>
  url.startsWith(service + ":") || url.startsWith(service + "s:"),
);

async function field(label, expected, root = card) {
  const term = root.getByText(label, {exact: true});
  assert.equal(await term.locator("..").locator("dd").innerText(), String(expected), label);
}
try {
  // Capture the selected text in this isolated browser instead of changing the user's clipboard.
  await page.addInitScript(() => {
    Object.defineProperty(navigator, "clipboard", {value: undefined});
    const exec = document.execCommand.bind(document);
    document.execCommand = (command, ...args) => {
      if (command !== "copy") return exec(command, ...args);
      window.copiedAccessText = document.activeElement?.value;
      return true;
    };
  });
  if (realm) {
    const [id, domain] = realm.split("@");
    const response = await page.request.get(origin + "/actr/realms/" + encodeURIComponent(domain) + "/" + encodeURIComponent(id));
    assert.equal(response.status(), 200, "Public Realm Profile");
    profile = await response.json();
  }
  await page.goto(new URL("/admin/login", base).href);
  await page.getByLabel("Password", {exact:true}).fill(password);
  await page.locator('button[type="submit"]').click();
  await page.getByRole("link", {name:"STUN", exact:true}).waitFor();

  const publishedResponse = await page.request.get(iceConfigUrl);
  assert.equal(publishedResponse.status(), 200, "Public ICE configuration");
  const publishedUrls = (await publishedResponse.json()).ice_servers.flatMap(entry => entry.urls);
  for (const [name, url, bind] of [
    ["STUN", undefined, iceBind],
    ["TURN", undefined, iceBind],
    ["WSS Relay", relayUrl, httpBind],
    ["Signaling", ws + "/signaling/ws", httpBind],
    ["Publisher Registry", origin + "/admin/api/publishers/admin/list", httpBind],
    ["Issuer", origin + "/issuer", httpBind],
    ["Signer", signerUrl, hp(signer?.ip ?? "127.0.0.1", signerPort) + " / TCP"],
  ]) {
    await page.getByRole("link", {name, exact:true}).click();
    const isIce = ["STUN","TURN"].includes(name);
    const currentUrls = isIce ? iceUrls(publishedUrls, name.toLowerCase()) : null;
    if (currentUrls) {
      for (const published of new Set(currentUrls)) {
        const address = publication.getByText(published, { exact: true }).first();
        await address.waitFor();
        const endpoint = address.locator("../..");
        const parsed = new URL(published.replace(":", "://"));
        const transport = parsed.searchParams.get("transport");
        const protocol = parsed.protocol.slice(0, -1).toUpperCase();
        await field("Host", parsed.hostname, endpoint);
        await field("Port", parsed.port || (protocol.endsWith("S") ? "5349" : "3478"), endpoint);
        await field("Protocol", protocol + (transport ? " / " + transport.toUpperCase() : ""), endpoint);
        if (process.env.LORAN_EXPECT_ICE_HOST) {
          assert.equal(parsed.hostname, process.env.LORAN_EXPECT_ICE_HOST);
          assert.notEqual(parsed.hostname, ice.ip);
        }
        await endpoint.getByRole("button", { name: "Copy " + published, exact: true }).click();
        assert.equal(await page.evaluate(() => window.copiedAccessText), published);
      }
      if (!currentUrls.length) await publication.getByText("No " + name + " endpoint is published.").waitFor();
      assert.deepEqual(await publication.locator("code.select-all").allTextContents(), currentUrls);
      const listenerSummary = card.getByText("Listener details", { exact: true });
      const listener = listenerSummary.locator("..");
      assert.equal(await listener.getAttribute("open"), null);
      assert.equal(await listener.getByText(bind, { exact: true }).isVisible(), false);
      await listenerSummary.click();
      await field("Local listener", bind, listener);
      await listenerSummary.click();
    } else {
      await card.getByText(url, {exact:true}).first().waitFor();
      const parsed = new URL(url);
      await field("Host", parsed.hostname);
      await field("Port", parsed.port || (["https:", "wss:"].includes(parsed.protocol) ? "443" : "80"));
      await field("Local listener", bind);
      if (name !== "Signer") await field("Path", parsed.pathname);
      if (name === "Signer") await field("gRPC method path", "/signer.v1.Signer/{method}");
      const protocol = name === "Signer" ? (mtls ? "gRPC / HTTP/2 with mTLS" : "gRPC / cleartext HTTP/2") : parsed.protocol.slice(0,-1).toUpperCase();
      await field("Protocol", protocol + " / TCP");
      await card.getByRole("button", {name:/^Copy /}).first().click();
      assert.equal(await page.evaluate(() => window.copiedAccessText), url, "Copied " + name);
    }
    if (name === "TURN") {
      await field("UDP relay port range", cfg.turn?.relay_port_range ?? "49152-65535");
      await field("TURN authentication realm", (cfg.turn?.realm ?? "loran.local") + " (TURN authentication domain)");
    }
    if (name === "Issuer") {
      for (const route of ["membership","register","renew","reissue","relay-identity"])
        assert(await card.getByText(url + "/" + route, {exact:true}).isVisible());
    }
    if (name === "Publisher Registry") {
      const parsed = new URL(url);
      assert(await card.getByText(new URL(origin).origin + "/registry/publishers/{name}/verifying-key", {exact:true}).isVisible());
      assert(await card.getByText(new URL(parsed.pathname, base).href, {exact:true}).first().isVisible());
    }
    if (profile && (isIce || name === "Signaling" || name === "Issuer")) {
      const summary = card.getByText("Published address by App", {exact:true});
      const appAddresses = summary.locator("..");
      await summary.click();
      await appAddresses.getByLabel("App", {exact:true}).selectOption(realm);
      await appAddresses.getByText("Realm Profile · revision " + profile.sequence, {exact:true}).waitFor();
      const urls = name === "Issuer" ? [profile.issuer.url] : name === "Signaling" ? profile.provider.signaling.map(s => s.url) :
        iceUrls(profile.provider.ice.flatMap(s => s.urls), name.toLowerCase());
      assert.deepEqual(await appAddresses.locator("code").allTextContents(), urls);
      if (currentUrls) {
        const differs = urls.some(value => !currentUrls.includes(value)) || currentUrls.some(value => !urls.includes(value));
        if (process.env.LORAN_EXPECT_ICE_HOST) assert(differs, "Fixture Profile should retain the old ICE address");
        assert.equal(await appAddresses.getByText("These App Profile addresses differ from the current AFP publication.").isVisible(), differs);
        const firstCopy = appAddresses.getByRole("button", { name: "Copy published endpoint", exact: true }).first();
        await firstCopy.click();
        assert.equal(await page.evaluate(() => window.copiedAccessText), urls[0]);
      }
    }
    console.log("PASS", name, currentUrls?.join(", ") ?? url, "|", bind);
  }
  for (const [status, expected] of [
    [200, "No STUN endpoint is published."],
    [404, "No STUN endpoint is published."],
    [503, "Published ICE endpoints are temporarily unavailable."],
  ]) {
    const respond = route => route.fulfill({ status, json: status === 200 ? { version: 1, ice_servers: [] } : {} });
    await page.route(iceConfigUrl, respond);
    try {
      await page.getByRole("link", { name: "STUN", exact: true }).click();
      await page.reload();
      await publication.getByText(expected, { exact: true }).waitFor();
      assert.equal(await publication.getByRole("button", { name: /^Copy / }).count(), 0);
      console.log("PASS ICE", status, expected);
    } finally {
      await page.unroute(iceConfigUrl, respond);
    }
  }
  assert.deepEqual(problems, []);
} finally {
  await browser.close();
}
