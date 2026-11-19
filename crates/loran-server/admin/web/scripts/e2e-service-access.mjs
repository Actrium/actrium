// Read-only browser check against a local AFP with all seven service pages.
// Set LORAN_ADMIN_URL, LORAN_ADMIN_CONFIG, LORAN_ADMIN_PASSWORD_FILE. Optionally
// set LORAN_TEST_REALM to compare against an existing public Realm Profile.
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
const iceAddress = hp(ice.advertised_ip, ice.advertised_port ?? 3478);
const iceBind = hp(ice.ip, ice.port) + " / UDP";
const httpBind = hp(http.ip, http.port) + " / TCP";
const signerPort = signer?.port ?? Math.min(http.port + 1, 65535);
const mtls = cfg.services.signer.require_mtls && cfg.services.signer.mtls_cert && cfg.services.signer.mtls_key && cfg.services.signer.mtls_ca_cert;
const signerUrl = mtls ? "https://" + hp(signer?.domain_name ?? http.domain_name, signerPort) : "http://127.0.0.1:" + signerPort;
const relayUrl = cfg.relay?.public_url || ws + "/relay/ws";
const realm = process.env.LORAN_TEST_REALM;
let profile;
const card = page.getByRole("region", { name: "Service access" });

async function field(label, expected) {
  const term = card.getByText(label, {exact: true});
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

  for (const [name, url, bind] of [
    ["STUN", "stun:" + iceAddress, iceBind],
    ["TURN", "turn:" + iceAddress + "?transport=udp", iceBind],
    ["WSS Relay", relayUrl, httpBind],
    ["Signaling", ws + "/signaling/ws", httpBind],
    ["Publisher Registry", origin + "/admin/api/publishers/admin/list", httpBind],
    ["Issuer", origin + "/issuer", httpBind],
    ["Signer", signerUrl, hp(signer?.ip ?? "127.0.0.1", signerPort) + " / TCP"],
  ]) {
    await page.getByRole("link", {name, exact:true}).click();
    await card.getByText(url, {exact:true}).first().waitFor();
    const isIce = ["STUN","TURN"].includes(name);
    const parsed = new URL(isIce ? url.replace(":", "://") : url);
    await field("Host", parsed.hostname);
    await field("Port", parsed.port || (["https:", "wss:"].includes(parsed.protocol) ? "443" : "80"));
    await field("Local listener", bind);
    if (!isIce && name !== "Signer") await field("Path", parsed.pathname);
    if (name === "Signer") await field("gRPC method path", "/signer.v1.Signer/{method}");
    const protocol = name === "Signer" ? (mtls ? "gRPC / HTTP/2 with mTLS" : "gRPC / cleartext HTTP/2") : parsed.protocol.slice(0,-1).toUpperCase();
    await field("Protocol", protocol + (isIce ? " / UDP" : " / TCP"));
    await card.getByRole("button", {name:/^Copy /}).first().click();
    assert.equal(await page.evaluate(() => window.copiedAccessText), url, "Copied " + name);
    if (name === "TURN") {
      await field("UDP relay port range", cfg.turn?.relay_port_range ?? "49152-65535");
      await field("TURN authentication realm", (cfg.turn?.realm ?? "loran.local") + " (TURN authentication domain)");
    }
    if (name === "Issuer") {
      for (const route of ["membership","register","renew","reissue","relay-identity"])
        assert(await card.getByText(url + "/" + route, {exact:true}).isVisible());
    }
    if (name === "Publisher Registry") {
      assert(await card.getByText(new URL(origin).origin + "/registry/publishers/{name}/verifying-key", {exact:true}).isVisible());
      assert(await card.getByText(new URL(parsed.pathname, base).href, {exact:true}).first().isVisible());
    }
    if (profile && (isIce || name === "Signaling" || name === "Issuer")) {
      await card.getByText("Published address by App", {exact:true}).click();
      await card.getByLabel("App", {exact:true}).selectOption(realm);
      await card.getByText("Realm Profile · revision " + profile.sequence, {exact:true}).waitFor();
      const urls = name === "Issuer" ? [profile.issuer.url] : name === "Signaling" ? profile.provider.signaling.map(s => s.url) :
        profile.provider.ice.flatMap(s => s.urls).filter(s => s.startsWith(name.toLowerCase() + ":"));
      assert.deepEqual(await card.locator("details code").allTextContents(), urls);
    }
    console.log("PASS", name, url, "|", bind);
  }
  assert.deepEqual(problems, []);
} finally {
  await browser.close();
}
