// Oxc-only adapter for dependency scanning
// Hard dependency on 'oxc-parser'. If not installed, this module will throw at runtime.

// Basic types for AST node structures (oxc-parser specific)
interface SourceLocation {
  start?: number;
  end?: number;
  line?: number;
  column?: number;
}

interface ASTNode {
  type?: string;
  [key: string]: unknown;
}

interface OxcParserAPI {
  parseSync?: (code: string, filename?: string) => ASTNode;
  parse?: (code: string, filename?: string) => ASTNode;
}

export type ScanDetail = { moduleId: string; kind: 'import' | 'export' | 'require' | 'dynamic'; loc?: SourceLocation };
export type ScanResult = {
  static: string[]; // from import/export and require('literal')
  dynamicLiteral: string[]; // from import('literal')
  dynamicNonLiteral: number; // count of import(expr) that is not string literal
  evalCount: number; // number of eval(...) calls
  details: ScanDetail[];
};

export async function analyzeModuleIDs(code: string): Promise<ScanResult> {
  // eslint-disable-next-line @typescript-eslint/no-var-requires
  const oxc: OxcParserAPI = require('oxc-parser');
  const parse = oxc.parseSync || oxc.parse;
  if (!parse) {
    throw new Error("oxc-parser not available. Please install it with 'npm i -D oxc-parser'.");
  }

  // oxc-parser expects either (code) or (code, filename: string)
  const ast = (() => {
    try { return parse(code, 'inline.js'); } catch { return parse(code); }
  })();

  const root: ASTNode = (ast && ((ast as ASTNode & {program?: ASTNode}).program || ast)) || ast;

  const foundStatic = new Set<string>();
  const foundDyn = new Set<string>();
  let dynNonLiteral = 0;
  let evalCount = 0;
  const details: ScanDetail[] = [];

  function deepFindString(obj: unknown, depth = 0): string | null {
    if (!obj || depth > 3) return null;
    if (typeof obj === 'string') return obj;
    if (typeof obj === 'object') {
      for (const k of Object.keys(obj)) {
        const v = (obj as Record<string, unknown>)[k];
        if (typeof v === 'string') return v;
        const nested = deepFindString(v, depth + 1);
        if (nested) return nested;
      }
    }
    return null;
  }

  function getStringLiteral(n: unknown): string | null {
    if (n == null) return null;
    if (typeof n === 'string') return n as string;
    const tryExtract = (obj: unknown): string | null => {
      if (!obj || typeof obj !== 'object') return null;
      const pick = (s: unknown) => {
        if (typeof s !== 'string') return null;
        // normalize quotes
        if ((s.startsWith('"') && s.endsWith('"')) || (s.startsWith("'") && s.endsWith("'"))) {
          return s.slice(1, -1);
        }
        return s;
      };
      const anyStr = deepFindString(obj);
      if (anyStr) return anyStr;
      const objRecord = obj as Record<string, unknown>;
      const v1 = pick(objRecord['value']);
      if (v1) return v1;
      const v2 = pick(objRecord['raw']);
      if (v2) return v2;
      const v3 = pick(objRecord['cooked']);
      if (v3) return v3;
      // oxc may nest value objects, dig one level deeper
      if (objRecord['value'] && typeof objRecord['value'] === 'object') {
        const v = tryExtract(objRecord['value']);
        if (v) return v;
      }
      return null;
    };
    if (typeof n === 'object') {
      const nNode = n as ASTNode;
      if (nNode.type === 'Literal' || nNode.type === 'StringLiteral') {
        const v = tryExtract(n);
        if (v) return v;
      }
      const v2 = tryExtract(n);
      if (v2) return v2;
    }
    return null;
  }

  function walk(node: ASTNode) {
    if (!node || typeof node !== 'object') return;
    const type = node.type;
    // Static imports
    // Generic Import/Export with source (covers parser variations)
    const nodeWithSource = node as ASTNode & { source?: unknown };
    if (node && nodeWithSource.source) {
      const v = getStringLiteral(nodeWithSource.source);
      if (v) {
        const nodeWithLoc = node as ASTNode & { loc?: SourceLocation; span?: SourceLocation };
        const loc = nodeWithLoc.loc ?? nodeWithLoc.span;
        if (String(type || '').includes('Export')) {
          foundStatic.add(v);
          const detail: ScanDetail = { moduleId: v, kind: 'export' };
          if (loc !== undefined) detail.loc = loc;
          details.push(detail);
        } else {
          foundStatic.add(v);
          const detail: ScanDetail = { moduleId: v, kind: 'import' };
          if (loc !== undefined) detail.loc = loc;
          details.push(detail);
        }
      }
    }
    // require('x')
    const nodeWithCallee = node as ASTNode & { callee?: ASTNode; arguments?: unknown[] };
    if (type === 'CallExpression' && nodeWithCallee.callee && nodeWithCallee.callee.type === 'Identifier' && (nodeWithCallee.callee as ASTNode & { name?: string }).name === 'require') {
      const arg = nodeWithCallee.arguments?.[0];
      const v = getStringLiteral(arg);
      if (v) {
        foundStatic.add(v);
        const nodeWithLoc = node as ASTNode & { loc?: SourceLocation; span?: SourceLocation };
        const loc = nodeWithLoc.loc ?? nodeWithLoc.span;
        const detail: ScanDetail = { moduleId: v, kind: 'require' };
        if (loc !== undefined) detail.loc = loc;
        details.push(detail);
      }
    }
    // import('x') - different parsers may represent as ImportExpression or CallExpression(callee: Import)
    if (type === 'ImportExpression' && nodeWithSource.source) {
      const v = getStringLiteral(nodeWithSource.source);
      if (v) {
        foundDyn.add(v);
        const nodeWithLoc = node as ASTNode & { loc?: SourceLocation; span?: SourceLocation };
        const loc = nodeWithLoc.loc ?? nodeWithLoc.span;
        const detail: ScanDetail = { moduleId: v, kind: 'dynamic' };
        if (loc !== undefined) detail.loc = loc;
        details.push(detail);
      }
      else dynNonLiteral += 1;
    }
    if (type === 'CallExpression' && nodeWithCallee.callee) {
      const cal = nodeWithCallee.callee;
      const calWithName = cal as ASTNode & { name?: string };
      const isImportIdent = (cal.type === 'Identifier' && calWithName.name === 'import');
      const isImportNode = (cal.type === 'Import');
      if (isImportIdent || isImportNode) {
        const arg = nodeWithCallee.arguments?.[0];
        const v = getStringLiteral(arg);
        if (v) {
          foundDyn.add(v);
          const nodeWithLoc = node as ASTNode & { loc?: SourceLocation; span?: SourceLocation };
          const loc = nodeWithLoc.loc ?? nodeWithLoc.span;
          const detail: ScanDetail = { moduleId: v, kind: 'dynamic' };
          if (loc !== undefined) detail.loc = loc;
          details.push(detail);
        }
        else dynNonLiteral += 1;
      }
    }
    // eval('...')
    if (type === 'CallExpression' && nodeWithCallee.callee && nodeWithCallee.callee.type === 'Identifier' && (nodeWithCallee.callee as ASTNode & { name?: string }).name === 'eval') {
      evalCount += 1;
    }
    // Recurse
    for (const key in node) {
      const child = node[key];
      if (Array.isArray(child)) child.forEach((c: unknown) => c && typeof c === 'object' && walk(c as ASTNode));
      else if (child && typeof child === 'object') walk(child as ASTNode);
    }
  }

  // Prefer walking root.program if exists
  walk(root);


  return { static: Array.from(foundStatic), dynamicLiteral: Array.from(foundDyn), dynamicNonLiteral: dynNonLiteral, evalCount, details };
}
