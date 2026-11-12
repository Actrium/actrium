// Oxc-only adapter for dependency scanning (CommonJS)
// Hard dependency on 'oxc-parser'. If not installed, this module will throw at runtime.

const oxc = require('oxc-parser');

const HOST_MODULE_PATTERN = /^host:[a-z0-9_-]+(?:\/[a-z0-9_-]+)*$/;

function isHostModuleId(value) {
  return typeof value === 'string' && HOST_MODULE_PATTERN.test(value);
}

// oxc distinguishes static (`a.b`) and computed (`a[b]`) member expressions.
// Both can read or write an object property, so the analyzers must treat them
// uniformly and fail closed when the computed key is not statically known.
function isMemberNode(node) {
  return (
    node?.type === 'MemberExpression' ||
    node?.type === 'StaticMemberExpression' ||
    node?.type === 'ComputedMemberExpression'
  );
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

  // Use regex to find require() calls with a plain single/double-quoted
  // module name. Template literals are classified through the AST below so an
  // interpolated template is not mistaken for a static dependency.
  const requirePattern = /\brequire\s*\(\s*(['"])([^'"]+)\1\s*\)/g;
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

  // Count require() calls whose argument is not a string literal. The v1
  // manifest cannot describe the resulting dependency.
  const nonLiteralRequirePattern = /\brequire\s*\(\s*([^'"`\s)])/g;
  while ((match = nonLiteralRequirePattern.exec(code)) !== null) {
    if (!isInCommentOrString(code, match.index)) {
      dynNonLiteral++;
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

  // `new Function(...)` is dynamic code evaluation as well.
  const dynamicFunctionPattern = /\bnew\s+Function\s*\(/g;
  while ((evalMatch = dynamicFunctionPattern.exec(code)) !== null) {
    if (!isInCommentOrString(code, evalMatch.index)) {
      evalCount++;
    }
  }

  // The regexes above miss require() arguments built with concatenation or
  // template literals, and dynamic evaluation reached without `eval(...)`
  // syntax (`Function(...)`, `(0, eval)(...)`). Use the AST for those shapes.
  try {
    const parsed = oxc.parseSync(code, { sourceFilename: 'bundle.js' });
    const ast = typeof parsed.program === 'string' ? JSON.parse(parsed.program) : parsed.program;
    const unwrap = (node) => {
      let current = node;
      while (current?.type === 'ParenthesizedExpression') current = current.expression;
      return current;
    };
    const globalObjectNames = new Set(['globalThis', 'global', 'window', 'self']);
    const globalAliases = new Set();
    const reflectAliases = new Set();
    const reflectMethodAliases = new Map();
    const stringAliases = new Map();
    const memberKeyOf = (node) => {
      if (!isMemberNode(node)) return undefined;
      if (!node.computed && node.property?.type === 'Identifier') return node.property.name;
      const keyNode = node.type === 'ComputedMemberExpression' ? node.expression : node.property;
      if (
        (keyNode?.type === 'StringLiteral' || keyNode?.type === 'Literal') &&
        typeof keyNode.value === 'string'
      ) {
        return keyNode.value;
      }
      if (keyNode?.type === 'Identifier' && stringAliases.has(keyNode.name)) {
        return stringAliases.get(keyNode.name);
      }
      return undefined;
    };
    const isGlobalObjectNode = (node) => {
      const value = unwrap(node);
      return (
        value?.type === 'Identifier' &&
        (globalObjectNames.has(value.name) || globalAliases.has(value.name))
      );
    };
    const isReflectNode = (node) => {
      const value = unwrap(node);
      return (
        value?.type === 'Identifier' && (value.name === 'Reflect' || reflectAliases.has(value.name))
      );
    };
    const aliasNames = new Set();
    const isDynamicValue = (node) => {
      const value = unwrap(node);
      if (!value) return false;
      if (value.type === 'Identifier') {
        return value.name === 'Function' || value.name === 'eval' || aliasNames.has(value.name);
      }
      if (isMemberNode(value)) {
        const key = memberKeyOf(value);
        return isGlobalObjectNode(value.object) && (key === 'Function' || key === 'eval');
      }
      if (value.type === 'SequenceExpression') {
        const last = value.expressions?.[value.expressions.length - 1];
        return isDynamicValue(last);
      }
      return false;
    };
    // Collect aliases of global objects, Reflect, Function and eval (including
    // alias chains). Repeat a few passes so declaration order does not matter.
    for (let pass = 0; pass < 6; pass++) {
      let changed = false;
      const collectAliases = (node) => {
        if (!node || typeof node !== 'object') return;
        if (Array.isArray(node)) {
          for (const child of node) collectAliases(child);
          return;
        }
        if (node.type === 'VariableDeclarator' && node.id?.type === 'Identifier' && node.init) {
          const name = node.id.name;
          const initValue = unwrap(node.init);
          if (initValue?.type === 'Identifier') {
            if (
              (globalObjectNames.has(initValue.name) || globalAliases.has(initValue.name)) &&
              !globalAliases.has(name)
            ) {
              globalAliases.add(name);
              changed = true;
            }
            if (
              (initValue.name === 'Reflect' || reflectAliases.has(initValue.name)) &&
              !reflectAliases.has(name)
            ) {
              reflectAliases.add(name);
              changed = true;
            }
          }
          if (
            (initValue?.type === 'StringLiteral' || initValue?.type === 'Literal') &&
            typeof initValue.value === 'string'
          ) {
            if (stringAliases.get(name) !== initValue.value) {
              stringAliases.set(name, initValue.value);
              changed = true;
            }
          } else if (initValue?.type === 'Identifier' && stringAliases.has(initValue.name)) {
            const stringValue = stringAliases.get(initValue.name);
            if (stringAliases.get(name) !== stringValue) {
              stringAliases.set(name, stringValue);
              changed = true;
            }
          }
          if (isMemberNode(initValue) && isReflectNode(initValue.object)) {
            const method = memberKeyOf(initValue);
            if (
              (method === 'construct' || method === 'apply') &&
              reflectMethodAliases.get(name) !== method
            ) {
              reflectMethodAliases.set(name, method);
              changed = true;
            }
          }
          if (isDynamicValue(node.init) && !aliasNames.has(name)) {
            aliasNames.add(name);
            changed = true;
          }
        }
        for (const key of Object.keys(node)) {
          if (
            key === 'type' ||
            key === 'start' ||
            key === 'end' ||
            key === 'loc' ||
            key === 'range'
          ) {
            continue;
          }
          const value = node[key];
          if (value && typeof value === 'object') collectAliases(value);
        }
      };
      collectAliases(ast);
      if (!changed) break;
    }
    const visit = (node) => {
      if (!node || typeof node !== 'object') return;
      if (Array.isArray(node)) {
        for (const child of node) visit(child);
        return;
      }
      if (node.type === 'CallExpression') {
        const callee = unwrap(node.callee);
        const arg = unwrap(node.arguments?.[0]);
        if (callee?.type === 'Identifier' && callee.name === 'require') {
          if (arg?.type === 'TemplateLiteral') {
            if ((arg.expressions?.length ?? 0) > 0) {
              dynNonLiteral++;
            } else {
              foundStatic.add(
                (arg.quasis ?? [])
                  .map((quasi) => quasi.value?.cooked ?? quasi.value?.raw ?? '')
                  .join('')
              );
            }
          } else if (
            arg &&
            (arg.type === 'BinaryExpression' ||
              arg.type === 'LogicalExpression' ||
              arg.type === 'ConditionalExpression')
          ) {
            dynNonLiteral++;
          }
        }
        const calleeKey = memberKeyOf(callee);
        // Direct `eval(...)` is already counted by the regex; count every other
        // evaluator reference, including aliases and global-object members.
        // The regex only matches `eval(`; optional calls (`eval?.(`) must be
        // counted by the AST instead of being excluded as already counted.
        const regexCountedEval =
          !node.optional &&
          ((callee?.type === 'Identifier' && callee.name === 'eval') ||
            (isMemberNode(callee) && isGlobalObjectNode(callee.object) && calleeKey === 'eval'));
        if (!regexCountedEval && isDynamicValue(callee)) evalCount++;
        const reflectMethod =
          callee?.type === 'Identifier' ? reflectMethodAliases.get(callee.name) : undefined;
        if (reflectMethod && isDynamicValue(node.arguments?.[0])) evalCount++;
        // Reflect.construct/apply(Function|globalThis.Function|alias, ...)
        if (
          isMemberNode(callee) &&
          isReflectNode(callee.object) &&
          (calleeKey === 'construct' || calleeKey === 'apply') &&
          isDynamicValue(node.arguments?.[0])
        ) {
          evalCount++;
        }
      }
      if (node.type === 'NewExpression') {
        const callee = unwrap(node.callee);
        const isBareFunction = callee?.type === 'Identifier' && callee.name === 'Function';
        // `new Function(...)` is already counted by the regex.
        if (isDynamicValue(callee) && !isBareFunction) evalCount++;
      }
      for (const key of Object.keys(node)) {
        if (
          key === 'type' ||
          key === 'start' ||
          key === 'end' ||
          key === 'loc' ||
          key === 'range'
        ) {
          continue;
        }
        const value = node[key];
        if (value && typeof value === 'object') visit(value);
      }
    };
    visit(ast);
  } catch (err) {
    console.warn('[oxc-adapter] dynamic-code scan failed:', err.message);
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

const FRAMEWORK_MODULES = new Set([
  'react',
  'react/jsx-runtime',
  'react/jsx-dev-runtime',
  'react-native',
]);

function isFrameworkRequire(node) {
  if (!node || node.type !== 'CallExpression' || node.callee?.type !== 'Identifier') {
    return false;
  }
  if (node.callee.name === 'require') {
    const arg = node.arguments?.[0];
    return (
      (arg?.type === 'StringLiteral' || arg?.type === 'Literal') &&
      typeof arg.value === 'string' &&
      FRAMEWORK_MODULES.has(arg.value)
    );
  }
  if (node.callee.name === '__toESM') {
    return isFrameworkRequire(node.arguments?.[0]);
  }
  return false;
}

/**
 * Extract component names from a transpiled bundle.
 *
 * JSX is already lowered to `jsx(...)` / `jsxs(...)` / `createElement(...)`
 * calls. The component can be a string tag, an `import_guest.View` member, or
 * a local identifier aliasing an SDK component (`const M = Modal; <M />`).
 * Bindings are resolved lexically so a parameter or local variable shadows an
 * outer component with the same name. When `failClosed` is set, identifiers
 * whose provenance cannot be resolved reject the build.
 */
function analyzeBundleComponents(code, options = {}) {
  if (!oxc || !oxc.parseSync) {
    throw new Error("oxc-parser not available. Please install it with 'npm i -D oxc-parser'.");
  }

  const components = new Set();

  const failClosed = (message) => {
    if (options.failClosed) {
      throw new Error(message);
    }
  };

  const createScope = (parent, isFunction = false) => ({
    parent,
    isFunction,
    bindings: new Map(),
    branchRoot: parent?.branchRoot ?? null,
  });

  // oxc emits some list nodes (for example function params) as `{ items }`
  // wrappers instead of plain arrays.
  const asArray = (value) =>
    Array.isArray(value) ? value : Array.isArray(value?.items) ? value.items : [];

  const declare = (scope, name, binding) => {
    scope.bindings.set(name, binding);
  };

  const assignBinding = (scope, name, binding) => {
    let current = scope;
    while (current) {
      if (current.bindings.has(name)) break;
      current = current.parent;
    }
    const branchRoot = scope.branchRoot;
    if (current) {
      // A branch may not mutate a binding that lives outside the branch's
      // cloned scope; shadow it in the branch root so joinBranches can merge
      // the branch states.
      if (branchRoot && current !== branchRoot && current.branchRoot !== branchRoot) {
        branchRoot.bindings.set(name, binding);
      } else {
        current.bindings.set(name, binding);
      }
      return;
    }
    (branchRoot ?? scope).bindings.set(name, binding);
  };

  const nearestFunctionScope = (scope) => {
    let current = scope;
    while (current && !current.isFunction) current = current.parent;
    return current ?? scope;
  };

  const resolveBindingValue = (binding, seen) => {
    if (!binding) return undefined;
    if (binding.kind === 'alias') {
      // Alias bindings created before the snapshot model still resolve lazily;
      // use the binding object itself as the cycle identity (alias bindings do
      // not have a stable `name`).
      if (seen.has(binding)) return { kind: 'unknown' };
      const nextSeen = new Set(seen);
      nextSeen.add(binding);
      return resolveBinding(binding.scope, binding.source, nextSeen);
    }
    if (binding.kind === 'memberAlias') {
      const objectBinding = resolveBinding(binding.scope, binding.objectName, seen);
      if (objectBinding?.kind === 'object') {
        const property = objectBinding.props.get(binding.propertyName);
        if (property) return resolveBindingValue(property, seen);
      }
      return { kind: 'unknown' };
    }
    return binding;
  };

  const resolveBinding = (scope, name, seen = new Set()) => {
    let current = scope;
    while (current) {
      if (current.bindings.has(name)) {
        return resolveBindingValue(current.bindings.get(name), seen);
      }
      current = current.parent;
    }
    return undefined;
  };

  const patternNames = (node) => {
    if (!node) return [];
    if (node.type === 'Identifier') return [node.name];
    if (node.type === 'RestElement') return patternNames(node.argument);
    if (node.type === 'FormalParameter') return patternNames(node.pattern);
    if (node.type === 'AssignmentPattern') return patternNames(node.left);
    if (node.type === 'ObjectPattern' || node.type === 'ArrayPattern') {
      const names = [];
      const items = asArray(node.properties).length
        ? asArray(node.properties)
        : asArray(node.elements);
      for (const property of items) {
        if (!property) continue;
        if (property.type === 'Property' || property.type === 'ObjectProperty') {
          names.push(...patternNames(property.value));
        } else {
          names.push(...patternNames(property));
        }
      }
      return names;
    }
    return [];
  };

  const assignmentTargetNames = (node) => {
    if (!node) return [];
    if (node.type === 'Identifier' || node.type === 'AssignmentTargetIdentifier') {
      return node.name ? [node.name] : [];
    }
    if (node.type === 'ObjectAssignmentTarget') {
      const names = [];
      for (const property of asArray(node.properties)) {
        if (property.type === 'AssignmentTargetPropertyIdentifier') {
          if (property.binding?.name) names.push(property.binding.name);
        } else if (property.type === 'AssignmentTargetPropertyProperty') {
          names.push(...assignmentTargetNames(property.binding));
        } else {
          names.push(...assignmentTargetNames(property));
        }
      }
      return names;
    }
    if (node.type === 'ArrayAssignmentTarget') {
      const names = [];
      for (const element of asArray(node.elements)) {
        names.push(...assignmentTargetNames(element));
      }
      return names;
    }
    if (node.type === 'AssignmentTargetWithDefault') {
      return assignmentTargetNames(node.binding);
    }
    if (node.type === 'AssignmentTargetRest') {
      return assignmentTargetNames(node.target);
    }
    return [];
  };

  const isComponentWrapperCall = (node) => {
    if (node?.type !== 'CallExpression') return false;
    const callee = node.callee;
    const name =
      callee?.type === 'Identifier'
        ? callee.name
        : callee?.property?.type === 'Identifier'
          ? callee.property.name
          : undefined;
    return name === 'memo' || name === 'forwardRef';
  };

  // React element factories can be aliased (`const h = createElement`). Track
  // their provenance so a factory alias still enters the component analysis.
  const JSX_FACTORY_NAMES = new Set(['jsx', 'jsxs', 'jsxDEV', 'createElement']);
  // React internals are not SDK capability components; Fragment/StrictMode/
  // Suspense must stay renderable without entering the allowlist.
  const REACT_INTERNAL_NAMES = new Set(['Fragment', 'StrictMode', 'Suspense']);

  const isJsxFactoryName = (name) => JSX_FACTORY_NAMES.has(name);
  const isReactInternalName = (name) => REACT_INTERNAL_NAMES.has(name);

  const memberPropertyName = (node, scope) => {
    if (!node) return undefined;
    const keyNode = node.type === 'ComputedMemberExpression' ? node.expression : node.property;
    if (node.type === 'ComputedMemberExpression' || node.computed) {
      if (!keyNode) return undefined;
      if (
        (keyNode.type === 'StringLiteral' || keyNode.type === 'Literal') &&
        typeof keyNode.value === 'string'
      ) {
        return keyNode.value;
      }
      if (keyNode.type === 'TemplateLiteral' && (keyNode.expressions?.length ?? 0) === 0) {
        return (keyNode.quasis ?? [])
          .map((quasi) => quasi.value?.cooked ?? quasi.value?.raw ?? '')
          .join('');
      }
      if (keyNode.type === 'Identifier') {
        const binding = resolveBinding(scope, keyNode.name);
        if (binding?.kind === 'literal' && typeof binding.value === 'string') {
          return binding.value;
        }
      }
      return undefined;
    }
    if (keyNode?.type === 'Identifier') return keyNode.name;
    if (
      (keyNode?.type === 'StringLiteral' || keyNode?.type === 'Literal') &&
      typeof keyNode.value === 'string'
    ) {
      return keyNode.value;
    }
    return undefined;
  };

  const bindingFromMember = (init, scope) => {
    const property = memberPropertyName(init, scope);
    if (property === undefined) return { kind: 'unknown' };
    const objectName = init.object?.type === 'Identifier' ? init.object.name : undefined;
    const objectBinding = objectName ? resolveBinding(scope, objectName) : undefined;
    // Trust the bundler namespace name unless the guest defined a local
    // object with that name; `const import_guest = { View: Modal }` must be
    // analyzed as an object.
    if (objectBinding?.kind !== 'object' && objectName && /^import_guest\d*$/.test(objectName)) {
      return { kind: 'sdk', name: property };
    }
    const isReactNamespace =
      objectBinding?.kind === 'framework' ||
      (objectBinding === undefined && /^import_react\d*$/.test(objectName ?? ''));
    if (isReactNamespace && isJsxFactoryName(property)) return { kind: 'jsxFactory' };
    if (isReactNamespace && isReactInternalName(property)) return { kind: 'framework' };
    // Snapshot the member value at assignment time. A later mutation of the
    // source object must not change what this alias refers to, and an arbitrary
    // object member named `Fragment` is not the React internal.
    if (objectBinding?.kind === 'object') {
      const propertyBinding = objectBinding.props.get(property);
      if (propertyBinding) {
        return resolveBindingValue(propertyBinding, new Set()) ?? { kind: 'unknown' };
      }
    }
    return { kind: 'unknown' };
  };

  const cloneBinding = (binding, seen = new Map()) => {
    if (!binding || binding.kind !== 'object') return binding;
    if (seen.has(binding)) return seen.get(binding);
    const clone = { kind: 'object', props: new Map(), unknown: Boolean(binding.unknown) };
    seen.set(binding, clone);
    for (const [key, value] of binding.props) {
      clone.props.set(key, cloneBinding(value, seen));
    }
    return clone;
  };

  const cloneScope = (scope) => {
    const bindings = new Map();
    // Share one seen map so two names that reference the same object keep
    // aliasing it after the clone.
    const seen = new Map();
    for (const [name, binding] of scope.bindings) {
      bindings.set(name, cloneBinding(binding, seen));
    }
    const clone = {
      parent: scope.parent,
      isFunction: scope.isFunction,
      bindings,
      branchRoot: null,
    };
    clone.branchRoot = clone;
    return clone;
  };

  const bindingsEqual = (a, b, seen = new Set()) => {
    if (a === b) return true;
    if (!a || !b || a.kind !== b.kind) return false;
    if (a.kind === 'object') {
      if (seen.has(a)) return true;
      seen.add(a);
      if (Boolean(a.unknown) !== Boolean(b.unknown)) return false;
      if (a.props.size !== b.props.size) return false;
      for (const [key, value] of a.props) {
        if (!b.props.has(key) || !bindingsEqual(value, b.props.get(key), seen)) return false;
      }
      return true;
    }
    if (a.kind === 'memberAlias') {
      return a.objectName === b.objectName && a.propertyName === b.propertyName;
    }
    if (a.kind === 'literal') return a.value === b.value;
    if (a.kind === 'sdk') return a.name === b.name;
    return true;
  };

  const collectCandidates = (binding, into = new Set(), seen = new Set()) => {
    if (!binding) return into;
    if (binding.kind === 'sdk') {
      into.add(binding.name);
    } else if (binding.kind === 'object' && !binding.unknown) {
      if (seen.has(binding)) return into;
      seen.add(binding);
      for (const value of binding.props.values()) collectCandidates(value, into, seen);
    }
    return into;
  };

  // Join branch states conservatively: keep a binding only when every path
  // agrees, otherwise mark it unknown so the use site fails closed instead of
  // accepting whichever branch happened to be visited last. Only bindings that
  // already exist in the outer scope are joined; block-scoped declarations stay
  // inside their branch.
  const joinBranches = (scope, branches) => {
    // Include bindings created (or shadowed) inside the branches; a `var`
    // declared in a conditional still belongs to the enclosing function.
    const names = new Set(scope.bindings.keys());
    for (const branch of branches) {
      for (const name of branch.bindings.keys()) names.add(name);
    }
    for (const name of names) {
      const values = branches.map((branch) =>
        branch.bindings.has(name) ? branch.bindings.get(name) : { kind: 'unknown' }
      );
      const first = values[0];
      if (values.every((value) => bindingsEqual(first, value))) {
        scope.bindings.set(name, first);
      } else {
        const candidates = new Set();
        for (const value of values) collectCandidates(value, candidates);
        scope.bindings.set(
          name,
          candidates.size
            ? { kind: 'unknown', candidates: Array.from(candidates).sort() }
            : { kind: 'unknown' }
        );
      }
    }
  };

  const bindingFromObject = (init, scope) => {
    const props = new Map();
    let unknown = false;
    for (const property of asArray(init.properties)) {
      if (property.type === 'SpreadElement' || property.type === 'SpreadProperty') {
        const spread = bindingFromInit(property.argument, scope);
        if (spread?.kind !== 'object') {
          // The spread can override every earlier property.
          props.clear();
          unknown = true;
          continue;
        }
        if (spread.unknown) {
          props.clear();
          unknown = true;
        }
        for (const [key, value] of spread.props) props.set(key, value);
        continue;
      }
      if (property.kind && property.kind !== 'init') {
        props.clear();
        unknown = true;
        continue;
      }
      const key = propertyName(property);
      if (key === undefined) {
        // A computed key can override every earlier property, but a later
        // literal key re-establishes a known value.
        props.clear();
        unknown = true;
        continue;
      }
      props.set(key, bindingFromInit(property.value, scope));
    }
    return { kind: 'object', props, unknown };
  };

  const bindingFromInit = (init, scope) => {
    if (!init) return { kind: 'unknown' };
    if (
      init.type === 'ArrowFunctionExpression' ||
      init.type === 'FunctionExpression' ||
      init.type === 'ClassExpression'
    ) {
      return { kind: 'local' };
    }
    if (isFrameworkRequire(init)) {
      return { kind: 'framework' };
    }
    if (isComponentWrapperCall(init)) {
      const wrapped = asArray(init.arguments)[0];
      return wrapped ? bindingFromInit(wrapped, scope) : { kind: 'unknown' };
    }
    if (init.type === 'Identifier') {
      // A local binding wins over the React-internal name heuristic; only an
      // unresolved name is treated as a factory/internal.
      const resolved = resolveBinding(scope, init.name);
      if (resolved) return resolved;
      if (isJsxFactoryName(init.name)) return { kind: 'jsxFactory' };
      if (isReactInternalName(init.name)) return { kind: 'framework' };
      return { kind: 'unknown' };
    }
    if (
      (init.type === 'StringLiteral' || init.type === 'Literal') &&
      typeof init.value === 'string'
    ) {
      return { kind: 'literal', value: init.value };
    }
    if (isMemberNode(init)) {
      return bindingFromMember(init, scope);
    }
    if (init.type === 'ObjectExpression') {
      return bindingFromObject(init, scope);
    }
    return { kind: 'unknown' };
  };

  const isJsxCall = (node, scope) => {
    if (node?.type !== 'CallExpression') return false;
    const callee = node.callee;
    if (callee?.type === 'Identifier') {
      if (isJsxFactoryName(callee.name)) return true;
      return resolveBinding(scope, callee.name)?.kind === 'jsxFactory';
    }
    if (isMemberNode(callee)) {
      const property = memberPropertyName(callee, scope);
      if (property !== undefined && isJsxFactoryName(property)) return true;
      if (callee.object?.type === 'Identifier') {
        const objectBinding = resolveBinding(scope, callee.object.name);
        if (objectBinding?.kind === 'jsxFactory') return true;
      }
    }
    return false;
  };

  const handleJsx = (node, scope) => {
    const first = asArray(node.arguments)[0];
    if (!first) return;

    if (
      (first.type === 'StringLiteral' || first.type === 'Literal') &&
      typeof first.value === 'string'
    ) {
      components.add(first.value);
      return;
    }

    if (first.type === 'Identifier') {
      const binding = resolveBinding(scope, first.name);
      if (binding?.kind === 'sdk') {
        components.add(binding.name);
      } else if (
        binding?.kind === 'local' ||
        binding?.kind === 'framework' ||
        binding?.kind === 'jsxFactory'
      ) {
        // Local components, React internals and element factories are not
        // SDK capability components.
      } else {
        const candidates = binding?.candidates?.length
          ? ` (possible: ${binding.candidates.join(', ')})`
          : '';
        failClosed(`Unable to establish component provenance for "${first.name}"${candidates}`);
      }
      return;
    }

    if (isMemberNode(first)) {
      const property = memberPropertyName(first, scope);
      if (property === undefined) {
        failClosed('Unable to establish component provenance for a dynamic JSX component');
        return;
      }
      // React internals (`import_react.Fragment`) stay renderable but are not
      // SDK capability components. Only trust the internal name when it comes
      // from the React namespace; `<bag.Fragment />` on an unknown object must
      // still fail closed.
      const objectName = first.object?.type === 'Identifier' ? first.object.name : undefined;
      const objectBinding = objectName ? resolveBinding(scope, objectName) : undefined;
      // Trust the bundler namespace name unless the guest defined a local
      // object with that name; `const import_guest = { View: Modal }` must be
      // analyzed as an object.
      if (objectBinding?.kind !== 'object' && objectName && /^import_guest\d*$/.test(objectName)) {
        components.add(property);
        return;
      }
      // Trust the `import_react*` name only when the variable has no local
      // binding; a guest-defined object with that name must be analyzed.
      const isReactNamespace =
        objectBinding?.kind === 'framework' ||
        (objectBinding === undefined && /^import_react\d*$/.test(objectName ?? ''));
      if (isReactInternalName(property) && isReactNamespace) {
        return;
      }
      if (
        isJsxFactoryName(property) &&
        (isReactNamespace || objectBinding?.kind === 'jsxFactory')
      ) {
        return;
      }

      if (objectBinding?.kind === 'object') {
        const propertyBinding = resolveBindingValue(objectBinding.props.get(property), new Set());
        if (propertyBinding?.kind === 'sdk') {
          components.add(propertyBinding.name);
          return;
        }
        if (propertyBinding?.kind === 'local') return;
      }
      if (objectBinding?.kind === 'framework') {
        return;
      }
      let root = first.object;
      while (root && isMemberNode(root)) {
        root = root.object;
      }
      if (root?.type === 'Identifier' && resolveBinding(scope, root.name)?.kind === 'framework') {
        return;
      }
      failClosed(
        `Unable to establish component provenance for "${objectName ?? 'member'}.${property}"`
      );
      return;
    }

    failClosed('Unable to establish component provenance for a dynamic JSX component');
  };

  const declarePattern = (pattern, initBinding, targetScope) => {
    if (
      pattern?.type === 'ObjectPattern' &&
      initBinding?.kind === 'object' &&
      !initBinding.unknown
    ) {
      for (const property of asArray(pattern.properties)) {
        if (!property) continue;
        if (property.type === 'RestElement') {
          for (const name of patternNames(property)) {
            declare(targetScope, name, { kind: 'unknown' });
          }
          continue;
        }
        const key = propertyName(property);
        const value = key === undefined ? undefined : initBinding.props.get(key);
        if (key === undefined || !value || property.value?.type !== 'Identifier') {
          for (const name of patternNames(property.value ?? property)) {
            declare(targetScope, name, { kind: 'unknown' });
          }
          continue;
        }
        declare(targetScope, property.value.name, resolveBindingValue(value, new Set()));
      }
      return;
    }
    for (const name of patternNames(pattern)) {
      declare(targetScope, name, { kind: 'unknown' });
    }
  };

  const hoistDeclarations = (statements, scope) => {
    for (const statement of asArray(statements)) {
      let declaration = statement;
      if (
        statement?.type === 'ExportNamedDeclaration' ||
        statement?.type === 'ExportDefaultDeclaration'
      ) {
        declaration = statement.declaration;
      }
      if (!declaration) continue;
      if (declaration.type === 'FunctionDeclaration' && declaration.id?.name) {
        declare(scope, declaration.id.name, { kind: 'local' });
      } else if (declaration.type === 'ClassDeclaration' && declaration.id?.name) {
        declare(scope, declaration.id.name, { kind: 'local' });
      } else if (declaration.type === 'VariableDeclaration') {
        const targetScope = declaration.kind === 'var' ? nearestFunctionScope(scope) : scope;
        for (const declarator of asArray(declaration.declarations)) {
          const binding = bindingFromInit(declarator.init, scope);
          if (declarator.id?.type === 'Identifier') {
            declare(targetScope, declarator.id.name, binding);
          } else {
            declarePattern(declarator.id, binding, targetScope);
          }
        }
      }
    }
  };

  // A destructuring assignment can target a member expression
  // (`({ C: bag.C } = value)`); the member write can no longer be tracked, so
  // taint the root object and let the use site fail closed.
  const taintMemberAssignmentTargets = (node, scope) => {
    if (!node || typeof node !== 'object') return;
    if (isMemberNode(node)) {
      let root = node.object;
      while (root && isMemberNode(root)) {
        root = root.object;
      }
      if (root?.type === 'Identifier') {
        const binding = resolveBinding(scope, root.name);
        if (binding?.kind === 'object') {
          binding.unknown = true;
          binding.props.clear();
        }
      }
      return;
    }
    for (const key of Object.keys(node)) {
      if (key === 'type' || key === 'start' || key === 'end' || key === 'loc' || key === 'range') {
        continue;
      }
      const value = node[key];
      if (value && typeof value === 'object') taintMemberAssignmentTargets(value, scope);
    }
  };

  const walk = (node, scope) => {
    if (!node || typeof node !== 'object') return;
    if (Array.isArray(node)) {
      for (const child of node) walk(child, scope);
      return;
    }

    switch (node.type) {
      case 'Program': {
        const programScope = createScope(scope);
        hoistDeclarations(node.body, programScope);
        for (const statement of asArray(node.body)) walk(statement, programScope);
        return;
      }
      case 'BlockStatement': {
        const blockScope = createScope(scope);
        hoistDeclarations(node.body, blockScope);
        for (const statement of asArray(node.body)) walk(statement, blockScope);
        return;
      }
      case 'FunctionDeclaration':
      case 'FunctionExpression':
      case 'ArrowFunctionExpression': {
        const functionScope = createScope(scope, true);
        if (node.type === 'FunctionDeclaration' && node.id?.name) {
          declare(scope, node.id.name, { kind: 'local' });
        } else if (node.type === 'FunctionExpression' && node.id?.name) {
          declare(functionScope, node.id.name, { kind: 'local' });
        }
        for (const parameter of asArray(node.params)) {
          for (const name of patternNames(parameter)) {
            declare(functionScope, name, { kind: 'unknown' });
          }
        }
        if (node.body?.type === 'BlockStatement') {
          hoistDeclarations(node.body.body, functionScope);
          for (const statement of asArray(node.body.body)) walk(statement, functionScope);
        } else {
          walk(node.body, functionScope);
        }
        return;
      }
      case 'VariableDeclaration': {
        const targetScope = node.kind === 'var' ? nearestFunctionScope(scope) : scope;
        for (const declaration of asArray(node.declarations)) {
          const binding = bindingFromInit(declaration.init, scope);
          if (declaration.id?.type === 'Identifier') {
            declare(targetScope, declaration.id.name, binding);
          } else {
            declarePattern(declaration.id, binding, targetScope);
          }
          walk(declaration.init, scope);
        }
        return;
      }
      case 'IfStatement': {
        walk(node.test, scope);
        // Analyze each path in its own cloned state and join conservatively.
        // A conditional write must not silently replace the binding used by
        // the code after the branch.
        const consequentScope = cloneScope(scope);
        walk(node.consequent, consequentScope);
        const alternateScope = node.alternate ? cloneScope(scope) : scope;
        if (node.alternate) walk(node.alternate, alternateScope);
        joinBranches(scope, [consequentScope, alternateScope]);
        return;
      }
      case 'SwitchStatement': {
        walk(node.discriminant, scope);
        const caseScopes = [];
        // A case without a trailing `break` falls through, so each case starts
        // from the previous case's end state. Joining the last state with the
        // pre-switch state keeps the no-match path conservative.
        let previousScope = scope;
        for (const switchCase of asArray(node.cases)) {
          const caseScope = cloneScope(previousScope);
          walk(switchCase.test, caseScope);
          for (const statement of asArray(switchCase.consequent)) {
            walk(statement, caseScope);
          }
          caseScopes.push(caseScope);
          previousScope = caseScope;
        }
        // Include the no-match path so a missing `default` also fails closed.
        caseScopes.push(scope);
        joinBranches(scope, caseScopes);
        return;
      }
      case 'ForStatement':
      case 'ForInStatement':
      case 'ForOfStatement':
      case 'WhileStatement':
      case 'DoWhileStatement': {
        // Loop bodies may execute zero or more times; join the body state with
        // the pre-loop state so a conditional write cannot escape the loop.
        const loopScope = cloneScope(scope);
        walk(node.init, loopScope);
        walk(node.test, loopScope);
        walk(node.update, loopScope);
        walk(node.left, loopScope);
        walk(node.right, loopScope);
        walk(node.body, loopScope);
        joinBranches(scope, [loopScope, scope]);
        return;
      }
      case 'ClassDeclaration': {
        if (node.id?.name) declare(scope, node.id.name, { kind: 'local' });
        walk(node.superClass, scope);
        walk(node.body, scope);
        return;
      }
      case 'AssignmentExpression': {
        const valueBinding =
          node.operator === '=' ? bindingFromInit(node.right, scope) : { kind: 'unknown' };
        if (node.left?.type === 'Identifier') {
          assignBinding(scope, node.left.name, valueBinding);
        } else if (
          node.left?.type === 'ObjectPattern' ||
          node.left?.type === 'ArrayPattern' ||
          node.left?.type === 'ObjectAssignmentTarget' ||
          node.left?.type === 'ArrayAssignmentTarget'
        ) {
          // Destructuring reassignment can replace SDK bindings; fail closed.
          for (const name of assignmentTargetNames(node.left)) {
            assignBinding(scope, name, { kind: 'unknown' });
          }
          taintMemberAssignmentTargets(node.left, scope);
        } else if (isMemberNode(node.left)) {
          const objectName =
            node.left.object?.type === 'Identifier' ? node.left.object.name : undefined;
          const key = memberPropertyName(node.left, scope);
          if (objectName) {
            const objectBinding = resolveBinding(scope, objectName);
            if (objectBinding?.kind === 'object') {
              if (key === undefined) {
                // A dynamic member write makes the object's contents unknown.
                objectBinding.unknown = true;
                objectBinding.props.clear();
              } else {
                objectBinding.props.set(key, valueBinding);
              }
            }
          } else {
            // Member write on a nested object (`a.b.C = ...`): taint the root.
            let root = node.left.object;
            while (root && isMemberNode(root)) {
              root = root.object;
            }
            if (root?.type === 'Identifier') {
              const rootBinding = resolveBinding(scope, root.name);
              if (rootBinding?.kind === 'object') {
                rootBinding.unknown = true;
                rootBinding.props.clear();
              }
            }
          }
        }
        walk(node.left, scope);
        walk(node.right, scope);
        return;
      }
      case 'CallExpression': {
        if (isJsxCall(node, scope)) {
          handleJsx(node, scope);
          for (const argument of node.arguments ?? []) walk(argument, scope);
          return;
        }
        break;
      }
      default:
        break;
    }

    for (const key of Object.keys(node)) {
      if (key === 'type' || key === 'start' || key === 'end' || key === 'loc' || key === 'range') {
        continue;
      }
      const value = node[key];
      if (value && typeof value === 'object') walk(value, scope);
    }
  };

  try {
    const result = oxc.parseSync(code, { sourceFilename: 'bundle.js' });
    const ast = typeof result.program === 'string' ? JSON.parse(result.program) : result.program;
    walk(ast, createScope(null));
  } catch (err) {
    if (options.failClosed) throw err;
    console.error('[oxc-adapter] analyzeBundleComponents failed:', err.message);
  }

  return Array.from(components).sort();
}

function isHostServiceRequire(node) {
  if (!node || node.type !== 'CallExpression') {
    return false;
  }
  const callee = node.callee;
  const calleeName =
    callee?.type === 'Identifier'
      ? callee.name
      : isMemberNode(callee) && callee.property?.type === 'Identifier'
        ? callee.property.name
        : undefined;
  if (calleeName === '__toESM') {
    return isHostServiceRequire(node.arguments?.[0]);
  }
  // `require('host:service')` in CJS and
  // `globalThis.__keel_importHostModule('host:service')` after the host
  // module rewrite both refer to the same service namespace.
  if (calleeName !== 'require' && calleeName !== '__keel_importHostModule') {
    return false;
  }
  const arg = node.arguments?.[0];
  return (arg?.type === 'StringLiteral' || arg?.type === 'Literal') && arg.value === 'host:service';
}

function propertyName(property) {
  if (!property || (property.type !== 'Property' && property.type !== 'ObjectProperty')) {
    return undefined;
  }
  const key = property.key;
  // A computed key is only statically known when it is a string literal.
  // Returning the identifier spelling for `[key]` would turn a dynamic
  // override into a literal key and let it bypass the manifest checks.
  if (property.computed) {
    return (key?.type === 'StringLiteral' || key?.type === 'Literal') &&
      typeof key.value === 'string'
      ? key.value
      : undefined;
  }
  if (key?.type === 'Identifier') return key.name;
  if ((key?.type === 'StringLiteral' || key?.type === 'Literal') && typeof key.value === 'string') {
    return key.value;
  }
  return undefined;
}

/**
 * Collect literal `host:service` routes and events from a lowered bundle.
 * Calls whose provenance cannot be established and dynamic route/event values
 * reject the build because the manifest cannot describe the operation.
 */
function analyzeServiceUsage(code) {
  if (!oxc || !oxc.parseSync) {
    throw new Error("oxc-parser not available. Please install it with 'npm i -D oxc-parser'.");
  }

  const SERVICE_METHOD_NAMES = new Set(['call', 'tell', 'on']);
  const UNKNOWN = Object.freeze({ kind: 'unknown' });
  const SERVICE = Object.freeze({ kind: 'service' });

  const rpc = new Set();
  const events = new Set();

  const asArray = (value) =>
    Array.isArray(value) ? value : Array.isArray(value?.items) ? value.items : [];

  const createScope = (parent, isFunction = false) => ({
    parent,
    isFunction,
    bindings: new Map(),
    branchRoot: parent?.branchRoot ?? null,
  });
  const declare = (scope, name, value) => {
    scope.bindings.set(name, value);
  };
  const nearestFunctionScope = (scope) => {
    let current = scope;
    while (current && !current.isFunction) current = current.parent;
    return current ?? scope;
  };
  const assignBinding = (scope, name, value) => {
    let current = scope;
    while (current) {
      if (current.bindings.has(name)) break;
      current = current.parent;
    }
    const branchRoot = scope.branchRoot;
    if (current) {
      if (branchRoot && current !== branchRoot && current.branchRoot !== branchRoot) {
        branchRoot.bindings.set(name, value);
      } else {
        current.bindings.set(name, value);
      }
      return;
    }
    (branchRoot ?? scope).bindings.set(name, value);
  };
  const resolveBinding = (scope, name) => {
    let current = scope;
    while (current) {
      if (current.bindings.has(name)) return current.bindings.get(name);
      current = current.parent;
    }
    return UNKNOWN;
  };

  const patternNames = (node) => {
    if (!node) return [];
    if (node.type === 'Identifier') return [node.name];
    if (node.type === 'RestElement') return patternNames(node.argument);
    if (node.type === 'FormalParameter') return patternNames(node.pattern);
    if (node.type === 'AssignmentPattern') return patternNames(node.left);
    if (node.type === 'ObjectPattern' || node.type === 'ArrayPattern') {
      const names = [];
      const items = asArray(node.properties).length
        ? asArray(node.properties)
        : asArray(node.elements);
      for (const item of items) {
        if (!item) continue;
        if (item.type === 'Property' || item.type === 'ObjectProperty') {
          names.push(...patternNames(item.value));
        } else {
          names.push(...patternNames(item));
        }
      }
      return names;
    }
    return [];
  };

  const memberKey = (node, scope) => {
    if (!node) return undefined;
    const keyNode = node.type === 'ComputedMemberExpression' ? node.expression : node.property;
    if (node.type === 'ComputedMemberExpression' || node.computed) {
      if (!keyNode) return undefined;
      if (
        (keyNode.type === 'StringLiteral' || keyNode.type === 'Literal') &&
        typeof keyNode.value === 'string'
      ) {
        return keyNode.value;
      }
      if (keyNode.type === 'TemplateLiteral' && (keyNode.expressions?.length ?? 0) === 0) {
        return (keyNode.quasis ?? [])
          .map((quasi) => quasi.value?.cooked ?? quasi.value?.raw ?? '')
          .join('');
      }
      if (keyNode.type === 'Identifier') {
        const binding = resolveBinding(scope, keyNode.name);
        if (binding?.kind === 'literal' && typeof binding.value === 'string') {
          return binding.value;
        }
      }
      return undefined;
    }
    if (keyNode?.type === 'Identifier') return keyNode.name;
    if (
      (keyNode?.type === 'StringLiteral' || keyNode?.type === 'Literal') &&
      typeof keyNode.value === 'string'
    ) {
      return keyNode.value;
    }
    return undefined;
  };

  const cloneValue = (value, seen = new Map()) => {
    if (!value) return value;
    if (value.kind === 'array') {
      return {
        kind: 'array',
        elements: value.elements.map((element) => cloneValue(element, seen)),
      };
    }
    if (value.kind === 'union') {
      return { kind: 'union', values: value.values.map((entry) => cloneValue(entry, seen)) };
    }
    if (value.kind !== 'object') return value;
    if (seen.has(value)) return seen.get(value);
    const clone = {
      kind: 'object',
      props: new Map(),
      serviceSource: Boolean(value.serviceSource),
      unknown: Boolean(value.unknown),
    };
    seen.set(value, clone);
    for (const [key, property] of value.props) {
      clone.props.set(key, cloneValue(property, seen));
    }
    return clone;
  };

  const cloneScope = (scope) => {
    const bindings = new Map();
    const seen = new Map();
    for (const [name, value] of scope.bindings) bindings.set(name, cloneValue(value, seen));
    const clone = {
      parent: scope.parent,
      isFunction: scope.isFunction,
      bindings,
      branchRoot: null,
    };
    clone.branchRoot = clone;
    return clone;
  };

  const valuesEqual = (a, b, seen = new Set()) => {
    if (a === b) return true;
    if (!a || !b || a.kind !== b.kind) return false;
    if (a.kind === 'object') {
      if (seen.has(a)) return true;
      seen.add(a);
      if (Boolean(a.unknown) !== Boolean(b.unknown)) return false;
      if (Boolean(a.serviceSource) !== Boolean(b.serviceSource)) return false;
      if (a.props.size !== b.props.size) return false;
      for (const [key, value] of a.props) {
        if (!b.props.has(key) || !valuesEqual(value, b.props.get(key), seen)) return false;
      }
      return true;
    }
    if (a.kind === 'array') {
      if (a.elements.length !== b.elements.length) return false;
      return a.elements.every((value, index) => valuesEqual(value, b.elements[index], seen));
    }
    if (a.kind === 'union') {
      if (a.values.length !== b.values.length) return false;
      return a.values.every((value) =>
        b.values.some((candidate) => valuesEqual(value, candidate, seen))
      );
    }
    if (a.kind === 'literal') return a.value === b.value;
    if (a.kind === 'method') return a.name === b.name;
    if (a.kind === 'function') return a.node === b.node;
    return true;
  };

  const joinBranches = (scope, branches) => {
    const names = new Set(scope.bindings.keys());
    for (const branch of branches) {
      for (const name of branch.bindings.keys()) names.add(name);
    }
    for (const name of names) {
      const values = branches.map((branch) =>
        branch.bindings.has(name) ? branch.bindings.get(name) : UNKNOWN
      );
      const first = values[0];
      if (values.every((value) => valuesEqual(first, value))) {
        scope.bindings.set(name, first);
      } else {
        // Keep the distinct non-unknown alternatives so a later call can still
        // record a service event from the branch that installs the method.
        const alternatives = [];
        for (const value of values) {
          if (value.kind === 'unknown') continue;
          if (!alternatives.some((candidate) => valuesEqual(candidate, value))) {
            alternatives.push(value);
          }
        }
        scope.bindings.set(
          name,
          alternatives.length ? { kind: 'union', values: alternatives } : UNKNOWN
        );
      }
    }
  };

  const valueFromExpression = (node, scope) => {
    if (!node) return UNKNOWN;
    // Bun lowers `import { call } from 'host:service'` to
    // `__toESM(require('host:service'))`; treat the namespace expression as
    // the service value itself.
    if (isHostServiceRequire(node)) return SERVICE;
    if (node.type === 'Identifier') {
      return resolveBinding(scope, node.name);
    }
    if (
      (node.type === 'StringLiteral' || node.type === 'Literal') &&
      typeof node.value === 'string'
    ) {
      return { kind: 'literal', value: node.value };
    }
    if (isMemberNode(node)) {
      const key = memberKey(node, scope);
      if (key === undefined) return UNKNOWN;
      if (isHostServiceRequire(node.object)) {
        return SERVICE_METHOD_NAMES.has(key) ? { kind: 'method', name: key } : UNKNOWN;
      }
      if (node.object?.type === 'Identifier') {
        const objectValue = resolveBinding(scope, node.object.name);
        if (objectValue.kind === 'service') {
          return SERVICE_METHOD_NAMES.has(key) ? { kind: 'method', name: key } : UNKNOWN;
        }
        if (objectValue.kind === 'object') {
          const propertyValue = objectValue.props.get(key);
          if (propertyValue) return propertyValue;
          if (objectValue.serviceSource && SERVICE_METHOD_NAMES.has(key)) {
            return { kind: 'method', name: key };
          }
        }
        return UNKNOWN;
      }
      const parent = resolveMemberParent(node, scope);
      if (parent) {
        const propertyValue = parent.props.get(key);
        if (propertyValue) return propertyValue;
        if (parent.serviceSource && SERVICE_METHOD_NAMES.has(key)) {
          return { kind: 'method', name: key };
        }
      }
      return UNKNOWN;
    }
    if (node.type === 'CallExpression') {
      // `on.bind(...)` keeps the service-method provenance.
      if (isMemberNode(node.callee) && memberKey(node.callee, scope) === 'bind') {
        const bound = valueFromExpression(node.callee.object, scope);
        if (bound.kind === 'method') return bound;
      }
      return UNKNOWN;
    }
    if (node.type === 'FunctionExpression' || node.type === 'ArrowFunctionExpression') {
      return { kind: 'function', node, scope };
    }
    if (node.type === 'ArrayExpression') {
      return {
        kind: 'array',
        elements: asArray(node.elements).map((element) =>
          element ? valueFromExpression(element, scope) : UNKNOWN
        ),
      };
    }
    if (node.type === 'ObjectExpression') {
      const props = new Map();
      let serviceSource = false;
      let unknown = false;
      for (const property of asArray(node.properties)) {
        if (property.type === 'SpreadElement' || property.type === 'SpreadProperty') {
          const spread = valueFromExpression(property.argument, scope);
          if (spread.kind === 'service') {
            serviceSource = true;
            continue;
          }
          if (spread.kind === 'object') {
            // The spread can override every earlier property.
            props.clear();
            if (spread.unknown) unknown = true;
            serviceSource = serviceSource || spread.serviceSource;
            for (const [key, value] of spread.props) props.set(key, value);
            continue;
          }
          props.clear();
          unknown = true;
          continue;
        }
        const key = propertyName(property);
        if (key === undefined) {
          props.clear();
          unknown = true;
          continue;
        }
        props.set(key, valueFromExpression(property.value, scope));
      }
      return { kind: 'object', props, serviceSource, unknown };
    }
    return UNKNOWN;
  };

  const declarePattern = (pattern, value, scope) => {
    if (pattern?.type === 'ObjectPattern') {
      if (value.kind === 'object') {
        for (const property of asArray(pattern.properties)) {
          if (!property || property.type === 'RestElement') continue;
          const key = propertyName(property);
          const propertyValue =
            key === undefined
              ? UNKNOWN
              : (value.props.get(key) ??
                (value.serviceSource && SERVICE_METHOD_NAMES.has(key)
                  ? { kind: 'method', name: key }
                  : UNKNOWN));
          if (property.value?.type === 'Identifier') {
            declare(scope, property.value.name, propertyValue);
          }
        }
        return;
      }
      if (value.kind === 'service') {
        for (const property of asArray(pattern.properties)) {
          if (!property || property.type === 'RestElement') continue;
          const key = propertyName(property);
          if (key && property.value?.type === 'Identifier') {
            declare(
              scope,
              property.value.name,
              SERVICE_METHOD_NAMES.has(key) ? { kind: 'method', name: key } : UNKNOWN
            );
          }
        }
        return;
      }
    }
    for (const name of patternNames(pattern)) declare(scope, name, UNKNOWN);
  };

  // Evaluate the final value of a string property in object-literal order.
  // A spread or dynamic key after the last literal occurrence can override it;
  // a later literal re-establishes a statically known value.
  const effectiveStringProperty = (argument, key) => {
    if (!argument || argument.type !== 'ObjectExpression') return undefined;
    let value;
    let mayOverride = false;
    for (const property of asArray(argument.properties)) {
      if (property.type === 'SpreadElement' || property.type === 'SpreadProperty') {
        mayOverride = true;
        continue;
      }
      const name = propertyName(property);
      if (name === undefined) {
        mayOverride = true;
        continue;
      }
      if (name !== key) continue;
      const propertyValue = property.value;
      if (
        !(propertyValue?.type === 'StringLiteral' || propertyValue?.type === 'Literal') ||
        typeof propertyValue.value !== 'string'
      ) {
        throw new Error(`host:service input requires a literal "${key}"`);
      }
      value = propertyValue.value;
      mayOverride = false;
    }
    if (mayOverride) {
      throw new Error(`host:service input cannot use a spread/dynamic key after "${key}"`);
    }
    return value;
  };

  const isServiceShapeProps = (props, dynamic = false) => {
    let hasRoute = false;
    let hasEvent = false;
    let hasPayload = false;
    for (const key of props.keys()) {
      if (key === 'route') hasRoute = true;
      else if (key === 'event') hasEvent = true;
      else if (key === 'payloadBase64') hasPayload = true;
    }
    // Only an object that actually carries a service route/event key is
    // suspicious; a plain `{...defaults, b: 2}` call must stay buildable.
    return (hasRoute && (hasPayload || dynamic)) || (hasEvent && (hasPayload || dynamic));
  };

  const isServiceArgumentShape = (argument, scope) => {
    if (!argument) return false;
    if (argument.type === 'Identifier' && scope) {
      // A forwarded parameter may hold the service input shape.
      const value = resolveBinding(scope, argument.name);
      if (value.kind === 'object') return isServiceShapeProps(value.props, value.unknown);
      if (value.kind === 'union') {
        return value.values.some(
          (entry) => entry.kind === 'object' && isServiceShapeProps(entry.props, entry.unknown)
        );
      }
      return false;
    }
    if (argument.type !== 'ObjectExpression') return false;
    const props = new Map();
    let dynamic = false;
    for (const property of asArray(argument.properties)) {
      if (property.type === 'SpreadElement' || property.type === 'SpreadProperty') {
        dynamic = true;
        continue;
      }
      const name = propertyName(property);
      if (name === undefined) {
        dynamic = true;
        continue;
      }
      props.set(name, UNKNOWN);
    }
    return isServiceShapeProps(props, dynamic);
  };

  const parsed = oxc.parseSync(code, { sourceFilename: 'bundle.js' });
  const ast = typeof parsed.program === 'string' ? JSON.parse(parsed.program) : parsed.program;

  let hasServiceModule = false;
  const deferredCalls = [];
  const deferredSeen = new Set();
  traverseAST(ast, (node) => {
    if (node.type === 'CallExpression' && isHostServiceRequire(node)) {
      hasServiceModule = true;
    }
  });

  const recordCall = (node, method) => {
    const argument = node.arguments?.[0];
    if (method === 'on') {
      if (
        (argument?.type === 'StringLiteral' || argument?.type === 'Literal') &&
        typeof argument.value === 'string'
      ) {
        events.add(argument.value);
        return;
      }
      const event = effectiveStringProperty(argument, 'event');
      if (event) {
        events.add(event);
        return;
      }
      throw new Error('host:service.on requires a statically known event name');
    }
    if (!argument || argument.type !== 'ObjectExpression') {
      throw new Error(`host:service.${method} requires an object literal`);
    }
    const route = effectiveStringProperty(argument, 'route');
    if (!route) {
      throw new Error(`host:service.${method} requires a literal "route"`);
    }
    rpc.add(route);
  };

  const callStack = new Set();
  // During the eager declaration walk, parameters are still unknown; a
  // service-shaped call there is a forwarding candidate, not a proven bypass.
  // The heuristic re-runs when the wrapper is actually called with known
  // arguments.
  let suppressUnknownHeuristic = 0;

  const analyzeFunctionBody = (fnNode, fnScope, replay = false) => {
    if (callStack.has(fnNode)) return;
    callStack.add(fnNode);
    if (!replay) suppressUnknownHeuristic++;
    try {
      if (fnNode.body?.type === 'BlockStatement') {
        const bodyScope = createScope(fnScope);
        // Function declarations and aliases inside the body are visible to
        // calls earlier in the same body.
        hoistDeclarations(fnNode.body.body, bodyScope);
        for (const statement of asArray(fnNode.body.body)) walk(statement, bodyScope);
      } else {
        walk(fnNode.body, fnScope);
      }
    } finally {
      if (!replay) suppressUnknownHeuristic--;
      callStack.delete(fnNode);
    }
  };

  const analyzeCall = (node, scope) => {
    let method;
    let calleeValue = UNKNOWN;
    const callee = node.callee;
    if (callee?.type === 'Identifier') {
      calleeValue = resolveBinding(scope, callee.name);
      if (calleeValue.kind === 'method') method = calleeValue.name;
    } else if (isMemberNode(callee)) {
      const key = memberKey(callee, scope);
      if (isHostServiceRequire(callee.object)) {
        // `require('host:service').on(...)` and the rewritten
        // `globalThis.__keel_importHostModule('host:service').on(...)`.
        method = key && SERVICE_METHOD_NAMES.has(key) ? key : undefined;
        calleeValue = method ? { kind: 'method', name: method } : UNKNOWN;
      } else {
        const objectName = callee.object?.type === 'Identifier' ? callee.object.name : undefined;
        if (objectName && key !== undefined) {
          const objectValue = resolveBinding(scope, objectName);
          if (objectValue.kind === 'service') {
            method = SERVICE_METHOD_NAMES.has(key) ? key : undefined;
            calleeValue = method ? { kind: 'method', name: method } : UNKNOWN;
          } else if (objectValue.kind === 'object') {
            const propertyValue = objectValue.props.get(key);
            if (propertyValue) {
              calleeValue = propertyValue;
            } else if (objectValue.serviceSource && SERVICE_METHOD_NAMES.has(key)) {
              calleeValue = { kind: 'method', name: key };
            } else {
              calleeValue = UNKNOWN;
            }
            if (calleeValue.kind === 'method') method = calleeValue.name;
          }
        } else if (key !== undefined) {
          // Nested member call such as `api.wrapper.on(...)`.
          const parent = resolveMemberParent(callee, scope);
          if (parent) {
            const propertyValue = parent.props.get(key);
            if (propertyValue) {
              calleeValue = propertyValue;
            } else if (parent.serviceSource && SERVICE_METHOD_NAMES.has(key)) {
              calleeValue = { kind: 'method', name: key };
            } else {
              calleeValue = UNKNOWN;
            }
            if (calleeValue.kind === 'method') method = calleeValue.name;
          }
        }
      }
    }

    if (calleeValue.kind === 'union') {
      let recorded = false;
      for (const value of calleeValue.values) {
        if (value.kind === 'method') {
          recordCall(node, value.name);
          recorded = true;
        } else if (value.kind === 'function') {
          // A branch selected one of several wrappers; replay each one so the
          // events of the non-first branch are not lost.
          const args = asArray(node.arguments);
          const params = asArray(value.node.params);
          const hasServiceArgument = args.some((argument) => {
            const argumentValue = valueFromExpression(argument, scope);
            return (
              argumentValue.kind === 'method' ||
              argumentValue.kind === 'service' ||
              argumentValue.kind === 'union' ||
              isServiceArgumentShape(argument, scope)
            );
          });
          if (!hasServiceArgument) continue;
          const fnScope = createScope(value.scope);
          params.forEach((parameter, index) => {
            const argumentValue = valueFromExpression(args[index], scope);
            const pattern = parameter?.type === 'FormalParameter' ? parameter.pattern : parameter;
            if (pattern?.type === 'Identifier') {
              declare(fnScope, pattern.name, argumentValue);
            } else {
              declarePattern(pattern, argumentValue, fnScope);
            }
          });
          analyzeFunctionBody(value.node, fnScope, true);
        }
      }
      if (recorded) return;
    }

    if (method) {
      recordCall(node, method);
      return;
    }

    if (calleeValue.kind === 'function') {
      // Propagate service-method arguments into the function body. This is how
      // `function wrap(fn) { fn(event, handler) } wrap(on)` is resolved.
      const args = asArray(node.arguments);
      const params = asArray(calleeValue.node.params);
      const hasServiceArgument = args.some((argument) => {
        const value = valueFromExpression(argument, scope);
        return (
          value.kind === 'method' ||
          value.kind === 'service' ||
          value.kind === 'union' ||
          isServiceArgumentShape(argument, scope)
        );
      });
      if (hasServiceArgument) {
        const fnScope = createScope(calleeValue.scope);
        params.forEach((parameter, index) => {
          const argumentValue = valueFromExpression(args[index], scope);
          const pattern = parameter?.type === 'FormalParameter' ? parameter.pattern : parameter;
          if (pattern?.type === 'Identifier') {
            declare(fnScope, pattern.name, argumentValue);
          } else {
            declarePattern(pattern, argumentValue, fnScope);
          }
        });
        analyzeFunctionBody(calleeValue.node, fnScope, true);
      }
      // A known local function may legitimately accept a route/event-shaped
      // object (a local API). Forwarding is handled by the replay above; only
      // unresolved callees fall through to the shape heuristic below.
      return;
    }

    // An unknown callee that carries the host:service input shape is
    // indistinguishable from a service call in the manifest; reject it when
    // the bundle actually uses host:service.
    if (
      suppressUnknownHeuristic === 0 &&
      hasServiceModule &&
      isServiceArgumentShape(node.arguments?.[0], scope)
    ) {
      throw new Error('Unable to establish host:service provenance for this call');
    }
  };

  const hoistDeclarations = (statements, scope) => {
    for (const statement of asArray(statements)) {
      let declaration = statement;
      if (
        statement?.type === 'ExportNamedDeclaration' ||
        statement?.type === 'ExportDefaultDeclaration'
      ) {
        declaration = statement.declaration;
      }
      if (!declaration) continue;
      if (declaration.type === 'FunctionDeclaration' && declaration.id?.name) {
        declare(scope, declaration.id.name, { kind: 'function', node: declaration, scope });
      } else if (declaration.type === 'VariableDeclaration') {
        const targetScope = declaration.kind === 'var' ? nearestFunctionScope(scope) : scope;
        for (const declarator of asArray(declaration.declarations)) {
          const value = valueFromExpression(declarator.init, scope);
          if (declarator.id?.type === 'Identifier') {
            declare(targetScope, declarator.id.name, value);
          } else {
            declarePattern(declarator.id, value, targetScope);
          }
        }
      }
    }
  };

  // Resolve the object whose property a member target writes to, including
  // nested paths such as `api.wrapper.on`.
  const resolveMemberParent = (node, scope) => {
    if (!isMemberNode(node)) return undefined;
    if (node.object?.type === 'Identifier') {
      const binding = resolveBinding(scope, node.object.name);
      return binding?.kind === 'object' ? binding : undefined;
    }
    if (isMemberNode(node.object)) {
      const parent = resolveMemberParent(node.object, scope);
      if (!parent) return undefined;
      const key = memberKey(node.object, scope);
      if (key === undefined) return undefined;
      const child = parent.props.get(key);
      return child?.kind === 'object' ? child : undefined;
    }
    return undefined;
  };

  // Apply a destructuring target: identifier targets update bindings, member
  // targets update the object's property value, and unresolved targets become
  // unknown so the use site fails closed.
  const applyAssignmentTarget = (target, value, scope) => {
    if (!target) return;
    if (target.type === 'Identifier' || target.type === 'AssignmentTargetIdentifier') {
      if (target.name) assignBinding(scope, target.name, value);
      return;
    }
    if (isMemberNode(target)) {
      const key = memberKey(target, scope);
      const parent = resolveMemberParent(target, scope);
      if (parent) {
        if (key === undefined) {
          parent.unknown = true;
          parent.props.clear();
        } else {
          parent.props.set(key, value);
        }
      }
      return;
    }
    if (target.type === 'ObjectAssignmentTarget' || target.type === 'ObjectPattern') {
      if (value.kind === 'service') {
        for (const property of asArray(target.properties)) {
          if (!property || property.type === 'RestElement') continue;
          const key =
            property.type === 'AssignmentTargetPropertyIdentifier'
              ? property.binding?.name
              : propertyName({
                  type: 'Property',
                  key: property.name ?? property.key,
                  computed: property.computed,
                });
          applyAssignmentTarget(
            property.type === 'AssignmentTargetPropertyIdentifier'
              ? property.binding
              : (property.binding ?? property.value),
            key && SERVICE_METHOD_NAMES.has(key) ? { kind: 'method', name: key } : UNKNOWN,
            scope
          );
        }
        return;
      }
      const source = value.kind === 'object' ? value : undefined;
      for (const property of asArray(target.properties)) {
        if (!property || property.type === 'RestElement') continue;
        if (property.type === 'AssignmentTargetPropertyIdentifier') {
          const name = property.binding?.name;
          applyAssignmentTarget(
            property.binding,
            source && name ? (source.props.get(name) ?? UNKNOWN) : UNKNOWN,
            scope
          );
          continue;
        }
        if (property.type === 'AssignmentTargetPropertyProperty') {
          const key = propertyName({
            type: 'Property',
            key: property.name ?? property.key,
            computed: property.computed,
          });
          applyAssignmentTarget(
            property.binding,
            source && key !== undefined ? (source.props.get(key) ?? UNKNOWN) : UNKNOWN,
            scope
          );
          continue;
        }
        if (property.type === 'Property' || property.type === 'ObjectProperty') {
          const key = propertyName(property);
          applyAssignmentTarget(
            property.value ?? property,
            source && key !== undefined ? (source.props.get(key) ?? UNKNOWN) : UNKNOWN,
            scope
          );
          continue;
        }
        applyAssignmentTarget(property.value ?? property, UNKNOWN, scope);
      }
      return;
    }
    if (target.type === 'ArrayAssignmentTarget' || target.type === 'ArrayPattern') {
      const elements = value.kind === 'array' ? value.elements : [];
      asArray(target.elements).forEach((element, index) => {
        applyAssignmentTarget(element, elements[index] ?? UNKNOWN, scope);
      });
      return;
    }
    if (target.type === 'AssignmentTargetPropertyProperty') {
      applyAssignmentTarget(target.binding, value, scope);
      return;
    }
    if (target.type === 'AssignmentTargetPropertyIdentifier') {
      if (target.binding?.name) assignBinding(scope, target.binding.name, value);
      return;
    }
    if (target.type === 'AssignmentTargetWithDefault') {
      applyAssignmentTarget(target.binding, value, scope);
      return;
    }
    if (target.type === 'AssignmentTargetRest') {
      applyAssignmentTarget(target.target, value, scope);
      return;
    }
    for (const name of patternNames(target)) assignBinding(scope, name, UNKNOWN);
  };

  const walk = (node, scope) => {
    if (!node || typeof node !== 'object') return;
    if (Array.isArray(node)) {
      for (const child of node) walk(child, scope);
      return;
    }

    switch (node.type) {
      case 'Program': {
        const programScope = createScope(null);
        // Function declarations and top-level bindings are visible throughout
        // the module; hoist them so a call before the declaration still
        // resolves the service method.
        hoistDeclarations(node.body, programScope);
        for (const statement of asArray(node.body)) walk(statement, programScope);
        // Replay calls after every assignment in the module has been applied.
        // A call before a late `subscribe = on` would otherwise be analyzed
        // against the initial placeholder function and lose the event.
        for (const call of deferredCalls.slice()) {
          analyzeCall(call.node, call.scope);
        }
        return;
      }
      case 'BlockStatement': {
        const blockScope = createScope(scope);
        hoistDeclarations(node.body, blockScope);
        for (const statement of asArray(node.body)) walk(statement, blockScope);
        return;
      }
      case 'FunctionDeclaration':
      case 'FunctionExpression':
      case 'ArrowFunctionExpression': {
        if (node.type === 'FunctionDeclaration' && node.id?.name) {
          declare(scope, node.id.name, { kind: 'function', node, scope });
        }
        const fnScope = createScope(scope, true);
        for (const parameter of asArray(node.params)) {
          for (const name of patternNames(parameter)) declare(fnScope, name, UNKNOWN);
        }
        analyzeFunctionBody(node, fnScope);
        return;
      }
      case 'VariableDeclaration': {
        for (const declaration of asArray(node.declarations)) {
          const value = valueFromExpression(declaration.init, scope);
          if (declaration.id?.type === 'Identifier') {
            declare(scope, declaration.id.name, value);
          } else {
            declarePattern(declaration.id, value, scope);
          }
          walk(declaration.init, scope);
        }
        return;
      }
      case 'AssignmentExpression': {
        const value = node.operator === '=' ? valueFromExpression(node.right, scope) : UNKNOWN;
        if (node.left?.type === 'Identifier') {
          assignBinding(scope, node.left.name, value);
        } else if (isMemberNode(node.left)) {
          const objectName =
            node.left.object?.type === 'Identifier' ? node.left.object.name : undefined;
          const key = memberKey(node.left, scope);
          if (objectName) {
            const objectValue = resolveBinding(scope, objectName);
            if (objectValue.kind === 'object') {
              if (key === undefined) {
                objectValue.unknown = true;
                objectValue.props.clear();
              } else {
                objectValue.props.set(key, value);
              }
            }
          }
        } else {
          applyAssignmentTarget(node.left, value, scope);
        }
        walk(node.left, scope);
        walk(node.right, scope);
        return;
      }
      case 'IfStatement': {
        walk(node.test, scope);
        const consequentScope = cloneScope(scope);
        walk(node.consequent, consequentScope);
        const alternateScope = node.alternate ? cloneScope(scope) : scope;
        if (node.alternate) walk(node.alternate, alternateScope);
        joinBranches(scope, [consequentScope, alternateScope]);
        return;
      }
      case 'SwitchStatement': {
        walk(node.discriminant, scope);
        const caseScopes = [];
        // A case without a trailing `break` falls through, so each case starts
        // from the previous case's end state. Joining the last state with the
        // pre-switch state keeps the no-match path conservative.
        let previousScope = scope;
        for (const switchCase of asArray(node.cases)) {
          const caseScope = cloneScope(previousScope);
          walk(switchCase.test, caseScope);
          for (const statement of asArray(switchCase.consequent)) {
            walk(statement, caseScope);
          }
          caseScopes.push(caseScope);
          previousScope = caseScope;
        }
        caseScopes.push(scope);
        joinBranches(scope, caseScopes);
        return;
      }
      case 'ForStatement':
      case 'ForInStatement':
      case 'ForOfStatement':
      case 'WhileStatement':
      case 'DoWhileStatement': {
        const loopScope = cloneScope(scope);
        walk(node.init, loopScope);
        walk(node.test, loopScope);
        walk(node.update, loopScope);
        walk(node.left, loopScope);
        walk(node.right, loopScope);
        walk(node.body, loopScope);
        joinBranches(scope, [loopScope, scope]);
        return;
      }
      case 'CallExpression': {
        analyzeCall(node, scope);
        if (!deferredSeen.has(node)) {
          deferredSeen.add(node);
          deferredCalls.push({ node, scope });
        }
        // Walk the callee too: an IIFE or a call expression in callee position
        // can hide a service subscription inside its function body.
        walk(node.callee, scope);
        for (const argument of asArray(node.arguments)) walk(argument, scope);
        return;
      }
      default: {
        for (const key of Object.keys(node)) {
          if (
            key === 'type' ||
            key === 'start' ||
            key === 'end' ||
            key === 'loc' ||
            key === 'range'
          ) {
            continue;
          }
          const value = node[key];
          if (value && typeof value === 'object') walk(value, scope);
        }
        return;
      }
    }
  };

  walk(ast, createScope(null));
  return { rpc: Array.from(rpc).sort(), events: Array.from(events).sort() };
}

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
