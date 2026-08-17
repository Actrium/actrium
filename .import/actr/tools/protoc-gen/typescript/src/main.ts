import type { DescFile, DescMessage, DescMethod } from "@bufbuild/protobuf";
import {
  createEcmaScriptPlugin,
  runNodeJs,
  type Schema,
} from "@bufbuild/protoplugin";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";

type PluginParams = {
  dispatchSpecPath?: string;
  dispatchSpecHash?: string;
};

type AnalyzedMethod = {
  method: DescMethod;
  requestCompanionName: string;
};

type TypeRef = {
  proto_type: string;
  type_name: string;
  proto_package: string;
  proto_file: string;
};

type MethodMetadata = {
  name: string;
  snake_name: string;
  route_key: string;
  input_ref: TypeRef;
  output_ref: TypeRef;
};

type DispatchRoute = {
  route_key: string;
  proto_file: string;
  proto_service: string;
  method: string;
  input_type: string;
  output_type: string;
  payload_type: string;
  dependency_alias?: string;
};

type DispatchSpec = {
  schema_version: string;
  routes: DispatchRoute[];
  spec_hash?: string;
};

type LocalServiceMetadata = {
  name: string;
  package: string;
  proto_file: string;
  handler_interface: string;
  workload_type: string;
  dispatcher_type: string;
  methods: MethodMetadata[];
};

type RemoteServiceMetadata = {
  name: string;
  package: string;
  proto_file: string;
  dependency_alias: string;
  client_type: string;
  methods: MethodMetadata[];
};

const VERSION = "0.9.0";

const plugin = createEcmaScriptPlugin<PluginParams>({
  name: "protoc-gen-actrframework-typescript",
  version: VERSION,
  parseOptions: parseOptions,
  generateTs(schema) {
    generateTypeScript(schema);
  },
});

runNodeJs(plugin);

function generateTypeScript(schema: Schema<PluginParams>): void {
  if (!schema.options.dispatchSpecPath) {
    throw new Error(
      "dispatch_spec=<path> is required; remote routing must come from DispatchSpec v1",
    );
  }
  const specContent = readFileSync(schema.options.dispatchSpecPath, "utf-8");
  const dispatchSpec = JSON.parse(specContent) as DispatchSpec;
  validateDispatchRoutes(dispatchSpec);
  if (dispatchSpec.schema_version !== "v1") {
    throw new Error(
      `Unexpected dispatch spec schema_version: ${dispatchSpec.schema_version}`,
    );
  }
  validateDispatchSpecHash(dispatchSpec, schema.options.dispatchSpecHash);

  const localServiceMetadata: LocalServiceMetadata[] = [];
  const remoteServiceMetadata: RemoteServiceMetadata[] = [];

  buildMetadataFromDispatchSpec(
    dispatchSpec,
    schema,
    localServiceMetadata,
    remoteServiceMetadata,
  );

  const generatedFiles = ["actr_dispatch.ts", "actr-gen-meta.json"];
  const remoteRouteKeys = new Set(
    dispatchSpec.routes
      .filter((route) => route.dependency_alias !== undefined)
      .map((route) => route.route_key),
  );
  for (const file of schema.files) {
    const remoteMethods = analyzeFileMethods(file).filter((entry) =>
      remoteRouteKeys.has(routeKeyForMethod(entry.method)),
    );
    if (remoteMethods.length === 0) {
      continue;
    }
    const requestCompanionNames = new Set<string>();
    for (const entry of remoteMethods) {
      if (requestCompanionNames.has(entry.requestCompanionName)) {
        throw new Error(
          `${routeKeyForMethod(entry.method)}: request companion '${entry.requestCompanionName}' is duplicated within ${file.name}. ` +
            "Each remote request type name must map to exactly one RPC in the generated file.",
        );
      }
      requestCompanionNames.add(entry.requestCompanionName);
    }
    generateClientFile(schema, file, remoteMethods);
    generatedFiles.push(`${normalizeProtoFileKey(file.name)}_client.ts`);
  }

  generateActrDispatch(schema, dispatchSpec);

  const metadataFile = schema.generateFile("actr-gen-meta.json");
  metadataFile.print(
    JSON.stringify(
      {
        plugin_version: VERSION,
        language: "typescript",
        schema_version: dispatchSpec.schema_version,
        dispatch_spec_hash:
          dispatchSpec.spec_hash ?? schema.options.dispatchSpecHash ?? "",
        generated_files: generatedFiles.sort(),
        generated_local_routes: dispatchSpec.routes
          .filter((route) => route.dependency_alias === undefined)
          .map((route) => route.route_key)
          .sort(),
        generated_remote_routes: dispatchSpec.routes
          .filter((route) => route.dependency_alias !== undefined)
          .map((route) => route.route_key)
          .sort(),
        local_services: localServiceMetadata,
        remote_services: remoteServiceMetadata,
      },
      null,
      2,
    ),
  );
}

function buildMetadataFromDispatchSpec(
  spec: DispatchSpec,
  schema: Schema<PluginParams>,
  localServiceMetadata: LocalServiceMetadata[],
  remoteServiceMetadata: RemoteServiceMetadata[],
): void {
  // Group routes by proto_service.
  const servicesByProtoService = new Map<string, DispatchRoute[]>();
  for (const route of spec.routes) {
    const routes = servicesByProtoService.get(route.proto_service) ?? [];
    routes.push(route);
    servicesByProtoService.set(route.proto_service, routes);
  }

  for (const [protoService, routes] of servicesByProtoService) {
    const aliases = new Set(routes.map((route) => route.dependency_alias));
    if (aliases.size !== 1) {
      throw new Error(
        `DispatchSpec mixes dependency aliases for service '${protoService}'`,
      );
    }
    const dependencyAlias = routes[0].dependency_alias;
    const isRemote = dependencyAlias !== undefined;
    if (isRemote && dependencyAlias.trim().length === 0) {
      throw new Error(
        `DispatchSpec remote service '${protoService}' has an empty dependency_alias`,
      );
    }
    const lastDot = protoService.lastIndexOf(".");
    const packageName = lastDot > 0 ? protoService.slice(0, lastDot) : "";
    const serviceName =
      lastDot > 0 ? protoService.slice(lastDot + 1) : protoService;

    const methodsByRoute = new Map<string, DescMethod>();
    for (const file of schema.files) {
      for (const service of file.services) {
        for (const method of service.methods) {
          methodsByRoute.set(routeKeyForMethod(method), method);
        }
      }
    }
    const methods = routes.map((route) => {
      const method = methodsByRoute.get(route.route_key);
      if (!method) {
        throw new Error(
          `DispatchSpec route '${route.route_key}' is not present in the protoc request`,
        );
      }
      return buildMethodMetadata(method);
    });

    if (isRemote) {
      remoteServiceMetadata.push({
        name: serviceName,
        package: packageName,
        proto_file: routes[0].proto_file,
        dependency_alias: dependencyAlias!,
        client_type: `${serviceName}Client`,
        methods,
      });
    } else {
      localServiceMetadata.push({
        name: serviceName,
        package: packageName,
        proto_file: routes[0].proto_file,
        handler_interface: `${serviceName}Handler`,
        workload_type: `${serviceName}Workload`,
        dispatcher_type: `${serviceName}Dispatcher`,
        methods,
      });
    }
  }
}

function generateActrDispatch(
  schema: Schema<PluginParams>,
  spec: DispatchSpec,
): void {
  const dispatchFile = schema.generateFile("actr_dispatch.ts");

  const localRoutes = spec.routes.filter((r) => !r.dependency_alias);
  const remoteRoutes = spec.routes.filter((r) => r.dependency_alias);
  const methodsByRoute = new Map<string, DescMethod>();
  for (const file of schema.files) {
    for (const service of file.services) {
      for (const method of service.methods) {
        methodsByRoute.set(routeKeyForMethod(method), method);
      }
    }
  }
  const methodForRoute = (route: DispatchRoute): DescMethod => {
    const method = methodsByRoute.get(route.route_key);
    if (!method) {
      throw new Error(
        `DispatchSpec route '${route.route_key}' is not present in the protoc request`,
      );
    }
    return method;
  };
  const fromBinary = dispatchFile.import("fromBinary", "@bufbuild/protobuf");
  const toBinary = dispatchFile.import("toBinary", "@bufbuild/protobuf");
  const actrContext = dispatchFile.import(
    "ActrContext",
    "@actrium/actr-workload",
    true,
  );
  const payloadType =
    remoteRoutes.length > 0
      ? dispatchFile.import("PayloadType", "@actrium/actr-workload")
      : undefined;

  dispatchFile.print("// Auto-generated unified dispatcher (DispatchSpec v1)");
  dispatchFile.print("// DO NOT EDIT this file manually");
  dispatchFile.print("");

  // Handler interface. It remains a marker for remote-only specs so every
  // generated dispatcher has the same five-argument signature.
  dispatchFile.print("export interface ActrDispatchHandler {");
  for (const route of localRoutes) {
    const methodName = dispatchHandlerMethodName(route);
    const method = methodForRoute(route);
    const inputType = dispatchFile.importShape(method.input);
    const outputType = dispatchFile.importShape(method.output);
    dispatchFile.print(
      "  ",
      methodName,
      "(req: ",
      inputType,
      ", ctx: ",
      actrContext,
      "): ",
      outputType,
      " | Promise<",
      outputType,
      ">;",
    );
  }
  dispatchFile.print("}");
  dispatchFile.print("");

  // actrDispatch function.
  const hasLocal = localRoutes.length > 0;
  dispatchFile.print("export async function actrDispatch(");
  dispatchFile.print("  ctx: ", actrContext, ",");
  dispatchFile.print("  handler: ActrDispatchHandler,");
  dispatchFile.print("  routeKey: string,");
  dispatchFile.print("  payload: Uint8Array,");
  dispatchFile.print("  timeoutMs: number,");
  dispatchFile.print("): Promise<Uint8Array> {");
  if (!hasLocal) {
    dispatchFile.print("  void handler;");
  }
  if (remoteRoutes.length === 0) {
    dispatchFile.print("  void timeoutMs;");
  }

  if (spec.routes.length === 0) {
    dispatchFile.print("  throw new Error(`UnknownRoute: ${routeKey}`);");
    dispatchFile.print("}");
    return;
  }

  dispatchFile.print("  switch (routeKey) {");
  for (const route of localRoutes) {
    const methodName = dispatchHandlerMethodName(route);
    const method = methodForRoute(route);
    const inputSchema = dispatchFile.importSchema(method.input);
    const outputSchema = dispatchFile.importSchema(method.output);
    dispatchFile.print("    case ", dispatchFile.string(route.route_key), ": {");
    dispatchFile.print(
      "      const req = ",
      fromBinary,
      "(",
      inputSchema,
      ", payload);",
    );
    dispatchFile.print(
      `      const resp = await handler.${methodName}(req, ctx);`,
    );
    dispatchFile.print(
      "      return ",
      toBinary,
      "(",
      outputSchema,
      ", resp);",
    );
    dispatchFile.print("    }");
  }

  if (remoteRoutes.length > 0) {
    for (const route of remoteRoutes) {
      const payloadTypeVariant = payloadTypeVariantFor(route.payload_type);
      dispatchFile.print("    case ", dispatchFile.string(route.route_key), ": {");
      dispatchFile.print(
        "      const actrType = await ctx.resolveDependency(",
        dispatchFile.string(route.dependency_alias ?? ""),
        ");",
      );
      dispatchFile.print("      const target = await ctx.discover(actrType);");
      dispatchFile.print(
        "      return ctx.callRaw(target, routeKey, payload, timeoutMs, ",
        payloadType!,
        ".",
        payloadTypeVariant,
        ");",
      );
      dispatchFile.print("    }");
    }
    dispatchFile.print("    default:");
    dispatchFile.print("      throw new Error(`UnknownRoute: ${routeKey}`);");
  } else {
    dispatchFile.print("    default:");
    dispatchFile.print("      throw new Error(`UnknownRoute: ${routeKey}`);");
  }
  dispatchFile.print("  }");
  dispatchFile.print("}");
}

function payloadTypeVariantFor(value: string): string {
  switch (value.toLowerCase()) {
    case "rpcreliable":
      return "RpcReliable";
    case "rpcsignal":
      return "RpcSignal";
    case "streamreliable":
      return "StreamReliable";
    case "streamlatencyfirst":
      return "StreamLatencyFirst";
    case "mediartp":
      return "MediaRtp";
    default:
      throw new Error(`Unsupported payload_type '${value}' in DispatchSpec route`);
  }
}

function generateClientFile(
  schema: Schema<PluginParams>,
  file: DescFile,
  analyzedMethods: AnalyzedMethod[],
): void {
  const generated = schema.generateFile(
    `${normalizeProtoFileKey(file.name)}_client.ts`,
  );
  generated.preamble(file);
  const bufferSymbol = generated.import("Buffer", "node:buffer");

  const toBinary = generated.import("toBinary", "@bufbuild/protobuf");
  const fromBinary = generated.import("fromBinary", "@bufbuild/protobuf");
  const create = generated.import("create", "@bufbuild/protobuf");
  const messageInitShape = generated.import(
    "MessageInitShape",
    "@bufbuild/protobuf",
    true,
  );

  for (const entry of analyzedMethods) {
    const requestType = generated.importShape(entry.method.input);
    const requestSchema = generated.importSchema(entry.method.input);
    const responseType = generated.importShape(entry.method.output);
    const responseSchema = generated.importSchema(entry.method.output);

    generated.print(
      generated.export("const", entry.requestCompanionName),
      " = {",
    );
    generated.print(
      "  routeKey: ",
      generated.string(routeKeyForMethod(entry.method)),
      ",",
    );
    generated.print(
      "  encode(message: ",
      messageInitShape,
      "<typeof ",
      requestSchema,
      ">): ",
      bufferSymbol,
      " {",
    );
    generated.print(
      "    return ",
      bufferSymbol,
      ".from(",
      toBinary,
      "(",
      requestSchema,
      ", ",
      create,
      "(",
      requestSchema,
      ", message)));",
    );
    generated.print("  },");
    generated.print("  decode(bytes: Uint8Array): ", requestType, " {");
    generated.print("    return ", fromBinary, "(", requestSchema, ", bytes);");
    generated.print("  },");
    generated.print("  response: {");
    generated.print(
      "    encode(message: ",
      messageInitShape,
      "<typeof ",
      responseSchema,
      ">): ",
      bufferSymbol,
      " {",
    );
    generated.print(
      "      return ",
      bufferSymbol,
      ".from(",
      toBinary,
      "(",
      responseSchema,
      ", ",
      create,
      "(",
      responseSchema,
      ", message)));",
    );
    generated.print("    },");
    generated.print("    decode(bytes: Uint8Array): ", responseType, " {");
    generated.print(
      "      return ",
      fromBinary,
      "(",
      responseSchema,
      ", bytes);",
    );
    generated.print("    },");
    generated.print("  },");
    generated.print("} as const;");
    generated.print("");
  }
}

function validateDispatchSpecHash(
  spec: DispatchSpec,
  optionHash: string | undefined,
): void {
  const expectedHash = optionHash ?? spec.spec_hash;
  if (!expectedHash) {
    throw new Error("DispatchSpec is missing spec_hash");
  }
  if (optionHash && spec.spec_hash && optionHash !== spec.spec_hash) {
    throw new Error(
      `DispatchSpec hash mismatch: option ${optionHash} does not match file ${spec.spec_hash}`,
    );
  }

  const withoutHash = { ...spec } as Record<string, unknown>;
  delete withoutHash.spec_hash;
  const actualHash = createHash("sha256")
    .update(JSON.stringify(canonicalizeJson(withoutHash)))
    .digest("hex");
  if (actualHash !== expectedHash) {
    throw new Error(
      `DispatchSpec hash mismatch: expected ${expectedHash}, got ${actualHash}`,
    );
  }
}

function validateDispatchRoutes(spec: DispatchSpec): void {
  if (typeof spec !== "object" || spec === null || Array.isArray(spec)) {
    throw new Error("DispatchSpec must be a JSON object");
  }
  const specRecord = spec as unknown as Record<string, unknown>;
  const allowedSpecFields = new Set<string>([
    "schema_version",
    "routes",
    "spec_hash",
  ]);
  const unknownSpecFields = Object.keys(specRecord).filter(
    (field) => !allowedSpecFields.has(field),
  );
  if (unknownSpecFields.length > 0) {
    throw new Error(
      `DispatchSpec contains unknown field(s): ${unknownSpecFields.sort().join(", ")}`,
    );
  }
  if (!Array.isArray(specRecord.routes)) {
    throw new Error("DispatchSpec routes must be an array");
  }
  if (typeof specRecord.schema_version !== "string") {
    throw new Error("DispatchSpec schema_version must be a string");
  }
  if (
    specRecord.spec_hash !== undefined &&
    typeof specRecord.spec_hash !== "string"
  ) {
    throw new Error("DispatchSpec spec_hash must be a string when present");
  }

  const routeKeys = new Set<string>();
  const aliasesByService = new Map<string, Set<string | undefined>>();
  const requiredRouteFields = [
    "route_key",
    "proto_file",
    "proto_service",
    "method",
    "input_type",
    "output_type",
    "payload_type",
  ] as const;
  const allowedRouteFields = new Set<string>([
    ...requiredRouteFields,
    "dependency_alias",
  ]);
  for (const route of spec.routes) {
    if (typeof route !== "object" || route === null || Array.isArray(route)) {
      throw new Error("DispatchSpec routes must contain JSON objects");
    }
    const routeRecord = route as unknown as Record<string, unknown>;
    const unknownRouteFields = Object.keys(routeRecord).filter(
      (field) => !allowedRouteFields.has(field),
    );
    if (unknownRouteFields.length > 0) {
      throw new Error(
        `DispatchSpec route contains unknown field(s): ${unknownRouteFields.sort().join(", ")}`,
      );
    }
    for (const field of requiredRouteFields) {
      if (typeof routeRecord[field] !== "string" || routeRecord[field] === "") {
        throw new Error(
          `DispatchSpec route field '${field}' must be a non-empty string`,
        );
      }
    }
    if (
      routeRecord.dependency_alias !== undefined &&
      typeof routeRecord.dependency_alias !== "string"
    ) {
      throw new Error(
        "DispatchSpec dependency_alias must be a string when present",
      );
    }
    if (!route.route_key) {
      throw new Error("DispatchSpec route_key must be non-empty");
    }
    if (routeKeys.has(route.route_key)) {
      throw new Error(
        `DispatchSpec contains duplicate route_key '${route.route_key}'`,
      );
    }
    routeKeys.add(route.route_key);
    if (
      route.dependency_alias !== undefined &&
      route.dependency_alias.trim().length === 0
    ) {
      throw new Error(
        `DispatchSpec remote route '${route.route_key}' has an empty dependency_alias`,
      );
    }
    const aliases = aliasesByService.get(route.proto_service) ?? new Set();
    aliases.add(route.dependency_alias);
    aliasesByService.set(route.proto_service, aliases);
  }
  for (const [service, aliases] of aliasesByService) {
    if (aliases.size !== 1) {
      throw new Error(
        `DispatchSpec mixes local/remote routes or dependency aliases for service '${service}'`,
      );
    }
  }
}

function canonicalizeJson(value: unknown): unknown {
  if (Array.isArray(value)) {
    return value.map(canonicalizeJson);
  }
  if (value !== null && typeof value === "object") {
    return Object.fromEntries(
      Object.entries(value as Record<string, unknown>)
        .sort(([left], [right]) => (left < right ? -1 : left > right ? 1 : 0))
        .map(([key, child]) => [key, canonicalizeJson(child)]),
    );
  }
  return value;
}

function dispatchHandlerMethodName(route: DispatchRoute): string {
  const serviceName =
    route.proto_service.split(".").pop() ?? route.proto_service;
  return toLowerCamelCase(
    `${toSnakeCase(serviceName)}_${toSnakeCase(route.method)}`,
  );
}

function parseOptions(
  rawOptions: { key: string; value: string }[],
): PluginParams {
  let dispatchSpecPath: string | undefined;
  let dispatchSpecHash: string | undefined;

  for (const { key, value } of rawOptions) {
    switch (key) {
      case "dispatch_spec":
        dispatchSpecPath = value;
        break;
      case "dispatch_spec_hash":
        dispatchSpecHash = value;
        break;
      case "target":
      case "import_extension":
        // Ignore — target=ts is the only supported mode.
        break;
      default:
        throw new Error(`Unknown option '${key}'.`);
    }
  }

  return {
    dispatchSpecPath,
    dispatchSpecHash,
  };
}

function analyzeFileMethods(file: DescFile): AnalyzedMethod[] {
  const analyzedMethods: AnalyzedMethod[] = [];

  for (const service of file.services) {
    for (const method of service.methods) {
      if (method.methodKind !== "unary") {
        throw new Error(
          `${service.typeName}.${method.name}: only unary RPC methods are supported for TypeScript generation.`,
        );
      }

      const requestCompanionName = requestCompanionNameForMethod(method);
      analyzedMethods.push({
        method,
        requestCompanionName,
      });
    }
  }

  return analyzedMethods;
}

function buildMethodMetadata(method: DescMethod): MethodMetadata {
  return {
    name: method.name,
    snake_name: toSnakeCase(method.name),
    route_key: routeKeyForMethod(method),
    input_ref: buildTypeRef(method.input),
    output_ref: buildTypeRef(method.output),
  };
}

function buildTypeRef(message: DescMessage): TypeRef {
  return {
    proto_type: message.typeName,
    type_name: relativeTypeName(message),
    proto_package: message.file.proto.package,
    proto_file: protoFilePath(message.file),
  };
}

function relativeTypeName(message: DescMessage): string {
  const packageName = message.file.proto.package;
  const packagePrefix = packageName ? `${packageName}.` : "";
  return message.typeName.startsWith(packagePrefix)
    ? message.typeName.slice(packagePrefix.length)
    : message.typeName;
}

function requestCompanionNameForMethod(method: DescMethod): string {
  return method.input.name;
}

function routeKeyForMethod(method: DescMethod): string {
  return `${method.parent.typeName}.${method.name}`;
}

function toLowerCamelCase(value: string): string {
  return value
    .replace(/(?:^|_)(.)/g, (_, c: string) => c.toUpperCase())
    .replace(/^./, (c) => c.toLowerCase());
}

function normalizePath(value: string): string {
  let normalized = value.replaceAll("\\", "/");
  while (normalized.startsWith("./")) {
    normalized = normalized.slice(2);
  }
  return normalized;
}

function protoFilePath(file: DescFile): string {
  return normalizeProtoPath(file.name);
}

function normalizeProtoPath(value: string): string {
  const normalized = normalizePath(value);
  return normalized.endsWith(".proto") ? normalized : `${normalized}.proto`;
}

function normalizeProtoFileKey(value: string): string {
  return normalizeProtoPath(value).slice(0, -".proto".length);
}

function toSnakeCase(value: string): string {
  return value
    .replace(/(.)([A-Z][a-z]+)/g, "$1_$2")
    .replace(/([a-z0-9])([A-Z])/g, "$1_$2")
    .toLowerCase();
}
