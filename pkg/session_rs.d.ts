/* tslint:disable */
/* eslint-disable */
export function resample(audio: Float32Array, fs_in: number, fs_out: number): Float32Array;
export class Session {
  free(): void;
  [Symbol.dispose](): void;
  constructor(cfg: any);
  search(audio: Float32Array): SessionQueryResult[];
  register(uuid: string, audio: Float32Array): void;
}
export class SessionQueryResult {
  private constructor();
  free(): void;
  [Symbol.dispose](): void;
  readonly uuid: string;
  readonly score: number;
  readonly keyStart: number;
  readonly keyEnd: number;
  readonly queryStart: number;
}

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
  readonly memory: WebAssembly.Memory;
  readonly __wbg_get_sessionqueryresult_keyEnd: (a: number) => number;
  readonly __wbg_get_sessionqueryresult_keyStart: (a: number) => number;
  readonly __wbg_get_sessionqueryresult_queryStart: (a: number) => number;
  readonly __wbg_get_sessionqueryresult_score: (a: number) => number;
  readonly __wbg_session_free: (a: number, b: number) => void;
  readonly __wbg_sessionqueryresult_free: (a: number, b: number) => void;
  readonly resample: (a: number, b: number, c: number, d: number) => [number, number];
  readonly session_new: (a: any) => number;
  readonly session_register: (a: number, b: number, c: number, d: number, e: number) => [number, number];
  readonly session_search: (a: number, b: number, c: number) => [number, number];
  readonly sessionqueryresult_uuid: (a: number) => [number, number];
  readonly __wbindgen_malloc: (a: number, b: number) => number;
  readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
  readonly __wbindgen_externrefs: WebAssembly.Table;
  readonly __wbindgen_free: (a: number, b: number, c: number) => void;
  readonly __externref_drop_slice: (a: number, b: number) => void;
  readonly __externref_table_dealloc: (a: number) => void;
  readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;
/**
* Instantiates the given `module`, which can either be bytes or
* a precompiled `WebAssembly.Module`.
*
* @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
*
* @returns {InitOutput}
*/
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
* If `module_or_path` is {RequestInfo} or {URL}, makes a request and
* for everything else, calls `WebAssembly.instantiate` directly.
*
* @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
*
* @returns {Promise<InitOutput>}
*/
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
