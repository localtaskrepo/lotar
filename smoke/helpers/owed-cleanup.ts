// Cleanup a test still owes when Vitest gives up on it (DEV-586). A timed-out test keeps running in the background, so
// its `finally` blocks have not run when the file ends; its browser, server and workspace would outlive the run and
// fail the temp and server guards on top of the timeout. Helpers register what they open here and release it once
// closed. After a file with a failed test, `helpers/setup.ts` runs what is still registered; after a passing file it
// leaves it, so the guards still report a test that forgets to clean up.
const owed = new Set<() => Promise<unknown>>();

/** Register `cleanup` until the returned release is called. */
export function oweCleanup(cleanup: () => Promise<unknown>): () => void {
    owed.add(cleanup);
    return () => owed.delete(cleanup);
}

/** Run and forget every registered cleanup, newest first: a browser before its server, a server before its workspace. */
export async function runOwedCleanups(): Promise<void> {
    const cleanups = [...owed].reverse();
    owed.clear();
    for (const cleanup of cleanups) {
        await cleanup().catch(() => undefined);
    }
}
