import { configureApiClient } from "@/api/client";

/**
 * sessionStorage key used to persist the bearer token across reloads in the
 * same browser tab. The key is tab-scoped on purpose — closing the tab
 * forgets the token and the user must paste the `?token=` URL again.
 */
export const TOKEN_STORAGE_KEY = "crosslink_api_token";

/**
 * Bootstrap API auth for the dashboard.
 *
 * `crosslink serve` protects every `/api/*` route (except `/api/v1/health`
 * and `/ws`) with a randomly-generated bearer token that it prints at
 * startup. The dashboard needs that token for its `fetch` calls to succeed.
 *
 * This function runs once, synchronously, before React mounts (stores
 * hydrate on the first render, so the client must be configured first):
 *
 * 1. If `?token=<value>` is present on the current URL, store it in
 *    `sessionStorage` and strip it from the URL via `history.replaceState`
 *    so it doesn't leak into the browser history.
 * 2. Otherwise, try to read a previously-persisted token from sessionStorage.
 * 3. If a token is available, wrap `globalThis.fetch` with a function that
 *    attaches `Authorization: Bearer <token>` to every request, and install
 *    it via `configureApiClient`. If no token is available, leave the
 *    client unconfigured — requests will 401 and the user will see the
 *    empty-state placeholders, matching the pre-fix behaviour.
 *
 * @returns the bearer token that was installed, or `null` if none was found.
 */
export function bootstrapAuth(): string | null {
  let token: string | null = null;

  try {
    const url = new URL(window.location.href);
    const urlToken = url.searchParams.get("token");
    if (urlToken) {
      token = urlToken;
      try {
        window.sessionStorage.setItem(TOKEN_STORAGE_KEY, urlToken);
      } catch {
        // sessionStorage may be unavailable (sandboxed iframes, some private
        // browsing modes). Fall through — the token still works for this load.
      }
      url.searchParams.delete("token");
      window.history.replaceState({}, "", url.toString());
    } else {
      try {
        token = window.sessionStorage.getItem(TOKEN_STORAGE_KEY);
      } catch {
        token = null;
      }
    }
  } catch {
    // Unknown failure resolving the current URL; skip auth setup.
    return null;
  }

  if (!token) return null;

  const nativeFetch = globalThis.fetch.bind(globalThis);
  const capturedToken = token;
  const authedFetch: typeof globalThis.fetch = (input, init) => {
    const headers = new Headers(init?.headers);
    headers.set("Authorization", `Bearer ${capturedToken}`);
    return nativeFetch(input, { ...init, headers });
  };
  configureApiClient({ fetchFn: authedFetch });

  return token;
}
