import { describe, expect, it } from "vitest";
import { formatVersion } from "./version";

describe("formatVersion", () => {
  it("joins app and scrcpy-server versions", () => {
    expect(formatVersion("0.1.0", "3.3.3")).toBe("v0.1.0 · scrcpy-server 3.3.3");
  });
});
