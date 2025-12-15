// Oxc-only adapter for dependency scanning
// Hard dependency on 'oxc-parser'. If not installed, this module will throw at runtime.

export type ScanDetail = {
  moduleId: string;
  kind: 'import' | 'export' | 'require' | 'dynamic';
  start?: number;
  end?: number;
};

export type ScanResult = {
  static: string[]; // from import/export and require('literal')
  dynamicLiteral: string[]; // from import('literal')
  dynamicNonLiteral: number; // count of import(expr) that is not string literal
  evalCount: number; // number of eval(...) calls
  details: ScanDetail[];
};

interface ASTNode {
  type?: string;
  [key: string]: unknown;
}

/**
 * Analyze module dependencies using oxc-parser
 *
 * Uses oxc-parser's built-in module information and Visitor API
 * for comprehensive dependency scanning.
 */
export async function analyzeModuleIDs(code: string): Promise<ScanResult> {
  // eslint-disable-next-line @typescript-eslint/no-var-requires
  const oxc = require('oxc-parser');

  if (!oxc.parseSync) {
    throw new Error("oxc-parser not available. Please install it with 'npm i -D oxc-parser'.");
  }

  // Parse with oxc-parser (correct signature: filename, sourceText)
  const result = oxc.parseSync('inline.js', code, {
    sourceType: 'module' as const,
  });

  const foundStatic = new Set<string>();
  const foundDyn = new Set<string>();
  let dynNonLiteral = 0;
  let evalCount = 0;
  const details: ScanDetail[] = [];

  // Use oxc-parser's built-in module information
  if (result.module) {
    // Static imports
    if (result.module.staticImports) {
      for (const imp of result.module.staticImports) {
        if (imp.moduleRequest) {
          // moduleRequest is an object with { value, start, end }
          const moduleName =
            typeof imp.moduleRequest === 'string' ? imp.moduleRequest : imp.moduleRequest.value;

          if (moduleName && typeof moduleName === 'string') {
            foundStatic.add(moduleName);
            details.push({
              moduleId: moduleName,
              kind: 'import',
              start: imp.start,
              end: imp.end,
            });
          }
        }
      }
    }

    // Static exports (re-exports)
    if (result.module.staticExports) {
      for (const exp of result.module.staticExports) {
        if (exp.moduleRequest) {
          // moduleRequest is an object with { value, start, end }
          const moduleName =
            typeof exp.moduleRequest === 'string' ? exp.moduleRequest : exp.moduleRequest.value;

          if (moduleName && typeof moduleName === 'string') {
            foundStatic.add(moduleName);
            details.push({
              moduleId: moduleName,
              kind: 'export',
              start: exp.start,
              end: exp.end,
            });
          }
        }
      }
    }

    // Dynamic imports
    if (result.module.dynamicImports) {
      for (const imp of result.module.dynamicImports) {
        // Dynamic imports can be literal or expression
        if (imp.start !== undefined) {
          // Try to extract literal value if available
          const sourceCode = code.substring(imp.start, imp.end);
          const literalMatch = sourceCode.match(/import\s*\(\s*['"`]([^'"`]+)['"`]\s*\)/);
          if (literalMatch?.[1]) {
            foundDyn.add(literalMatch[1]);
            details.push({
              moduleId: literalMatch[1],
              kind: 'dynamic',
              start: imp.start,
              end: imp.end,
            });
          } else {
            dynNonLiteral += 1;
          }
        }
      }
    }
  }

  // Use Visitor API to find require() calls and eval()
  if (oxc.Visitor && result.program) {
    const visitor = new oxc.Visitor({
      CallExpression(node: ASTNode & { callee?: ASTNode; arguments?: unknown[] }) {
        const callee = node.callee;
        if (!callee) return;

        // Check for require('module')
        if (
          callee.type === 'Identifier' &&
          (callee as ASTNode & { name?: string }).name === 'require'
        ) {
          const args = node.arguments;
          if (args && args.length > 0) {
            const firstArg = args[0] as ASTNode & { value?: unknown; type?: string };
            if (firstArg.type === 'Literal' || firstArg.type === 'StringLiteral') {
              const value = firstArg.value;
              if (typeof value === 'string') {
                foundStatic.add(value);
                const detail: ScanDetail = {
                  moduleId: value,
                  kind: 'require',
                };
                const start = (node as ASTNode & { start?: number }).start;
                const end = (node as ASTNode & { end?: number }).end;
                if (start !== undefined) detail.start = start;
                if (end !== undefined) detail.end = end;
                details.push(detail);
              }
            }
          }
        }

        // Check for eval()
        if (
          callee.type === 'Identifier' &&
          (callee as ASTNode & { name?: string }).name === 'eval'
        ) {
          evalCount += 1;
        }
      },
    });

    visitor.visit(result.program);
  }

  return {
    static: Array.from(foundStatic),
    dynamicLiteral: Array.from(foundDyn),
    dynamicNonLiteral: dynNonLiteral,
    evalCount,
    details,
  };
}
