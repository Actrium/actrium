/**
 * Babel plugin to inject source file and line number into function expressions.
 * This enables DevTools to navigate directly to function definitions.
 *
 * Transforms:
 *   onPress={() => doSomething()}
 *
 * Into:
 *   onPress=Object.assign(() => doSomething(), {
 *     __sourceFile: "/path/to/file.tsx",
 *     __sourceLine: 42
 *   })
 *
 * Only applies to functions passed as JSX props to avoid performance overhead.
 */

console.log('[Babel Plugin] function-source-location loaded');

module.exports = function functionSourceLocationPlugin({ types: t }) {
  console.log('[Babel Plugin] function-source-location initialized');
  let fileCount = 0;
  let jsxAttrCount = 0;
  let transformCount = 0;
  return {
    name: 'function-source-location',
    visitor: {
      Program(path, state) {
        fileCount++;
        if (state.filename && state.filename.includes('countapp')) {
          console.log(`[Babel Plugin] Processing file: ${state.filename}`);
        }
      },
      // Handle JSX attributes with function expressions
      JSXAttribute(path, state) {
        jsxAttrCount++;
        const value = path.node.value;

        // Only process JSX expression containers
        if (!t.isJSXExpressionContainer(value)) return;

        const expression = value.expression;

        // Check if expression is a function (arrow or regular)
        if (!t.isArrowFunctionExpression(expression) && !t.isFunctionExpression(expression)) {
          return;
        }

        // Skip if already wrapped (has __sourceFile property)
        if (
          t.isCallExpression(expression) &&
          t.isMemberExpression(expression.callee) &&
          t.isIdentifier(expression.callee.property, { name: 'assign' })
        ) {
          return;
        }

        // Get source location
        const loc = expression.loc;
        if (!loc) return;

        const filename = state.filename || 'unknown';
        const line = loc.start.line;

        // Get the attribute name for logging
        const attrName = path.node.name?.name || 'unknown';
        transformCount++;
        if (filename.includes('countapp') || filename.includes('App')) {
          console.log(`[Babel Plugin] Transforming ${attrName} at ${filename}:${line}`);
        }

        // Create Object.assign(fn, { __sourceFile, __sourceLine })
        const wrappedFunction = t.callExpression(
          t.memberExpression(t.identifier('Object'), t.identifier('assign')),
          [
            expression,
            t.objectExpression([
              t.objectProperty(t.identifier('__sourceFile'), t.stringLiteral(filename)),
              t.objectProperty(t.identifier('__sourceLine'), t.numericLiteral(line)),
            ]),
          ]
        );

        // Replace the expression
        value.expression = wrappedFunction;
      },
    },
  };
};
