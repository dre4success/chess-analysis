import type { RustFinding } from './review-data.ts';
export { convertFinding } from './review-data.ts';
export type { RustEval, RustFinding, SavedReview } from './review-data.ts';
export type RustStep = {
  requests: { fen: string; nodes: number; position_command?: string }[];
  findings: RustFinding[];
  positions: number;
  error?: string;
};
export type CoreExports = {
  memory: WebAssembly.Memory;
  review_alloc: (length: number) => number;
  review_free: (pointer: number, length: number) => void;
  review_step: (pointer: number, length: number) => bigint;
};
let compiled: Promise<WebAssembly.Module> | undefined;
export class RustReview {
  private core: CoreExports;
  constructor(core: CoreExports) {
    this.core = core;
  }
  static async create() {
    compiled ??= fetch('/rust/chess_review.wasm')
      .then(async (res) => {
        if (!res.ok) throw new Error('The Rust review engine could not be loaded.');
        return WebAssembly.compile(await res.arrayBuffer());
      })
      .catch((error) => {
        compiled = undefined;
        throw error;
      });
    const instance = await WebAssembly.instantiate(await compiled, {});
    return new RustReview(instance.exports as unknown as CoreExports);
  }
  call(command: unknown): RustStep {
    const bytes = new TextEncoder().encode(JSON.stringify(command));
    const input = this.core.review_alloc(bytes.length);
    let output = 0,
      length = 0;
    try {
      new Uint8Array(this.core.memory.buffer, input, bytes.length).set(bytes);
      const result = this.core.review_step(input, bytes.length);
      output = Number(result & 0xffffffffn);
      length = Number(result >> 32n);
      const text = new TextDecoder().decode(
        new Uint8Array(this.core.memory.buffer, output, length),
      );
      const step = JSON.parse(text) as RustStep;
      if (step.error) throw new Error(step.error);
      return step;
    } finally {
      this.core.review_free(input, bytes.length);
      if (output) this.core.review_free(output, length);
    }
  }
}
