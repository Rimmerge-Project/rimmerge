import { describe, expect, it } from "vitest";

import type { TagSignalDto } from "@/types/generated/TagSignalDto";
import { tagSignalLabel } from "@/utils/tagSignal";

function t(key: string, params?: Record<string, unknown>): string {
  switch (key) {
    case "tagSignal.urlContains":
      return `url contains "${params?.["needle"]}"`;
    case "tagSignal.assemblyRefTo":
      return `assembly ref to ${params?.["mod"]}`;
    case "tagSignal.dependsOn":
      return `depends on ${params?.["mod"]}`;
    default:
      throw new Error(`unexpected key ${key}`);
  }
}

const label = (id: string): string => `Label(${id})`;

describe("tagSignalLabel", () => {
  it("renders urlContains with the quoted needle", () => {
    const signal: TagSignalDto = { kind: "urlContains", needle: "example.com" };
    expect(tagSignalLabel(signal, t, label)).toBe('url contains "example.com"');
  });

  it("renders assemblyRefTo through the mod label resolver", () => {
    const signal: TagSignalDto = { kind: "assemblyRefTo", modId: "a.mod" };
    expect(tagSignalLabel(signal, t, label)).toBe("assembly ref to Label(a.mod)");
  });

  it("renders dependsOn through the mod label resolver", () => {
    const signal: TagSignalDto = { kind: "dependsOn", modId: "b.mod" };
    expect(tagSignalLabel(signal, t, label)).toBe("depends on Label(b.mod)");
  });
});
