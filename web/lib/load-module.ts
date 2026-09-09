type Browser = {
  sessionStorage: Pick<Storage, 'getItem' | 'setItem' | 'removeItem'>;
  location: Pick<Location, 'reload'>;
  navigator: Pick<Navigator, 'onLine'>;
};

const RELOAD_KEY = 'tempo:module-reload';
const FETCH_FAILURE =
  /Failed to fetch dynamically imported module|Importing a module script failed|error loading dynamically imported module|Unable to preload CSS/i;

export async function loadModule<T>(
  load: () => Promise<T>,
  browser: Browser | undefined = typeof window === 'undefined' ? undefined : window,
): Promise<T> {
  let result: T;
  try {
    result = await load();
  } catch (error) {
    if (browser && error instanceof Error && FETCH_FAILURE.test(error.message)) {
      try {
        // A rebuild can remove a chunk still referenced by an open tab. Reload
        // once, preserving the URL (and saved review); never loop while offline
        // or if the new build is broken. A successful import clears the guard.
        if (browser.navigator.onLine && !browser.sessionStorage.getItem(RELOAD_KEY)) {
          browser.sessionStorage.setItem(RELOAD_KEY, 'attempted');
          browser.location.reload();
        }
      } catch {
        // Without storage we cannot guard a reload; let the panel offer a retry.
      }
    }
    throw error;
  }
  try {
    browser?.sessionStorage.removeItem(RELOAD_KEY);
  } catch {
    // Storage is optional; a successfully loaded module must still render.
  }
  return result;
}
