import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

type PrimitiveTypeName = "string" | "number" | "boolean" | "unknown";

type FieldTypeSpec = `${PrimitiveTypeName}` | `${PrimitiveTypeName}?`;

type ContractsEventSpec = {
  summary?: string;
  payload?: Record<string, FieldTypeSpec>;
};

type AskContractsSpecV1 = {
  name: string;
  version: number;
  hostToGuest: Record<string, ContractsEventSpec>;
  guestToHost: Record<string, ContractsEventSpec>;
};

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const askitRoot = path.resolve(__dirname, "..");

function parseFieldType(raw: unknown): { ts: PrimitiveTypeName; optional: boolean } {
  if (typeof raw !== "string") return { ts: "unknown", optional: false };
  const optional = raw.endsWith("?");
  const base = (optional ? raw.slice(0, -1) : raw) as PrimitiveTypeName | string;
  if (base === "string" || base === "number" || base === "boolean" || base === "unknown") {
    return { ts: base, optional };
  }
  return { ts: "unknown", optional };
}

function renderPayloadType(payload: unknown): string {
  if (!payload || typeof payload !== "object" || Array.isArray(payload)) return "unknown";
  const keys = Object.keys(payload).sort();
  const fields = keys.map((k) => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const { ts, optional } = parseFieldType((payload as any)[k]);
    return `${JSON.stringify(k)}${optional ? "?:" : ":"} ${ts};`;
  });
  return `{ ${fields.join(" ")} }`;
}

function renderPayloadMap(events: Record<string, ContractsEventSpec>): string {
  const names = Object.keys(events).sort();
  const lines = names.map((name) => {
    const evt = events[name];
    const payloadType = renderPayloadType(evt?.payload);
    return `  ${JSON.stringify(name)}: ${payloadType};`;
  });
  return `{\n${lines.join("\n")}\n}`;
}

function renderPayloadSchema(payload: unknown): string {
  if (!payload || typeof payload !== "object" || Array.isArray(payload)) return "{}";
  const keys = Object.keys(payload).sort();
  const fields = keys.map((k) => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const { ts, optional } = parseFieldType((payload as any)[k]);
    return `${JSON.stringify(k)}: [${JSON.stringify(ts)}, ${optional}] as const`;
  });
  return `{ ${fields.join(", ")} }`;
}

function renderPayloadSchemaMap(events: Record<string, ContractsEventSpec>): string {
  const names = Object.keys(events).sort();
  const lines = names.map((name) => {
    const evt = events[name];
    const schema = renderPayloadSchema(evt?.payload);
    return `  ${JSON.stringify(name)}: ${schema},`;
  });
  return `{\n${lines.join("\n")}\n} as const`;
}

function renderConstNames(names: string[], constName: string): string {
  const items = names.map((n) => JSON.stringify(n)).join(", ");
  return `export const ${constName} = [${items}] as const;`;
}

function renderTypeGuard(constName: string, typeName: string, fnName: string): string {
  return `export function ${fnName}(name: string): name is ${typeName} {\n  return ((${constName} as readonly string[]).includes(name));\n}`;
}

async function main(): Promise<void> {
  const defaultSpecPath = path.resolve(askitRoot, "specs", "contracts", "ask.contracts.v1.json");
  const defaultOutPath = path.resolve(askitRoot, "src", "contracts", "generated.ts");

  const specPath = process.argv[2] ? path.resolve(process.argv[2]) : defaultSpecPath;
  const outPath = process.argv[3] ? path.resolve(process.argv[3]) : defaultOutPath;

  const raw = await readFile(specPath, "utf8");
  const spec = JSON.parse(raw) as AskContractsSpecV1;

  const hostToGuestNames = Object.keys(spec.hostToGuest ?? {}).sort();
  const guestToHostNames = Object.keys(spec.guestToHost ?? {}).sort();

  const content = `/**
 * 由脚本自动生成，请勿手改。
 *
 * 来源：${path.relative(askitRoot, specPath)}
 * 生成：bun run ${path.relative(askitRoot, path.resolve(__dirname, "generate-contracts.ts"))}
 */

export const ASK_CONTRACT_NAME = ${JSON.stringify(spec.name)} as const;
export const ASK_CONTRACT_VERSION = ${JSON.stringify(spec.version)} as const;

export type AskContractName = typeof ASK_CONTRACT_NAME;
export type AskContractVersion = typeof ASK_CONTRACT_VERSION;

export type HostToGuestEventPayloads = ${renderPayloadMap(spec.hostToGuest ?? {})};
export type HostToGuestEventName = keyof HostToGuestEventPayloads;
${renderConstNames(hostToGuestNames, "HOST_TO_GUEST_EVENT_NAMES")}
${renderTypeGuard("HOST_TO_GUEST_EVENT_NAMES", "HostToGuestEventName", "isHostToGuestEventName")}

export type GuestToHostEventPayloads = ${renderPayloadMap(spec.guestToHost ?? {})};
export type GuestToHostEventName = keyof GuestToHostEventPayloads;
${renderConstNames(guestToHostNames, "GUEST_TO_HOST_EVENT_NAMES")}
${renderTypeGuard("GUEST_TO_HOST_EVENT_NAMES", "GuestToHostEventName", "isGuestToHostEventName")}

export type HostToGuestEvent<E extends HostToGuestEventName = HostToGuestEventName> = {
  name: E;
  payload: HostToGuestEventPayloads[E];
};

export type GuestToHostEvent<E extends GuestToHostEventName = GuestToHostEventName> = {
  name: E;
  payload: GuestToHostEventPayloads[E];
};

type PrimitiveTypeName = ${JSON.stringify(["string", "number", "boolean", "unknown"]).replace(/"/g, "'")}[number];
type FieldSchema = readonly [type: PrimitiveTypeName, optional: boolean];

function isPlainObject(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function validatePayloadAgainstSchema(payload: unknown, schema: Record<string, FieldSchema>): boolean {
  if (!isPlainObject(payload)) return false;
  const obj = payload as Record<string, unknown>;
  for (const key of Object.keys(schema)) {
    const field = schema[key];
    if (!field) continue;
    const [type, optional] = field;

    if (!(key in obj)) {
      if (!optional) return false;
      continue;
    }

    const value = obj[key];
    if (value === undefined) {
      if (!optional) return false;
      continue;
    }

    if (type === 'unknown') continue;
    if (typeof value !== type) return false;
  }
  return true;
}

export const HOST_TO_GUEST_PAYLOAD_SCHEMA = ${renderPayloadSchemaMap(spec.hostToGuest ?? {})};
export function validateHostToGuestPayload<E extends HostToGuestEventName>(
  name: E,
  payload: unknown
): payload is HostToGuestEventPayloads[E] {
  const schema = (HOST_TO_GUEST_PAYLOAD_SCHEMA as Record<string, Record<string, FieldSchema>>)[name];
  if (!schema) return false;
  return validatePayloadAgainstSchema(payload, schema);
}

export const GUEST_TO_HOST_PAYLOAD_SCHEMA = ${renderPayloadSchemaMap(spec.guestToHost ?? {})};
export function validateGuestToHostPayload<E extends GuestToHostEventName>(
  name: E,
  payload: unknown
): payload is GuestToHostEventPayloads[E] {
  const schema = (GUEST_TO_HOST_PAYLOAD_SCHEMA as Record<string, Record<string, FieldSchema>>)[name];
  if (!schema) return false;
  return validatePayloadAgainstSchema(payload, schema);
}

`;

  await mkdir(path.dirname(outPath), { recursive: true });
  await writeFile(outPath, content, "utf8");
  // eslint-disable-next-line no-console
  console.log(`[askit] contracts generated: ${path.relative(process.cwd(), outPath)}`);
}

main().catch((err) => {
  // eslint-disable-next-line no-console
  console.error(err);
  process.exitCode = 1;
});
