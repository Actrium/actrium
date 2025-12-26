/**
 * Type declarations for QuickJS WASM module
 */

interface QuickJSWASMModule {
  // Emscripten utilities
  ccall: (name: string, returnType: string | null, argTypes: string[], args: unknown[]) => unknown;
  cwrap: (
    name: string,
    returnType: string | null,
    argTypes: string[]
  ) => (...args: unknown[]) => unknown;
  // biome-ignore lint/complexity/noBannedTypes: Emscripten API requires Function type
  addFunction: (fn: Function, signature: string) => number;
  removeFunction: (ptr: number) => void;
  UTF8ToString: (ptr: number) => string;
  stringToUTF8: (str: string, outPtr: number, maxBytes: number) => void;
  _malloc: (size: number) => number;
  _free: (ptr: number) => void;
  HEAPU8: Uint8Array;

  // QuickJS C API bindings
  _qjs_init: () => number;
  _qjs_destroy: () => void;
  _qjs_eval: (codePtr: number) => number;
  _qjs_eval_void: (codePtr: number) => number;
  _qjs_set_global_json: (namePtr: number, valuePtr: number) => number;
  _qjs_get_global_json: (namePtr: number) => number;
  _qjs_set_host_callback: (fnPtr: number) => void;
  _qjs_install_host_functions: () => void;
  _qjs_set_timer_callback: (fnPtr: number) => void;
  _qjs_install_timer_functions: () => void;
  _qjs_fire_timer: (timerId: number) => void;
  _qjs_install_console: () => void;
  _qjs_execute_pending_jobs: () => number;
  _qjs_free_string: (ptr: number) => void;
  _qjs_get_memory_usage: () => number;
}

type QuickJSWASMFactory = () => Promise<QuickJSWASMModule>;

declare const createQuickJSSandbox: QuickJSWASMFactory;
export default createQuickJSSandbox;
