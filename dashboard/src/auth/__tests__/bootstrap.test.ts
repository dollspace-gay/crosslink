import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { bootstrapAuth, TOKEN_STORAGE_KEY } from "../bootstrap";
import { configureApiClient } from "@/api/client";

// Observe `configureApiClient` without touching the real module singleton.
vi.mock("@/api/client", () => ({
  configureApiClient: vi.fn(),
}));

const mockedConfigure = configureApiClient as unknown as ReturnType<typeof vi.fn>;

describe("bootstrapAuth", () => {
  const originalFetch = globalThis.fetch;
  let fetchSpy: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    vi.clearAllMocks();
    window.sessionStorage.clear();
    window.history.replaceState({}, "", "/");
    fetchSpy = vi.fn(() =>
      Promise.resolve(new Response("{}", { status: 200 })),
    );
    globalThis.fetch = fetchSpy as unknown as typeof globalThis.fetch;
  });

  afterEach(() => {
    globalThis.fetch = originalFetch;
  });

  it("returns null and does not configure the client when no token is present", () => {
    const result = bootstrapAuth();

    expect(result).toBeNull();
    expect(mockedConfigure).not.toHaveBeenCalled();
  });

  it("extracts a token from ?token=, persists it, scrubs the URL, and configures the client", () => {
    window.history.replaceState({}, "", "/?token=secret123&foo=bar");

    const result = bootstrapAuth();

    expect(result).toBe("secret123");
    expect(window.sessionStorage.getItem(TOKEN_STORAGE_KEY)).toBe("secret123");
    expect(window.location.search).toBe("?foo=bar");
    expect(mockedConfigure).toHaveBeenCalledTimes(1);
  });

  it("reuses a token stored by a previous load when ?token= is absent", () => {
    window.sessionStorage.setItem(TOKEN_STORAGE_KEY, "persisted456");

    const result = bootstrapAuth();

    expect(result).toBe("persisted456");
    expect(mockedConfigure).toHaveBeenCalledTimes(1);
  });

  it("prefers the URL token over the stored token to support re-auth", () => {
    window.sessionStorage.setItem(TOKEN_STORAGE_KEY, "oldtoken");
    window.history.replaceState({}, "", "/?token=newtoken");

    const result = bootstrapAuth();

    expect(result).toBe("newtoken");
    expect(window.sessionStorage.getItem(TOKEN_STORAGE_KEY)).toBe("newtoken");
  });

  it("installs a fetch wrapper that attaches Authorization: Bearer <token>", async () => {
    window.history.replaceState({}, "", "/?token=abc");

    bootstrapAuth();

    const call = mockedConfigure.mock.calls[0]?.[0] as
      | { fetchFn?: typeof globalThis.fetch }
      | undefined;
    expect(call?.fetchFn).toBeDefined();

    await call!.fetchFn!("/api/v1/agents", { method: "GET" });

    expect(fetchSpy).toHaveBeenCalledTimes(1);
    const [, init] = fetchSpy.mock.calls[0] as [unknown, RequestInit];
    const headers = new Headers(init.headers);
    expect(headers.get("Authorization")).toBe("Bearer abc");
  });

  it("preserves caller-supplied headers when attaching Authorization", async () => {
    window.history.replaceState({}, "", "/?token=xyz");

    bootstrapAuth();

    const call = mockedConfigure.mock.calls[0]?.[0] as
      | { fetchFn?: typeof globalThis.fetch }
      | undefined;
    expect(call?.fetchFn).toBeDefined();

    await call!.fetchFn!("/api/v1/x", {
      method: "POST",
      headers: { "Content-Type": "application/json", "X-Foo": "bar" },
      body: "{}",
    });

    const [, init] = fetchSpy.mock.calls[0] as [unknown, RequestInit];
    const headers = new Headers(init.headers);
    expect(headers.get("Authorization")).toBe("Bearer xyz");
    expect(headers.get("Content-Type")).toBe("application/json");
    expect(headers.get("X-Foo")).toBe("bar");
  });

  it("gracefully degrades when sessionStorage.setItem throws", () => {
    const spy = vi
      .spyOn(Storage.prototype, "setItem")
      .mockImplementation(() => {
        throw new Error("QuotaExceededError");
      });
    window.history.replaceState({}, "", "/?token=abc");

    const result = bootstrapAuth();

    // The token still works for this load; persistence just failed.
    expect(result).toBe("abc");
    expect(window.location.search).toBe("");
    expect(mockedConfigure).toHaveBeenCalledTimes(1);

    spy.mockRestore();
  });
});
