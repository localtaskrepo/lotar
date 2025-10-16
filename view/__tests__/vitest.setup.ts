import { afterEach, beforeAll, vi } from 'vitest';

const jsonResponse = (data: unknown) =>
  new Response(JSON.stringify({ data }), {
    status: 200,
    headers: { 'Content-Type': 'application/json' },
  });

beforeAll(() => {
  vi.stubGlobal(
    'fetch',
    vi.fn(async (input: RequestInfo | URL) => {
      const url = typeof input === 'string' ? input : input.toString();

      if (url.includes('/api/tasks/export')) {
        return new Response('', {
          status: 200,
          headers: { 'Content-Type': 'text/csv' },
        });
      }

      return jsonResponse(null);
    }),
  );
});

afterEach(() => {
  vi.clearAllMocks();
});
