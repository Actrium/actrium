/** Structured-clone protocol for the generic native-guest compute pool. */
export type ComputePoolToWorker =
  | { kind: 'init'; wasm: ArrayBuffer; maxBytes: number; maxArgs: number }
  | {
      kind: 'run';
      id: number;
      exportName: string;
      input: ArrayBuffer;
      outputCapacity: number;
      args: number[];
    }
  | { kind: 'destroy' };

export type ComputeWorkerToPool =
  | { kind: 'ready' }
  | { kind: 'result'; id: number; output: ArrayBuffer }
  | { kind: 'error'; message: string; id?: number };
