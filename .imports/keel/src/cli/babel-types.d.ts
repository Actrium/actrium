/**
 * Type declarations for @babel/core
 */

declare module '@babel/core' {
  export interface PluginObj<S = unknown> {
    name?: string;
    visitor: Visitor<S>;
  }

  export interface NodePath<T = Node> {
    node: T;
    parentPath: NodePath | null;
    parent: Node;
    scope: Scope;
    get(key: string): NodePath | NodePath[];
    isFunction(): boolean;
    isArrowFunctionExpression(): boolean;
    isFunctionDeclaration(): boolean;
    isFunctionExpression(): boolean;
    isClassMethod(): boolean;
    isObjectMethod(): boolean;
  }

  export interface Scope {
    generateUidIdentifier(name: string): types.Identifier;
  }

  // biome-ignore lint/complexity/noBannedTypes: babel types namespace
  export interface Node extends Object {
    type: string;
    start?: number | null;
    end?: number | null;
    loc?: SourceLocation | null;
  }

  export interface SourceLocation {
    start: Position;
    end: Position;
    filename?: string;
  }

  export interface Position {
    line: number;
    column: number;
  }

  export interface Visitor<S = unknown> {
    Function?: (path: NodePath<types.Function>, state: S) => void;
    FunctionDeclaration?: (path: NodePath<types.FunctionDeclaration>, state: S) => void;
    FunctionExpression?: (path: NodePath<types.FunctionExpression>, state: S) => void;
    ArrowFunctionExpression?: (path: NodePath<types.ArrowFunctionExpression>, state: S) => void;
    ClassMethod?: (path: NodePath<types.ClassMethod>, state: S) => void;
    ObjectMethod?: (path: NodePath<types.ObjectMethod>, state: S) => void;
    JSXAttribute?: (path: NodePath<types.JSXAttribute>, state: S) => void;
    CallExpression?: (path: NodePath<types.CallExpression>, state: S) => void;
  }

  export namespace types {
    interface Node {
      type: string;
      start?: number | null;
      end?: number | null;
      loc?: SourceLocation | null;
    }

    interface Identifier extends Node {
      type: 'Identifier';
      name: string;
    }

    interface Expression extends Node {}
    interface Statement extends Node {}
    interface Pattern extends Node {}

    interface Function extends Node {
      params: Array<Pattern>;
      body: BlockStatement | Expression;
    }

    interface FunctionDeclaration extends Function {
      type: 'FunctionDeclaration';
      id: Identifier | null;
    }

    interface FunctionExpression extends Function {
      type: 'FunctionExpression';
      id: Identifier | null;
    }

    interface ArrowFunctionExpression extends Function {
      type: 'ArrowFunctionExpression';
    }

    interface ClassMethod extends Function {
      type: 'ClassMethod';
      key: Expression;
    }

    interface ObjectMethod extends Function {
      type: 'ObjectMethod';
      key: Expression;
    }

    interface BlockStatement extends Node {
      type: 'BlockStatement';
      body: Statement[];
    }

    interface ExpressionStatement extends Statement {
      type: 'ExpressionStatement';
      expression: Expression;
    }

    interface AssignmentExpression extends Expression {
      type: 'AssignmentExpression';
      operator: string;
      left: Pattern | Expression;
      right: Expression;
    }

    interface CallExpression extends Expression {
      type: 'CallExpression';
      callee: Expression;
      arguments: Array<Expression | SpreadElement>;
    }

    interface SpreadElement extends Node {
      type: 'SpreadElement';
      argument: Expression;
    }

    interface MemberExpression extends Expression {
      type: 'MemberExpression';
      object: Expression;
      property: Expression | Identifier;
      computed: boolean;
    }

    interface ObjectExpression extends Expression {
      type: 'ObjectExpression';
      properties: Array<ObjectProperty | ObjectMethod | SpreadElement>;
    }

    interface ObjectProperty extends Node {
      type: 'ObjectProperty';
      key: Expression | Identifier;
      value: Expression | Pattern;
      computed: boolean;
      shorthand: boolean;
    }

    interface StringLiteral extends Expression {
      type: 'StringLiteral';
      value: string;
    }

    interface NumericLiteral extends Expression {
      type: 'NumericLiteral';
      value: number;
    }

    // JSX Types
    interface JSXAttribute extends Node {
      type: 'JSXAttribute';
      name: JSXIdentifier | JSXNamespacedName;
      value: JSXExpressionContainer | StringLiteral | JSXElement | null;
    }

    interface JSXIdentifier extends Node {
      type: 'JSXIdentifier';
      name: string;
    }

    interface JSXNamespacedName extends Node {
      type: 'JSXNamespacedName';
      namespace: JSXIdentifier;
      name: JSXIdentifier;
    }

    interface JSXExpressionContainer extends Node {
      type: 'JSXExpressionContainer';
      expression: Expression | JSXEmptyExpression;
    }

    interface JSXEmptyExpression extends Node {
      type: 'JSXEmptyExpression';
    }

    interface JSXElement extends Node {
      type: 'JSXElement';
    }

    // Type guards
    function isFunction(node: Node | null | undefined): node is Function;
    function isFunctionDeclaration(node: Node | null | undefined): node is FunctionDeclaration;
    function isFunctionExpression(node: Node | null | undefined): node is FunctionExpression;
    function isArrowFunctionExpression(
      node: Node | null | undefined
    ): node is ArrowFunctionExpression;
    function isClassMethod(node: Node | null | undefined): node is ClassMethod;
    function isObjectMethod(node: Node | null | undefined): node is ObjectMethod;
    function isBlockStatement(node: Node | null | undefined): node is BlockStatement;
    function isCallExpression(node: Node | null | undefined): node is CallExpression;
    function isMemberExpression(node: Node | null | undefined): node is MemberExpression;
    function isIdentifier(
      node: Node | null | undefined,
      opts?: { name?: string }
    ): node is Identifier;
    function isObjectExpression(node: Node | null | undefined): node is ObjectExpression;
    function isObjectProperty(node: Node | null | undefined): node is ObjectProperty;
    function isJSXExpressionContainer(
      node: Node | null | undefined
    ): node is JSXExpressionContainer;

    // Builders
    function identifier(name: string): Identifier;
    function stringLiteral(value: string): StringLiteral;
    function numericLiteral(value: number): NumericLiteral;
    function expressionStatement(expression: Expression): ExpressionStatement;
    function assignmentExpression(
      operator: string,
      left: Pattern | Expression,
      right: Expression
    ): AssignmentExpression;
    function memberExpression(
      object: Expression,
      property: Expression | Identifier,
      computed?: boolean
    ): MemberExpression;
    function callExpression(
      callee: Expression,
      args: Array<Expression | SpreadElement>
    ): CallExpression;
    function objectExpression(
      properties: Array<ObjectProperty | ObjectMethod | SpreadElement>
    ): ObjectExpression;
    function objectProperty(
      key: Expression | Identifier,
      value: Expression | Pattern,
      computed?: boolean,
      shorthand?: boolean
    ): ObjectProperty;
  }

  export interface TransformOptions {
    filename?: string;
    plugins?: Array<PluginObj | [unknown, unknown]>;
    sourceMaps?: boolean | 'inline' | 'both';
    sourceFileName?: string;
    parserOpts?: {
      plugins?: string[];
    };
  }

  export interface BabelFileResult {
    code: string | undefined;
    map?: object;
    ast?: object;
  }

  export function transform(code: string, opts?: TransformOptions): BabelFileResult | null;

  export function transformSync(code: string, opts?: TransformOptions): BabelFileResult | null;

  export function transformAsync(
    code: string,
    opts?: TransformOptions
  ): Promise<BabelFileResult | null>;
}
