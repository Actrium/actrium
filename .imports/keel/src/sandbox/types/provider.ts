/**
 * @rill/sandbox - JSEngineProvider Interface
 *
 * This file defines the core interfaces for abstracting different JavaScript sandbox
 * implementations (like JSC, QuickJS, VM, and Web Workers).
 *
 * - JSEngineProvider: The top-level factory for creating a runtime.
 * - JSEngineRuntime: A runtime instance that can create one or more isolated contexts.
 * - JSEngineContext: A single, isolated JS execution environment.
 */

/**
 * Available sandbox types for DefaultProvider.
 *
 * Platform availability:
 * - VM: Node.js / Bun only
 * - Worker: Web browsers only
 * - JSC: Apple platforms only (iOS, macOS, tvOS, visionOS)
 * - QuickJS: Cross-platform native (iOS, Android, macOS, Windows)
 */
export enum SandboxType {
  /** Node.js vm module (Node/Bun only) */
  VM = 'vm',
  /** Web Worker sandbox (browsers only) */
  Worker = 'worker',
  /** JavaScriptCore via JSI (Apple platforms only, zero binary overhead) */
  JSC = 'jsc',
  /** QuickJS via JSI (cross-platform native) */
  QuickJS = 'quickjs',
}

/**
 * JSEngineContext - Represents an isolated JS execution context.
 * This is the actual sandbox environment where code is executed.
 */
export interface JSEngineContext {
  /**
   * Synchronously evaluates a string of JavaScript code.
   * Note: This method blocks the current thread until execution is complete.
   * It may not be available in all environments (e.g., Web Workers).
   * @param code The JavaScript code string to execute.
   * @returns The result of the last executed expression in the code.
   */
  eval: (code: string) => unknown;

  /**
   * Asynchronously evaluates a string of JavaScript code.
   * This is the preferred method as it does not block the main thread.
   * @param code The JavaScript code string to execute.
   * @returns A Promise that resolves with the result of the last expression.
   */
  evalAsync?: (code: string) => Promise<unknown>;

  /**
   * Sets a global variable in the sandbox's global scope.
   * @param name The name of the global variable.
   * @param value The value to set.
   */
  setGlobal: (name: string, value: unknown) => void;

  /**
   * Gets a global variable from the sandbox's global scope.
   * Note: This may only retrieve serializable values in some providers due to threading limitations.
   * @param name The name of the global variable.
   * @returns The value of the variable.
   */
  getGlobal: (name: string) => unknown;

  /**
   * Disposes of this context, releasing all associated resources.
   */
  dispose: () => void;
}

/**
 * JSEngineRuntime - An instance of a JS runtime.
 * It can create and manage one or more isolated JSEngineContexts.
 */
export interface JSEngineRuntime {
  /**
   * Creates a new, isolated JS execution context.
   * @returns A JSEngineContext instance.
   */
  createContext: () => JSEngineContext;

  /**
   * Disposes of this runtime and all contexts it has created.
   */
  dispose: () => void;
}

/**
 * Options for creating a JSEngineRuntime.
 */
export interface JSEngineRuntimeOptions {
  /**
   * The execution timeout for scripts in milliseconds.
   * Note: Not all providers support hard interruption.
   */
  timeout?: number;

  /**
   * The memory limit in bytes.
   * Note: This is a hint and enforcement depends on the underlying engine.
   */
  memoryLimit?: number;

  /**
   * Other engine-specific options.
   */
  [key: string]: unknown;
}

/**
 * JSEngineProvider - The top-level abstraction for a JS engine.
 * Responsible for creating runtime instances based on configuration.
 */
export interface JSEngineProvider {
  /**
   * Creates a JS runtime instance.
   * This method can be asynchronous, e.g., if it needs to load a WASM file.
   * @param options Configuration options for the runtime.
   * @returns A JSEngineRuntime instance or a Promise resolving to one.
   */
  createRuntime: (options?: JSEngineRuntimeOptions) => Promise<JSEngineRuntime> | JSEngineRuntime;
}
