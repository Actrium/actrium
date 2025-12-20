// TypeScript types and re-export for oxcAdapter.js
// The actual implementation is in oxcAdapter.js to avoid Bun/TypeScript module loading issues with oxc-parser

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

/**
 * Analyze module dependencies using oxc-parser
 *
 * Uses oxc-parser's built-in module information and Visitor API
 * for comprehensive dependency scanning.
 */
export function analyzeModuleIDs(code: string): ScanResult {
  // eslint-disable-next-line @typescript-eslint/no-var-requires
  const impl = require('./oxcAdapter.js');
  return impl.analyzeModuleIDs(code);
}
