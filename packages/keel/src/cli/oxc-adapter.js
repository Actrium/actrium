// Oxc-only adapter for dependency scanning (CommonJS)
// Hard dependency on 'oxc-parser'. If not installed, this module will throw at runtime.

const oxc = require('oxc-parser');

const HOST_MODULE_PATTERN = /^host:[a-z0-9_-]+(?:\/[a-z0-9_-]+)*$/;

function isHostModuleId(value) {
  return typeof value === 'string' && HOST_MODULE_PATTERN.test(value);
}

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

    // moduleLexerSync.exports contains exported symbol names, not dependency module ids.
    // Re-export sources are already represented in the imports collection.
  } catch (err) {
    // If lexer fails, continue with regex-based detection
    console.warn('[oxc-adapter] moduleLexerSync failed:', err.message);
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

/**
 * Analyze Keel host module imports and Guest exports.
 *
 * Static named imports are the only accepted host import shape:
 *   import { openProfile } from 'host:navigation'
 *
 * Other host module access forms are intentionally rejected here so the
 * capability boundary stays visible in source and statically auditable.
 *
 * @param {string} code
 * @returns {{
 *   hostImports: Array<{
 *     moduleId: string,
 *     specifiers: Array<{ imported: string | null, local: string, kind: string }>,
 *     kind: string,
 *     start?: number,
 *     end?: number
 *   }>,
 *   hostCapabilities: string[],
 *   guestExports: string[],
 *   hasDefaultExport: boolean,
 *   violations: Array<{ code: string, moduleId: string, message: string, start?: number, end?: number }>
 * }}
 */
function analyzeHostBoundary(code) {
  const hostImports = [];
  const hostCapabilities = new Set();
  const guestExports = new Set();
  const violations = [];
  let hasDefaultExport = false;

  try {
    const result = oxc.parseSync(code, {
      sourceType: 'module',
      sourceFilename: 'keel-guest-boundary.tsx',
    });

    if (result.errors && result.errors.length > 0) {
      return {
        hostImports,
        hostCapabilities: [],
        guestExports: [],
        hasDefaultExport,
        violations,
      };
    }

    const ast = typeof result.program === 'string' ? JSON.parse(result.program) : result.program;

    for (const node of ast.body || []) {
      if (node.type === 'ImportDeclaration') {
        collectHostImport(node, hostImports, hostCapabilities, violations);
      } else if (node.type === 'ExportNamedDeclaration') {
        collectGuestNamedExport(node, guestExports);
        collectHostReExport(node, violations);
      } else if (node.type === 'ExportDefaultDeclaration') {
        hasDefaultExport = true;
      }
    }
  } catch (err) {
    console.warn('[oxc-adapter] analyzeHostBoundary failed:', err.message);
  }

  const moduleScan = analyzeModuleIDs(code);
  for (const detail of moduleScan.details) {
    if (!isHostModuleId(detail.moduleId)) continue;

    if (detail.kind === 'dynamic') {
      violations.push({
        code: 'host-dynamic-import',
        moduleId: detail.moduleId,
        message: `Dynamic host module import is not allowed: ${detail.moduleId}. Use a static named import.`,
        start: detail.start,
        end: detail.end,
      });
      hostImports.push({
        moduleId: detail.moduleId,
        specifiers: [],
        kind: 'dynamic',
        start: detail.start,
        end: detail.end,
      });
    } else if (detail.kind === 'require') {
      violations.push({
        code: 'host-require',
        moduleId: detail.moduleId,
        message: `CommonJS require() is not allowed for host modules: ${detail.moduleId}. Use a static named import.`,
        start: detail.start,
        end: detail.end,
      });
      hostImports.push({
        moduleId: detail.moduleId,
        specifiers: [],
        kind: 'require',
        start: detail.start,
        end: detail.end,
      });
    }
  }

  return {
    hostImports,
    hostCapabilities: Array.from(hostCapabilities).sort(),
    guestExports: Array.from(guestExports).sort(),
    hasDefaultExport,
    violations,
  };
}

function collectHostImport(node, hostImports, hostCapabilities, violations) {
  const moduleId = getStringLiteralValue(node.source);
  if (!isHostModuleId(moduleId)) return;

  // A whole `import type { ... } from 'host:*'` is erased by the compiler and
  // crosses no runtime boundary, so it declares no capability. Skip it entirely:
  // it must not enter the manifest, nor trip the default/namespace/bare rules
  // below (a type-only default/namespace import is not a runtime violation).
  if (node.importKind === 'type') return;

  const specifiers = [];

  for (const specifier of node.specifiers || []) {
    if (specifier.type === 'ImportSpecifier') {
      // An inline `import { type X } from 'host:*'` specifier is likewise erased
      // — it is not a runtime capability, so it must not enter the manifest.
      if (specifier.importKind === 'type') continue;
      const imported = getImportSpecifierName(specifier.imported);
      const local = getImportSpecifierName(specifier.local) || imported;
      if (imported) {
        specifiers.push({
          imported,
          local,
          kind: 'named',
        });
        hostCapabilities.add(`${moduleId}.${imported}`);
      }
    } else if (specifier.type === 'ImportDefaultSpecifier') {
      const local = getImportSpecifierName(specifier.local) || 'default';
      specifiers.push({
        imported: 'default',
        local,
        kind: 'default',
      });
      violations.push({
        code: 'host-import-default',
        moduleId,
        message: `Default import is not allowed for host module ${moduleId}. Use named imports.`,
        start: specifier.start,
        end: specifier.end,
      });
    } else if (specifier.type === 'ImportNamespaceSpecifier') {
      const local = getImportSpecifierName(specifier.local) || '*';
      specifiers.push({
        imported: null,
        local,
        kind: 'namespace',
      });
      violations.push({
        code: 'host-import-namespace',
        moduleId,
        message: `Namespace import is not allowed for host module ${moduleId}. Use named imports.`,
        start: specifier.start,
        end: specifier.end,
      });
    }
  }

  // Only a genuinely bare `import 'host:*'` (no specifiers in SOURCE) is a bare
  // import. Guard on the source count, not the post-filter `specifiers`: a
  // statement whose only specifiers were inline `type` ones is not bare — it just
  // contributes nothing at runtime — and must not be flagged.
  if ((node.specifiers || []).length === 0) {
    specifiers.push({
      imported: null,
      local: '',
      kind: 'bare',
    });
    violations.push({
      code: 'host-import-bare',
      moduleId,
      message: `Bare host module import is not allowed: ${moduleId}. Import named capabilities instead.`,
      start: node.start,
      end: node.end,
    });
  }

  hostImports.push({
    moduleId,
    specifiers,
    kind: 'import',
    start: node.start,
    end: node.end,
  });
}

function collectGuestNamedExport(node, guestExports) {
  if (node.source) return;

  if (node.declaration) {
    const declaration = node.declaration;

    if (
      (declaration.type === 'FunctionDeclaration' || declaration.type === 'ClassDeclaration') &&
      declaration.id?.name
    ) {
      guestExports.add(declaration.id.name);
      return;
    }

    if (declaration.type === 'VariableDeclaration') {
      for (const item of declaration.declarations || []) {
        if (item.id?.type === 'Identifier' && item.id.name) {
          guestExports.add(item.id.name);
        }
      }
    }

    return;
  }

  for (const specifier of node.specifiers || []) {
    const exported = getExportSpecifierName(specifier.exported);
    if (exported) {
      guestExports.add(exported);
    }
  }
}

function collectHostReExport(node, violations) {
  const moduleId = getStringLiteralValue(node.source);
  if (!isHostModuleId(moduleId)) return;

  violations.push({
    code: 'host-reexport',
    moduleId,
    message: `Re-exporting a host module is not allowed: ${moduleId}. Import named capabilities where they are used.`,
    start: node.start,
    end: node.end,
  });
}

function getStringLiteralValue(node) {
  if (!node) return null;
  if (typeof node.value === 'string') return node.value;
  if (typeof node.raw === 'string') {
    return node.raw.replace(/^['"]|['"]$/g, '');
  }
  return null;
}

function getImportSpecifierName(node) {
  if (!node) return null;
  if (typeof node.name === 'string') return node.name;
  if (typeof node.value === 'string') return node.value;
  return null;
}

function getExportSpecifierName(node) {
  if (!node) return null;
  if (typeof node.name === 'string') return node.name;
  if (typeof node.value === 'string') return node.value;
  return null;
}

/**
 * Analyze JSX props and infer types for JSI optimization
 *
 * @param {string} code - The JavaScript/TypeScript/JSX code to analyze
 * @returns {{
 *   propHints: Array<{
 *     location: string,
 *     element: string,
 *     props: Record<string, string>
 *   }>,
 *   stats: {
 *     totalElements: number,
 *     jsiSafeProps: number,
 *     functionsProps: number,
 *     unknownProps: number
 *   }
 * }}
 */
function analyzeJSXProps(code) {
  if (!oxc || !oxc.parseSync) {
    throw new Error("oxc-parser not available. Please install it with 'npm i -D oxc-parser'.");
  }

  const propHints = [];
  const components = new Set();
  const stats = {
    totalElements: 0,
    jsiSafeProps: 0,
    functionProps: 0,
    unknownProps: 0,
  };

  try {
    // Parse code to AST
    // Note: oxc.parseSync signature is (code, options), not (filename, code, options)
    const result = oxc.parseSync(code, {
      sourceFilename: 'anonymous.tsx',
    });

    if (result.errors && result.errors.length > 0) {
      console.warn('[oxc-adapter] Parse errors:', result.errors.length);
      // Continue with partial AST
    }

    // oxc-parser returns program as JSON string, need to parse it
    const ast = typeof result.program === 'string' ? JSON.parse(result.program) : result.program;

    // Traverse AST to find JSX elements
    traverseAST(ast, (node) => {
      if (node.type === 'JSXElement' || node.type === 'JSXFragment') {
        stats.totalElements++;

        if (node.type === 'JSXElement') {
          const elementName = getJSXElementName(node.openingElement);
          components.add(elementName);
          const props = {};

          // Analyze each prop
          for (const attr of node.openingElement.attributes || []) {
            if (attr.type === 'JSXAttribute') {
              // attr.name can be JSXIdentifier or Identifier
              const propName =
                attr.name && (attr.name.type === 'JSXIdentifier' || attr.name.type === 'Identifier')
                  ? attr.name.name
                  : null;
              if (!propName) continue;

              const propType = inferPropType(attr.value);
              props[propName] = propType;

              // Update stats
              if (propType === 'function') stats.functionProps++;
              else if (
                propType.startsWith('jsi-safe') ||
                propType === 'string' ||
                propType === 'number' ||
                propType === 'boolean' ||
                propType === 'true'
              )
                stats.jsiSafeProps++;
              else if (propType === 'unknown') stats.unknownProps++;
            }
          }

          // Store hint with location (use start:end instead of span)
          const location =
            node.start !== undefined && node.end !== undefined
              ? `${node.start}:${node.end}`
              : 'unknown';
          propHints.push({
            location,
            element: elementName,
            props,
          });
        }
      }
    });
  } catch (err) {
    console.error('[oxc-adapter] analyzeJSXProps failed:', err.message);
    // Return empty result on error
  }

  return { propHints, components: Array.from(components).sort(), stats };
}

/**
 * Get JSX element name from opening element
 */
function getJSXElementName(openingElement) {
  if (!openingElement || !openingElement.name) return 'Unknown';

  const name = openingElement.name;

  // Handle both JSXIdentifier and Identifier types
  if (name.type === 'JSXIdentifier' || name.type === 'Identifier') {
    return name.name;
  } else if (name.type === 'JSXMemberExpression' || name.type === 'StaticMemberExpression') {
    // e.g., React.Component
    return getJSXMemberExpressionName(name);
  } else if (name.type === 'JSXNamespacedName') {
    // e.g., svg:path
    return `${name.namespace.name}:${name.name.name}`;
  }

  return 'Unknown';
}

function getJSXMemberExpressionName(expr) {
  const parts = [];
  let current = expr;

  while (current) {
    if (current.type === 'JSXMemberExpression') {
      if (current.property?.name) {
        parts.unshift(current.property.name);
      }
      current = current.object;
    } else if (current.type === 'JSXIdentifier') {
      parts.unshift(current.name);
      break;
    } else {
      break;
    }
  }

  return parts.join('.');
}

/**
 * Infer prop type from JSX attribute value
 */
function inferPropType(value) {
  if (!value) return 'true'; // Boolean shorthand: <Component prop />

  // String literal
  if (value.type === 'StringLiteral') {
    return 'string';
  }

  // JSX expression container
  if (value.type === 'JSXExpressionContainer') {
    const expr = value.expression;

    // Null/undefined
    if (expr.type === 'NullLiteral') return 'null';
    if (expr.type === 'Identifier' && expr.name === 'undefined') return 'undefined';

    // Number literal
    if (expr.type === 'NumericLiteral') return 'number';

    // Boolean literal
    if (expr.type === 'BooleanLiteral') return 'boolean';

    // String literal (in expression)
    if (expr.type === 'StringLiteral') return 'string';

    // Function (arrow function or function expression)
    if (expr.type === 'ArrowFunctionExpression' || expr.type === 'FunctionExpression') {
      return 'function';
    }

    // Object literal
    if (expr.type === 'ObjectExpression') {
      return analyzeObjectExpression(expr);
    }

    // Array literal
    if (expr.type === 'ArrayExpression') {
      return analyzeArrayExpression(expr);
    }

    // Variable reference or other expressions
    return 'unknown';
  }

  // JSX element as prop value
  if (value.type === 'JSXElement' || value.type === 'JSXFragment') {
    return 'jsx-element';
  }

  return 'unknown';
}

/**
 * Analyze object literal to determine if it's JSI-safe
 */
function analyzeObjectExpression(node) {
  if (!node.properties || node.properties.length === 0) {
    return 'jsi-safe-object'; // Empty object is safe
  }

  // Check if all properties are JSI-safe
  for (const prop of node.properties) {
    // Spread operator or computed keys - not JSI-safe
    if (prop.type !== 'ObjectProperty') {
      return 'object';
    }

    // Check if key is simple
    if (prop.computed || (prop.key.type !== 'Identifier' && prop.key.type !== 'StringLiteral')) {
      return 'object';
    }

    // Recursively check value type
    const valueType = inferPropTypeFromExpression(prop.value);
    if (!isJSISafeType(valueType)) {
      return 'object';
    }
  }

  return 'jsi-safe-object';
}

/**
 * Analyze array literal to determine if it's JSI-safe
 */
function analyzeArrayExpression(node) {
  if (!node.elements || node.elements.length === 0) {
    return 'jsi-safe-array'; // Empty array is safe
  }

  // Check if all elements are JSI-safe
  for (const elem of node.elements) {
    if (!elem) continue; // Sparse array element

    // Spread element - not JSI-safe
    if (elem.type === 'SpreadElement') {
      return 'array';
    }

    const elemType = inferPropTypeFromExpression(elem);
    if (!isJSISafeType(elemType)) {
      return 'array';
    }
  }

  return 'jsi-safe-array';
}

/**
 * Infer type from any expression node
 */
function inferPropTypeFromExpression(expr) {
  if (!expr) return 'unknown';

  if (expr.type === 'NullLiteral') return 'null';
  if (expr.type === 'NumericLiteral') return 'number';
  if (expr.type === 'BooleanLiteral') return 'boolean';
  if (expr.type === 'StringLiteral') return 'string';
  if (expr.type === 'ArrowFunctionExpression' || expr.type === 'FunctionExpression') {
    return 'function';
  }
  if (expr.type === 'ObjectExpression') return analyzeObjectExpression(expr);
  if (expr.type === 'ArrayExpression') return analyzeArrayExpression(expr);

  return 'unknown';
}

/**
 * Check if a type is JSI-safe (primitive types and jsi-safe composites)
 */
function isJSISafeType(type) {
  return [
    'string',
    'number',
    'boolean',
    'null',
    'undefined',
    'true',
    'jsi-safe-object',
    'jsi-safe-array',
  ].includes(type);
}

/**
 * Simple AST traversal helper
 */
function traverseAST(node, visitor) {
  if (!node || typeof node !== 'object') return;

  // Visit current node
  visitor(node);

  // Traverse children
  for (const key in node) {
    if (key === 'span' || key === 'loc' || key === 'range') continue; // Skip metadata

    const child = node[key];

    if (Array.isArray(child)) {
      for (const item of child) {
        traverseAST(item, visitor);
      }
    } else if (child && typeof child === 'object') {
      traverseAST(child, visitor);
    }
  }
}

/**
 * Extract component names from a transpiled bundle.
 *
 * JSX is already lowered to `jsx(...)` / `jsxs(...)` / `createElement(...)`
 * calls. The component can be a string tag, an `import_guest.View` member, or
 * a local identifier aliasing an SDK component (`const M = Modal; <M />`).
 * When `failClosed` is set, identifiers whose provenance cannot be resolved
 * reject the build instead of silently widening the allowlist.
 */
function analyzeBundleComponents(code, options = {}) {
  if (!oxc || !oxc.parseSync) {
    throw new Error("oxc-parser not available. Please install it with 'npm i -D oxc-parser'.");
  }

  const components = new Set();
  const sdkAliases = new Map();
  const aliasSources = new Map();
  const localComponents = new Set();
  const jsxIdentifiers = new Set();

  try {
    const result = oxc.parseSync(code, { sourceFilename: 'bundle.js' });
    const ast = typeof result.program === 'string' ? JSON.parse(result.program) : result.program;

    traverseAST(ast, (node) => {
      if (node.type === 'FunctionDeclaration' && node.id?.name) {
        localComponents.add(node.id.name);
      } else if (node.type === 'ClassDeclaration' && node.id?.name) {
        localComponents.add(node.id.name);
      } else if (node.type === 'VariableDeclarator' && node.id?.type === 'Identifier') {
        const name = node.id.name;
        const init = node.init;
        if (init?.type === 'ArrowFunctionExpression' || init?.type === 'FunctionExpression') {
          localComponents.add(name);
        } else if (init?.type === 'Identifier') {
          aliasSources.set(name, init.name);
        } else if (
          (init?.type === 'MemberExpression' || init?.type === 'StaticMemberExpression') &&
          init.object?.type === 'Identifier' &&
          /^import_guest\d*$/.test(init.object.name) &&
          init.property?.type === 'Identifier'
        ) {
          sdkAliases.set(name, init.property.name);
        }
      }

      if (node.type !== 'CallExpression') return;
      const callee = node.callee;
      const calleeName =
        callee?.type === 'Identifier'
          ? callee.name
          : callee?.type === 'MemberExpression' || callee?.type === 'StaticMemberExpression'
            ? callee.property?.name
            : undefined;
      if (
        calleeName !== 'jsx' &&
        calleeName !== 'jsxs' &&
        calleeName !== 'jsxDEV' &&
        calleeName !== 'createElement'
      ) {
        return;
      }

      const first = node.arguments?.[0];
      if (!first) return;
      if (
        (first.type === 'StringLiteral' || first.type === 'Literal') &&
        typeof first.value === 'string'
      ) {
        components.add(first.value);
      } else if (
        (first.type === 'MemberExpression' || first.type === 'StaticMemberExpression') &&
        first.property?.type === 'Identifier'
      ) {
        // Only Guest-module bindings (`import_guest.View`) describe capability
        // components. Local values such as `item.Component` or
        // `this.props.wrapper` must not leak into the manifest.
        const object = first.object;
        const objectName = object?.type === 'Identifier' ? object.name : undefined;
        if (objectName && /^import_guest\d*$/.test(objectName)) {
          components.add(first.property.name);
        }
      } else if (first.type === 'Identifier') {
        jsxIdentifiers.add(first.name);
      }
    });

    let changed = true;
    while (changed) {
      changed = false;
      for (const [name, source] of aliasSources) {
        if (!sdkAliases.has(name) && sdkAliases.has(source)) {
          sdkAliases.set(name, sdkAliases.get(source));
          changed = true;
        }
      }
    }

    for (const name of jsxIdentifiers) {
      const sdkName = sdkAliases.get(name);
      if (sdkName) {
        components.add(sdkName);
      } else if (!localComponents.has(name) && options.failClosed) {
        throw new Error(`Unable to establish component provenance for "${name}"`);
      }
    }
  } catch (err) {
    if (options.failClosed) throw err;
    console.error('[oxc-adapter] analyzeBundleComponents failed:', err.message);
  }

  return Array.from(components).sort();
}

function isHostServiceRequire(node) {
  if (!node || node.type !== 'CallExpression' || node.callee?.type !== 'Identifier') {
    return false;
  }
  if (node.callee.name === 'require') {
    const arg = node.arguments?.[0];
    return (
      (arg?.type === 'StringLiteral' || arg?.type === 'Literal') && arg.value === 'host:service'
    );
  }
  if (node.callee.name === '__toESM') {
    return isHostServiceRequire(node.arguments?.[0]);
  }
  return false;
}

function propertyName(property) {
  if (!property || (property.type !== 'Property' && property.type !== 'ObjectProperty')) {
    return undefined;
  }
  const key = property.key;
  if (key?.type === 'Identifier') return key.name;
  if ((key?.type === 'StringLiteral' || key?.type === 'Literal') && typeof key.value === 'string') {
    return key.value;
  }
  return undefined;
}

/**
 * Collect literal `host:service` routes and events from a lowered bundle.
 * Dynamic route/event values reject the build because the manifest cannot
 * describe which operation the Guest actually invokes.
 */
function analyzeServiceUsage(code) {
  if (!oxc || !oxc.parseSync) {
    throw new Error("oxc-parser not available. Please install it with 'npm i -D oxc-parser'.");
  }

  const serviceBindings = new Set();
  const serviceMethods = new Map();
  const rpc = new Set();
  const events = new Set();

  const result = oxc.parseSync(code, { sourceFilename: 'bundle.js' });
  const ast = typeof result.program === 'string' ? JSON.parse(result.program) : result.program;

  traverseAST(ast, (node) => {
    if (
      node.type === 'VariableDeclarator' &&
      node.id?.type === 'Identifier' &&
      isHostServiceRequire(node.init)
    ) {
      serviceBindings.add(node.id.name);
    }
    if (
      node.type === 'VariableDeclarator' &&
      node.id?.type === 'ObjectPattern' &&
      isHostServiceRequire(node.init)
    ) {
      for (const property of node.id.properties ?? []) {
        const method = propertyName(property);
        if (method && property.value?.type === 'Identifier') {
          serviceMethods.set(property.value.name, method);
        }
      }
    }
    if (
      node.type === 'VariableDeclarator' &&
      node.id?.type === 'Identifier' &&
      (node.init?.type === 'MemberExpression' || node.init?.type === 'StaticMemberExpression') &&
      node.init.object?.type === 'Identifier' &&
      serviceBindings.has(node.init.object.name) &&
      node.init.property?.type === 'Identifier'
    ) {
      serviceMethods.set(node.id.name, node.init.property.name);
    }

    if (node.type !== 'CallExpression') return;
    let method;
    if (
      (node.callee?.type === 'MemberExpression' ||
        node.callee?.type === 'StaticMemberExpression') &&
      node.callee.object?.type === 'Identifier' &&
      serviceBindings.has(node.callee.object.name) &&
      node.callee.property?.type === 'Identifier'
    ) {
      method = node.callee.property.name;
    } else if (node.callee?.type === 'Identifier') {
      method = serviceMethods.get(node.callee.name);
    }
    if (method !== 'call' && method !== 'tell' && method !== 'on') return;

    const argument = node.arguments?.[0];
    if (!argument || argument.type !== 'ObjectExpression') {
      throw new Error(`host:service.${method} requires an object literal`);
    }
    const key = method === 'on' ? 'event' : 'route';
    const property = (argument.properties ?? []).find(
      (candidate) => propertyName(candidate) === key
    );
    const value = property?.value;
    if (
      !property ||
      !(value?.type === 'StringLiteral' || value?.type === 'Literal') ||
      typeof value.value !== 'string'
    ) {
      throw new Error(`host:service.${method} requires a literal "${key}"`);
    }
    if (method === 'on') {
      events.add(value.value);
    } else {
      rpc.add(value.value);
    }
  });

  return { rpc: Array.from(rpc).sort(), events: Array.from(events).sort() };
}

/**
 * Component names imported from `keel/guest` and used in JSX.
 *
 * Local components (`<Panel/>`) are not capability components and must not be
 * validated against the Guest component allowlist. This analysis runs on the
 * entry source, so minification cannot hide the names.
 */
/**
 * Static import specifiers from a source module.
 *
 * moduleLexerSync does not reliably lex JSX/TSX sources, so this uses the
 * full parser to collect relative imports for the source module graph.
 */
function analyzeSourceImports(code) {
  if (!oxc || !oxc.parseSync) {
    throw new Error("oxc-parser not available. Please install it with 'npm i -D oxc-parser'.");
  }

  const imports = new Set();
  try {
    const result = oxc.parseSync(code, { sourceType: 'module', sourceFilename: 'source.tsx' });
    const ast = typeof result.program === 'string' ? JSON.parse(result.program) : result.program;

    traverseAST(ast, (node) => {
      if (
        (node.type === 'ImportDeclaration' ||
          node.type === 'ExportNamedDeclaration' ||
          node.type === 'ExportAllDeclaration') &&
        typeof node.source?.value === 'string'
      ) {
        imports.add(node.source.value);
      }
    });
  } catch (err) {
    console.error('[oxc-adapter] analyzeSourceImports failed:', err.message);
  }

  return Array.from(imports);
}

function analyzeSourceComponents(code) {
  if (!oxc || !oxc.parseSync) {
    throw new Error("oxc-parser not available. Please install it with 'npm i -D oxc-parser'.");
  }

  // Local JSX binding name -> imported SDK component name. Aliased imports
  // (`Modal as M`) must be resolved through this map, not compared directly.
  const componentBindings = new Map();
  const jsxNames = new Set();

  try {
    const result = oxc.parseSync(code, { sourceFilename: 'source.tsx' });
    const ast = typeof result.program === 'string' ? JSON.parse(result.program) : result.program;

    traverseAST(ast, (node) => {
      if (node.type === 'ImportDeclaration' && node.source?.value === 'keel/guest') {
        for (const specifier of node.specifiers ?? []) {
          const localName = specifier.local?.name;
          if (!localName) continue;
          componentBindings.set(localName, specifier.imported?.name ?? localName);
        }
      }
      if (node.type === 'JSXElement') {
        const localName = getJSXElementName(node.openingElement);
        const importedName = componentBindings.get(localName);
        if (importedName) {
          jsxNames.add(importedName);
        } else if (localName.includes('.')) {
          // Namespace imports (`import * as guest`) render `guest.View`.
          const [root, member] = localName.split('.');
          if (componentBindings.has(root) && member) {
            jsxNames.add(member);
          }
        }
      }
    });
  } catch (err) {
    console.error('[oxc-adapter] analyzeSourceComponents failed:', err.message);
  }

  return Array.from(jsxNames).sort();
}

module.exports = {
  analyzeModuleIDs,
  analyzeHostBoundary,
  analyzeJSXProps,
  analyzeBundleComponents,
  analyzeServiceUsage,
  analyzeSourceComponents,
  analyzeSourceImports,
};
