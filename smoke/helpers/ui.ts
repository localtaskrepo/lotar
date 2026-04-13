import { Browser, BrowserContext, chromium, Page } from '@playwright/test';

export interface BrowserSessionOptions {
    readonly headless?: boolean;
}

export interface PageVisitOptions extends BrowserSessionOptions {
    readonly waitUntil?: 'load' | 'domcontentloaded' | 'networkidle';
}

/**
 * Perform a reliable HTML5 drag-and-drop between two elements.
 *
 * Playwright's built-in `page.dragAndDrop()` uses mouse events that don't
 * always fire the full dragstart → dragenter → dragover → drop → dragend
 * sequence expected by Vue `@drag*` handlers. This helper dispatches the
 * native HTML5 DragEvent sequence inside the page instead.
 */
export async function html5DragAndDrop(
    page: Page,
    sourceSelector: string,
    targetSelector: string,
    options: { ctrlKey?: boolean; altKey?: boolean; metaKey?: boolean } = {},
): Promise<void> {
    await page.evaluate(
        ({ src, tgt, opts }) => {
            const source = document.querySelector(src) as HTMLElement | null;
            const target = document.querySelector(tgt) as HTMLElement | null;
            if (!source || !target) {
                throw new Error(`DnD: could not find source (${src}) or target (${tgt})`);
            }

            const dt = new DataTransfer();

            const eventOpts = {
                bubbles: true,
                cancelable: true,
                ctrlKey: opts.ctrlKey ?? false,
                altKey: opts.altKey ?? false,
                metaKey: opts.metaKey ?? false,
            };

            source.dispatchEvent(new DragEvent('dragstart', { ...eventOpts, dataTransfer: dt }));
            target.dispatchEvent(new DragEvent('dragenter', { ...eventOpts, dataTransfer: dt }));
            target.dispatchEvent(new DragEvent('dragover', { ...eventOpts, dataTransfer: dt }));
            target.dispatchEvent(new DragEvent('drop', { ...eventOpts, dataTransfer: dt }));
            source.dispatchEvent(new DragEvent('dragend', { ...eventOpts, dataTransfer: dt }));
        },
        { src: sourceSelector, tgt: targetSelector, opts: options },
    );
}

export async function withBrowser<T>(
    options: BrowserSessionOptions,
    callback: (context: BrowserContext, browser: Browser) => Promise<T>,
): Promise<T> {
    const browser = await chromium.launch({ headless: options.headless ?? true });
    const context = await browser.newContext();

    try {
        return await callback(context, browser);
    } finally {
        await context.close();
        await browser.close();
    }
}

export async function withPage<T>(
    url: string,
    callback: (page: Page) => Promise<T>,
    options: PageVisitOptions = {},
): Promise<T> {
    return withBrowser(options, async (context) => {
        const page = await context.newPage();
        await page.goto(url, { waitUntil: options.waitUntil ?? 'domcontentloaded' });

        try {
            return await callback(page);
        } finally {
            await page.close();
        }
    });
}
