import { chromium, Browser, BrowserContext, Page } from '@playwright/test';

export interface BrowserSessionOptions {
  readonly headless?: boolean;
}

export interface PageVisitOptions extends BrowserSessionOptions {
  readonly waitUntil?: 'load' | 'domcontentloaded' | 'networkidle';
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
