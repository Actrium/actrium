/**
 * Babel plugin to inject source file and line number into function expressions.
 * This enables DevTools to navigate directly to function definitions.
 *
 * For JSX source files, transforms:
 *   onPress={() => doSomething()}
 *
 * Into:
 *   onPress=Object.assign(() => doSomething(), {
 *     __sourceFile: "/path/to/file.tsx",
 *     __sourceLine: 42
 *   })
 *
 * For bundled code (post-JSX transform), looks for jsx/jsxs/createElement calls
 * and transforms function props in the same way.
 */

import type { PluginObj } from '@babel/core';

interface PluginState {
  filename?: string;
}

// JSX function names to look for in bundled code
const JSX_CALLEE_NAMES = new Set(['jsx', 'jsxs', '_jsx', '_jsxs', 'jsxDEV', '_jsxDEV']);

export default function functionSourceLocationPlugin({
  types: t,
}: {
  types: typeof t;
}): PluginObj<PluginState> {
  /**
   * Create Object.assign(fn, { __sourceFile, __sourceLine }) wrapper
   */
  function wrapFunction(
    expression: t.ArrowFunctionExpression | t.FunctionExpression,
    filename: string,
    line: number
  ): t.CallExpression {
    return t.callExpression(t.memberExpression(t.identifier('Object'), t.identifier('assign')), [
      expression,
      t.objectExpression([
        t.objectProperty(t.identifier('__sourceFile'), t.stringLiteral(filename)),
        t.objectProperty(t.identifier('__sourceLine'), t.numericLiteral(line)),
      ]),
    ]);
  }

  /**
   * Check if expression is already wrapped with Object.assign
   */
  function isAlreadyWrapped(expression: t.Expression): boolean {
    return (
      t.isCallExpression(expression) &&
      t.isMemberExpression(expression.callee) &&
      t.isIdentifier(expression.callee.object, { name: 'Object' }) &&
      t.isIdentifier(expression.callee.property, { name: 'assign' })
    );
  }

  /**
   * Check if this is a JSX function call (jsx, jsxs, React.createElement, etc.)
   */
  function isJSXCall(callee: t.Expression): boolean {
    // Direct call: jsx(), jsxs(), _jsx(), etc.
    if (t.isIdentifier(callee) && JSX_CALLEE_NAMES.has(callee.name)) {
      return true;
    }

    // Member expression: React.createElement, ReactJSXRuntime.jsx, etc.
    if (t.isMemberExpression(callee)) {
      const property = callee.property;
      if (t.isIdentifier(property)) {
        if (property.name === 'createElement') return true;
        if (JSX_CALLEE_NAMES.has(property.name)) return true;
      }
    }

    return false;
  }

  return {
    name: 'function-source-location',
    visitor: {
      // Handle JSX attributes with function expressions (for source files)
      JSXAttribute(path, state) {
        const value = path.node.value;

        // Only process JSX expression containers
        if (!t.isJSXExpressionContainer(value)) return;

        const expression = value.expression;

        // Check if expression is a function (arrow or regular)
        if (!t.isArrowFunctionExpression(expression) && !t.isFunctionExpression(expression)) {
          return;
        }

        // Skip if already wrapped
        if (isAlreadyWrapped(expression)) {
          return;
        }

        // Get source location
        const loc = expression.loc;
        if (!loc) return;

        const filename = state.filename || 'unknown';
        const line = loc.start.line;

        // Replace the expression with wrapped version
        value.expression = wrapFunction(expression, filename, line);
      },

      // Handle bundled code: jsx(), jsxs(), React.createElement() calls
      CallExpression(path, state) {
        const { callee, arguments: args } = path.node;

        // Check if this is a JSX function call
        if (!isJSXCall(callee)) return;

        // Props are typically the second argument
        const propsArg = args[1];
        if (!propsArg || !t.isObjectExpression(propsArg)) return;

        const filename = state.filename || 'unknown';

        // Process each property in the props object
        for (const prop of propsArg.properties) {
          if (!t.isObjectProperty(prop)) continue;

          const value = prop.value;

          // Check if value is a function expression
          if (!t.isArrowFunctionExpression(value) && !t.isFunctionExpression(value)) {
            continue;
          }

          // Skip if already wrapped
          if (isAlreadyWrapped(value)) {
            continue;
          }

          // Get source location
          const loc = value.loc;
          if (!loc) continue;

          const line = loc.start.line;

          // Replace the function with wrapped version
          prop.value = wrapFunction(value, filename, line);
        }
      },
    },
  };
}
