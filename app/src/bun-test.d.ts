// `bun test` provides this module; typed here instead of pulling in @types/bun.
declare module "bun:test" {
  export function test(name: string, fn: () => void): void;
  export function expect(value: unknown): {
    toBe: (expected: unknown) => void;
    toEqual: (expected: unknown) => void;
  };
}
