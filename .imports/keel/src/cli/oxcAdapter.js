// Oxc-only adapter for dependency scanning (CommonJS)
// Hard dependency on 'oxc-parser'. If not installed, this module will throw at runtime.

const oxc = require('oxc-parser');

/**
 * Analyze module dependencies using oxc-parser
 *
 * Uses moduleLexerSync for ES6 imports/exports and regex patterns for require() and eval()
 *
 * @param {string} code - The JavaScript/TypeScript code to analyze
 * @returns {{
 *   static: string[],
 *   dynamicLiteral: string[],
 *   dynamicNonLiteral: number,
 *   evalCount: number,
 *   details: Array<{moduleId: string, kind: string, start?: number, end?: number}>
 * }}
 */
function analyzeModuleIDs(code) {
  if (!oxc || !oxc.moduleLexerSync) {
    throw new Error("oxc-parser not available. Please install it with 'npm i -D oxc-parser'.");
  }

  const foundStatic = new Set();
  const foundDyn = new Set();
  let dynNonLiteral = 0;
  let evalCount = 0;
  const details = [];

  // Use moduleLexerSync for ES6 imports and exports
  try {
    const lexerResult = oxc.moduleLexerSync(code);

    // Process static imports
    if (lexerResult.imports) {
      for (const imp of lexerResult.imports) {
        if (imp.n) {
          // imp.n is the module specifier
          foundStatic.add(imp.n);
          details.push({
            moduleId: imp.n,
            kind: imp.d >= 0 ? 'dynamic' : 'import',
            start: imp.s,
            end: imp.e,
          });

          // Check if it's a dynamic import (d >= 0 means dynamic)
          if (imp.d >= 0) {
            foundDyn.add(imp.n);
          }
        } else if (imp.d >= 0) {
          // Dynamic import with non-literal specifier
          dynNonLiteral++;
        }
      }
    }

    // Process static exports (re-exports)
    if (lexerResult.exports) {
      for (const exp of lexerResult.exports) {
        if (exp.n) {
          foundStatic.add(exp.n);
          details.push({
            moduleId: exp.n,
            kind: 'export',
            start: exp.s,
            end: exp.e,
          });
        }
      }
    }
  } catch (err) {
    // If lexer fails, continue with regex-based detection
    console.warn('[oxcAdapter] moduleLexerSync failed:', err.message);
  }

  // Helper: Check if a position is inside a comment or string literal
  function isInCommentOrString(code, pos) {
    const beforePos = code.substring(0, pos);

    // Check if in single-line comment
    const lastNewline = beforePos.lastIndexOf('\n');
    const linePart = beforePos.substring(lastNewline + 1);
    if (linePart.indexOf('//') !== -1) {
      return true;
    }

    // Check if in multi-line comment
    const openComment = beforePos.lastIndexOf('/*');
    const closeComment = beforePos.lastIndexOf('*/');
    if (openComment > closeComment) {
      return true;
    }

    // Check if in string literal (simplified - count quotes before position)
    // This is not perfect but works for most cases
    const doubleQuotes = (beforePos.match(/(?<!\\)"/g) || []).length;
    const singleQuotes = (beforePos.match(/(?<!\\)'/g) || []).length;
    const backticks = (beforePos.match(/(?<!\\)`/g) || []).length;

    // If odd number of quotes, we're inside a string
    return doubleQuotes % 2 === 1 || singleQuotes % 2 === 1 || backticks % 2 === 1;
  }

  // Use regex to find require() calls
  // Matches: require('module'), require("module"), require(`module`)
  const requirePattern = /\brequire\s*\(\s*(['"`])([^'"`]+)\1\s*\)/g;
  let match;
  while ((match = requirePattern.exec(code)) !== null) {
    // Skip if this match is inside a comment or string
    if (!isInCommentOrString(code, match.index)) {
      const moduleName = match[2];
      foundStatic.add(moduleName);
      details.push({
        moduleId: moduleName,
        kind: 'require',
        start: match.index,
        end: match.index + match[0].length,
      });
    }
  }

  // Count eval() calls (skip those in comments or strings)
  const evalPattern = /\beval\s*\(/g;
  let evalMatch;
  while ((evalMatch = evalPattern.exec(code)) !== null) {
    if (!isInCommentOrString(code, evalMatch.index)) {
      evalCount++;
    }
  }

  return {
    static: Array.from(foundStatic),
    dynamicLiteral: Array.from(foundDyn),
    dynamicNonLiteral: dynNonLiteral,
    evalCount,
    details,
  };
}

module.exports = { analyzeModuleIDs };
