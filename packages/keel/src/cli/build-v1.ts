import fs from 'node:fs';
import path from 'node:path';
import { parseSync } from 'oxc-parser';
import { V1_RUNTIME_IMPORTS, V1_SERVICE_PREFIX, type V1Link, v1ServiceDispatchId } from '../v1';

/** AST fields used for imports and emitted CommonJS require calls. */
type Node = {
  type: string;
  start: number;
  end: number;
  name?: string;
  value?: string;
  body?: Node[];
  importKind?: string;
  exportKind?: string;
  source?: Node;
  specifiers?: Node[];
  imported?: Node;
  callee?: Node;
  expression?: Node;
  arguments?: Node[];
};

/** Parse the source graph and the final CommonJS body with the same parser. */
function parse(code: string, file: string, sourceType: 'module' | 'script'): Node {
  const result = parseSync(code, { sourceFilename: file, sourceType });
  // A parser error must never become an empty linkage.
  if (result.errors.length) {
    throw new Error(`${file}: ${result.errors.map((error) => error.message).join('\n')}`);
  }
  return JSON.parse(result.program);
}

/** Identify modules whose named imports confer a Guest capability. */
function isCapabilityModule(moduleId: string): boolean {
  return moduleId === 'keel/guest' || moduleId.startsWith('host:');
}

/** Collect capability imports from one input file before Bun can erase unused bindings. */
export function collectV1SourceLinks(code: string, file: string): V1Link[] {
  const links: V1Link[] = [];
  for (const node of parse(code, file, 'module').body ?? []) {
    const moduleId = node.source?.value;
    // Local barrels are handled by visiting every module in Bun's source graph.
    if (!moduleId || !isCapabilityModule(moduleId)) continue;
    // Type-only declarations do not acquire any runtime capability.
    if (node.importKind === 'type' || node.exportKind === 'type') continue;
    // Require a named import at the boundary, including when a local barrel re-exports it.
    if (node.type !== 'ImportDeclaration' || !node.specifiers?.length) {
      throw new Error(`${file}: v1 requires named imports from "${moduleId}"`);
    }
    for (const specifier of node.specifiers) {
      // Inline type imports are erased as well.
      if (specifier.importKind === 'type') continue;
      const name = specifier.imported?.name ?? specifier.imported?.value;
      // Namespace, default and bare imports would hide which capabilities the Guest gets.
      if (specifier.type !== 'ImportSpecifier' || !name) {
        throw new Error(`${file}: v1 requires named imports from "${moduleId}"`);
      }
      links.push([moduleId, name]);
    }
  }
  return links;
}

/** Walk parsed nodes without depending on Bun's generated binding names. */
function visit(node: Node, check: (node: Node) => void): void {
  check(node);
  for (const child of Object.values(node)) {
    // Oxc represents children as nodes or arrays of nodes.
    if (Array.isArray(child)) {
      for (const item of child) {
        // Array elisions and non-node metadata do not participate in validation.
        if (item?.type) visit(item, check);
      }
    } else if (child?.type) {
      visit(child, check);
    }
  }
}

/** Verify and rewrite only literal external requires in Bun's output. */
export function linkV1Bundle(
  code: string,
  links: V1Link[],
  externals: Readonly<Record<string, string>>
): { code: string; links: V1Link[] } {
  const modules = new Map(links.map(([id, name]) => [id, name]));
  const runtimeImports = new Set<string>();
  const edits: Array<{ start: number; end: number; replacement: string }> = [];
  visit(parse(code, 'guest.js', 'script'), (node) => {
    // Dynamic imports and explicit dynamic code entry points are outside the v1 bundle format.
    if (
      node.type === 'ImportExpression' ||
      (node.type === 'Identifier' && ['eval', 'Function'].includes(node.name ?? '')) ||
      (node.type === 'ComputedMemberExpression' &&
        ['eval', 'Function'].includes(node.expression?.value ?? ''))
    ) {
      throw new Error(
        'capability_mismatch: dynamic imports and dynamic code are unavailable in v1'
      );
    }
    // Bun emits literal CommonJS requires for externals.
    if (node.type !== 'CallExpression' || node.callee?.name !== 'require') return;
    const moduleId = node.arguments?.[0]?.value;
    // Reject opaque runtime dependencies, even when the generic strict flag is disabled.
    if (!moduleId || node.arguments?.length !== 1) {
      throw new Error('unknown_capability: v1 requires a literal module ID');
    }
    const runtimeImport = V1_RUNTIME_IMPORTS.some((id) => id === moduleId);
    // A runtime dependency needs a standard runtime ID or a collected source link.
    if (!runtimeImport && !modules.has(moduleId)) {
      throw new Error(`unknown_capability: unlinked v1 dependency "${moduleId}"`);
    }
    // Include implicit React JSX runtime dependencies emitted by Bun.
    if (runtimeImport) runtimeImports.add(moduleId);
    let replacement = `globalThis.${externals[moduleId]}`;
    // Resolve Host imports to the module installed by the per-Guest dispatch table.
    if (moduleId.startsWith('host:')) {
      const wireId = moduleId.startsWith(V1_SERVICE_PREFIX)
        ? v1ServiceDispatchId(moduleId, modules.get(moduleId)!)
        : moduleId;
      replacement = `globalThis.__keel_importHostModule(${JSON.stringify(wireId)})`;
    }
    edits.push({ start: node.start, end: node.end, replacement });
  });

  // Oxc offsets are UTF-8 byte positions, including after non-ASCII source literals.
  const bytes = Buffer.from(code);
  let offset = 0;
  const chunks: string[] = [];
  for (const { start, end, replacement } of edits.sort((a, b) => a.start - b.start)) {
    chunks.push(bytes.subarray(offset, start).toString(), replacement);
    offset = end;
  }
  chunks.push(bytes.subarray(offset).toString());
  return {
    code: chunks.join(''),
    links: [...links, ...Array.from(runtimeImports, (id): V1Link => [id])],
  };
}

/** Resolve an output through its physical directory and reject file symlinks. */
export function physicalV1Output(target: string): string {
  const directory = fs.realpathSync(path.dirname(target));
  const output = path.join(directory, path.basename(target));
  // Reject even dangling symlinks to keep the write at the checked package path.
  if (fs.lstatSync(output, { throwIfNoEntry: false })?.isSymbolicLink()) {
    throw new Error('capability_mismatch: v1 output cannot be a symbolic link');
  }
  return output;
}

/** Use physical paths so equivalent cwd/output spellings produce the same signed entry. */
export function normalizedV1GuestEntry(target: string, payloadRoot: string): string {
  const root = fs.realpathSync(payloadRoot);
  const relative = path.relative(root, physicalV1Output(target));
  // A package cannot reference files outside its physical payload root.
  if (
    !relative ||
    relative === '..' ||
    relative.startsWith(`..${path.sep}`) ||
    path.isAbsolute(relative)
  ) {
    throw new Error('capability_mismatch: v1 Guest output is outside the package root');
  }
  return relative.split(path.sep).join('/');
}
